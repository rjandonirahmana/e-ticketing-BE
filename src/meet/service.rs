//! meet/service.rs — Registry ruang meet + orkestrasi waiting room & relay.
//!
//! Murni in-memory & signaling — tidak ada thread SFU / UDP seperti `live`.
//! Aman di-`clone` lewat `Arc` dan dipakai dari banyak task WS sekaligus
//! (DashMap mengurus konkurensi).

use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::mpsc;

use super::room::{MeetRoom, MeetRoomInfo, Peer, PeerInfo};

/// Seberapa sering menyapu room yatim (dibuat lewat `POST /api/meet/rooms` tapi
/// host tak pernah membuka WS, atau edge disconnect yang lolos cleanup).
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
/// Room dengan 0 peserta yang lebih tua dari ini dianggap yatim dan dibuang.
/// Harus > jendela create→connect agar room yang baru dibuat tidak ikut tersapu.
const ORPHAN_MAX_AGE_SECS: i64 = 120;
/// Batas keras jumlah room serentak — cegah OOM dari abuse (spam buat room).
/// Aman untuk box kecil; naikkan bila perlu.
const MAX_ROOMS: usize = 500;
/// Batas keras peserta per room. Mesh P2P ideal ≤ ~6; beri sedikit buffer.
/// Di atas ini, koneksi mesh (N×(N-1)) membebani browser, bukan server.
const MAX_PEERS_PER_ROOM: usize = 12;

pub struct MeetService {
    rooms: Arc<DashMap<String, Arc<MeetRoom>>>,
}

impl MeetService {
    pub fn new() -> Arc<Self> {
        let rooms: Arc<DashMap<String, Arc<MeetRoom>>> = Arc::new(DashMap::new());
        Self::spawn_orphan_sweeper(rooms.clone());
        Arc::new(Self { rooms })
    }

    /// Task periodik pembuang room yatim. Room yang masih aktif selalu punya ≥1
    /// peserta (host) sehingga tak akan tersapu; `end_room` tetap menangani
    /// pembubaran normal saat host keluar. Ini hanya jaring pengaman agar room
    /// yang dibuat tapi tak pernah tersambung tidak menumpuk selamanya.
    fn spawn_orphan_sweeper(rooms: Arc<DashMap<String, Arc<MeetRoom>>>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(SWEEP_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                let now = chrono::Utc::now();
                let before = rooms.len();
                rooms.retain(|_, room| {
                    let empty = room.peers.is_empty();
                    let age = (now - room.created_at).num_seconds();
                    !(empty && age > ORPHAN_MAX_AGE_SECS)
                });
                let removed = before.saturating_sub(rooms.len());
                if removed > 0 {
                    tracing::info!(removed, "meet: menyapu room yatim (0 peserta, basi)");
                }
            }
        });
    }

    /// Buat (atau buat-ulang idempoten) ruang meet milik satu host. Id
    /// deterministik `meet_{host_id}` — sama seperti `live`, sehingga merchant
    /// yang membuka ulang tab tidak menumpuk room basi.
    pub fn create_room(&self, host_id: &str, host_name: &str) -> Result<MeetRoomInfo, String> {
        let room_id = format!("meet_{host_id}");
        // Hard cap: tolak kalau sudah penuh DAN ini room baru (bukan re-create
        // room milik host yang sama, yang sifatnya idempoten/mengganti).
        if !self.rooms.contains_key(&room_id) && self.rooms.len() >= MAX_ROOMS {
            return Err("Kapasitas server meet penuh, coba lagi nanti".into());
        }
        // Bersihkan room lama (peserta lama akan putus sendiri saat WS-nya tutup;
        // di sini cukup ganti agar daftar peserta mulai bersih).
        let room = Arc::new(MeetRoom::new(
            room_id.clone(),
            host_id.to_string(),
            host_name.to_string(),
        ));
        let info = room.info();
        self.rooms.insert(room_id, room);
        Ok(info)
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
        // Hard cap peserta per room (host dikecualikan agar host selalu bisa masuk).
        if !is_host && room.peers.len() >= MAX_PEERS_PER_ROOM {
            return false;
        }
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
