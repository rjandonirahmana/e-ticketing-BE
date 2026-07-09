//! meet/room.rs — State satu ruang "meet" (konferensi P2P mesh).
//!
//! Berbeda dari `live` (SFU 1-ke-banyak), meet adalah mesh dua arah: server
//! HANYA relay signaling + kontrol admit (waiting room). Tidak ada media yang
//! lewat server — browser saling konek langsung.

use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::mpsc;

/// Kapasitas buffer saluran keluar per peer. 64 cukup untuk burst SDP/ICE saat
/// negosiasi; di atas itu = peer benar-benar macet → sinyal di-drop, bukan OOM.
pub const MEET_CHAN_CAP: usize = 64;

/// Satu peserta yang terhubung ke ruang meet (host atau tamu).
pub struct Peer {
    pub id: String,
    pub name: String,
    pub photo: Option<String>,
    pub is_host: bool,
    /// `false` selama tamu masih di waiting room; `true` setelah host meng-admit.
    /// Host selalu `true`.
    pub admitted: bool,
    /// Saluran keluar ke task WebSocket peer ini. Handler WS lain mengirim JSON
    /// (string) ke sini; task pemilik socket meneruskannya ke browser.
    ///
    /// BOUNDED (`MEET_CHAN_CAP`): peer lambat (mobile throttled / tab background)
    /// tak boleh membuat antrean signaling (SDP/ICE) tumbuh tak terbatas → OOM.
    /// Penuh → sinyal di-drop (`try_send`); peer akan re-negosiasi/ICE-restart.
    pub tx: mpsc::Sender<String>,
}

/// Ringkasan peserta untuk dikirim ke klien (roster / daftar admit).
#[derive(Debug, Clone, Serialize)]
pub struct PeerInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub photo: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetRoomInfo {
    pub room_id: String,
    pub host_id: String,
    pub host_name: String,
    pub created_at: i64,
    pub participant_count: usize,
}

pub struct MeetRoom {
    pub room_id: String,
    pub host_id: String,
    pub host_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub peers: DashMap<String, Peer>,
}

impl MeetRoom {
    pub fn new(room_id: String, host_id: String, host_name: String) -> Self {
        Self {
            room_id,
            host_id,
            host_name,
            created_at: chrono::Utc::now(),
            peers: DashMap::new(),
        }
    }

    pub fn info(&self) -> MeetRoomInfo {
        MeetRoomInfo {
            room_id: self.room_id.clone(),
            host_id: self.host_id.clone(),
            host_name: self.host_name.clone(),
            created_at: self.created_at.timestamp_millis(),
            participant_count: self.peers.iter().filter(|p| p.admitted).count(),
        }
    }

    /// Peserta yang sudah di-admit (termasuk host) — kecuali `except`.
    pub fn admitted_peers(&self, except: Option<&str>) -> Vec<PeerInfo> {
        self.peers
            .iter()
            .filter(|p| p.admitted && Some(p.id.as_str()) != except)
            .map(|p| PeerInfo {
                id: p.id.clone(),
                name: p.name.clone(),
                photo: p.photo.clone(),
            })
            .collect()
    }

    /// Tamu yang masih menunggu izin host (waiting room).
    pub fn pending_peers(&self) -> Vec<PeerInfo> {
        self.peers
            .iter()
            .filter(|p| !p.admitted)
            .map(|p| PeerInfo {
                id: p.id.clone(),
                name: p.name.clone(),
                photo: p.photo.clone(),
            })
            .collect()
    }

    /// Kirim satu pesan JSON ke peer tertentu. Diamkan bila peer tak ada /
    /// socket-nya sudah tutup (`tx` error) — pembersihan ditangani saat WS putus.
    pub fn send_to(&self, peer_id: &str, msg: &str) {
        if let Some(p) = self.peers.get(peer_id) {
            let _ = p.tx.try_send(msg.to_string());
        }
    }

    /// Kirim ke semua peer yang sudah admitted, kecuali `except`.
    pub fn broadcast_admitted(&self, msg: &str, except: Option<&str>) {
        for p in self.peers.iter() {
            if p.admitted && Some(p.id.as_str()) != except {
                let _ = p.tx.try_send(msg.to_string());
            }
        }
    }

    /// `tx` koneksi host (untuk memberi tahu permintaan join). `None` bila host
    /// belum/sedang tidak terhubung.
    pub fn host_tx(&self) -> Option<mpsc::Sender<String>> {
        self.peers
            .iter()
            .find(|p| p.is_host)
            .map(|p| p.tx.clone())
    }
}
