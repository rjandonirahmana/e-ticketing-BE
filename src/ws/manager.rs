//! ws/manager.rs — Production-scale WebSocket connection manager.
//!
//! ## Architecture
//! - 1 koneksi per user (kick lama saat login baru)
//! - WsTx = mpsc::Sender<Arc<str>> — broadcast O(1) alokasi, bukan O(n) clone
//! - Redis pubsub global (1 subscriber task, bukan per-user)
//! - Backoff eksponensial saat Redis disconnect
//! - DashMap shrink periodik agar heap tidak membengkak pasca spike
//! - Semaphore global untuk limit total koneksi WS

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use ahash::RandomState;
use dashmap::DashMap;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use tokio::sync::{Semaphore, mpsc};
use tokio_util::sync::CancellationToken;

use crate::ws::proto::WsEvent;

// ── Tuning constants ──────────────────────────────────────────────────────────

/// Buffer per koneksi — 32 × ~500 byte = ~16 KB max backpressure per user.
const CHAN_BUF: usize = 32;

/// Max koneksi WS simultan. Sesuaikan dengan RAM server.
/// 10_000 koneksi × ~8 KB overhead = ~80 MB.
const MAX_CONNECTIONS: usize = 10_000;

/// Heartbeat interval.
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// Timeout tanpa Pong → putus koneksi.
const PONG_TIMEOUT: Duration = Duration::from_secs(10);

/// Redis channel prefix untuk direct message.
const CH_USER: &str = "ws:u:";

/// Redis channel prefix untuk broadcast room.
const CH_ROOM: &str = "ws:r:";

/// Interval shrink DashMap — bebaskan memori pasca spike.
const SHRINK_INTERVAL: Duration = Duration::from_secs(300);

// ── Types ─────────────────────────────────────────────────────────────────────

/// Arc<str> agar broadcast ke N user hanya 1 alokasi + N clone pointer.
pub type WsTx = mpsc::Sender<Arc<str>>;

// ── WsManager ─────────────────────────────────────────────────────────────────

pub struct WsManager {
    /// user_id → outbound sender.
    sessions: DashMap<Arc<str>, WsTx, RandomState>,

    /// Satu Redis ConnectionManager — reuse, tidak buat koneksi baru per publish.
    redis: ConnectionManager,

    /// Counter pesan di-drop karena channel penuh.
    pub dropped: Arc<AtomicU64>,

    /// Semaphore untuk limit total koneksi simultan.
    conn_limit: Arc<Semaphore>,

    /// Counter koneksi aktif.
    active_conns: Arc<AtomicUsize>,

    shutdown: CancellationToken,
}

impl WsManager {
    pub async fn new(redis_client: redis::Client) -> anyhow::Result<Arc<Self>> {
        let redis = ConnectionManager::new(redis_client.clone()).await?;
        let shutdown = CancellationToken::new();
        let sessions = DashMap::with_hasher(RandomState::new());
        let conn_limit = Arc::new(Semaphore::new(MAX_CONNECTIONS));

        let mgr = Arc::new(Self {
            sessions,
            redis,
            dropped: Arc::new(AtomicU64::new(0)),
            conn_limit,
            active_conns: Arc::new(AtomicUsize::new(0)),
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

    /// Coba acquire slot koneksi. Return None jika sudah penuh (server overload).
    pub fn try_connect(
        &self,
        user_id: &str,
    ) -> Option<(
        mpsc::Receiver<Arc<str>>,
        CancellationToken,
        tokio::sync::OwnedSemaphorePermit,
    )> {
        // Non-blocking — langsung tolak jika server penuh
        let permit = self.conn_limit.clone().try_acquire_owned().ok()?;

        let (tx, rx) = mpsc::channel::<Arc<str>>(CHAN_BUF);
        let conn_token = CancellationToken::new();
        let key: Arc<str> = user_id.into();

        // Kick koneksi lama (1 user = 1 koneksi)
        if let Some((_, old_tx)) = self.sessions.remove(&key) {
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

    /// Hapus session dari registry. Permit di-drop otomatis oleh caller.
    pub fn disconnect(&self, user_id: &str) {
        self.sessions.remove(user_id);
        self.active_conns.fetch_sub(1, Ordering::Relaxed);
        tracing::debug!(user_id, "WS disconnected");
    }

    // ── Send ──────────────────────────────────────────────────────────────────

    /// Kirim ke satu user. Local-first; fallback Redis jika user di instance lain.
    pub async fn send_to(&self, user_id: &str, event: WsEvent) {
        let json: Arc<str> = Arc::from(event.to_json());
        if !self.deliver_local(user_id, json.clone()) {
            self.redis_publish(&format!("{CH_USER}{user_id}"), &json)
                .await;
        }
    }

    /// Broadcast ke room — O(1) Redis publish.
    /// Semua instance forward ke local sessions mereka sendiri via subscriber.
    pub async fn broadcast_room(&self, room_id: &str, event: WsEvent) {
        let json: Arc<str> = Arc::from(event.to_json());
        self.redis_publish(&format!("{CH_ROOM}{room_id}"), &json)
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
                        if tx.send(ping).await.is_err() {
                            break;
                        }
                        match tokio::time::timeout(PONG_TIMEOUT, pong_rx.recv()).await {
                            Ok(Some(())) => {} // pong diterima
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

    // ── Internal ──────────────────────────────────────────────────────────────

    /// Deliver ke local session. Return false jika user tidak ada di instance ini.
    fn deliver_local(&self, user_id: &str, json: Arc<str>) -> bool {
        if let Some(tx) = self.sessions.get(user_id) {
            match tx.try_send(json) {
                Ok(_) => return true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    drop(tx);
                    self.sessions.remove(user_id);
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(user_id, "WS channel full — dropping session");
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    drop(tx);
                    self.sessions.remove(user_id);
                }
            }
        }
        false
    }

    async fn redis_publish(&self, channel: &str, json: &str) {
        let mut conn = self.redis.clone();
        if let Err(e) = conn.publish::<_, _, ()>(channel, json).await {
            tracing::warn!(channel, error=%e, "WS Redis publish failed");
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
                                    // Arc<str> — clone murah saat broadcast
                                    let json: Arc<str> = Arc::from(payload);

                                    if let Some(uid) = channel.strip_prefix(CH_USER) {
                                        mgr.deliver_local(uid, json);
                                    } else if channel.strip_prefix(CH_ROOM).is_some() {
                                        // Deliver ke semua local session
                                        mgr.sessions.iter().for_each(|entry| {
                                            let _ = entry.value().try_send(json.clone());
                                        });
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
                        let len = mgr.sessions.len();
                        let cap = mgr.sessions.capacity();
                        if cap > 0 && len < cap / 2 {
                            mgr.sessions.shrink_to_fit();
                            tracing::debug!(len, cap, "DashMap shrunk");
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
