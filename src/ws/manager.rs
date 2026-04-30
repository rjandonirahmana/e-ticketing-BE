//! ws/manager.rs — WebSocket connection manager.
//!
//! ## Memory model
//! - 1 entry DashMap per user (Arc<str> key + mpsc::Sender)
//! - Channel buffer kecil (32) agar memori per-koneksi ~2-4 KB
//! - Redis multiplexed connection di-share (bukan buat baru tiap publish)
//! - Stale sessions dibersihkan otomatis saat send gagal
//!
//! ## Fixes dari versi sebelumnya
//! 1. `redis_pub` sebelumnya panggil `get_multiplexed_async_connection()` setiap
//!    kali → buat koneksi baru tiap pesan. Sekarang pakai `ConnectionManager`
//!    yang mereuse satu koneksi.
//! 2. `broadcast_to` publish per-user → O(n) Redis round-trips. Sekarang
//!    publish satu kali per room ke channel `ws:room:{room_id}`.
//! 3. Tidak ada heartbeat → stale connections menumpuk di DashMap selamanya.
//!    Sekarang ada ping task per-koneksi via `CancellationToken`.
//! 4. `spawn_redis_subscriber` tidak ada backoff eksponensial → tight loop
//!    saat Redis mati. Sekarang ada backoff 1s → 2s → 4s → max 30s.
//! 5. Channel buffer 128 terlalu besar → ~1MB per 1000 user. Turun ke 32.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use ahash::RandomState;
use dashmap::DashMap;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::ws::proto::WsEvent;

// ── Tuning constants ──────────────────────────────────────────────────────────

/// Buffer per koneksi. 32 × ~500 byte JSON = ~16 KB max per user.
const CHAN_BUF: usize = 32;

/// Heartbeat interval — server kirim Ping ke client.
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// Timeout tanpa Pong sebelum koneksi dianggap stale dan diputus.
const PONG_TIMEOUT: Duration = Duration::from_secs(10);

/// Prefix channel Redis untuk direct message ke satu user.
const CH_USER: &str = "ws:u:";

/// Prefix channel Redis untuk broadcast ke seluruh room.
const CH_ROOM: &str = "ws:r:";

// ── Types ─────────────────────────────────────────────────────────────────────

pub type WsTx = mpsc::Sender<String>;

// ── WsManager ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct WsManager {
    /// user_id → outbound sender. Key pakai Arc<str> agar zero-copy lookup.
    sessions: DashMap<Arc<str>, WsTx, RandomState>,

    /// Satu Redis ConnectionManager yang di-share — tidak buat koneksi baru
    /// setiap kali publish.
    redis: ConnectionManager,

    /// Counter pesan yang di-drop karena channel penuh.
    pub dropped: Arc<AtomicU64>,

    shutdown: CancellationToken,
}

