use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;

use super::room::{LiveRoom, RoomInfo, ViewerInfo};
use super::sfu::{SfuCommand, SfuEngine, SfuEvent};

/// Batas keras jumlah room live serentak — jaring pengaman RAM/CPU. Id room
/// deterministik per merchant (`live_{id}`), jadi praktis sudah dibatasi jumlah
/// merchant; ini menutup skenario ekstrem.
const MAX_LIVE_ROOMS: usize = 200;
/// Batas keras penonton unik per room live — plafon RAM/CPU (tiap subscriber =
/// satu peer SFU). Bukan target, hanya pelindung; SFU single-thread realistis
/// jauh di bawah ini.
const MAX_VIEWERS_PER_ROOM: usize = 500;

pub struct LiveStreamService {
    rooms: Arc<DashMap<String, Arc<LiveRoom>>>,
    cmd_tx: mpsc::Sender<SfuCommand>,
    // Broadcast daftar room terbaru ke klien WS `/ws/lives` setiap ada perubahan
    // (room dibuat/berhenti, penonton masuk/keluar) — pengganti polling HTTP.
    changes_tx: broadcast::Sender<Vec<RoomInfo>>,
    // SFU berjalan di OS thread sendiri (loop blocking UDP), product loop di tokio task.
    _sfu_handle: std::thread::JoinHandle<()>,
    _product_handle: JoinHandle<()>,
}

fn snapshot(rooms: &DashMap<String, Arc<LiveRoom>>) -> Vec<RoomInfo> {
    rooms.iter().map(|r| r.info()).collect()
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
    let ip = detect_candidate_ip().unwrap_or(IpAddr::from([127, 0, 0, 1]));
    // Kandidat ICE yang diiklankan adalah satu-satunya alamat yang dipakai browser
    // penonton untuk menjangkau SFU. IP privat/loopback HANYA bisa dihubungi dari
    // mesin/LAN yang sama — penonton di seluler atau jaringan lain akan "gabisa
    // masuk" (ICE tak pernah connect). Di produksi WAJIB set `SFU_PUBLIC_IP` ke IP
    // publik server + buka UDP port-nya. Teriakkan keras supaya misconfig kelihatan.
    if ip.is_loopback() || is_private(ip) {
        tracing::warn!(
            advertised_ip = %ip,
            "SFU_PUBLIC_IP tak diset → mengiklankan IP privat/loopback sebagai ICE \
             candidate. Penonton di luar LAN/mesin ini TIDAK akan bisa join. Set \
             SFU_PUBLIC_IP=<ip_publik_server> dan buka UDP port SFU untuk produksi."
        );
    }
    ip
}

