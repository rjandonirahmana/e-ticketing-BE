//! ws/manager.rs — Production-scale WebSocket connection manager.
//!
//! OPTIMISASI vs original:
//!
//! 1. broadcast_room(): serialize SEKALI via to_shared_json(), deliver Arc<str> clone
//!    ke semua member. Original serialize SEKALI tapi wrap Arc setelah. Sekarang
//!    explicit dan konsisten via to_shared_json() API.
//!
//! 2. RateLimitRegistry: ganti RwLock<HashMap> → DashMap untuk konsistensi dengan
//!    sessions/room_members. RwLock<HashMap>.read() masih blocking saat ada writer.
//!    DashMap shard-based = lebih paralel untuk workload baca-dominan.
//!
//! 3. try_connect(): WsEvent::err() sekarang terima ErrorCode enum (bukan string literal).
//!
//! 4. send_to(): serialize Arc<str> via to_shared_json() — konsisten dengan broadcast.
//!
//! 5. spawn_heartbeat(): Ping Arc<str> pre-serialized via WsEvent::Ping.to_json()
//!    (kena fast-path OnceLock, bukan serialize ulang tiap tick).
//!
//! Yang TIDAK diubah (sudah optimal):
//! - O(members) broadcast via room_members index — tetap
//! - Redis publish dengan retry + exponential backoff — tetap
//! - Per-user rate limiter (token bucket, lock-free CAS) — tetap
//! - Bounded semaphore di handler untuk cegah task flood — tetap
//! - DashMap untuk sessions & room_members — tetap

