//! meet/service.rs — Registry ruang meet + orkestrasi waiting room & relay.
//!
//! Murni in-memory & signaling — tidak ada thread SFU / UDP seperti `live`.
//! Aman di-`clone` lewat `Arc` dan dipakai dari banyak task WS sekaligus
//! (DashMap mengurus konkurensi).

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::mpsc;

use super::room::{MeetRoom, MeetRoomInfo, Peer, PeerInfo};

pub struct MeetService {
    rooms: Arc<DashMap<String, Arc<MeetRoom>>>,
}

impl MeetService {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            rooms: Arc::new(DashMap::new()),
        })
    }

    /// Buat (atau buat-ulang idempoten) ruang meet milik satu host. Id
    /// deterministik `meet_{host_id}` — sama seperti `live`, sehingga merchant
    /// yang membuka ulang tab tidak menumpuk room basi.
    pub fn create_room(&self, host_id: &str, host_name: &str) -> MeetRoomInfo {
        let room_id = format!("meet_{host_id}");
        // Bersihkan room lama (peserta lama akan putus sendiri saat WS-nya tutup;
        // di sini cukup ganti agar daftar peserta mulai bersih).
        let room = Arc::new(MeetRoom::new(
            room_id.clone(),
            host_id.to_string(),
            host_name.to_string(),
        ));
        let info = room.info();
        self.rooms.insert(room_id, room);
        info
    }

    pub fn get_room(&self, room_id: &str) -> Option<Arc<MeetRoom>> {
        self.rooms.get(room_id).map(|r| r.clone())
    }

    pub fn info(&self, room_id: &str) -> Option<MeetRoomInfo> {
        self.rooms.get(room_id).map(|r| r.info())
    }

    pub fn list_rooms(&self) -> Vec<MeetRoomInfo> {
        self.rooms.iter().map(|r| r.info()).collect()
    }

    /// Daftarkan koneksi baru ke sebuah room. Host langsung `admitted`; tamu
    /// masuk waiting room (`admitted=false`). Mengembalikan `false` bila room
    /// tidak ada.
    pub fn register_peer(
        &self,
        room_id: &str,
        peer_id: &str,
        name: &str,
        photo: Option<String>,
        is_host: bool,
        tx: mpsc::UnboundedSender<String>,
    ) -> bool {
        let Some(room) = self.rooms.get(room_id) else {
            return false;
        };
        room.peers.insert(
            peer_id.to_string(),
            Peer {
                id: peer_id.to_string(),
                name: name.to_string(),
                photo,
                is_host,
                admitted: is_host,
                tx,
            },
        );
        true
    }

    /// Host meng-admit satu tamu. Mengembalikan info peer yang di-admit + daftar
    /// peer admitted lain (agar tamu bisa initiate offer mesh ke mereka).
    /// `None` bila peer tidak ada / sudah admitted.
    pub fn admit(&self, room_id: &str, peer_id: &str) -> Option<(PeerInfo, Vec<PeerInfo>)> {
        let room = self.rooms.get(room_id)?;
        {
            let mut p = room.peers.get_mut(peer_id)?;
            if p.admitted {
                return None;
            }
            p.admitted = true;
        }
        let info = {
            let p = room.peers.get(peer_id)?;
            PeerInfo {
                id: p.id.clone(),
                name: p.name.clone(),
                photo: p.photo.clone(),
            }
        };
        let others = room.admitted_peers(Some(peer_id));
        Some((info, others))
    }

    /// Hapus room (dipanggil saat host keluar / meet dibubarkan).
    pub fn end_room(&self, room_id: &str) {
        self.rooms.remove(room_id);
    }
}

impl Default for MeetService {
    fn default() -> Self {
        Self {
            rooms: Arc::new(DashMap::new()),
        }
    }
}