/// IP privat (RFC1918 / link-local) yang tak bisa dijangkau dari internet.
fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        // ULA fc00::/7 atau link-local fe80::/10.
        IpAddr::V6(v6) => {
            let seg = v6.segments();
            (seg[0] & 0xfe00) == 0xfc00 || (seg[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Deteksi IP outbound tanpa mengirim paket: `connect` UDP hanya menetapkan
/// route, lalu `local_addr` memberi IP interface yang akan dipakai.
///
/// `target` menentukan rute mana yang ditanyakan — dan itu penting, lihat
/// `detect_candidate_ip`.
fn detect_ip_for_route(target: &str) -> Option<IpAddr> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect(target).ok()?;
    let ip = sock.local_addr().ok()?.ip();
    if ip.is_unspecified() { None } else { Some(ip) }
}

/// Pilih alamat yang diiklankan sebagai ICE host candidate.
///
/// ── KENAPA TIDAK CUKUP BERTANYA RUTE KE 8.8.8.8 ─────────────────────────────
///
/// Versi sebelumnya hanya melakukan itu, dan hasilnya benar di mesin biasa
/// tetapi SALAH di mesin ber-VPN: rute ke internet keluar lewat terowongan,
/// sehingga yang terdeteksi adalah alamat WireGuard/Tailscale (mis.
/// `10.13.13.2`) — antarmuka yang TIDAK dipakai untuk melayani browser di
/// mesin atau LAN yang sama.
///
/// Kegagalannya sangat menyesatkan karena ICE tetap BERHASIL. Socket SFU
/// terikat `0.0.0.0`, jadi paket ke alamat mana pun tetap sampai, dan STUN
/// hanya butuh itu. Yang gagal adalah DTLS: balasan SFU keluar dengan alamat
/// sumber yang dipilih kernel menurut rute ke browser — bukan alamat VPN yang
/// tadi diiklankan — dan browser membuang paket yang tak cocok dengan pasangan
/// kandidat yang sudah disepakati. Hasil akhirnya `Connected`, lalu diam total,
/// lalu putus saat consent freshness habis. Layar hitam tanpa satu pun galat.
///
/// ── CARA MEMILIHNYA ─────────────────────────────────────────────────────────
///
/// Ditanyakan DUA rute lalu dibandingkan:
///   * rute ke internet publik  → antarmuka default (bisa terowongan VPN)
///   * rute ke alamat LAN privat → antarmuka jaringan lokal
///
/// Bila keduanya berbeda, yang dipakai adalah rute LAN: ia jauh lebih mungkin
/// menjadi antarmuka yang sama dengan yang dipakai kernel saat membalas
/// browser, dan kesimetrisan itulah satu-satunya syarat DTLS.
///
/// Heuristik ini sengaja tidak berpretensi selalu benar — tak ada jawaban yang
/// selalu benar tanpa tahu di mana penontonnya berada. Karena itu ia hanya
/// berlaku ketika `SFU_PUBLIC_IP` tidak diisi, dan `SFU_PUBLIC_IP` tetap satu-
/// satunya cara yang pasti.
fn detect_candidate_ip() -> Option<IpAddr> {
    let publik = detect_ip_for_route("8.8.8.8:80");
    // 10.255.255.255 tak perlu ada — `connect` UDP tidak mengirim apa pun, ia
    // hanya meminta kernel memilih rute.
    let lan = detect_ip_for_route("10.255.255.255:80")
        .or_else(|| detect_ip_for_route("192.168.255.255:80"));

    match (publik, lan) {
        (Some(p), Some(l)) if p != l => {
            tracing::warn!(
                rute_internet = %p,
                rute_lan = %l,
                "SFU: rute internet dan rute LAN memakai antarmuka BERBEDA (khas \
                 mesin ber-VPN). Mengiklankan alamat rute LAN, karena itu yang \
                 dipakai kernel saat membalas klien di mesin/jaringan yang sama. \
                 Setel SFU_PUBLIC_IP bila tebakan ini keliru."
            );
            Some(l)
        }
        (Some(p), _) => Some(p),
        (None, Some(l)) => Some(l),
        (None, None) => None,
    }
}

impl LiveStreamService {
    pub fn new(sfu_bind_addr: SocketAddr) -> Arc<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<SfuCommand>(256);
        let (product_tx, product_rx) = mpsc::channel::<SfuEvent>(256);

        let candidate_ip = resolve_candidate_ip(sfu_bind_addr.ip());
        let candidate_addr = SocketAddr::new(candidate_ip, sfu_bind_addr.port());
        tracing::info!(%candidate_addr, "SFU advertising ICE host candidate");

        let sfu_handle = std::thread::spawn(move || {
            SfuEngine::run(sfu_bind_addr, candidate_addr, cmd_rx, product_tx);
        });

        let (changes_tx, _) = broadcast::channel::<Vec<RoomInfo>>(16);

        let rooms: Arc<DashMap<String, Arc<LiveRoom>>> = Arc::new(DashMap::new());
        let rooms_clone = rooms.clone();
        let changes_evt = changes_tx.clone();
        let product_handle = tokio::spawn(async move {
            // Konsumen tunggal — terima langsung dari receiver, tanpa Arc<Mutex>.
            let mut product_rx = product_rx;
            while let Some(product) = product_rx.recv().await {
                match product {
                    SfuEvent::StreamStopped { room_id } => {
                        tracing::info!(room_id, "Live stream stopped");
                        rooms_clone.remove(&room_id);
                        let _ = changes_evt.send(snapshot(&rooms_clone));
                    }
                    SfuEvent::SubscriberLeft { room_id, subscriber_id } => {
                        // Koneksi penonton putus tanpa leave eksplisit.
                        if let Some(room) = rooms_clone.get(&room_id) {
                            room.remove_subscriber(&subscriber_id);
                        }
                        let _ = changes_evt.send(snapshot(&rooms_clone));
                    }
                    SfuEvent::ViewerCount { room_id, count } => {
                        tracing::debug!(room_id, count, "Viewer count update");
                    }
                    SfuEvent::IceCandidate { room_id, peer_id, .. } => {
                        tracing::debug!(room_id, peer_id, "ICE candidate generated");
                    }
                }
            }
        });

        Arc::new(Self {
            rooms,
            cmd_tx,
            changes_tx,
            _sfu_handle: sfu_handle,
            _product_handle: product_handle,
        })
    }

    /// Klien WS `/ws/lives` berlangganan perubahan daftar room di sini.
    pub fn subscribe_changes(&self) -> broadcast::Receiver<Vec<RoomInfo>> {
        self.changes_tx.subscribe()
    }

    /// Kirim snapshot daftar room terbaru ke semua klien WS.
    fn notify_change(&self) {
        let _ = self.changes_tx.send(snapshot(&self.rooms));
    }

    pub async fn create_room(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        event_slug: Option<&str>,
    ) -> Result<RoomInfo, String> {
        let room_id = format!("live_{}", merchant_id);
        // Hard cap: tolak room baru bila penuh (re-create room sendiri tetap boleh).
        if !self.rooms.contains_key(&room_id) && self.rooms.len() >= MAX_LIVE_ROOMS {
            return Err("Kapasitas live server penuh, coba lagi nanti".into());
        }
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
        self.notify_change();

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
        // Hard cap penonton: tolak SEBELUM membuat peer SFU bila room penuh.
        if let Some(room) = self.rooms.get(room_id) {
            if room.viewer_count() >= MAX_VIEWERS_PER_ROOM {
                return Err("Room live penuh, coba lagi nanti".into());
            }
        }
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
        self.notify_change();

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
        self.notify_change();
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
        self.notify_change();
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
// ─── Uji siklus hidup siaran ──────────────────────────────────────────────────
//
// Yang diuji di sini BUKAN WebRTC-nya. Media, ICE, dan DTLS butuh peramban
// sungguhan; yang bisa — dan justru paling perlu — diuji adalah RANTAI SINYAL
// yang membuat UI penonton terasa benar tanpa satu pun polling ke server:
//
//     stop_room()  →  room hilang dari `rooms`
//                  →  `changes_tx` menyiarkan snapshot TANPA room itu
//                  →  `live_subscribe_ws_loop` melihat room-nya lenyap
//                  →  kirim "stream_ended" ke penonton, lalu tutup koneksi
//
// Langkah terakhir ada di `live/api.rs` dan memerlukan WebSocket sungguhan,
// tetapi SYARAT yang dipakainya — "room_id saya tidak ada lagi di snapshot" —
// sepenuhnya ditentukan di berkas ini. Kalau `stop_room` lupa menyiarkan, atau
// menyiarkan snapshot yang masih memuat room-nya, penonton akan menatap video
// beku selamanya dan tak ada satu pun galat yang muncul. Uji ini menjaga
// justru bagian yang gagalnya paling sunyi itu.
#[cfg(test)]
mod tests_siklus_siaran {
    use super::*;
    use std::time::Duration;

    /// SFU dijalankan sungguhan, tetapi diikat ke port ephemeral loopback.
    /// `CreateRoom`/`StopRoom` tak menyentuh jaringan sama sekali — keduanya
    /// hanya mengubah peta di dalam engine lalu membalas lewat oneshot — jadi
    /// jalur yang diuji di sini adalah jalur produksi yang sebenarnya, bukan
    /// tiruan.
    fn layanan() -> Arc<LiveStreamService> {
        LiveStreamService::new("127.0.0.1:0".parse().unwrap())
    }

    fn penonton(id: &str) -> ViewerInfo {
        ViewerInfo { id: id.into(), name: format!("Penonton {id}"), photo_url: None }
    }

    /// Menunggu satu siaran perubahan, dengan batas waktu supaya kegagalan
    /// muncul sebagai assert yang jelas alih-alih uji yang menggantung.
    async fn tunggu_snapshot(
        rx: &mut broadcast::Receiver<Vec<RoomInfo>>,
    ) -> Vec<RoomInfo> {
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("tak ada siaran perubahan dalam 2 detik")
            .expect("channel broadcast tertutup")
    }

    /// SKENARIO UTAMA: merchant siaran, tiga orang menonton, siaran berakhir.
    ///
    /// Setelah berakhir, snapshot yang disiarkan TIDAK BOLEH lagi memuat room
    /// itu — itulah satu-satunya hal yang membuat ketiga penonton dikeluarkan
    /// dari halaman siaran secara otomatis.
    #[tokio::test]
    async fn siaran_berakhir_mengeluarkan_semua_penonton() {
        let svc = layanan();
        let mut rx = svc.subscribe_changes();

        let info = svc
            .create_room("merchant-1", "Toko Satu", None)
            .await
            .expect("room gagal dibuat");
        let room_id = info.room_id.clone();

        // Siaran perubahan saat room dibuat.
        let snap = tunggu_snapshot(&mut rx).await;
        assert!(snap.iter().any(|r| r.room_id == room_id), "room baru harus muncul di snapshot");

        // Tiga penonton bergabung. `subscribe_sdp` butuh SDP sungguhan dari
        // peramban, jadi penonton didaftarkan langsung ke room — lapisan yang
        // sama persis yang dipanggil `subscribe_sdp` setelah SFU membalas.
        {
            let room = svc.rooms.get(&room_id).expect("room harus ada");
            for i in 1..=3 {
                room.add_subscriber(&format!("conn-{i}"), penonton(&format!("u{i}")));
            }
        }
        assert_eq!(
            svc.get_room(&room_id).map(|r| r.viewer_count),
            Some(3),
            "ketiga penonton harus terhitung"
        );

        // Siaran berakhir.
        svc.stop_room(&room_id).await.expect("stop_room gagal");

        let snap = tunggu_snapshot(&mut rx).await;
        assert!(
            !snap.iter().any(|r| r.room_id == room_id),
            "room yang sudah berhenti TIDAK boleh ada di snapshot — inilah sinyal \
             yang dipakai live_subscribe_ws_loop untuk mengirim stream_ended"
        );
        assert!(svc.get_room(&room_id).is_none(), "room harus lenyap dari daftar");
        assert!(!svc.is_live(&room_id), "is_live harus false setelah berhenti");
    }

    /// Penonton yang keluar sendiri (tutup tab) memicu siaran perubahan juga —
    /// itu yang membuat angka penonton di layar merchant turun tanpa polling.
    #[tokio::test]
    async fn penonton_keluar_menyiarkan_hitungan_baru() {
        let svc = layanan();
        let info = svc.create_room("merchant-2", "Toko Dua", None).await.unwrap();
        let room_id = info.room_id.clone();

        {
            let room = svc.rooms.get(&room_id).unwrap();
            room.add_subscriber("conn-1", penonton("u1"));
            room.add_subscriber("conn-2", penonton("u2"));
        }

        // Berlangganan SESUDAH join supaya siaran yang tertangkap benar-benar
        // milik aksi keluar, bukan sisa siaran sebelumnya.
        let mut rx = svc.subscribe_changes();
        svc.remove_subscriber(&room_id, "conn-1").await.unwrap();

        let snap = tunggu_snapshot(&mut rx).await;
        let room = snap.iter().find(|r| r.room_id == room_id).expect("room masih siaran");
        assert_eq!(room.viewer_count, 1, "tinggal satu penonton");
    }

    /// "GO LIVE" dua kali berturut-turut tidak boleh gagal.
    ///
    /// Room id bersifat deterministik per merchant (`live_{id}`), jadi merchant
    /// yang menutup tab tanpa menekan STOP akan meninggalkan room yatim. Kalau
    /// pembuatan ulang menolak karena "sudah ada", merchant itu terkunci dari
    /// siarannya sendiri sampai proses server dimulai ulang.
    #[tokio::test]
    async fn go_live_ulang_menggantikan_room_yatim() {
        let svc = layanan();
        let a = svc.create_room("merchant-3", "Toko Tiga", None).await.unwrap();
        let b = svc
            .create_room("merchant-3", "Toko Tiga", None)
            .await
            .expect("GO LIVE kedua harus berhasil, bukan ditolak");
        assert_eq!(a.room_id, b.room_id, "room id deterministik per merchant");
        assert_eq!(svc.list_rooms().len(), 1, "tak boleh jadi dua room");
        assert_eq!(b.viewer_count, 0, "room baru mulai dari nol penonton");
    }

    /// Menghentikan room yang tak ada tidak boleh panik atau menyiarkan room
    /// hantu — dipanggil dari `live_publish_ws_loop` pada SETIAP penutupan WS
    /// publisher, termasuk yang belum sempat bernegosiasi.
    #[tokio::test]
    async fn stop_room_tak_dikenal_aman() {
        let svc = layanan();
        let _ = svc.stop_room("live_entah").await;
        assert!(svc.list_rooms().is_empty());
    }
}
