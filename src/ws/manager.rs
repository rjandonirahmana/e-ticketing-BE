//! ws/manager.rs — WebSocket session manager.
//!
//! Setiap user yang connect didaftarkan di DashMap<user_id, Sender<String>>.
//! Fanout per-room: iterate member_ids dan send ke masing-masing.
//! Multi-instance: publish ke Redis channel `ws:room:{room_id}` agar
//! instance lain forward ke local sessions.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use ahash::RandomState;
use dashmap::DashMap;
use redis::AsyncCommands;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::ws::proto::WsEvent;

const CHAN_BUF: usize = 128;
const REDIS_CH_PREFIX: &str = "ws:room:";

pub type WsTx = mpsc::Sender<String>;

pub struct WsManager {
    sessions: DashMap<Arc<str>, WsTx, RandomState>,
    redis_client: Arc<redis::Client>,
    pub dropped: Arc<AtomicU64>,
    shutdown: CancellationToken,
}

impl WsManager {
    pub fn new(redis_client: redis::Client) -> Arc<Self> {
        let shutdown = CancellationToken::new();
        let mgr = Arc::new(Self {
            sessions: DashMap::with_hasher(RandomState::new()),
            redis_client: Arc::new(redis_client),
            dropped: Arc::new(AtomicU64::new(0)),
            shutdown: shutdown.clone(),
        });
        Self::spawn_redis_subscriber(mgr.clone());
        mgr
    }

    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }

    // ── Connect / Disconnect ──────────────────────────────────────────────────

    pub fn connect(&self, user_id: &str) -> mpsc::Receiver<String> {
        let (tx, rx) = mpsc::channel(CHAN_BUF);
        let key: Arc<str> = user_id.into();
        // Kick existing
        if let Some((_, old)) = self.sessions.remove(&key) {
            let _ = old.try_send(WsEvent::err("REPLACED", "Replaced by new connection").to_json());
        }
        self.sessions.insert(key, tx);
        tracing::debug!(user_id, "WS connected");
        rx
    }

    pub fn disconnect(&self, user_id: &str) {
        self.sessions.remove(user_id);
        tracing::debug!(user_id, "WS disconnected");
    }

    // ── Send ──────────────────────────────────────────────────────────────────

    pub async fn send_to(&self, user_id: &str, event: WsEvent) {
        let json = event.to_json();
        if !self.send_local(user_id, &json) {
            self.redis_pub(&format!("{REDIS_CH_PREFIX}user:{user_id}"), &json)
                .await;
        }
    }

    /// Broadcast ke semua user_id dalam list (biasanya semua member room)
    pub async fn broadcast_to(&self, user_ids: &[String], event: WsEvent) {
        let json = event.to_json();
        let mut remote: Vec<&str> = Vec::new();
        for uid in user_ids {
            if !self.send_local(uid, &json) {
                remote.push(uid);
            }
        }
        // Satu publish ke room channel untuk instance lain
        if !remote.is_empty() {
            // Kita tidak tahu room_id di sini — publish individual
            // Dalam produksi, extract room_id dari event dan pub ke room channel
            for uid in remote {
                self.redis_pub(&format!("{REDIS_CH_PREFIX}user:{uid}"), &json)
                    .await;
            }
        }
    }

    pub fn is_online(&self, user_id: &str) -> bool {
        self.sessions.contains_key(user_id)
    }

    pub fn online_count(&self) -> usize {
        self.sessions.len()
    }
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn send_local(&self, user_id: &str, json: &str) -> bool {
        if let Some(tx) = self.sessions.get(user_id) {
            match tx.try_send(json.to_string()) {
                Ok(_) => return true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    drop(tx);
                    self.sessions.remove(user_id);
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(user_id, "WS channel full, dropping");
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    drop(tx);
                    self.sessions.remove(user_id);
                }
            }
        }
        false
    }

    async fn redis_pub(&self, channel: &str, json: &str) {
        match self.redis_client.get_multiplexed_async_connection().await {
            Ok(mut conn) => {
                if let Err(e) = conn.publish::<_, _, ()>(channel, json).await {
                    tracing::warn!(channel, error=%e, "WS Redis publish failed");
                }
            }
            Err(e) => tracing::error!(error=%e, "WS Redis connect failed"),
        }
    }

    fn spawn_redis_subscriber(mgr: Arc<Self>) {
        let client = mgr.redis_client.clone();
        let shutdown = mgr.shutdown.clone();
        tokio::spawn(async move {
            loop {
                if shutdown.is_cancelled() {
                    break;
                }
                match client.get_async_pubsub().await {
                    Ok(mut ps) => {
                        if let Err(e) = ps.psubscribe("ws:room:user:*").await {
                            tracing::error!("WS psubscribe failed: {e}");
                            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                            continue;
                        }
                        tracing::info!("WS Redis subscriber ready");
                        let mut stream = ps.on_message();
                        loop {
                            tokio::select! {
                                _ = shutdown.cancelled() => return,
                                msg = futures::StreamExt::next(&mut stream) => {
                                    let Some(msg) = msg else { break; };
                                    let ch = msg.get_channel_name().to_string();
                                    let json: String = match msg.get_payload() {
                                        Ok(s) => s,
                                        Err(_) => continue,
                                    };
                                    // ch = "ws:room:user:{user_id}"
                                    if let Some(uid) = ch.strip_prefix("ws:room:user:") {
                                        mgr.send_local(uid, &json);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => tracing::error!("WS Redis pubsub connect: {e}"),
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }
}
