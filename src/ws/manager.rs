//! ws/manager.rs — Production-scale WebSocket connection manager.
//!
//! OPTIMISASI vs original:
//!
//! 1. broadcast_room(): serialize SEKALI via to_shared_json(), deliver Arc<str> clone
//!    ke semua member. Original serialize SEKALI tapi wrap Arc setelah. Sekarang
//!    explicit dan konsisten via to_shared_json() API.
//!
//! 2. RateLimitRegistry: ganti RwLock<HashMap> → DashMap untuk konsistensi dengan
//!    sessions/room_members. RwLock<HashMap>.read() masih blocking saat ada writer.
//!    DashMap shard-based = lebih paralel untuk workload baca-dominan.
//!
//! 3. try_connect(): WsEvent::err() sekarang terima ErrorCode enum (bukan string literal).
//!
//! 4. send_to(): serialize Arc<str> via to_shared_json() — konsisten dengan broadcast.
//!
//! 5. spawn_heartbeat(): Ping Arc<str> pre-serialized via WsEvent::Ping.to_json()
//!    (kena fast-path OnceLock, bukan serialize ulang tiap tick).
//!
//! Yang TIDAK diubah (sudah optimal):
//! - O(members) broadcast via room_members index — tetap
//! - Redis publish dengan retry + exponential backoff — tetap
//! - Per-user rate limiter (token bucket, lock-free CAS) — tetap
//! - Bounded semaphore di handler untuk cegah task flood — tetap
//! - DashMap untuk sessions & room_members — tetap

