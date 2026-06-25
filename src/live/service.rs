use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;

use super::room::{LiveRoom, RoomInfo, ViewerInfo};
use super::sfu::{SfuCommand, SfuEngine, SfuEvent};

pub struct LiveStreamService {
    rooms: Arc<DashMap<String, Arc<LiveRoom>>>,
    cmd_tx: mpsc::Sender<SfuCommand>,
    // SFU berjalan di OS thread sendiri (loop blocking UDP), event loop di tokio task.
    _sfu_handle: std::thread::JoinHandle<()>,
    _event_handle: JoinHandle<()>,
}

/// IP yang diiklankan sebagai host ICE candidate. Socket boleh bind ke
/// `0.0.0.0` (semua interface), tapi str0m menolak `0.0.0.0` sebagai kandidat
/// ("invalid ip 0.0.0.0") — kandidat harus IP konkret yang bisa dihubungi klien.
///
/// Urutan: `SFU_PUBLIC_IP` (untuk produksi / di belakang NAT) → IP LAN hasil
/// deteksi (agar perangkat lain di WiFi sama bisa konek) → `127.0.0.1` (dev).
fn resolve_candidate_ip(bind_ip: IpAddr) -> IpAddr {
    if !bind_ip.is_unspecified() {
        return bind_ip;
    }
    if let Ok(s) = std::env::var("SFU_PUBLIC_IP") {
        if let Ok(ip) = s.trim().parse::<IpAddr>() {
            return ip;
        }
    }
    detect_local_ip().unwrap_or(IpAddr::from([127, 0, 0, 1]))
}

/// Deteksi IP outbound utama tanpa mengirim paket: `connect` UDP hanya menetapkan
/// route, lalu `local_addr` memberi IP interface yang dipakai.
fn detect_local_ip() -> Option<IpAddr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("8.8.8.8:80").ok()?;
    let ip = sock.local_addr().ok()?.ip();
    if ip.is_unspecified() { None } else { Some(ip) }
}

impl LiveStreamService {
    pub fn new(sfu_bind_addr: SocketAddr) -> Arc<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<SfuCommand>(256);
        let (event_tx, event_rx) = mpsc::channel::<SfuEvent>(256);

        let candidate_ip = resolve_candidate_ip(sfu_bind_addr.ip());
        let candidate_addr = SocketAddr::new(candidate_ip, sfu_bind_addr.port());
        tracing::info!(%candidate_addr, "SFU advertising ICE host candidate");

        let sfu_handle = std::thread::spawn(move || {
            SfuEngine::run(sfu_bind_addr, candidate_addr, cmd_rx, event_tx);
        });

        let rooms: Arc<DashMap<String, Arc<LiveRoom>>> = Arc::new(DashMap::new());
        let rooms_clone = rooms.clone();
        let event_handle = tokio::spawn(async move {
            let rx = Arc::new(Mutex::new(event_rx));
            loop {
                let mut guard = rx.lock().await;
                if let Some(event) = guard.recv().await {
                    drop(guard);
                    match event {
                        SfuEvent::StreamStopped { room_id } => {
                            tracing::info!(room_id, "Live stream stopped");
                            rooms_clone.remove(&room_id);
                        }
                        SfuEvent::ViewerCount { room_id, count } => {
                            tracing::debug!(room_id, count, "Viewer count update");
                        }
                        SfuEvent::IceCandidate { room_id, peer_id, .. } => {
                            tracing::debug!(room_id, peer_id, "ICE candidate generated");
                        }
                    }
                } else {
                    break;
                }
            }
        });

        Arc::new(Self {
            rooms,
            cmd_tx,
            _sfu_handle: sfu_handle,
            _event_handle: event_handle,
        })
    }

    pub async fn create_room(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        event_slug: Option<&str>,
    ) -> Result<RoomInfo, String> {
        let room_id = format!("live_{}", merchant_id);
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(SfuCommand::CreateRoom {
                room_id: room_id.clone(),
                merchant_id: merchant_id.to_string(),
                merchant_name: merchant_name.to_string(),
                event_slug: event_slug.map(String::from),
                respond_to: tx,
            })
            .await
            .map_err(|e| e.to_string())?;

        let info = rx.await.map_err(|e| e.to_string())??;

        let room = Arc::new(LiveRoom::new(
            info.room_id.clone(),
            merchant_id.to_string(),
            merchant_name.to_string(),
            event_slug.map(String::from),
            self.cmd_tx.clone(),
        ));
        self.rooms.insert(room_id, room);

        Ok(info)
    }

    pub async fn publish_sdp(&self, room_id: &str, sdp_offer: &str) -> Result<String, String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(SfuCommand::PublishSdp {
                room_id: room_id.to_string(),
                sdp_offer: sdp_offer.to_string(),
                respond_to: tx,
            })
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())?
    }

    pub async fn publish_ice(
        &self,
        room_id: &str,
        candidate: &str,
        sdp_mid: &str,
        sdp_mline_index: u32,
    ) -> Result<(), String> {
        self.cmd_tx
            .send(SfuCommand::PublishIce {
                room_id: room_id.to_string(),
                candidate: candidate.to_string(),
                sdp_mid: sdp_mid.to_string(),
                sdp_mline_index,
            })
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn subscribe_sdp(
        &self,
        room_id: &str,
        subscriber_id: &str,
        sdp_offer: &str,
        viewer: ViewerInfo,
    ) -> Result<String, String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(SfuCommand::SubscribeSdp {
                room_id: room_id.to_string(),
                subscriber_id: subscriber_id.to_string(),
                sdp_offer: sdp_offer.to_string(),
                respond_to: tx,
            })
            .await
            .map_err(|e| e.to_string())?;
        let result = rx.await.map_err(|e| e.to_string())??;

        if let Some(room) = self.rooms.get(room_id) {
            room.add_subscriber(subscriber_id, viewer);
        }

        Ok(result)
    }

    pub async fn subscribe_ice(
        &self,
        room_id: &str,
        subscriber_id: &str,
        candidate: &str,
        sdp_mid: &str,
        sdp_mline_index: u32,
    ) -> Result<(), String> {
        self.cmd_tx
            .send(SfuCommand::SubscribeIce {
                room_id: room_id.to_string(),
                subscriber_id: subscriber_id.to_string(),
                candidate: candidate.to_string(),
                sdp_mid: sdp_mid.to_string(),
                sdp_mline_index,
            })
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn remove_subscriber(&self, room_id: &str, subscriber_id: &str) -> Result<(), String> {
        if let Some(room) = self.rooms.get(room_id) {
            room.remove_subscriber(subscriber_id);
        }
        self.cmd_tx
            .send(SfuCommand::RemoveSubscriber {
                room_id: room_id.to_string(),
                subscriber_id: subscriber_id.to_string(),
            })
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn stop_room(&self, room_id: &str) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        self.cmd_tx
            .send(SfuCommand::StopRoom {
                room_id: room_id.to_string(),
                respond_to: tx,
            })
            .await
            .map_err(|e| e.to_string())?;
        rx.await.map_err(|e| e.to_string())??;
        self.rooms.remove(room_id);
        Ok(())
    }

    pub fn list_rooms(&self) -> Vec<RoomInfo> {
        self.rooms.iter().map(|r| r.info()).collect()
    }

    pub fn get_room(&self, room_id: &str) -> Option<RoomInfo> {
        self.rooms.get(room_id).map(|r| r.info())
    }

    pub fn is_live(&self, room_id: &str) -> bool {
        self.rooms.contains_key(room_id)
    }
}