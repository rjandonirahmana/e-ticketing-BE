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
        tx: mpsc::Sender<String>,
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

// ── Uji ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests_meet {
    use super::*;

    /// Kanal berkapasitas 1: cukup untuk membedakan "terkirim" dari "hilang"
    /// tanpa perlu jaringan sungguhan.
    fn peserta() -> (mpsc::Sender<String>, mpsc::Receiver<String>) {
        mpsc::channel(1)
    }

    fn layanan() -> Arc<MeetService> {
        // `Default`, bukan `new()`: `new()` menyalakan penyapu berkala yang
        // butuh runtime tokio hidup dan tak ada hubungannya dengan apa pun yang
        // diuji di sini.
        Arc::new(MeetService::default())
    }

    fn daftar(
        svc: &MeetService,
        room: &str,
        id: &str,
        host: bool,
    ) -> mpsc::Receiver<String> {
        let (tx, rx) = peserta();
        assert!(
            svc.register_peer(room, id, id, None, host, tx),
            "pendaftaran {id} ditolak"
        );
        rx
    }

    // ── Daur hidup ruangan ────────────────────────────────────────────────

    /// Membuka ulang tab TIDAK boleh menumpuk ruangan basi — id-nya
    /// deterministik justru untuk itu.
    #[test]
    fn buat_ulang_tak_menumpuk_ruangan() {
        let svc = layanan();
        let a = svc.create_room("host1", "Host").unwrap();
        let b = svc.create_room("host1", "Host").unwrap();
        assert_eq!(a.room_id, b.room_id);
        assert_eq!(svc.list_rooms().len(), 1);
    }

    /// Dan daftar pesertanya harus mulai BERSIH: peserta dari sesi sebelumnya
    /// sudah tak punya socket, membiarkan mereka berarti ruangan baru lahir
    /// dengan hantu di dalamnya.
    #[test]
    fn buat_ulang_mengosongkan_peserta_lama() {
        let svc = layanan();
        let r = svc.create_room("host1", "Host").unwrap().room_id;
        let _rx = daftar(&svc, &r, "host1", true);
        let _rx2 = daftar(&svc, &r, "tamu", false);

        svc.create_room("host1", "Host").unwrap();
        assert_eq!(svc.get_room(&r).unwrap().peers.len(), 0);
    }

    #[test]
    fn daftar_ke_ruangan_tak_dikenal_ditolak() {
        let svc = layanan();
        let (tx, _rx) = peserta();
        assert!(!svc.register_peer("meet_hantu", "a", "A", None, false, tx));
    }

    #[test]
    fn bubar_menghapus_ruangan() {
        let svc = layanan();
        let r = svc.create_room("host1", "Host").unwrap().room_id;
        svc.end_room(&r);
        assert!(svc.get_room(&r).is_none());
        assert!(svc.list_rooms().is_empty());
    }

    // ── Ruang tunggu ──────────────────────────────────────────────────────

    #[test]
    fn host_langsung_masuk_tamu_menunggu() {
        let svc = layanan();
        let r = svc.create_room("host1", "Host").unwrap().room_id;
        let _h = daftar(&svc, &r, "host1", true);
        let _t = daftar(&svc, &r, "tamu", false);

        let room = svc.get_room(&r).unwrap();
        assert!(room.peers.get("host1").unwrap().admitted);
        assert!(!room.peers.get("tamu").unwrap().admitted);
        assert_eq!(room.pending_peers().len(), 1);
    }

    #[test]
    fn admit_mengembalikan_peserta_lain_yang_sudah_masuk() {
        let svc = layanan();
        let r = svc.create_room("host1", "Host").unwrap().room_id;
        let _h = daftar(&svc, &r, "host1", true);
        let _a = daftar(&svc, &r, "a", false);
        let _b = daftar(&svc, &r, "b", false);

        svc.admit(&r, "a").unwrap();
        let (info, lain) = svc.admit(&r, "b").unwrap();
        assert_eq!(info.id, "b");
        // Host dan "a" sudah masuk; "b" sendiri dikecualikan.
        let mut ids: Vec<_> = lain.iter().map(|p| p.id.clone()).collect();
        ids.sort();
        assert_eq!(ids, vec!["a", "host1"]);
    }

    /// Menekan "izinkan" dua kali tak boleh menghasilkan pemberitahuan kedua —
    /// di sisi peserta lain itu tampak seperti orang yang sama masuk dua kali.
    #[test]
    fn admit_dua_kali_tak_berlaku() {
        let svc = layanan();
        let r = svc.create_room("host1", "Host").unwrap().room_id;
        let _h = daftar(&svc, &r, "host1", true);
        let _a = daftar(&svc, &r, "a", false);
        assert!(svc.admit(&r, "a").is_some());
        assert!(svc.admit(&r, "a").is_none());
    }

    #[test]
    fn admit_peserta_tak_dikenal_tak_memanikkan() {
        let svc = layanan();
        let r = svc.create_room("host1", "Host").unwrap().room_id;
        assert!(svc.admit(&r, "tak_ada").is_none());
        assert!(svc.admit("meet_hantu", "a").is_none());
    }

    // ── Batas kapasitas ───────────────────────────────────────────────────

    /// Mesh N×(N−1) membebani peramban, bukan server — batasnya nyata.
    #[test]
    fn tamu_melebihi_batas_ditolak_host_tetap_diterima() {
        let svc = layanan();
        let r = svc.create_room("host1", "Host").unwrap().room_id;

        let mut simpan = Vec::new();
        for i in 0..MAX_PEERS_PER_ROOM {
            simpan.push(daftar(&svc, &r, &format!("t{i}"), false));
        }
        let (tx, _rx) = peserta();
        assert!(
            !svc.register_peer(&r, "kelebihan", "X", None, false, tx),
            "tamu ke-{} seharusnya ditolak",
            MAX_PEERS_PER_ROOM + 1
        );

        // Host dikecualikan: ia harus selalu bisa kembali ke ruangannya sendiri,
        // bahkan saat ruangannya penuh — kalau tidak, tak ada yang bisa
        // membubarkannya.
        let (tx, _rx) = peserta();
        assert!(svc.register_peer(&r, "host1", "Host", None, true, tx));
    }

    // ── Koneksi lemah ─────────────────────────────────────────────────────

    /// SKENARIO INTI: satu peserta dengan sambungan buruk.
    ///
    /// Kanalnya penuh karena perangkatnya tak sanggup menyusul. Yang TIDAK boleh
    /// terjadi: pengiriman menggantung, atau peserta lain ikut kehilangan pesan.
    /// Satu orang di dalam kereta bawah tanah tidak boleh membekukan rapat untuk
    /// semua orang lain.
    #[test]
    fn peserta_lambat_tak_menahan_peserta_lain() {
        let svc = layanan();
        let r = svc.create_room("host1", "Host").unwrap().room_id;
        let mut rx_host = daftar(&svc, &r, "host1", true);
        let mut rx_sehat = daftar(&svc, &r, "sehat", false);
        let _rx_lambat = daftar(&svc, &r, "lambat", false);
        svc.admit(&r, "sehat").unwrap();
        svc.admit(&r, "lambat").unwrap();

        let room = svc.get_room(&r).unwrap();

        // Penuhi kanal si lambat (kapasitas 1) dan JANGAN pernah dibaca.
        room.send_to("lambat", "pesan-1");

        // Siaran berikutnya tak boleh menggantung, dan yang sehat tetap dapat.
        room.broadcast_admitted("penting", None);

        assert_eq!(rx_sehat.try_recv().unwrap(), "penting");
        assert_eq!(rx_host.try_recv().unwrap(), "penting");
    }

    /// Peserta yang koneksinya PUTUS (penerimanya sudah lenyap) juga tak boleh
    /// menjatuhkan siapa pun. Ini keadaan yang lumrah: WebSocket menutup lebih
    /// dulu daripada pembersihan yang menyusul sesudahnya.
    #[test]
    fn peserta_terputus_tak_menjatuhkan_siaran() {
        let svc = layanan();
        let r = svc.create_room("host1", "Host").unwrap().room_id;
        let mut rx_host = daftar(&svc, &r, "host1", true);
        let rx_putus = daftar(&svc, &r, "putus", false);
        svc.admit(&r, "putus").unwrap();

        drop(rx_putus); // socketnya hilang

        let room = svc.get_room(&r).unwrap();
        room.send_to("putus", "halo");
        room.broadcast_admitted("tetap-jalan", None);

        assert_eq!(rx_host.try_recv().unwrap(), "tetap-jalan");
    }

    /// Yang masih menunggu izin tidak boleh ikut mendengar isi rapat.
    #[test]
    fn yang_menunggu_izin_tak_menerima_siaran() {
        let svc = layanan();
        let r = svc.create_room("host1", "Host").unwrap().room_id;
        let _h = daftar(&svc, &r, "host1", true);
        let mut rx_tunggu = daftar(&svc, &r, "tunggu", false);

        svc.get_room(&r).unwrap().broadcast_admitted("rahasia", None);
        assert!(rx_tunggu.try_recv().is_err());
    }

    #[test]
    fn kirim_ke_peserta_tak_dikenal_aman() {
        let svc = layanan();
        let r = svc.create_room("host1", "Host").unwrap().room_id;
        svc.get_room(&r).unwrap().send_to("hantu", "halo");
    }
}