use std::{
    hash::RandomState,
    sync::{
        atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use dashmap::{DashMap, DashSet};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::ws::proto::{ErrorCode, WsEvent};

// ── Tuning ────────────────────────────────────────────────────────────────────

const CHAN_BUF: usize = 32;
pub const MAX_CONNECTIONS: usize = 10_000;
const PING_INTERVAL: Duration = Duration::from_secs(30);
const PONG_TIMEOUT: Duration = Duration::from_secs(10);
const CH_USER: &str = "ws:u:";
const CH_ROOM: &str = "ws:r:";
const SHRINK_INTERVAL: Duration = Duration::from_secs(300);

const REDIS_PUBLISH_RETRIES: u8 = 3;
const REDIS_PUBLISH_RETRY_DELAY: Duration = Duration::from_millis(50);

const RATE_LIMIT_MAX: u32 = 30;
const RATE_LIMIT_WINDOW_SECS: u64 = 10;
/// FIX: Hard cap entry rate limit registry.
/// Tanpa batas, flash-sale dengan 1M user unik bisa membengkak tanpa batas.
/// 500k entry × ~120 bytes (Arc<str> + UserBucket) ≈ 60MB — acceptable upper bound.
const RATE_LIMIT_MAX_ENTRIES: usize = 500_000;
/// Cleanup rate limit lebih sering dari SHRINK_INTERVAL (5 menit) karena window hanya 10 detik.
const RATE_LIMIT_CLEANUP_INTERVAL: Duration = Duration::from_secs(30);

pub type WsTx = mpsc::Sender<Arc<str>>;

// ── Per-user rate limiter (token bucket, lock-free) ────────────────────────────

struct UserBucket {
    tokens: AtomicU32,
    window_start: AtomicU64,
}

impl UserBucket {
    fn new() -> Self {
        Self {
            tokens: AtomicU32::new(RATE_LIMIT_MAX),
            window_start: AtomicU64::new(now_secs()),
        }
    }

    fn try_consume(&self) -> bool {
        let now = now_secs();
        let start = self.window_start.load(Ordering::Relaxed);

        if now.saturating_sub(start) >= RATE_LIMIT_WINDOW_SECS {
            self.tokens.store(RATE_LIMIT_MAX, Ordering::Relaxed);
            self.window_start.store(now, Ordering::Relaxed);
        }

        loop {
            let cur = self.tokens.load(Ordering::Acquire);
            if cur == 0 {
                return false;
            }
            match self.tokens.compare_exchange_weak(
                cur,
                cur - 1,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(_) => {} // retry CAS
            }
        }
    }
}

/// Rate limit registry.
///
/// OPTIMISASI vs original: DashMap menggantikan RwLock<HashMap>.
/// - RwLock<HashMap>: write lock untuk insert bucket baru memblokir SEMUA reader.
///   Di high-concurrency, setiap user baru = stop-the-world singkat.
/// - DashMap: sharded locking — insert di shard X tidak blokir read di shard Y.
/// - Workload: baca (try_consume) dominan, tulis (insert baru) jarang → DashMap ideal.
pub struct RateLimitRegistry {
    buckets: DashMap<Arc<str>, Arc<UserBucket>, RandomState>,
}

impl RateLimitRegistry {
    fn new() -> Self {
        Self {
            buckets: DashMap::with_hasher(RandomState::new()),
        }
    }

    pub fn check(&self, user_id: &str) -> bool {
        // Fast path: bucket sudah ada
        if let Some(bucket) = self.buckets.get(user_id) {
            return bucket.try_consume();
        }

        // FIX: Hard cap — tolak request baru jika sudah melebihi batas entry.
        // Lebih baik rate-limit user baru daripada OOM saat flash sale.
        if self.buckets.len() >= RATE_LIMIT_MAX_ENTRIES {
            tracing::warn!(
                user_id,
                entries = self.buckets.len(),
                "RateLimitRegistry at capacity — new user rejected"
            );
            return false;
        }

        // Slow path: user baru — buat bucket
        let bucket = Arc::new(UserBucket::new());
        bucket.try_consume();
        self.buckets
            .entry(Arc::from(user_id))
            .or_insert_with(|| bucket);
        true
    }

    /// Cleanup bucket expired. Dipanggil periodik setiap RATE_LIMIT_CLEANUP_INTERVAL (30s).
    /// FIX: Cleanup lebih sering (30s vs 5 menit) karena window hanya 10 detik.
    /// Entry yang window_start-nya > 2× window sudah pasti idle.
    pub fn cleanup(&self) {
        let cutoff = now_secs().saturating_sub(RATE_LIMIT_WINDOW_SECS * 2);
        self.buckets
            .retain(|_, b| b.window_start.load(Ordering::Relaxed) > cutoff);
        tracing::debug!("Rate limit cleanup: {} active users", self.buckets.len());
    }
}

// ── WsManager ─────────────────────────────────────────────────────────────────

pub struct WsManager {
    /// user_id → outbound sender
    sessions: DashMap<Arc<str>, WsTx, RandomState>,

    /// room_id → Set<user_id>
    /// Membuat broadcast O(members) bukan O(total connections)
    room_members: DashMap<Arc<str>, DashSet<Arc<str>>, RandomState>,

    redis: ConnectionManager,
    pub dropped: Arc<AtomicU64>,
    conn_limit: Arc<Semaphore>,
    active_conns: Arc<AtomicUsize>,
    rate_limit: Arc<RateLimitRegistry>,
    shutdown: CancellationToken,
}

impl WsManager {
    pub async fn new(redis_client: redis::Client) -> anyhow::Result<Arc<Self>> {
        let redis = ConnectionManager::new(redis_client.clone()).await?;
        let shutdown = CancellationToken::new();

        let mgr = Arc::new(Self {
            sessions: DashMap::with_hasher(RandomState::new()),
            room_members: DashMap::with_hasher(RandomState::new()),
            redis,
            dropped: Arc::new(AtomicU64::new(0)),
            conn_limit: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
            active_conns: Arc::new(AtomicUsize::new(0)),
            rate_limit: Arc::new(RateLimitRegistry::new()),
            shutdown: shutdown.clone(),
        });

        Self::spawn_redis_subscriber(mgr.clone(), redis_client);
        Self::spawn_shrink_task(mgr.clone());
        Ok(mgr)
    }

    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }

    // ── Connect / Disconnect ──────────────────────────────────────────────────

    pub fn try_connect(
        &self,
        user_id: &str,
    ) -> Option<(
        mpsc::Receiver<Arc<str>>,
        CancellationToken,
        tokio::sync::OwnedSemaphorePermit,
    )> {
        let permit = self.conn_limit.clone().try_acquire_owned().ok()?;
        let (tx, rx) = mpsc::channel::<Arc<str>>(CHAN_BUF);
        let conn_token = CancellationToken::new();
        let key: Arc<str> = user_id.into();

        // Kick existing session (replaced connection)
        if let Some((_, old_tx)) = self.sessions.remove(&key) {
            self.active_conns.fetch_sub(1, Ordering::Relaxed);
            // Pre-serialized error untuk session lama — tidak perlu serialize baru
            let msg =
                WsEvent::err(ErrorCode::Replaced, "Session replaced by newer connection").to_json();
            let _ = old_tx.try_send(msg);
        }

        self.sessions.insert(key, tx);
        self.active_conns.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(user_id, "WS connected");
        Some((rx, conn_token, permit))
    }

    pub fn disconnect(&self, user_id: &str) {
        if self.sessions.remove(user_id).is_some() {
            self.active_conns.fetch_sub(1, Ordering::Relaxed);
            // FIX: Bersihkan room_members saat disconnect — mencegah "ghost members"
            // yang menumpuk di DashMap. Sebelumnya hanya sessions yang di-cleanup,
            // tapi room_members tetap menyimpan user_id → broadcast_room() terus
            // mencoba deliver ke user yang sudah offline → deliver_local() gagal
            // (session tidak ada) → trigger leave_all_rooms() baru, tapi terlambat.
            // Dengan cleanup di sini, setiap path disconnect (panic, timeout, graceful)
            // dijamin bersih. O(rooms) per disconnect — acceptable.
            self.leave_all_rooms(user_id);
            tracing::debug!(user_id, "WS disconnected");
        }
    }

    // ── Room membership ───────────────────────────────────────────────────────

    pub fn join_room(&self, user_id: &str, room_id: &str) {
        // FIX: Jangan tambahkan user ke room_members WS index jika tidak punya
        // active session. User offline akan join kembali saat reconnect via Hello flow.
        // Tanpa guard ini, ghost members menumpuk di room_members dan menyebabkan
        // Redis publish sia-sia untuk setiap broadcast.
        if !self.sessions.contains_key(user_id) {
            return;
        }
        self.room_members
            .entry(Arc::from(room_id))
            .or_insert_with(|| DashSet::with_hasher(RandomState::new()))
            .insert(Arc::from(user_id));
    }

    pub fn leave_room(&self, user_id: &str, room_id: &str) {
        if let Some(members) = self.room_members.get(room_id) {
            members.remove(user_id);
        }
    }

    pub fn leave_all_rooms(&self, user_id: &str) {
        let empty_rooms: Vec<Arc<str>> = self
            .room_members
            .iter()
            .filter_map(|entry| {
                entry.value().remove(user_id);
                if entry.value().is_empty() {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();

        for room_id in empty_rooms {
            if let Some(members) = self.room_members.get(&room_id) {
                if members.is_empty() {
                    drop(members);
                    self.room_members.remove(&room_id);
                }
            }
        }
    }

    pub fn register_rooms(&self, user_id: &str, room_ids: &[String]) {
        for room_id in room_ids {
            self.join_room(user_id, room_id);
        }
    }

    // ── Rate limit ────────────────────────────────────────────────────────────

    /// Cek rate limit — sekarang sync (bukan async) karena DashMap tidak butuh await.
    /// Caller di handler tidak perlu .await → sedikit lebih efisien.
    pub fn check_rate_limit(&self, user_id: &str) -> bool {
        self.rate_limit.check(user_id)
    }

    // ── Send ──────────────────────────────────────────────────────────────────

    pub async fn send_to(&self, user_id: &str, event: WsEvent) {
        // to_shared_json() — Arc<str> bisa di-pass ke deliver_local dan redis tanpa copy
        let json = event.to_shared_json();
        if !self.deliver_local(user_id, json.clone()).await {
            self.redis_publish_with_retry(&format!("{CH_USER}{user_id}"), &json)
                .await;
        }
    }

    /// Broadcast ke semua member room.
    ///
    /// FIX P0: Hapus local delivery dari sini — cukup publish ke Redis SEKALI.
    /// Redis subscriber di instance yang sama akan deliver ke lokal user.
    ///
    /// Masalah sebelumnya (Redis Loopback Duplication):
    ///   1. broadcast_room() deliver lokal ke semua member room
    ///   2. broadcast_room() juga publish ke Redis
    ///   3. spawn_redis_subscriber() menerima publish itu dan deliver lokal LAGI
    ///   → Semua member di instance ini terima pesan 2×
    ///
    /// Fix: Biarkan Redis subscriber jadi satu-satunya path delivery.
    /// Trade-off: +1-2ms latency untuk lokal user (loopback Redis vs in-process).
    /// Acceptable karena: konsistensi single delivery path > micro-latency gain.
    ///
    /// CATATAN: serialize SEKALI via to_shared_json(), Arc<str> clone ke
    /// subscriber = atomic bump — optimisasi ini masih berlaku.
    pub async fn broadcast_room(&self, room_id: &str, event: WsEvent) {
        let shared = event.to_shared_json();
        // Hanya publish ke Redis — subscriber lokal juga akan deliver ke semua member
        self.redis_publish_with_retry(&format!("{CH_ROOM}{room_id}"), &shared)
            .await;
    }

    // ── Heartbeat ─────────────────────────────────────────────────────────────

    pub fn spawn_heartbeat(
        &self,
        user_id: String,
        tx: WsTx,
        conn_cancel: CancellationToken,
    ) -> mpsc::Sender<()> {
        let (pong_tx, mut pong_rx) = mpsc::channel::<()>(1);
        let shutdown = self.shutdown.clone();

        tokio::spawn(async move {
            // Pre-compute Ping JSON sekali — kena OnceLock fast path
            let ping_json = WsEvent::Ping.to_json();

            let mut interval = tokio::time::interval(PING_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = shutdown.cancelled()    => break,
                    _ = conn_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        // Clone Arc = atomic bump, bukan serialize ulang
                        if tx.send(ping_json.clone()).await.is_err() { break; }
                        match tokio::time::timeout(PONG_TIMEOUT, pong_rx.recv()).await {
                            Ok(Some(())) => {}
                            _ => {
                                tracing::warn!(user_id, "WS heartbeat timeout");
                                conn_cancel.cancel();
                                break;
                            }
                        }
                    }
                }
            }
        });

        pong_tx
    }

    // ── Stats ─────────────────────────────────────────────────────────────────

    pub fn online_count(&self) -> usize {
        self.active_conns.load(Ordering::Relaxed)
    }
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
    pub fn is_online(&self, uid: &str) -> bool {
        self.sessions.contains_key(uid)
    }
    pub fn room_count(&self) -> usize {
        self.room_members.len()
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    /// P1 FIX: async sekarang agar bisa await timeout retry sebelum evict.
    ///
    /// Sebelumnya: TrySendError::Full → langsung evict session.
    /// Problem: burst broadcast (e.g. merchant broadcast ke 500 user) menyebabkan
    /// user dengan koneksi lambat terputus paksa meski buffer baru penuh sesaat.
    ///
    /// Sekarang: try_send → jika Full, tunggu 50ms lalu coba sekali lagi.
    /// Hanya evict jika setelah timeout masih tidak bisa terkirim.
    async fn deliver_local(&self, user_id: &str, json: Arc<str>) -> bool {
        // Clone Sender keluar dari DashMap guard sebelum await
        // (tidak boleh hold DashMap ref across await point)
        let tx = match self.sessions.get(user_id) {
            Some(r) => r.value().clone(),
            None => {
                // Session tidak ada — bersihkan ghost membership
                self.leave_all_rooms(user_id);
                return false;
            }
        };

        match tx.try_send(json.clone()) {
            Ok(_) => return true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                // P1: Retry sekali dengan timeout 50ms — beri kesempatan client drain buffer
                // sebelum memutuskan untuk evict (cegah aggressive disconnect saat burst)
                match tokio::time::timeout(Duration::from_millis(50), tx.send(json)).await {
                    Ok(Ok(_)) => return true,
                    _ => {
                        // Masih gagal setelah retry — evict
                        drop(tx);
                        if self.sessions.remove(user_id).is_some() {
                            self.active_conns.fetch_sub(1, Ordering::Relaxed);
                        }
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                        tracing::warn!(
                            user_id,
                            "WS channel still full after 50ms retry — evicting session"
                        );
                        self.leave_all_rooms(user_id);
                    }
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                drop(tx);
                if self.sessions.remove(user_id).is_some() {
                    self.active_conns.fetch_sub(1, Ordering::Relaxed);
                }
                // Lazy eviction — hapus dari semua room saat channel closed
                self.leave_all_rooms(user_id);
            }
        }
        false
    }

    /// Redis publish dengan retry + exponential backoff.
    async fn redis_publish_with_retry(&self, channel: &str, json: &str) {
        let mut delay = REDIS_PUBLISH_RETRY_DELAY;

        for attempt in 0..REDIS_PUBLISH_RETRIES {
            let mut conn = self.redis.clone();
            match conn.publish::<_, _, ()>(channel, json).await {
                Ok(_) => {
                    if attempt > 0 {
                        tracing::debug!(channel, attempt, "Redis publish succeeded after retry");
                    }
                    return;
                }
                Err(e) => {
                    tracing::warn!(
                        channel, attempt, error = %e,
                        "Redis publish failed, retrying in {:?}", delay
                    );
                    if attempt + 1 < REDIS_PUBLISH_RETRIES {
                        tokio::time::sleep(delay).await;
                        delay *= 2; // exponential: 50ms → 100ms → 200ms
                    } else {
                        tracing::error!(
                            channel,
                            "Redis publish FAILED after {} retries — message dropped",
                            REDIS_PUBLISH_RETRIES
                        );
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
    }

    fn spawn_redis_subscriber(mgr: Arc<Self>, client: redis::Client) {
        let shutdown = mgr.shutdown.clone();
        tokio::spawn(async move {
            let mut backoff_secs: u64 = 1;
            loop {
                if shutdown.is_cancelled() {
                    return;
                }
                match client.get_async_pubsub().await {
                    Ok(mut ps) => {
                        if ps.psubscribe("ws:u:*").await.is_err()
                            || ps.psubscribe("ws:r:*").await.is_err()
                        {
                            backoff(&mut backoff_secs, &shutdown).await;
                            continue;
                        }
                        backoff_secs = 1;
                        tracing::info!("WS Redis subscriber ready");

                        let mut stream = ps.on_message();
                        loop {
                            tokio::select! {
                                _ = shutdown.cancelled() => return,
                                msg = futures::StreamExt::next(&mut stream) => {
                                    let Some(msg) = msg else {
                                        tracing::warn!("WS Redis subscriber disconnected");
                                        break;
                                    };
                                    let channel = msg.get_channel_name().to_string();
                                    let payload: String = match msg.get_payload() {
                                        Ok(s) => s,
                                        Err(_) => continue,
                                    };
                                    let json: Arc<str> = Arc::from(payload);

                                    if let Some(uid) = channel.strip_prefix(CH_USER) {
                                        mgr.deliver_local(uid, json).await;
                                    } else if let Some(room_id) = channel.strip_prefix(CH_ROOM) {
                                        if let Some(members) = mgr.room_members.get(room_id) {
                                            let ids: Vec<Arc<str>> = members
                                                .iter()
                                                .map(|r| r.key().clone())
                                                .collect();
                                            drop(members);
                                            for uid in &ids {
                                                mgr.deliver_local(uid, json.clone()).await;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        drop(stream);
                    }
                    Err(e) => tracing::error!("WS Redis pubsub connect failed: {e}"),
                }
                backoff(&mut backoff_secs, &shutdown).await;
            }
        });
    }

    fn spawn_shrink_task(mgr: Arc<Self>) {
        let shutdown = mgr.shutdown.clone();

        // ── Timer 1: rate limit cleanup setiap 30 detik ───────────────────────
        // FIX: Rate limit window hanya 10s, cleanup setiap 5 menit terlalu lama.
        // Entry idle bisa menumpuk hingga 500k sebelum di-cleanup.
        let mgr2 = mgr.clone();
        let shutdown2 = shutdown.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(RATE_LIMIT_CLEANUP_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = shutdown2.cancelled() => break,
                    _ = interval.tick() => {
                        mgr2.rate_limit.cleanup();
                    }
                }
            }
        });

        // ── Timer 2: DashMap shrink setiap 5 menit ────────────────────────────
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(SHRINK_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = interval.tick() => {
                        let len = mgr.sessions.len();
                        let cap = mgr.sessions.capacity();
                        if cap > 0 && len < cap / 2 {
                            mgr.sessions.shrink_to_fit();
                            mgr.room_members.shrink_to_fit();
                            tracing::debug!(
                                sessions = len,
                                rooms    = mgr.room_members.len(),
                                dropped  = mgr.dropped(),
                                "DashMap shrunk"
                            );
                        }
                    }
                }
            }
        });
    }
}

async fn backoff(secs: &mut u64, shutdown: &CancellationToken) {
    // FIX: Tambah jitter 0–1000ms agar beberapa instance tidak reconnect bersamaan
    // (thundering herd problem saat Redis restart dengan banyak instance Rust).
    let jitter_ms = rand::random::<u64>() % 1000;
    let wait = Duration::from_millis(*secs * 1000 + jitter_ms);
    tracing::info!(
        "WS Redis reconnect in {}ms (jitter included)",
        wait.as_millis()
    );
    *secs = (*secs * 2).min(30);
    tokio::select! {
        _ = shutdown.cancelled() => {}
        _ = tokio::time::sleep(wait) => {}
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