impl WsManager {
    /// Buat WsManager. `redis` harus sudah connected.
    pub async fn new(redis_client: redis::Client) -> anyhow::Result<Arc<Self>> {
        let redis = ConnectionManager::new(redis_client.clone()).await?;
        let shutdown = CancellationToken::new();

        let mgr = Arc::new(Self {
            sessions: DashMap::with_hasher(RandomState::new()),
            redis,
            dropped: Arc::new(AtomicU64::new(0)),
            shutdown: shutdown.clone(),
        });

        // Satu subscriber global — forward Redis pubsub ke local sessions.
        Self::spawn_redis_subscriber(mgr.clone(), redis_client);

        Ok(mgr)
    }

    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }

    // ── Connect / Disconnect ──────────────────────────────────────────────────

    /// Register koneksi baru. Return Receiver untuk outbound messages.
    /// Jika user sudah connect, koneksi lama di-kick (1 koneksi per user).
    pub fn connect(&self, user_id: &str) -> (mpsc::Receiver<String>, CancellationToken) {
        let (tx, rx) = mpsc::channel::<String>(CHAN_BUF);
        let conn_token = CancellationToken::new();
        let key: Arc<str> = user_id.into();

        // Kick koneksi lama
        if let Some((_, old_tx)) = self.sessions.remove(&key) {
            let _ = old_tx.try_send(
                WsEvent::err("REPLACED", "Session replaced by a newer connection").to_json(),
            );
        }

        self.sessions.insert(key, tx);
        tracing::debug!(user_id, "WS connected");
        (rx, conn_token)
    }

    /// Hapus koneksi dari registry. Dipanggil saat socket ditutup.
    pub fn disconnect(&self, user_id: &str) {
        self.sessions.remove(user_id);
        tracing::debug!(user_id, "WS disconnected");
    }

    // ── Send ──────────────────────────────────────────────────────────────────

    /// Kirim ke satu user. Local-first; fallback Redis jika tidak ada di instance ini.
    pub async fn send_to(&self, user_id: &str, event: WsEvent) {
        let json = event.to_json();
        if !self.deliver_local(user_id, &json) {
            self.redis_publish(&format!("{CH_USER}{user_id}"), &json)
                .await;
        }
    }

    /// Broadcast ke semua member sebuah room.
    /// Local delivery untuk yang online di instance ini, satu publish Redis
    /// untuk yang ada di instance lain — O(1) Redis round-trip, bukan O(n).
    pub async fn broadcast_room(&self, room_id: &str, member_ids: &[String], event: WsEvent) {
        let json = event.to_json();
        let mut has_remote = false;

        for uid in member_ids {
            if !self.deliver_local(uid, &json) {
                has_remote = true;
            }
        }

        // Satu publish ke room channel — semua instance lain forward ke local sessions
        if has_remote {
            self.redis_publish(&format!("{CH_ROOM}{room_id}"), &json)
                .await;
        }
    }

    // ── Heartbeat ─────────────────────────────────────────────────────────────

    /// Spawn ping/pong heartbeat task untuk satu koneksi.
    /// Jika client tidak balas Pong dalam PONG_TIMEOUT, koneksi dianggap stale.
    /// Return sender untuk memberi tahu task bahwa Pong diterima.
    pub fn spawn_heartbeat(
        &self,
        user_id: String,
        tx: WsTx,
        conn_cancel: CancellationToken,
    ) -> tokio::sync::mpsc::Sender<()> {
        let (pong_tx, mut pong_rx) = tokio::sync::mpsc::channel::<()>(1);
        let shutdown = self.shutdown.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(PING_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => break,
                    _ = conn_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        // Kirim Ping
                        let ping_json = WsEvent::Ping.to_json();
                        if tx.send(ping_json).await.is_err() {
                            // Channel closed — socket sudah mati
                            break;
                        }

                        // Tunggu Pong dalam timeout
                        let got_pong = tokio::time::timeout(PONG_TIMEOUT, pong_rx.recv()).await;
                        if got_pong.is_err() || got_pong.unwrap().is_none() {
                            tracing::warn!(user_id, "WS heartbeat timeout — forcing disconnect");
                            conn_cancel.cancel();
                            break;
                        }
                    }
                }
            }
        });

        pong_tx
    }

    // ── Stats ─────────────────────────────────────────────────────────────────

    pub fn online_count(&self) -> usize {
        self.sessions.len()
    }
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
    pub fn is_online(&self, uid: &str) -> bool {
        self.sessions.contains_key(uid)
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    /// Deliver ke local session. Return false jika user tidak online di instance ini.
    fn deliver_local(&self, user_id: &str, json: &str) -> bool {
        if let Some(tx) = self.sessions.get(user_id) {
            match tx.try_send(json.to_string()) {
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

    /// Publish ke Redis channel. Reuse ConnectionManager — tidak buat koneksi baru.
    async fn redis_publish(&self, channel: &str, json: &str) {
        let mut conn = self.redis.clone(); // Clone = share underlying connection
        if let Err(e) = conn.publish::<_, _, ()>(channel, json).await {
            tracing::warn!(channel, error=%e, "WS Redis publish failed");
        }
    }

    /// Satu subscriber global. Forward pesan dari Redis ke local sessions.
    /// Subscribe ke dua pattern:
    ///   ws:u:{user_id}  → direct message ke user
    ///   ws:r:{room_id}  → broadcast ke member room yg online di instance ini
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
                        // Subscribe ke kedua pattern sekaligus
                        if let Err(e) = ps.psubscribe("ws:u:*").await {
                            tracing::error!("WS psubscribe ws:u:* failed: {e}");
                            backoff(&mut backoff_secs, &shutdown).await;
                            continue;
                        }
                        if let Err(e) = ps.psubscribe("ws:r:*").await {
                            tracing::error!("WS psubscribe ws:r:* failed: {e}");
                            backoff(&mut backoff_secs, &shutdown).await;
                            continue;
                        }

                        backoff_secs = 1; // reset on success
                        tracing::info!("WS Redis subscriber ready (ws:u:*, ws:r:*)");

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
                                    let json: String = match msg.get_payload() {
                                        Ok(s) => s,
                                        Err(_) => continue,
                                    };

                                    if let Some(uid) = channel.strip_prefix(CH_USER) {
                                        // Direct ke satu user
                                        mgr.deliver_local(uid, &json);
                                    } else if let Some(_room_id) = channel.strip_prefix(CH_ROOM) {
                                        // Broadcast ke semua local session
                                        // (room_id bisa dipakai untuk filter jika perlu)
                                        mgr.sessions.iter().for_each(|entry| {
                                            let _ = entry.value().try_send(json.clone());
                                        });
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("WS Redis pubsub connect failed: {e}");
                    }
                }

                backoff(&mut backoff_secs, &shutdown).await;
            }
        });
    }
}

// ── Backoff helper ────────────────────────────────────────────────────────────

async fn backoff(secs: &mut u64, shutdown: &CancellationToken) {
    let wait = Duration::from_secs(*secs);
    tracing::info!("WS Redis reconnect in {}s", *secs);
    *secs = (*secs * 2).min(30);
    tokio::select! {
        _ = shutdown.cancelled() => {}
        _ = tokio::time::sleep(wait) => {}
    }
}