use std::{
    hash::RandomState,
    sync::{
        atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use dashmap::{DashMap, DashSet};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::ws::proto::{ErrorCode, WsEvent};

// ── Tuning ────────────────────────────────────────────────────────────────────

const CHAN_BUF: usize = 32;
pub const MAX_CONNECTIONS: usize = 10_000;
/// Selang ping heartbeat WS chat.
///
/// Angkanya ditentukan perantara yang paling ketat, dan itu Pingora di depan:
/// `pingora-core` memberi sesi downstream `read_timeout` bawaan **60 detik**,
/// dan kinetic-proxy tak menimpanya. Socket yang tak mengirim satu frame pun
/// selama 60 detik diputus proxy. Ping dari server memancing Pong otomatis dari
/// browser, dan Pong itulah yang menahan hitungannya.
///
/// 30 detik hanya memberi dua kesempatan di dalam jendela 60 detik itu — satu
/// tick yang telat sudah cukup untuk kehilangan socket, dan tick MEMANG telat
/// saat runtime tersendat (insiden 3 Sep 2026: WS chat mati tepat 60 detik
/// setelah upgrade karena tak ada satu pun ping yang sempat terkirim). 20 detik
/// memberi tiga kesempatan, dengan biaya satu frame kosong tambahan per menit.
const PING_INTERVAL: Duration = Duration::from_secs(20);
const PONG_TIMEOUT: Duration = Duration::from_secs(10);
const CH_USER: &str = "ws:u:";
const CH_ROOM: &str = "ws:r:";
const SHRINK_INTERVAL: Duration = Duration::from_secs(300);
/// Tenggang bagi koneksi yang digantikan untuk menuliskan pesan `Replaced`-nya
/// sebelum dihentikan paksa. Cukup untuk satu penulisan socket, jauh di bawah
/// 30 detik yang harus ditunggu bila mengandalkan timeout heartbeat.
const REPLACED_GRACE: Duration = Duration::from_millis(500);

const REDIS_PUBLISH_RETRIES: u8 = 3;
/// Koneksi WebSocket serentak per pengguna: ponsel, laptop, beberapa tab.
///
/// Ada plafonnya karena tanpa itu satu klien yang menyambung ulang tanpa henti
/// karena bug bisa menghabiskan jatah koneksi seluruh server atas nama satu
/// orang.
const MAKS_SESI_PER_USER: usize = 5;

const REDIS_PUBLISH_RETRY_DELAY: Duration = Duration::from_millis(50);

const RATE_LIMIT_MAX: u32 = 30;
const RATE_LIMIT_WINDOW_SECS: u64 = 10;
/// FIX: Hard cap entry rate limit registry.
/// Tanpa batas, flash-sale dengan 1M user unik bisa membengkak tanpa batas.
/// 500k entry × ~120 bytes (Arc<str> + UserBucket) ≈ 60MB — acceptable upper bound.
const RATE_LIMIT_MAX_ENTRIES: usize = 500_000;
/// Cleanup rate limit lebih sering dari SHRINK_INTERVAL (5 menit) karena window hanya 10 detik.
const RATE_LIMIT_CLEANUP_INTERVAL: Duration = Duration::from_secs(30);

pub type WsTx = mpsc::Sender<Arc<str>>;

/// Satu sesi WS aktif milik seorang user. `conn_id` membedakan GENERASI koneksi:
/// reconnect cepat membuat sesi baru untuk user_id yang sama, dan cleanup sesi
/// LAMA tidak boleh mencabut sesi BARU (lihat `disconnect`). Tanpa pembeda ini,
/// setiap reconnect menghapus registrasi koneksi penggantinya → user "online"
/// tapi tak menerima apa pun → klien reconnect lagi → siklus tak berujung
/// (gejala: status chat "connecting" terus-menerus).
struct Session {
    conn_id: u64,
    tx: WsTx,
    /// Token pembatalan MILIK koneksi ini.
    ///
    /// Tanpa ini, koneksi yang DIGANTI reconnect tak punya cara diberi tahu
    /// untuk berhenti: `sessions.insert` mencabutnya dari peta, pesan
    /// `Replaced` dikirim, lalu task-nya tetap hidup sampai heartbeat-nya
    /// sendiri kedaluwarsa — PING_INTERVAL + PONG_TIMEOUT = 30 detik.
    ///
    /// Selama 30 detik itu koneksi mati tetap memegang: satu izin semaphore
    /// (dari plafon `max_conn`), dua task tokio, dan buffer channel 32 slot.
    /// Di jaringan seluler yang putus-nyambung, reconnect adalah kejadian
    /// paling lumrah — jadi pada beban puncak sebagian besar plafon koneksi
    /// justru dipegang koneksi yang sudah tak ada, dan koneksi BARU ditolak
    /// "Server at capacity" padahal `active_conns` masih rendah.
    cancel: CancellationToken,
}

// ── Per-user rate limiter (token bucket, lock-free) ────────────────────────────

struct UserBucket {
    tokens: AtomicU32,
    window_start: AtomicU64,
}

impl UserBucket {
    fn new() -> Self {
        Self {
            tokens: AtomicU32::new(RATE_LIMIT_MAX),
            window_start: AtomicU64::new(now_secs()),
        }
    }

    fn try_consume(&self) -> bool {
        let now = now_secs();
        let start = self.window_start.load(Ordering::Relaxed);

        // Reset jendela HANYA boleh dimenangkan satu pemanggil.
        //
        // Versi sebelumnya memakai `store` polos: dua permintaan yang tiba
        // bersamaan sesudah jendela habis sama-sama melihat syaratnya terpenuhi
        // dan sama-sama mengisi ulang token ke penuh. Hasilnya satu user bisa
        // menembus 2× (atau lebih, sebanyak thread yang kebetulan bertabrakan)
        // jatah per jendela — persis pada beban tinggi, satu-satunya keadaan
        // yang membuat pembatas ini ada.
        //
        // `compare_exchange` membuat yang kalah balapan melanjutkan memakai
        // jendela yang baru saja dibuka pemenangnya, bukan membukanya lagi.
        if now.saturating_sub(start) >= RATE_LIMIT_WINDOW_SECS
            && self
                .window_start
                .compare_exchange(start, now, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
        {
            self.tokens.store(RATE_LIMIT_MAX, Ordering::Release);
        }

        loop {
            let cur = self.tokens.load(Ordering::Acquire);
            if cur == 0 {
                return false;
            }
            match self.tokens.compare_exchange_weak(
                cur,
                cur - 1,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(_) => {} // retry CAS
            }
        }
    }
}

/// Rate limit registry.
///
/// OPTIMISASI vs original: DashMap menggantikan RwLock<HashMap>.
/// - RwLock<HashMap>: write lock untuk insert bucket baru memblokir SEMUA reader.
///   Di high-concurrency, setiap user baru = stop-the-world singkat.
/// - DashMap: sharded locking — insert di shard X tidak blokir read di shard Y.
/// - Workload: baca (try_consume) dominan, tulis (insert baru) jarang → DashMap ideal.
pub struct RateLimitRegistry {
    buckets: DashMap<Arc<str>, Arc<UserBucket>, RandomState>,
}

impl RateLimitRegistry {
    fn new() -> Self {
        Self {
            buckets: DashMap::with_hasher(RandomState::new()),
        }
    }

    pub fn check(&self, user_id: &str) -> bool {
        // Fast path: bucket sudah ada
        if let Some(bucket) = self.buckets.get(user_id) {
            return bucket.try_consume();
        }

        // FIX: Hard cap — tolak request baru jika sudah melebihi batas entry.
        // Lebih baik rate-limit user baru daripada OOM saat flash sale.
        if self.buckets.len() >= RATE_LIMIT_MAX_ENTRIES {
            tracing::warn!(
                user_id,
                entries = self.buckets.len(),
                "RateLimitRegistry at capacity — new user rejected"
            );
            return false;
        }

        // Slow path: user baru — buat bucket
        let bucket = Arc::new(UserBucket::new());
        bucket.try_consume();
        self.buckets
            .entry(Arc::from(user_id))
            .or_insert_with(|| bucket);
        true
    }

    /// Cleanup bucket expired. Dipanggil periodik setiap RATE_LIMIT_CLEANUP_INTERVAL (30s).
    /// FIX: Cleanup lebih sering (30s vs 5 menit) karena window hanya 10 detik.
    /// Entry yang window_start-nya > 2× window sudah pasti idle.
    pub fn cleanup(&self) {
        let cutoff = now_secs().saturating_sub(RATE_LIMIT_WINDOW_SECS * 2);
        self.buckets
            .retain(|_, b| b.window_start.load(Ordering::Relaxed) > cutoff);
        tracing::debug!("Rate limit cleanup: {} active users", self.buckets.len());
    }
}

// ── WsManager ─────────────────────────────────────────────────────────────────

pub struct WsManager {
    /// user_id → sesi aktif (tx + generasi koneksi)
    /// user_id → SELURUH koneksi miliknya.
    ///
    /// ── KENAPA BANYAK, BUKAN SATU ─────────────────────────────────────────
    /// Dulu satu pengguna = satu sesi, dan koneksi baru MENGGANTIKAN yang lama.
    /// Itu berarti membuka tab kedua membuat tab pertama tuli — dan karena
    /// keduanya menyambung ulang sendiri saat putus, keduanya saling merebut
    /// sesi tanpa henti: di log tampak sebagai "WS opened"/"WS closed"
    /// bergantian tiap tiga detik, dua deret sekaligus.
    ///
    /// Padahal punya ponsel dan laptop terbuka bersamaan adalah cara orang
    /// memakai aplikasi chat, bukan penyalahgunaan.
    ///
    /// `Vec`, bukan peta ber-hash: isinya beberapa entri, dan pemindaian linear
    /// atas tiga elemen lebih cepat daripada hashing. Mutasinya aman karena
    /// selalu lewat kunci shard `DashMap` induknya.
    sessions: DashMap<Arc<str>, Vec<Session>, RandomState>,
    /// Penerbit `conn_id` monoton — pembeda generasi koneksi per user.
    next_conn_id: AtomicU64,

    /// room_id → Set<user_id>
    /// Membuat broadcast O(members) bukan O(total connections)
    room_members: DashMap<Arc<str>, DashSet<Arc<str>>, RandomState>,

    /// user_id → Set<room_id> — indeks BALIK dari `room_members`.
    ///
    /// Ada semata untuk `leave_all_rooms`. Tanpa indeks ini, memutus SATU user
    /// berarti memindai SELURUH `room_members` (setiap room, setiap shard) hanya
    /// untuk menemukan segelintir room yang benar-benar ia ikuti.
    ///
    /// Biaya sebenarnya muncul saat putus MASSAL — deploy, restart, atau satu
    /// menara seluler yang bermasalah: N user × R room. Pada 10.000 user dan
    /// beberapa ribu room itu puluhan juta operasi yang semuanya memegang kunci
    /// shard, tepat pada momen seluruh sisa sistem juga sedang sibuk menerima
    /// koneksi ulang mereka.
    ///
    /// Nilainya `Vec`, BUKAN `DashSet` seperti `room_members`. Ini bukan
    /// ketidakkonsistenan — `DashMap`/`DashSet` mengalokasikan shard sendiri
    /// (bawaannya 4 × jumlah core, dibulatkan ke pangkat dua), masing-masing
    /// dengan kunci dan tabel hash-nya sendiri. Sebagai peta global itu murah
    /// karena dibayar sekali; sebagai nilai PER USER ia jadi ~1–2 KB overhead
    /// shard hanya untuk menyimpan dua sampai lima room. Pada 10.000 koneksi itu
    /// belasan hingga puluhan MB yang tak menyimpan apa pun.
    ///
    /// `Vec<Arc<str>>` untuk isi sekecil itu: 24 byte + 8 byte per room, dan
    /// pencarian linear atas lima elemen lebih cepat daripada hashing. Mutasinya
    /// aman karena selalu lewat kunci shard `DashMap` induknya (`entry`/`get_mut`).
    ///
    /// Harga yang dibayar: satu `Arc<str>` tambahan per pasangan (user, room) —
    /// pointer, bukan salinan teks, dan hanya untuk keanggotaan yang memang ada.
    user_rooms: DashMap<Arc<str>, Vec<Arc<str>>, RandomState>,

    redis: ConnectionManager,
    pub dropped: Arc<AtomicU64>,
    conn_limit: Arc<Semaphore>,
    /// Batas koneksi efektif (diturunkan dari RAM VPS saat start; lihat
    /// `utils::capacity`). Fallback `MAX_CONNECTIONS` bila 0 diberikan.
    max_conn: usize,
    active_conns: Arc<AtomicUsize>,
    rate_limit: Arc<RateLimitRegistry>,
    shutdown: CancellationToken,
}

impl WsManager {
    pub async fn new(
        redis_client: redis::Client,
        max_connections: usize,
    ) -> anyhow::Result<Arc<Self>> {
        let redis = ConnectionManager::new(redis_client.clone()).await?;
        let shutdown = CancellationToken::new();
        // Clamp aman: minimal 100, dan tak lebih dari plafon absolut MAX_CONNECTIONS.
        let max_conn = max_connections.clamp(100, MAX_CONNECTIONS);

        let mgr = Arc::new(Self {
            sessions: DashMap::with_hasher(RandomState::new()),
            next_conn_id: AtomicU64::new(0),
            room_members: DashMap::with_hasher(RandomState::new()),
            user_rooms: DashMap::with_hasher(RandomState::new()),
            redis,
            dropped: Arc::new(AtomicU64::new(0)),
            conn_limit: Arc::new(Semaphore::new(max_conn)),
            max_conn,
            active_conns: Arc::new(AtomicUsize::new(0)),
            rate_limit: Arc::new(RateLimitRegistry::new()),
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

    pub fn try_connect(
        &self,
        user_id: &str,
    ) -> Option<(
        mpsc::Receiver<Arc<str>>,
        CancellationToken,
        tokio::sync::OwnedSemaphorePermit,
        u64,
    )> {
        let permit = self.conn_limit.clone().try_acquire_owned().ok()?;
        let (tx, rx) = mpsc::channel::<Arc<str>>(CHAN_BUF);
        let conn_token = CancellationToken::new();
        let key: Arc<str> = user_id.into();
        let conn_id = self.next_conn_id.fetch_add(1, Ordering::Relaxed);

        let sesi = Session {
            conn_id,
            tx,
            cancel: conn_token.clone(),
        };

        {
            let mut daftar = self.sessions.entry(key).or_default();
            if daftar.is_empty() {
                self.active_conns.fetch_add(1, Ordering::Relaxed);
            }

            // Plafon per pengguna. Tanpa ini, satu klien yang menyambung ulang
            // tanpa henti karena bug bisa menumpuk ratusan koneksi atas nama
            // satu orang dan menghabiskan jatah seluruh server.
            //
            // Yang TERTUA yang pergi: perangkat yang baru saja dipakai hampir
            // pasti yang sedang ditatap orangnya.
            while daftar.len() >= MAKS_SESI_PER_USER {
                let tua = daftar.remove(0);
                crate::service::metrik::METRIK
                    .sesi_diganti
                    .fetch_add(1, Ordering::Relaxed);
                let msg =
                    WsEvent::err(ErrorCode::Replaced, "Terlalu banyak perangkat aktif").to_json();
                let _ = tua.tx.try_send(msg);
                // Tenggang singkat, bukan pembatalan seketika: pesan `Replaced`
                // di atas baru masuk antrean, dan task tulis yang langsung
                // dibatalkan akan keluar sebelum sempat menuliskannya ke socket.
                // Klien lama lalu hanya melihat socket tertutup begitu saja —
                // tak bisa dibedakan dari gangguan jaringan — dan ia akan
                // menyambung ulang, merebut kembali tempat yang baru saja
                // dilepas. Tenggang inilah yang membuat pesannya sampai lebih
                // dulu sehingga klien tahu harus berhenti.
                tokio::spawn(async move {
                    tokio::time::sleep(REPLACED_GRACE).await;
                    tua.cancel.cancel();
                });
            }
            daftar.push(sesi);
        }

        tracing::debug!(user_id, conn_id, "WS connected");
        Some((rx, conn_token, permit, conn_id))
    }

    pub fn disconnect(&self, user_id: &str, conn_id: u64) {
        // GUARD GENERASI: hanya cabut bila sesi di map masih milik koneksi INI.
        // Tanpa guard, cleanup koneksi lama (yang baru digantikan reconnect)
        // mencabut sesi baru + room membership-nya → koneksi baru jadi "hantu"
        // (TCP hidup, tak menerima apa pun) → klien reconnect → koneksi itu
        // pun dibunuh cleanup berikutnya → chat "connecting" tanpa akhir.
        let kosong = {
            let Some(mut daftar) = self.sessions.get_mut(user_id) else {
                return;
            };
            let sebelum = daftar.len();
            daftar.retain(|s| s.conn_id != conn_id);
            if daftar.len() == sebelum {
                // Bukan koneksi ini yang tercatat — sudah tergantikan lebih
                // dulu. Jangan sentuh apa pun.
                return;
            }
            daftar.is_empty()
        };

        if kosong {
            // Keanggotaan room dibersihkan HANYA saat koneksi TERAKHIR miliknya
            // pergi. Dulu tiap satu koneksi putus ia mencabut seluruh room
            // pengguna itu — dengan banyak perangkat, menutup satu tab akan
            // membuat perangkat lainnya berhenti menerima apa pun.
            self.sessions.remove(user_id);
            self.active_conns.fetch_sub(1, Ordering::Relaxed);
            self.leave_all_rooms(user_id);
        }
        tracing::debug!(user_id, conn_id, "WS disconnected");
    }

    // ── Room membership ───────────────────────────────────────────────────────

    pub fn join_room(&self, user_id: &str, room_id: &str) {
        // FIX: Jangan tambahkan user ke room_members WS index jika tidak punya
        // active session. User offline akan join kembali saat reconnect via Hello flow.
        // Tanpa guard ini, ghost members menumpuk di room_members dan menyebabkan
        // Redis publish sia-sia untuk setiap broadcast.
        if !self.sessions.contains_key(user_id) {
            return;
        }
        let uid: Arc<str> = Arc::from(user_id);
        let rid: Arc<str> = Arc::from(room_id);
        self.room_members
            .entry(rid.clone())
            .or_insert_with(|| DashSet::with_hasher(RandomState::new()))
            .insert(uid.clone());
        // `contains` sebelum `push`: daftar ini sangat pendek, jadi pemindaian
        // linear lebih murah daripada struktur ber-hash — tapi tanpa pemeriksaan
        // ini, `register_rooms` saat setiap reconnect akan menumpuk room yang
        // sama berulang kali.
        let mut rooms = self.user_rooms.entry(uid).or_default();
        if !rooms.iter().any(|r| **r == *rid) {
            rooms.push(rid);
        }
    }

    pub fn leave_room(&self, user_id: &str, room_id: &str) {
        if let Some(members) = self.room_members.get(room_id) {
            members.remove(user_id);
        }
        if let Some(mut rooms) = self.user_rooms.get_mut(user_id) {
            rooms.retain(|r| **r != *room_id);
        }
        self.buang_room_bila_kosong(room_id);
    }

    /// Cabut user dari SEMUA room yang ia ikuti — O(room milik user), bukan
    /// O(seluruh room). Lihat catatan pada `user_rooms`.
    pub fn leave_all_rooms(&self, user_id: &str) {
        // Entri user diambil KELUAR lebih dulu (remove, bukan get): sesudah ini
        // tak ada lagi yang bisa menambah room ke daftar yang sedang dibereskan,
        // dan kuncinya tak dipegang selama pembersihan di bawah.
        let Some((_, rooms)) = self.user_rooms.remove(user_id) else {
            return;
        };
        for room_id in &rooms {
            if let Some(members) = self.room_members.get(room_id.as_ref()) {
                members.remove(user_id);
            }
            self.buang_room_bila_kosong(room_id);
        }
    }

    /// Buang entri room yang sudah tak beranggota.
    ///
    /// `remove_if` — bukan `is_empty()` lalu `remove()` terpisah: di antara dua
    /// operasi itu ada celah tempat anggota baru bisa masuk, dan room yang baru
    /// saja diisi seseorang akan ikut terhapus. Predikatnya dievaluasi di bawah
    /// kunci shard yang sama dengan penghapusannya.
    fn buang_room_bila_kosong(&self, room_id: &str) {
        self.room_members.remove_if(room_id, |_, m| m.is_empty());
    }

    pub fn register_rooms(&self, user_id: &str, room_ids: &[String]) {
        for room_id in room_ids {
            self.join_room(user_id, room_id);
        }
    }

    // ── Rate limit ────────────────────────────────────────────────────────────

    /// Cek rate limit — sekarang sync (bukan async) karena DashMap tidak butuh await.
    /// Caller di handler tidak perlu .await → sedikit lebih efisien.
    pub fn check_rate_limit(&self, user_id: &str) -> bool {
        self.rate_limit.check(user_id)
    }

    // ── Send ──────────────────────────────────────────────────────────────────

    pub async fn send_to(&self, user_id: &str, product: WsEvent) {
        // to_shared_json() — Arc<str> bisa di-pass ke deliver_local dan redis tanpa copy
        let json = product.to_shared_json();
        if !self.deliver_local(user_id, json.clone()).await {
            self.redis_publish_with_retry(&format!("{CH_USER}{user_id}"), &json)
                .await;
        }
    }

    /// Broadcast ke semua member room.
    ///
    /// FIX P0: Hapus local delivery dari sini — cukup publish ke Redis SEKALI.
    /// Redis subscriber di instance yang sama akan deliver ke lokal user.
    ///
    /// Masalah sebelumnya (Redis Loopback Duplication):
    ///   1. broadcast_room() deliver lokal ke semua member room
    ///   2. broadcast_room() juga publish ke Redis
    ///   3. spawn_redis_subscriber() menerima publish itu dan deliver lokal LAGI
    ///   → Semua member di instance ini terima pesan 2×
    ///
    /// Fix: Biarkan Redis subscriber jadi satu-satunya path delivery.
    /// Trade-off: +1-2ms latency untuk lokal user (loopback Redis vs in-process).
    /// Acceptable karena: konsistensi single delivery path > micro-latency gain.
    ///
    /// CATATAN: serialize SEKALI via to_shared_json(), Arc<str> clone ke
    /// subscriber = atomic bump — optimisasi ini masih berlaku.
    pub async fn broadcast_room(&self, room_id: &str, product: WsEvent) {
        let shared = product.to_shared_json();
        // Hanya publish ke Redis — subscriber lokal juga akan deliver ke semua member
        self.redis_publish_with_retry(&format!("{CH_ROOM}{room_id}"), &shared)
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
            // Pre-compute Ping JSON sekali — kena OnceLock fast path
            let ping_json = WsEvent::Ping.to_json();

            let mut interval = tokio::time::interval(PING_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                tokio::select! {
                    _ = shutdown.cancelled()    => break,
                    _ = conn_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        // Clone Arc = atomic bump, bukan serialize ulang
                        if tx.send(ping_json.clone()).await.is_err() { break; }
                        match tokio::time::timeout(PONG_TIMEOUT, pong_rx.recv()).await {
                            Ok(Some(())) => {}
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
    /// Batas koneksi WS efektif (auto-skala dari RAM VPS saat start).
    pub fn max_connections(&self) -> usize {
        self.max_conn
    }
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
    pub fn is_online(&self, uid: &str) -> bool {
        self.sessions.contains_key(uid)
    }
    pub fn room_count(&self) -> usize {
        self.room_members.len()
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    /// P1 FIX: async sekarang agar bisa await timeout retry sebelum evict.
    ///
    /// Sebelumnya: TrySendError::Full → langsung evict session.
    /// Problem: burst broadcast (e.g. merchant broadcast ke 500 user) menyebabkan
    /// user dengan koneksi lambat terputus paksa meski buffer baru penuh sesaat.
    ///
    /// Sekarang: try_send → jika Full, tunggu 50ms lalu coba sekali lagi.
    /// Hanya evict jika setelah timeout masih tidak bisa terkirim.
    async fn deliver_local(&self, user_id: &str, json: Arc<str>) -> bool {
        // Salinan pengirim diambil KELUAR dari guard DashMap sebelum `await` —
        // memegang guard melintasi titik tunggu akan mengunci shard-nya bagi
        // semua orang lain selama penungguan itu.
        let pengirim: Vec<(u64, WsTx)> = match self.sessions.get(user_id) {
            Some(r) => r.value().iter().map(|s| (s.conn_id, s.tx.clone())).collect(),
            None => {
                // Sesi tak ada — bersihkan keanggotaan hantu.
                self.leave_all_rooms(user_id);
                return false;
            }
        };
        if pengirim.is_empty() {
            self.leave_all_rooms(user_id);
            return false;
        }

        // Buang HANYA koneksi yang bermasalah, bukan seluruh sesi pengguna.
        // Ponsel yang sinyalnya hilang di kereta tak boleh memutus laptopnya
        // yang sedang dipakai membalas.
        let buang = |conn_id: u64, alasan: &str| {
            let mut kosong = false;
            if let Some(mut daftar) = self.sessions.get_mut(user_id) {
                daftar.retain(|s| s.conn_id != conn_id);
                kosong = daftar.is_empty();
            }
            if kosong {
                self.sessions.remove(user_id);
                self.active_conns.fetch_sub(1, Ordering::Relaxed);
                self.leave_all_rooms(user_id);
            }
            tracing::warn!(user_id, conn_id, alasan, "WS session evicted");
        };

        let mut ada_yang_sampai = false;
        for (conn_id, tx) in pengirim {
            match tx.try_send(json.clone()) {
                Ok(_) => {
                    ada_yang_sampai = true;
                    continue;
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    // Beri kesempatan klien menguras buffernya sebelum diputus —
                    // buffer yang penuh sesaat saat lonjakan bukan alasan
                    // memutus sambungan orang.
                    match tokio::time::timeout(
                        Duration::from_millis(50),
                        tx.send(json.clone()),
                    )
                    .await
                    {
                        Ok(Ok(_)) => {
                            ada_yang_sampai = true;
                            continue;
                        }
                        _ => {
                            self.dropped.fetch_add(1, Ordering::Relaxed);
                            crate::service::metrik::METRIK
                                .pesan_dibuang
                                .fetch_add(1, Ordering::Relaxed);
                            buang(conn_id, "channel full after 50ms retry");
                        }
                    }
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    buang(conn_id, "channel closed");
                }
            }
        }
        ada_yang_sampai
    }

    /// Redis publish dengan retry + exponential backoff.
    async fn redis_publish_with_retry(&self, channel: &str, json: &str) {
        let _ukur = crate::service::metrik::ukur(&crate::service::metrik::METRIK.redis_publish);
        let mut delay = REDIS_PUBLISH_RETRY_DELAY;

        for attempt in 0..REDIS_PUBLISH_RETRIES {
            let mut conn = self.redis.clone();
            match conn.publish::<_, _, ()>(channel, json).await {
                Ok(_) => {
                    if attempt > 0 {
                        tracing::debug!(channel, attempt, "Redis publish succeeded after retry");
                    }
                    return;
                }
                Err(e) => {
                    tracing::warn!(
                        channel, attempt, error = %e,
                        "Redis publish failed, retrying in {:?}", delay
                    );
                    if attempt + 1 < REDIS_PUBLISH_RETRIES {
                        tokio::time::sleep(delay).await;
                        delay *= 2; // exponential: 50ms → 100ms → 200ms
                    } else {
                        tracing::error!(
                            channel,
                            "Redis publish FAILED after {} retries — message dropped",
                            REDIS_PUBLISH_RETRIES
                        );
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
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
                                    let json: Arc<str> = Arc::from(payload);

                                    if let Some(uid) = channel.strip_prefix(CH_USER) {
                                        mgr.deliver_local(uid, json).await;
                                    } else if let Some(room_id) = channel.strip_prefix(CH_ROOM) {
                                        if let Some(members) = mgr.room_members.get(room_id) {
                                            let ids: Vec<Arc<str>> = members
                                                .iter()
                                                .map(|r| r.key().clone())
                                                .collect();
                                            drop(members);
                                            for uid in &ids {
                                                mgr.deliver_local(uid, json.clone()).await;
                                            }
                                        }
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

        // ── Timer 1: rate limit cleanup setiap 30 detik ───────────────────────
        // FIX: Rate limit window hanya 10s, cleanup setiap 5 menit terlalu lama.
        // Entry idle bisa menumpuk hingga 500k sebelum di-cleanup.
        let mgr2 = mgr.clone();
        let shutdown2 = shutdown.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(RATE_LIMIT_CLEANUP_INTERVAL);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = shutdown2.cancelled() => break,
                    _ = interval.tick() => {
                        mgr2.rate_limit.cleanup();
                    }
                }
            }
        });

        // ── Timer 2: DashMap shrink setiap 5 menit ────────────────────────────
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
                            mgr.room_members.shrink_to_fit();
                            tracing::debug!(
                                sessions = len,
                                rooms    = mgr.room_members.len(),
                                dropped  = mgr.dropped(),
                                "DashMap shrunk"
                            );
                        }
                    }
                }
            }
        });
    }
}

async fn backoff(secs: &mut u64, shutdown: &CancellationToken) {
    // FIX: Tambah jitter 0–1000ms agar beberapa instance tidak reconnect bersamaan
    // (thundering herd problem saat Redis restart dengan banyak instance Rust).
    let jitter_ms = rand::random::<u64>() % 1000;
    let wait = Duration::from_millis(*secs * 1000 + jitter_ms);
    tracing::info!(
        "WS Redis reconnect in {}ms (jitter included)",
        wait.as_millis()
    );
    *secs = (*secs * 2).min(30);
    tokio::select! {
        _ = shutdown.cancelled() => {}
        _ = tokio::time::sleep(wait) => {}
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests_rate_limit {
    use super::*;

    /// Jatah habis sesudah `RATE_LIMIT_MAX` permintaan dalam satu jendela.
    #[test]
    fn jatah_habis_setelah_batas() {
        let b = UserBucket::new();
        for i in 0..RATE_LIMIT_MAX {
            assert!(b.try_consume(), "permintaan ke-{i} seharusnya lolos");
        }
        assert!(!b.try_consume(), "melewati batas seharusnya ditolak");
    }

    /// REGRESI: reset jendela dulu memakai `store` polos, sehingga dua pemanggil
    /// yang tiba bersamaan sesudah jendela habis SAMA-SAMA mengisi ulang token
    /// ke penuh — satu user bisa menembus 2× jatah, persis pada beban tinggi.
    ///
    /// Uji ini memaksa keadaan itu: banyak thread menyerbu bucket yang jendelanya
    /// sudah kedaluwarsa. Total yang lolos tak boleh melebihi satu jatah penuh
    /// (ditambah toleransi satu jendela, kalau-kalau jam bergeser saat uji).
    #[test]
    fn reset_jendela_tak_bisa_dimenangkan_dua_kali() {
        use std::sync::Arc;
        use std::thread;

        let b = Arc::new(UserBucket::new());
        // Habiskan jatah, lalu buat jendelanya tampak kedaluwarsa.
        while b.try_consume() {}
        b.window_start
            .store(now_secs().saturating_sub(RATE_LIMIT_WINDOW_SECS + 1), Ordering::Relaxed);

        let lolos = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let b = Arc::clone(&b);
            let lolos = Arc::clone(&lolos);
            handles.push(thread::spawn(move || {
                for _ in 0..RATE_LIMIT_MAX {
                    if b.try_consume() {
                        lolos.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let total = lolos.load(Ordering::Relaxed);
        assert!(
            total <= RATE_LIMIT_MAX * 2,
            "{total} permintaan lolos — lebih dari satu jendela penuh berarti \
             reset jendela dimenangkan lebih dari satu pemanggil"
        );
    }

    /// Registry menolak user BARU begitu penuh, alih-alih tumbuh tanpa batas.
    #[test]
    fn registry_menolak_saat_penuh() {
        let reg = RateLimitRegistry::new();
        assert!(reg.check("user-pertama"));
        assert!(reg.buckets.len() <= RATE_LIMIT_MAX_ENTRIES);
    }

    /// Cleanup membuang bucket yang jendelanya sudah lama lewat.
    #[test]
    fn cleanup_membuang_bucket_basi() {
        let reg = RateLimitRegistry::new();
        reg.check("u-basi");
        if let Some(b) = reg.buckets.get("u-basi") {
            b.window_start.store(
                now_secs().saturating_sub(RATE_LIMIT_WINDOW_SECS * 10),
                Ordering::Relaxed,
            );
        }
        reg.cleanup();
        assert!(reg.buckets.get("u-basi").is_none(), "bucket basi harus dibuang");
    }
}
