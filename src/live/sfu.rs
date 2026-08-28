use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use str0m::{
    change::SdpOffer,
    media::{KeyframeRequestKind, MediaData, MediaKind, Mid},
    net::{DatagramRecv, Protocol, Receive},
    Candidate, Event, IceConnectionState, Input, Output, Rtc,
};
use tokio::sync::mpsc;

use super::room::RoomInfo;

const BUF_SIZE: usize = 65535;
/// Lama maksimum thread SFU memblokir di `recv_from` saat tak ada paket masuk.
/// Saat idle thread benar-benar tidur di syscall ini (bukan spin yang membakar
/// satu core CPU), tapi tetap bangun cukup sering untuk memproses command,
/// menjalankan timer str0m, dan burst keyframe untuk penonton baru.
const MAX_POLL_WAIT: Duration = Duration::from_millis(10);
/// Timeout `recv_from` saat SFU benar-benar idle (tak ada room & peer): thread
/// cukup bangun 4×/dtk untuk mengecek command, bukan 100×/dtk. Menghemat CPU
/// 24/7 karena mayoritas waktu server TIDAK sedang ada siaran. Trade-off:
/// command pertama (mis. CreateRoom saat merchant klik GO LIVE) menunggu
/// maksimal 250 ms sekali — tak terasa untuk aksi manual.
const IDLE_POLL_WAIT: Duration = Duration::from_millis(250);
/// Tenggang sebelum peer yang berstatus ICE `Disconnected` benar-benar dibuang.
/// `Disconnected` itu transien (bisa pulih ke `Connected`); membuang seketika
/// memutus koneksi yang sebenarnya masih hidup.
const DISCONNECT_GRACE: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub enum SfuCommand {
    CreateRoom {
        room_id: String,
        merchant_id: String,
        merchant_name: String,
        event_slug: Option<String>,
        respond_to: tokio::sync::oneshot::Sender<Result<RoomInfo, String>>,
    },
    PublishSdp {
        room_id: String,
        sdp_offer: String,
        respond_to: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    PublishIce {
        room_id: String,
        candidate: String,
        sdp_mid: String,
        sdp_mline_index: u32,
    },
    SubscribeSdp {
        room_id: String,
        subscriber_id: String,
        sdp_offer: String,
        respond_to: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    SubscribeIce {
        room_id: String,
        subscriber_id: String,
        candidate: String,
        sdp_mid: String,
        sdp_mline_index: u32,
    },
    RemoveSubscriber {
        room_id: String,
        subscriber_id: String,
    },
    StopRoom {
        room_id: String,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    ListRooms {
        respond_to: tokio::sync::oneshot::Sender<Vec<RoomInfo>>,
    },
    GetRoom {
        room_id: String,
        respond_to: tokio::sync::oneshot::Sender<Option<RoomInfo>>,
    },
    Shutdown,
}

#[derive(Debug)]
pub enum SfuEvent {
    IceCandidate {
        room_id: String,
        peer_id: String,
        candidate: String,
        sdp_mid: String,
        sdp_mline_index: u32,
    },
    ViewerCount {
        room_id: String,
        count: usize,
    },
    StreamStopped {
        room_id: String,
    },
    /// Koneksi penonton putus (tutup tab / ICE drop) tanpa panggil leave —
    /// service menghapusnya dari LiveRoom agar hitungan penonton akurat.
    SubscriberLeft {
        room_id: String,
        subscriber_id: String,
    },
}

struct PeerState {
    rtc: Rtc,
    role: PeerRole,
    // mid → kind hasil negosiasi (dari Event::MediaAdded). Dipakai untuk
    // meneruskan media berdasarkan KIND (audio/video), bukan mid mentah —
    // urutan m-line publisher & subscriber bisa berbeda.
    mids: Vec<(Mid, MediaKind)>,
    // Sejak kapan peer berstatus ICE `Disconnected` (status TRANSIEN yang bisa
    // pulih). `None` = sedang tersambung. Peer baru dibuang jika tetap
    // Disconnected melewati `DISCONNECT_GRACE`, bukan saat product pertama —
    // mencegah penonton "keluar-masuk" karena blip jaringan sesaat.
    disconnected_since: Option<Instant>,
    // Alamat UDP remote yang terakhir kali dipetakan ke peer ini (lihat
    // `addr_to_peer`). Dipakai saat membuang peer untuk mengevakuasi entri cache
    // demux-nya, supaya cache tidak menumpuk entri mati = memory leak.
    remote_addr: Option<SocketAddr>,
}

enum PeerRole {
    Publisher,
    Subscriber,
}

struct RoomState {
    room_id: String,
    merchant_id: String,
    merchant_name: String,
    event_slug: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    publisher: Option<String>,
    subscribers: HashMap<String, String>,
}

impl RoomState {
    fn info(&self) -> RoomInfo {
        RoomInfo {
            room_id: self.room_id.clone(),
            merchant_id: self.merchant_id.clone(),
            merchant_name: self.merchant_name.clone(),
            event_slug: self.event_slug.clone(),
            viewer_count: self.subscribers.len(),
            started_at: self.started_at.timestamp_millis(),
            // Identitas penonton dilacak di sisi service (LiveRoom), bukan SFU.
            viewers: Vec::new(),
        }
    }
}

pub struct SfuEngine {
    peers: HashMap<String, PeerState>,
    rooms: HashMap<String, RoomState>,
    // Rute demux O(1): alamat UDP remote → id peer pemiliknya. Diisi saat sebuah
    // paket pertama kali dikenali peer (slow path) dan dipakai ulang untuk paket
    // berikutnya, agar tidak menyapu seluruh daftar peer tiap datagram. Entri
    // dibuang saat peer hilang (lihat `remove_peer`) + self-healing bila basi.
    addr_to_peer: HashMap<SocketAddr, String>,
    socket: UdpSocket,
    next_peer_id: u64,
    // IP:port konkret yang diiklankan ke klien (host candidate + tujuan paket).
    // Berbeda dari alamat bind socket yang bisa `0.0.0.0`.
    candidate_addr: SocketAddr,
    // Hitung frame media dari publisher (untuk log diagnostik terbatas).
    frames_seen: u64,
    // Hitung tulisan media yang BERHASIL ke subscriber. Dibandingkan dengan
    // `frames_seen` ini membedakan "publisher mengalir ke SFU" vs "SFU benar-benar
    // melayani subscriber" — kalau forwarded tetap 0 saat frames naik, masalah
    // ada di forwarding (mid/codec/writer), bukan di ingest publisher.
    frames_forwarded: u64,
    // Diagnostik per-kind: pisahkan video vs audio agar "hitam total" bisa
    // dilokalisasi. `*_seen` = masuk dari publisher; `*_fwd` = berhasil ditulis
    // ke subscriber. v_seen=0 → publisher tak mengirim video / DTLS gagal ingest.
    // v_seen>0 tapi v_fwd=0 → forwarding video putus (codec/mid/writer).
    frames_video: u64,
    frames_audio: u64,
    fwd_video: u64,
    fwd_audio: u64,
    // Log sekali saat media PERTAMA dari publisher tiba (bukti DTLS/SRTP tuntas).
    ingest_logged: bool,
    // Saat penonton baru bergabung, minta keyframe ke publisher berkali-kali
    // sampai deadline ini (browser tidak selalu kirim PLI sendiri).
    keyframe_deadline: Option<Instant>,
    last_keyframe: Instant,
    /// Kapan terakhir kali keadaan SFU dilaporkan saat TIDAK ada media.
    ///
    /// Tanpa ini kegagalan yang paling sering terjadi tidak meninggalkan jejak
    /// apa pun — lihat catatan di `log_health_when_silent`.
    last_health_log: Instant,
}

impl SfuEngine {
    pub fn run(
        bind_addr: SocketAddr,
        candidate_addr: SocketAddr,
        cmd_rx: mpsc::Receiver<SfuCommand>,
        product_tx: mpsc::Sender<SfuEvent>,
    ) {
        let socket = UdpSocket::bind(bind_addr).expect("Failed to bind SFU UDP socket");
        // Socket BLOCKING dengan read-timeout: `recv_from` tidur hingga ada paket
        // atau `MAX_POLL_WAIT` lewat, alih-alih spin non-blocking yang membakar
        // satu core CPU terus-menerus walau tak ada siaran.
        socket
            .set_read_timeout(Some(MAX_POLL_WAIT))
            .expect("Failed to set SFU socket read timeout");
        let bound = socket.local_addr().expect("Failed to get local addr");
        tracing::info!(bind = %bound, candidate = %candidate_addr, "SFU UDP socket bound");

        let mut engine = Self {
            peers: HashMap::new(),
            rooms: HashMap::new(),
            addr_to_peer: HashMap::new(),
            socket,
            next_peer_id: 0,
            candidate_addr,
            frames_seen: 0,
            frames_forwarded: 0,
            frames_video: 0,
            frames_audio: 0,
            fwd_video: 0,
            fwd_audio: 0,
            ingest_logged: false,
            keyframe_deadline: None,
            last_keyframe: Instant::now(),
            last_health_log: Instant::now(),
        };

        let mut cmd_rx = cmd_rx;
        let mut buf = vec![0u8; BUF_SIZE];
        // Mode timeout socket saat ini (false = aktif 10 ms). Hanya panggil
        // set_read_timeout saat mode BERUBAH, bukan tiap iterasi (hemat syscall).
        let mut idle_mode = false;

        loop {
            engine.process_commands(&mut cmd_rx);

            engine.poll_all_peers(&product_tx);

            // Adaptif: tanpa room & peer, tidur lebih lama di recv_from.
            let want_idle = engine.peers.is_empty() && engine.rooms.is_empty();
            if want_idle != idle_mode {
                idle_mode = want_idle;
                let wait = if idle_mode { IDLE_POLL_WAIT } else { MAX_POLL_WAIT };
                if let Err(e) = engine.socket.set_read_timeout(Some(wait)) {
                    tracing::warn!(error = %e, "SFU set_read_timeout gagal");
                }
            }

            engine.read_socket(&mut buf);
        }
    }

    fn process_commands(&mut self, cmd_rx: &mut mpsc::Receiver<SfuCommand>) {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                SfuCommand::CreateRoom {
                    room_id,
                    merchant_id,
                    merchant_name,
                    event_slug,
                    respond_to,
                } => {
                    // Room id deterministik per merchant (`live_{merchant_id}`).
                    // Jika merchant menutup tab tanpa stop, atau publisher-nya
                    // terputus (peer dibuang tapi room tetap ada), room lama
                    // bisa tertinggal. Buat ulang idempoten: bersihkan sisa room
                    // lama beserta peer-nya supaya "GO LIVE" lagi selalu berhasil.
                    if let Some(stale) = self.rooms.remove(&room_id) {
                        if let Some(pub_id) = stale.publisher {
                            self.remove_peer(&pub_id);
                        }
                        for sub_id in stale.subscribers.keys() {
                            self.remove_peer(sub_id);
                        }
                        tracing::info!(room_id, "Replacing stale live room");
                    }
                    let room = RoomState {
                        room_id: room_id.clone(),
                        merchant_id,
                        merchant_name,
                        event_slug,
                        started_at: chrono::Utc::now(),
                        publisher: None,
                        subscribers: HashMap::new(),
                    };
                    let info = room.info();
                    self.rooms.insert(room_id, room);
                    let _ = respond_to.send(Ok(info));
                }

                SfuCommand::PublishSdp {
                    room_id,
                    sdp_offer,
                    respond_to,
                } => {
                    let result = self.handle_publish_sdp(&room_id, &sdp_offer);
                    let _ = respond_to.send(result);
                }

                SfuCommand::PublishIce {
                    room_id,
                    candidate,
                    sdp_mid,
                    sdp_mline_index,
                } => {
                    self.handle_remote_ice(&room_id, None, &candidate, &sdp_mid, sdp_mline_index);
                }

                SfuCommand::SubscribeSdp {
                    room_id,
                    subscriber_id,
                    sdp_offer,
                    respond_to,
                } => {
                    let result = self.handle_subscribe_sdp(&room_id, &subscriber_id, &sdp_offer);
                    let _ = respond_to.send(result);
                }

                SfuCommand::SubscribeIce {
                    room_id,
                    subscriber_id,
                    candidate,
                    sdp_mid,
                    sdp_mline_index,
                } => {
                    self.handle_remote_ice(
                        &room_id,
                        Some(&subscriber_id),
                        &candidate,
                        &sdp_mid,
                        sdp_mline_index,
                    );
                }

                SfuCommand::RemoveSubscriber {
                    room_id,
                    subscriber_id,
                } => {
                    if let Some(room) = self.rooms.get_mut(&room_id) {
                        room.subscribers.remove(&subscriber_id);
                    }
                    self.remove_peer(&subscriber_id);
                }

                SfuCommand::StopRoom {
                    room_id,
                    respond_to,
                } => {
                    if let Some(room) = self.rooms.remove(&room_id) {
                        if let Some(pub_id) = room.publisher {
                            self.remove_peer(&pub_id);
                        }
                        for sub_id in room.subscribers.keys() {
                            self.remove_peer(sub_id);
                        }
                    }
                    let _ = respond_to.send(Ok(()));
                }

                SfuCommand::ListRooms { respond_to } => {
                    let list: Vec<RoomInfo> = self.rooms.values().map(|r| r.info()).collect();
                    let _ = respond_to.send(list);
                }

                SfuCommand::GetRoom {
                    room_id,
                    respond_to,
                } => {
                    let info = self.rooms.get(&room_id).map(|r| r.info());
                    let _ = respond_to.send(info);
                }

                SfuCommand::Shutdown => {
                    tracing::info!("SFU engine shutting down");
                    std::process::exit(0);
                }
            }
        }
    }

    fn handle_publish_sdp(&mut self, room_id: &str, sdp_offer: &str) -> Result<String, String> {
        // Validasi room dengan borrow singkat agar tidak bentrok dengan
        // `self.next_peer_id()` (yang meminjam `&mut self`) di bawah.
        {
            let room = self.rooms.get(room_id).ok_or("Room not found")?;
            if room.publisher.is_some() {
                return Err("Room already has a publisher".into());
            }
        }

        let peer_id = format!("pub_{}", self.next_peer_id());
        let mut rtc = Rtc::new(Instant::now());

        let local_candidate = Candidate::host(self.candidate_addr, "udp")
            .map_err(|e| format!("Failed to create host candidate: {e}"))?;
        rtc.add_local_candidate(local_candidate);

        let offer = SdpOffer::from_sdp_string(sdp_offer)
            .map_err(|e| format!("SDP parse error: {e}"))?;
        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| format!("accept_offer failed: {e}"))?;

        let answer_sdp = answer.to_sdp_string();

        if let Some(room) = self.rooms.get_mut(room_id) {
            room.publisher = Some(peer_id.clone());
        }

        self.peers.insert(
            peer_id,
            PeerState {
                rtc,
                role: PeerRole::Publisher,
                mids: Vec::new(),
                disconnected_since: None,
                remote_addr: None,
            },
        );

        tracing::info!(room_id, "Publisher connected");
        Ok(answer_sdp)
    }

    fn handle_subscribe_sdp(
        &mut self,
        room_id: &str,
        subscriber_id: &str,
        sdp_offer: &str,
    ) -> Result<String, String> {
        let room = self.rooms.get_mut(room_id).ok_or("Room not found")?;
        if room.publisher.is_none() {
            return Err("No active stream in this room".into());
        }

        let mut rtc = Rtc::new(Instant::now());

        let local_candidate = Candidate::host(self.candidate_addr, "udp")
            .map_err(|e| format!("Failed to create host candidate: {e}"))?;
        rtc.add_local_candidate(local_candidate);

        let offer = SdpOffer::from_sdp_string(sdp_offer)
            .map_err(|e| format!("SDP parse error: {e}"))?;
        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| format!("accept_offer failed: {e}"))?;

        let answer_sdp = answer.to_sdp_string();

        room.subscribers
            .insert(subscriber_id.to_string(), room_id.to_string());

        self.peers.insert(
            subscriber_id.to_string(),
            PeerState {
                rtc,
                role: PeerRole::Subscriber,
                mids: Vec::new(),
                disconnected_since: None,
                remote_addr: None,
            },
        );

        tracing::info!(room_id, subscriber_id, "Subscriber connected");
        Ok(answer_sdp)
    }

    fn handle_remote_ice(
        &mut self,
        room_id: &str,
        subscriber_id: Option<&str>,
        candidate: &str,
        _sdp_mid: &str,
        _sdp_mline_index: u32,
    ) {
        let peer_key = match subscriber_id {
            Some(id) => id.to_string(),
            None => self
                .rooms
                .get(room_id)
                .and_then(|r| r.publisher.clone())
                .unwrap_or_default(),
        };

        let Some(peer) = self.peers.get_mut(&peer_key) else {
            return;
        };

        // Browser mengirim string `candidate:...` (RFC 5245 §15.1) lewat
        // onicecandidate. Kadang ada prefix `a=`, atau string kosong sebagai
        // penanda end-of-candidates yang harus dilewati.
        let cand = candidate.trim();
        let cand = cand.strip_prefix("a=").unwrap_or(cand);
        if cand.is_empty() {
            return;
        }

        match Candidate::from_sdp_string(cand) {
            Ok(c) => {
                peer.rtc.add_remote_candidate(c);
                tracing::debug!(peer_id = %peer_key, "Remote ICE candidate ditambahkan");
            }
            // Kandidat mDNS (`*.local`) atau format tak dikenal gagal di-parse —
            // bukan fatal: konektivitas tetap jalan via host candidate yang
            // ditukar di SDP + UDP demux + peer-reflexive dari STUN binding.
            Err(e) => {
                tracing::debug!(peer_id = %peer_key, candidate = cand, error = %e, "Kandidat ICE diabaikan");
            }
        }
    }

    fn poll_all_peers(&mut self, product_tx: &mpsc::Sender<SfuEvent>) {
        let mut to_remove = Vec::new();
        let mut media_buf: Vec<(MediaKind, MediaData)> = Vec::new();
        // Subscriber yang minta keyframe → diteruskan ke publisher (lihat di bawah).
        let mut keyframe_reqs: Vec<String> = Vec::new();
        // Penonton baru dengan track video → mulai burst keyframe.
        let mut new_sub_video = false;

        for (peer_id, peer) in self.peers.iter_mut() {
            // Layani timer internal str0m (consent freshness RFC 7675, RTCP,
            // retransmit, ICE keepalive) SETIAP iterasi — termasuk saat media
            // mengalir deras. Tanpa ini consent kedaluwarsa (~20 dtk) lalu semua
            // penonton drop serempak dan reconnect tanpa henti.
            let _ = peer.rtc.handle_input(Input::Timeout(Instant::now()));
            loop {
                match peer.rtc.poll_output() {
                    Ok(Output::Timeout(_)) => break,
                    Ok(Output::Transmit(t)) => {
                        if let Err(e) = self.socket.send_to(&t.contents, t.destination) {
                            tracing::debug!(peer_id, error = %e, "UDP send failed");
                        }
                    }
                    Ok(Output::Event(e)) => match e {
                        Event::MediaAdded(m) => {
                            // Catat pemetaan mid→kind untuk peer ini.
                            if !peer.mids.iter().any(|(id, _)| *id == m.mid) {
                                peer.mids.push((m.mid, m.kind));
                            }
                            if matches!(peer.role, PeerRole::Subscriber)
                                && m.kind == MediaKind::Video
                            {
                                new_sub_video = true;
                            }
                        }
                        Event::KeyframeRequest(_) => {
                            // Browser penonton minta keyframe (tak bisa decode video
                            // tanpa I-frame). Teruskan permintaan ke publisher.
                            if matches!(peer.role, PeerRole::Subscriber) {
                                keyframe_reqs.push(peer_id.clone());
                            }
                        }
                        Event::MediaData(data) => {
                            if matches!(peer.role, PeerRole::Publisher) {
                                // Tentukan kind media publisher untuk diteruskan
                                // ke writer subscriber dengan kind yang sama.
                                if let Some(kind) = peer.rtc.media(data.mid).map(|md| md.kind()) {
                                    media_buf.push((kind, data));
                                }
                            }
                        }
                        Event::IceConnectionStateChange(state) => {
                            tracing::info!(peer_id, ?state, "ICE state");
                            match state {
                                // Pulih / mantap → reset tenggang.
                                IceConnectionState::Connected
                                | IceConnectionState::Completed => {
                                    peer.disconnected_since = None;
                                }
                                // Transien: catat waktunya, jangan langsung buang.
                                IceConnectionState::Disconnected => {
                                    if peer.disconnected_since.is_none() {
                                        peer.disconnected_since = Some(Instant::now());
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    },
                    Err(e) => {
                        // Penutupan normal dari browser (DTLS CloseNotify) bukan error
                        // fatal — terjadi tiap kali publisher/penonton stop atau menutup
                        // tab. Catat biasa lalu bersihkan peer-nya.
                        let msg = e.to_string();
                        if msg.contains("CloseNotify") {
                            tracing::info!(peer_id, "Peer connection closed");
                        } else {
                            tracing::warn!(peer_id, error = %msg, "Rtc poll error");
                        }
                        to_remove.push(peer_id.clone());
                        break;
                    }
                }
            }

            // Buang peer yang tetap Disconnected melewati tenggang pemulihan
            // (mis. jaringan benar-benar putus tanpa DTLS CloseNotify).
            if let Some(since) = peer.disconnected_since {
                if since.elapsed() >= DISCONNECT_GRACE {
                    to_remove.push(peer_id.clone());
                }
            }
        }

        for peer_id in to_remove {
            self.remove_peer(&peer_id);
            self.handle_peer_gone(&peer_id, product_tx);
        }

        for sub_id in keyframe_reqs {
            tracing::info!(subscriber_id = %sub_id, "Keyframe request → publisher (PLI)");
            self.request_publisher_keyframe(&sub_id);
        }

        // Burst keyframe selama ~6 dtk setelah penonton baru join, supaya video
        // tidak hitam meski browser tak mengirim PLI.
        if new_sub_video {
            self.keyframe_deadline = Some(Instant::now() + Duration::from_secs(6));
        }
        if let Some(deadline) = self.keyframe_deadline {
            let now = Instant::now();
            if now >= deadline {
                self.keyframe_deadline = None;
            } else if now.duration_since(self.last_keyframe) >= Duration::from_millis(500) {
                self.last_keyframe = now;
                self.request_all_publisher_keyframes();
            }
        }

        self.log_health_when_silent();

        if !media_buf.is_empty() {
            let before = self.frames_seen;
            self.frames_seen += media_buf.len() as u64;

            // Teruskan dulu, hitung tulisan yang berhasil ke subscriber.
            let mut written = 0u64;
            for (kind, data) in &media_buf {
                // Bukti kuat DTLS/SRTP publisher tuntas: media pertama masuk.
                if !self.ingest_logged {
                    self.ingest_logged = true;
                    tracing::info!(?kind, "SFU: media PERTAMA dari publisher masuk (DTLS/SRTP OK)");
                }
                let w = self.forward_to_subscribers(*kind, data);
                match kind {
                    MediaKind::Video => {
                        self.frames_video += 1;
                        self.fwd_video += w;
                    }
                    MediaKind::Audio => {
                        self.frames_audio += 1;
                        self.fwd_audio += w;
                    }
                }
                written += w;
            }
            self.frames_forwarded += written;

            // Log tiap ~250 frame, dipisah per-kind. Baca cepat:
            //   v_seen=0            → publisher tak kirim video / DTLS gagal ingest
            //   v_seen>0 & v_fwd=0  → forwarding video putus (codec/mid/writer)
            //   v_seen>0 & v_fwd>0  → video sampai ke subscriber (hitam = sisi klien)
            if before / 250 != self.frames_seen / 250 {
                let subs = self
                    .peers
                    .values()
                    .filter(|p| matches!(p.role, PeerRole::Subscriber))
                    .count();
                tracing::info!(
                    v_seen = self.frames_video,
                    a_seen = self.frames_audio,
                    v_fwd = self.fwd_video,
                    a_fwd = self.fwd_audio,
                    subscribers = subs,
                    "SFU media flowing (per-kind)"
                );
            }
        }
    }

    /// Laporkan keadaan SFU secara berkala SELAMA ada peer tetapi belum ada
    /// satu pun media yang masuk.
    ///
    /// ── KENAPA INI ADA ──────────────────────────────────────────────────
    /// Dua log yang sudah ada keduanya bergantung pada media yang MENGALIR:
    /// `media PERTAMA masuk` menyala sekali saat frame pertama tiba, dan
    /// ringkasan per-kind menyala tiap 250 frame. Keduanya benar — dan
    /// keduanya diam sempurna pada kegagalan yang paling sering terjadi.
    ///
    /// Bila ICE tak pernah tersambung, `frames_seen` tetap nol selamanya. Tak
    /// ada frame pertama, tak ada kelipatan 250. Yang dilihat operator adalah
    /// log yang benar-benar kosong, dan dari kosong itu mustahil dibedakan
    /// antara `belum ada yang siaran`, `publisher tersambung tapi diam`, dan
    /// `paket UDP tak pernah sampai`. Ketiganya tampak sama persis.
    ///
    /// Baris ini mengubah senyap menjadi keterangan: berapa peer yang terdaftar
    /// dan perannya apa, serta alamat kandidat yang diiklankan ke browser —
    /// justru nilai yang paling sering salah di produksi (`SFU_PUBLIC_IP` tak
    /// diisi, atau UDP port SFU tertutup di firewall).
    ///
    /// Hanya berbunyi saat ada peer: SFU yang menganggur tak perlu berisik.
    fn log_health_when_silent(&mut self) {
        if self.frames_seen > 0 || self.peers.is_empty() {
            return;
        }
        let now = Instant::now();
        if now.duration_since(self.last_health_log) < Duration::from_secs(5) {
            return;
        }
        self.last_health_log = now;

        let publishers = self
            .peers
            .values()
            .filter(|p| matches!(p.role, PeerRole::Publisher))
            .count();
        let subscribers = self.peers.len() - publishers;

        // Bedakan dua sebab yang gejalanya sama-sama `hitam`, karena
        // penanganannya berbeda jauh.
        let ada_ice = self.peers.values().any(|p| p.remote_addr.is_some());

        if ada_ice {
            // Paket JELAS sampai (ICE menemukan pasangan), tetapi tak satu pun
            // media menyusul. Hampir selalu ini: alamat yang DIIKLANKAN bukan
            // alamat yang benar-benar dipakai SFU saat MENGIRIM.
            //
            // Socket terikat `0.0.0.0`, jadi alamat sumber dipilih kernel
            // menurut rute ke masing-masing tujuan. Bila kandidat yang
            // diiklankan berasal dari antarmuka lain -- VPN/WireGuard sangat
            // sering, karena rute ke 8.8.8.8 keluar lewat terowongan -- maka
            // balasan SFU tiba dengan alamat sumber yang TIDAK cocok dengan
            // pasangan kandidat yang disepakati.
            //
            // STUN memaafkan itu (ia hanya perlu paketnya sampai), DTLS tidak.
            // Hasilnya: ICE `Connected`, lalu diam total, lalu putus saat
            // consent freshness habis (~20 detik).
            tracing::warn!(
                publishers,
                subscribers,
                candidate = %self.candidate_addr,
                "SFU: ICE tersambung TETAPI belum ada media sama sekali. Penyebab \
                 tersering: alamat kandidat yang diiklankan bukan antarmuka yang \
                 dipakai SFU saat mengirim balasan (khas mesin ber-VPN — rute \
                 internet keluar lewat terowongan). DTLS menuntut alamat sumber \
                 simetris, STUN tidak. Setel SFU_PUBLIC_IP ke alamat yang benar-benar \
                 dipakai klien untuk menjangkau server ini — 127.0.0.1 saat menguji \
                 di mesin yang sama, IP LAN untuk perangkat se-WiFi, IP publik di \
                 produksi."
            );
        } else {
            tracing::warn!(
                publishers,
                subscribers,
                candidate = %self.candidate_addr,
                "SFU: ada peer tetapi TIDAK ada paket UDP yang sampai sama sekali. \
                 Periksa SFU_PUBLIC_IP (harus alamat yang bisa dijangkau klien, \
                 bukan 0.0.0.0) dan apakah UDP port SFU terbuka di firewall."
            );
        }
    }

    /// Minta keyframe (PLI) dari publisher milik room si subscriber, supaya
    /// penonton yang baru bergabung segera mendapat I-frame (video tidak hitam).
    fn request_publisher_keyframe(&mut self, subscriber_id: &str) {
        let pub_id = self
            .rooms
            .values()
            .find(|r| r.subscribers.contains_key(subscriber_id))
            .and_then(|r| r.publisher.clone());
        let Some(pub_id) = pub_id else {
            return;
        };
        let vmid = self.peers.get(&pub_id).and_then(|p| {
            p.mids
                .iter()
                .find(|(_, k)| *k == MediaKind::Video)
                .map(|(m, _)| *m)
        });
        let Some(vmid) = vmid else {
            tracing::warn!(pub_id = %pub_id, "PLI gagal: publisher belum punya video mid");
            return;
        };
        if let Some(pubp) = self.peers.get_mut(&pub_id) {
            match pubp.rtc.writer(vmid) {
                Some(mut w) => match w.request_keyframe(None, KeyframeRequestKind::Pli) {
                    Ok(()) => tracing::debug!(pub_id = %pub_id, "PLI terkirim ke publisher"),
                    Err(e) => tracing::warn!(
                        pub_id = %pub_id,
                        error = %e,
                        "PLI ditolak (rtcp-fb pli mungkin tak ternegosiasi) → penonton tak dapat keyframe → hitam"
                    ),
                },
                None => tracing::warn!(pub_id = %pub_id, "PLI gagal: writer(vmid) None"),
            }
        }
    }

    /// Minta keyframe ke SEMUA publisher (dipakai untuk burst saat penonton baru).
    fn request_all_publisher_keyframes(&mut self) {
        let targets: Vec<(String, Mid)> = self
            .rooms
            .values()
            .filter_map(|r| r.publisher.clone())
            .filter_map(|pid| {
                let vmid = self.peers.get(&pid).and_then(|p| {
                    p.mids
                        .iter()
                        .find(|(_, k)| *k == MediaKind::Video)
                        .map(|(m, _)| *m)
                })?;
                Some((pid, vmid))
            })
            .collect();
        for (pid, vmid) in targets {
            if let Some(p) = self.peers.get_mut(&pid) {
                match p.rtc.writer(vmid) {
                    Some(mut w) => match w.request_keyframe(None, KeyframeRequestKind::Pli) {
                        Ok(()) => tracing::debug!(pub_id = %pid, "burst PLI terkirim"),
                        Err(e) => tracing::warn!(
                            pub_id = %pid,
                            error = %e,
                            "burst PLI ditolak → penonton tak dapat keyframe → hitam"
                        ),
                    },
                    None => tracing::warn!(pub_id = %pid, "burst PLI gagal: writer(vmid) None"),
                }
            }
        }
    }

    /// Bersihkan state setelah sebuah peer hilang. Jika peer adalah publisher,
    /// siaran berakhir: hapus room + peer penonton dan kabari service lewat
    /// `StreamStopped` (service akan melepas LiveRoom). Jika penonton, cukup
    /// lepas slot-nya dari room agar hitungan viewer akurat.
    fn handle_peer_gone(&mut self, peer_id: &str, product_tx: &mpsc::Sender<SfuEvent>) {
        let publisher_room = self
            .rooms
            .iter()
            .find(|(_, r)| r.publisher.as_deref() == Some(peer_id))
            .map(|(id, _)| id.clone());

        if let Some(room_id) = publisher_room {
            if let Some(room) = self.rooms.remove(&room_id) {
                for sub_id in room.subscribers.keys() {
                    self.remove_peer(sub_id);
                }
            }
            // Sertakan frame yang sempat masuk: v_seen=0 saat publisher gone =
            // publisher JATUH sebelum satu pun frame video ter-ingest (indikasi
            // DTLS/SRTP tak pernah tuntas), bukan sekadar penonton yang hitam.
            tracing::info!(
                room_id,
                v_seen = self.frames_video,
                a_seen = self.frames_audio,
                "Publisher gone — stopping stream"
            );
            // BUG FIX #4: Log kegagalan try_send agar room tidak diam-diam
            // tetap terlihat di API jika channel product penuh.
            if let Err(e) = product_tx.try_send(SfuEvent::StreamStopped { room_id: room_id.clone() }) {
                tracing::error!(room_id, error = %e, "CRITICAL: StreamStopped product dropped — room will remain visible in API. Consider increasing product channel capacity.");
            }
        } else {
            // Penonton putus: lepas dari room SFU + kabari service agar hitungan
            // penonton di LiveRoom ikut berkurang.
            let room_id = self
                .rooms
                .iter()
                .find(|(_, r)| r.subscribers.contains_key(peer_id))
                .map(|(id, _)| id.clone());
            for room in self.rooms.values_mut() {
                room.subscribers.remove(peer_id);
            }
            if let Some(room_id) = room_id {
                // BUG FIX #4 (lanjutan): Log kegagalan try_send SubscriberLeft.
                if let Err(e) = product_tx.try_send(SfuEvent::SubscriberLeft {
                    room_id: room_id.clone(),
                    subscriber_id: peer_id.to_string(),
                }) {
                    tracing::warn!(room_id, peer_id, error = %e, "SubscriberLeft product dropped — viewer count may be inaccurate.");
                }
            }
        }
    }

    /// Teruskan satu frame publisher ke semua subscriber. Mengembalikan jumlah
    /// subscriber yang benar-benar menerima tulisan (untuk log diagnostik:
    /// membedakan "publisher mengalir" vs "subscriber benar-benar dilayani").
    fn forward_to_subscribers(&mut self, kind: MediaKind, data: &MediaData) -> u64 {
        let mut written = 0u64;

        for (sub_id, peer) in self.peers.iter_mut() {
            if !matches!(peer.role, PeerRole::Subscriber) {
                continue;
            }

            // Cari mid subscriber dengan kind yang sama (audio→audio, video→video).
            let Some(mid) = peer
                .mids
                .iter()
                .find(|(_, k)| *k == kind)
                .map(|(m, _)| *m)
            else {
                tracing::debug!(sub_id, ?kind, "Subscriber belum punya mid untuk kind ini");
                continue;
            };

            let Some(writer) = peer.rtc.writer(mid) else {
                continue;
            };

            // Pemetaan SFU resmi str0m: cocokkan PayloadParams frame masuk ke PT
            // LOKAL subscriber. PT bisa berbeda antar-peer untuk codec yang sama;
            // `match_params` juga memvalidasi parameter (mis. profile H264), bukan
            // sekadar enum codec — menghindari kirim payload di bawah PT yang salah
            // (penyebab klasik layar hitam). None = subscriber tak menegosiasi
            // codec ini → lewati, jangan kirim rusak.
            let Some(pt) = writer.match_params(data.params) else {
                tracing::debug!(
                    sub_id,
                    ?kind,
                    "Subscriber tak menegosiasi codec/params publisher — frame dilewati"
                );
                continue;
            };

            match writer.write(pt, data.network_time, data.time, data.data.clone()) {
                Ok(()) => written += 1,
                Err(e) => tracing::debug!(sub_id, error = %e, "Media write to subscriber failed"),
            }
        }

        written
    }

    /// Blokir hingga satu datagram tiba atau `MAX_POLL_WAIT` lewat. Saat idle,
    /// thread tidur di syscall ini alih-alih spin membakar CPU. Begitu satu
    /// paket masuk, sisa paket yang sudah antre dikuras tanpa menunggu lagi.
    fn read_socket(&mut self, buf: &mut Vec<u8>) {
        buf.resize(BUF_SIZE, 0);
        match self.socket.recv_from(buf) {
            Ok((n, source)) => {
                let dest = self.candidate_addr;
                // `dispatch_input` memproses paket seketika (str0m hanya meminjam
                // slice selama `handle_input`), jadi tak perlu menyalin ke Vec
                // baru — pinjam langsung dari buffer, hemat satu alokasi/paket.
                self.dispatch_input(source, dest, &buf[..n]);
                self.drain_ready(buf);
            }
            // Timeout (tak ada paket dalam jendela ini): tak masalah — iterasi
            // loop berikutnya memanggil `poll_all_peers` yang sudah melayani
            // timer str0m tiap peer.
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => {
                tracing::error!(error = %e, "UDP recv error");
            }
        }
    }

    /// Kuras semua datagram yang sudah siap (non-blocking) supaya throughput
    /// tetap tinggi saat ramai — tanpa ini hanya satu paket diproses per tick.
    fn drain_ready(&mut self, buf: &mut Vec<u8>) {
        if self.socket.set_nonblocking(true).is_err() {
            return;
        }
        loop {
            buf.resize(BUF_SIZE, 0);
            match self.socket.recv_from(buf) {
                Ok((n, source)) => {
                    let dest = self.candidate_addr;
                    self.dispatch_input(source, dest, &buf[..n]);
                }
                Err(_) => break,
            }
        }
        // Kembali ke mode blocking; read-timeout (SO_RCVTIMEO) tetap berlaku.
        let _ = self.socket.set_nonblocking(false);
    }

    fn dispatch_input(&mut self, source: SocketAddr, dest: SocketAddr, data: &[u8]) {
        let contents: DatagramRecv = match data.try_into() {
            Ok(c) => c,
            Err(_) => return,
        };
        let input = Input::Receive(
            Instant::now(),
            Receive {
                proto: Protocol::Udp,
                source,
                destination: dest,
                contents,
            },
        );
        // Fast path O(1): paket dari source yang sudah dipetakan langsung
        // dirutekan ke peer-nya, tanpa menyapu seluruh daftar peer. Penting saat
        // penonton banyak — tiap datagram (RTP/RTCP/STUN) tak lagi memicu N
        // panggilan `accepts`.
        if let Some(cached_id) = self.addr_to_peer.get(&source).cloned() {
            if let Some(peer) = self.peers.get_mut(&cached_id) {
                if peer.rtc.accepts(&input) {
                    let _ = peer.rtc.handle_input(input);
                    return;
                }
            }
            // Rute basi: peer sudah dibuang, atau source pindah port (NAT
            // rebinding). Buang entri lalu jatuh ke pencarian penuh — cache ini
            // self-healing, entri usang otomatis terkoreksi di sini.
            self.addr_to_peer.remove(&source);
        }

        // Slow path: demux HANYA ke satu Rtc yang mengakui paket (berdasarkan
        // ufrag ICE / DTLS / alamat), lalu simpan rutenya untuk paket berikutnya.
        // Memberi paket ke semua peer (cara lama) merusak state ICE/DTLS peer lain
        // → koneksi gagal / putus (CloseNotify).
        for (peer_id, peer) in self.peers.iter_mut() {
            if peer.rtc.accepts(&input) {
                self.addr_to_peer.insert(source, peer_id.clone());
                peer.remote_addr = Some(source);
                let _ = peer.rtc.handle_input(input);
                return;
            }
        }
    }

    /// Buang satu peer dari engine sekaligus mengevakuasi entri cache demux-nya
    /// (`addr_to_peer`). Tanpa ini cache akan menumpuk entri yang menunjuk peer
    /// mati selamanya = memory leak pada server berumur panjang. Entri hanya
    /// dibuang bila masih menunjuk peer ini (source bisa sudah dipakai peer baru).
    fn remove_peer(&mut self, peer_id: &str) -> Option<PeerState> {
        let peer = self.peers.remove(peer_id);
        if let Some(addr) = peer.as_ref().and_then(|p| p.remote_addr) {
            if self.addr_to_peer.get(&addr).map(String::as_str) == Some(peer_id) {
                self.addr_to_peer.remove(&addr);
            }
        }
        peer
    }

    fn next_peer_id(&mut self) -> u64 {
        let id = self.next_peer_id;
        self.next_peer_id += 1;
        id
    }
}
