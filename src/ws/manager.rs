//! ws/manager.rs — Production-scale WebSocket connection manager.
//!
//! Fix dari iterasi sebelumnya:
//! 1. O(members) broadcast via room_members index
//! 2. Redis publish dengan retry + logging (bukan fire-and-forget)
//! 3. Per-user rate limiting (token bucket, in-memory, no Redis)

use std::{
    collections::HashMap,
    hash::RandomState,
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use dashmap::{DashMap, DashSet};
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use tokio::sync::{RwLock, Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use crate::ws::proto::WsEvent;

// ── Tuning ────────────────────────────────────────────────────────────────────

const CHAN_BUF: usize = 32;
pub const MAX_CONNECTIONS: usize = 10_000;
const PING_INTERVAL: Duration = Duration::from_secs(30);
const PONG_TIMEOUT: Duration = Duration::from_secs(10);
const CH_USER: &str = "ws:u:";
const CH_ROOM: &str = "ws:r:";
const SHRINK_INTERVAL: Duration = Duration::from_secs(300);

/// Redis publish: berapa kali retry sebelum give up
const REDIS_PUBLISH_RETRIES: u8 = 3;
/// Delay antar retry Redis publish
const REDIS_PUBLISH_RETRY_DELAY: Duration = Duration::from_millis(50);

/// Rate limit per user: max N message per window
const RATE_LIMIT_MAX: u32 = 30;
/// Window rate limit dalam detik
const RATE_LIMIT_WINDOW_SECS: u64 = 10;

pub type WsTx = mpsc::Sender<Arc<str>>;

// ── Per-user rate limiter (token bucket) ──────────────────────────────────────

struct UserBucket {
    /// Token tersisa dalam window ini
    tokens: AtomicU32,
    /// Timestamp awal window (unix seconds)
    window_start: AtomicU64,
}

impl UserBucket {
    fn new() -> Self {
        Self {
            tokens: AtomicU32::new(RATE_LIMIT_MAX),
            window_start: AtomicU64::new(now_secs()),
        }
    }

    /// Return true jika request diizinkan, false jika rate-limited.
    fn try_consume(&self) -> bool {
        let now = now_secs();
        let start = self.window_start.load(Ordering::Relaxed);
        let elapsed = now.saturating_sub(start);

        // Reset window kalau sudah lewat
        if elapsed >= RATE_LIMIT_WINDOW_SECS {
            self.tokens.store(RATE_LIMIT_MAX, Ordering::Relaxed);
            self.window_start.store(now, Ordering::Relaxed);
        }

        // CAS loop: kurangi token hanya kalau masih ada
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

/// Registry per-user rate limit bucket.
/// Pakai RwLock<HashMap> karena write (insert IP baru) jarang,
/// read (cek existing bucket) sering.
pub struct RateLimitRegistry {
    buckets: RwLock<HashMap<Arc<str>, Arc<UserBucket>>>,
}

impl RateLimitRegistry {
    fn new() -> Self {
        Self {
            buckets: RwLock::new(HashMap::new()),
        }
    }

    pub async fn check(&self, user_id: &str) -> bool {
        // Fast path: bucket sudah ada
        {
            let guard = self.buckets.read().await;
            if let Some(bucket) = guard.get(user_id) {
                return bucket.try_consume();
            }
        }
        // Slow path: buat bucket baru untuk user ini
        let bucket = Arc::new(UserBucket::new());
        bucket.try_consume(); // consume 1 untuk request sekarang
        self.buckets
            .write()
            .await
            .insert(Arc::from(user_id), bucket);
        true
    }

    /// Cleanup bucket lama (panggil periodik).
    pub async fn cleanup(&self) {
        let cutoff = now_secs().saturating_sub(RATE_LIMIT_WINDOW_SECS * 2);
        let mut guard = self.buckets.write().await;
        guard.retain(|_, b| b.window_start.load(Ordering::Relaxed) > cutoff);
        tracing::debug!("Rate limit cleanup: {} active users", guard.len());
    }
}

// ── WsManager ─────────────────────────────────────────────────────────────────

pub struct WsManager {
    /// user_id → outbound sender
    sessions: DashMap<Arc<str>, WsTx, RandomState>,

    /// room_id → Set<user_id>
    /// FIX 1: index ini membuat broadcast O(members) bukan O(total connections)
    room_members: DashMap<Arc<str>, DashSet<Arc<str>>, RandomState>,

    redis: ConnectionManager,
    pub dropped: Arc<AtomicU64>,
    conn_limit: Arc<Semaphore>,
    active_conns: Arc<AtomicUsize>,

    /// FIX 3: per-user rate limiter
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

        if let Some((_, old_tx)) = self.sessions.remove(&key) {
            self.active_conns.fetch_sub(1, Ordering::Relaxed);
            let msg: Arc<str> = Arc::from(
                WsEvent::err("REPLACED", "Session replaced by newer connection").to_json(),
            );
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
            tracing::debug!(user_id, "WS disconnected");
        }
    }

    // ── Room membership ───────────────────────────────────────────────────────

    pub fn join_room(&self, user_id: &str, room_id: &str) {
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

    /// Cek rate limit untuk user. Return false = rate-limited, tolak message.
    /// Panggil ini di handler sebelum dispatch ke service layer.
    pub async fn check_rate_limit(&self, user_id: &str) -> bool {
        self.rate_limit.check(user_id).await
    }

    // ── Send ──────────────────────────────────────────────────────────────────

    pub async fn send_to(&self, user_id: &str, event: WsEvent) {
        let json: Arc<str> = Arc::from(event.to_json());
        if !self.deliver_local(user_id, json.clone()) {
            self.redis_publish_with_retry(&format!("{CH_USER}{user_id}"), &json)
                .await;
        }
    }

    /// FIX 1: O(members) broadcast via room_members index.
    ///
    /// Sebelum: iterate SEMUA sessions (10k conn = 10k loop per message).
    /// Sekarang: hanya iterate member room itu.
    ///
    /// FIX 2: Redis publish dengan retry (bukan fire-and-forget).
    pub async fn broadcast_room(&self, room_id: &str, event: WsEvent) {
        let json: Arc<str> = Arc::from(event.to_json());

        // Local delivery — O(members) bukan O(all connections)
        if let Some(members) = self.room_members.get(room_id) {
            let ids: Vec<Arc<str>> = members.iter().map(|r| r.key().clone()).collect();
            drop(members); // lepas lock sebelum deliver
            for uid in &ids {
                self.deliver_local(uid, json.clone());
            }
        }

        // Cross-instance delivery via Redis
        self.redis_publish_with_retry(&format!("{CH_ROOM}{room_id}"), &json)
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
            let mut interval = tokio::time::interval(PING_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = shutdown.cancelled()    => break,
                    _ = conn_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        let ping: Arc<str> = Arc::from(WsEvent::Ping.to_json());
                        if tx.send(ping).await.is_err() { break; }
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

    fn deliver_local(&self, user_id: &str, json: Arc<str>) -> bool {
        if let Some(tx) = self.sessions.get(user_id) {
            match tx.try_send(json) {
                Ok(_) => return true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    drop(tx);
                    if self.sessions.remove(user_id).is_some() {
                        self.active_conns.fetch_sub(1, Ordering::Relaxed);
                    }
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(user_id, "WS channel full — dropping session");
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    drop(tx);
                    if self.sessions.remove(user_id).is_some() {
                        self.active_conns.fetch_sub(1, Ordering::Relaxed);
                    }
                }
            }
        }
        false
    }

    /// FIX 2: Redis publish dengan retry + structured logging.
    /// Sebelum: fire-and-forget → message hilang saat Redis hiccup.
    /// Sekarang: retry N kali dengan delay eksponensial, log setiap failure.
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
                        channel,
                        attempt,
                        error  = %e,
                        "Redis publish failed, retrying in {:?}", delay
                    );
                    if attempt + 1 < REDIS_PUBLISH_RETRIES {
                        tokio::time::sleep(delay).await;
                        delay *= 2; // exponential backoff: 50ms → 100ms → 200ms
                    } else {
                        // Semua retry habis — message ini hilang
                        // Di masa depan: tulis ke local queue / dead-letter untuk recovery
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
                                        mgr.deliver_local(uid, json);
                                    } else if let Some(room_id) = channel.strip_prefix(CH_ROOM) {
                                        // FIX 1: pakai room_members index (O(members))
                                        if let Some(members) = mgr.room_members.get(room_id) {
                                            let ids: Vec<Arc<str>> = members
                                                .iter()
                                                .map(|r| r.key().clone())
                                                .collect();
                                            drop(members);
                                            for uid in &ids {
                                                mgr.deliver_local(uid, json.clone());
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
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(SHRINK_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = interval.tick() => {
                        mgr.rate_limit.cleanup().await;
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
    let wait = Duration::from_secs(*secs);
    tracing::info!("WS Redis reconnect in {}s", *secs);
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
