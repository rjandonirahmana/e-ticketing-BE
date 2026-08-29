//! state.rs — AppState: shared application state untuk seluruh handler.
//!
//! UPDATED: tambah StoryService + PgStoryRepository

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

use crate::config::config::{RustFsConfig, WahaConfig};
use crate::repository::{
    banner::PgBannerRepository, cart::PgCartRepository, product::PgProductRepository,
    group_chat::PgGroupChatRepository, merchant::PgMerchantRepository,
    notification::PgNotificationRepository, order::PgOrderRepository,
    payment::PgPaymentRepository, refresh_token::PgRefreshTokenRepository,
    story::PgStoryRepository, ticket::PgTicketRepository,
    user::PgUserRepository,
};
use crate::service::affinity::AffinityService;
use crate::service::norifications::NotificationService;
use crate::service::notification_store::NotificationStoreService;
use crate::service::{
    auth::AuthService, banners::BannerService, cart::CartService, product::ProductService,
    group_chat::GroupChatService, merchant::MerchantService, order::OrderService,
    payment::PaymentService, refresh::RefreshService, storage::StorageService,
    story::StoryService, ticket::TicketService,
};
use crate::live::LiveStreamService;
use crate::meet::MeetService;
use crate::utils::jwt::JwtService;
use crate::ws::manager::WsManager;
use deadpool_postgres::Pool;
use moka::future::Cache;
use redis::aio::ConnectionManager;
use reqwest::Client as HttpClient;

pub type DefaultBannerSvc = BannerService<PgBannerRepository>;
pub type DefaultStorySvc = StoryService<PgStoryRepository>;

/// In-process TTL cache untuk data publik yang jarang berubah.
/// Mencegah DB hit berulang per SSR request pada data statis.
pub struct PublicCache {
    pub banners: Cache<(), Vec<crate::models::banners::Banner>>,
    pub categories: Cache<(), Vec<String>>,
    /// Key: canonical query string (page|city|category|search|per_page).
    /// 30 s TTL — cukup untuk meredam burst traffic tanpa data stale terasa.
    pub products: Cache<String, crate::web::models::PaginatedProducts>,
    /// Key: product slug. 60 s TTL — product detail jarang berubah.
    pub product_detail: Cache<String, crate::web::models::ProductWithVariants>,
    /// Key: merchant_id. Profil publik /m/{id} — sub-query TERBERAT di halaman
    /// (followers + products_count + rating agg). Viewer-invariant (is_following
    /// dihitung terpisah per-viewer), jadi aman di-cache. 60 s TTL — konsisten
    /// dgn product_detail; follower/rating boleh stale sesaat saat traffic tinggi.
    pub merchant_profile: Cache<String, crate::models::merchant::MerchantPublicProfile>,
    /// Key: merchant_id (== user_id pemilik). Grup story profil merchant —
    /// dipakai story-ring di SETIAP buka product detail DAN panel STORY /m/{id}.
    /// Viewer-invariant (`list_my_group` mengembalikan `viewed`=FALSE konstan),
    /// jadi aman lintas-viewer. TTL 30 s (story lebih dinamis dari profil):
    /// story baru muncul ≤30 s. Menghapus 1 query DB per buka detail.
    pub merchant_stories: Cache<String, Vec<crate::web::state::stories::StoryGroup>>,
    /// Respons JSON REST publik (/api/products*, /api/banners) yang SUDAH
    /// terserialisasi, sebagai `Bytes` (clone murah, langsung jadi body).
    /// Tanpa ini setiap request REST = query DB + serialisasi ulang.
    pub rest: Cache<String, bytes::Bytes>,
}

impl PublicCache {
    pub fn new() -> Self {
        Self {
            banners: Cache::builder()
                .max_capacity(1)
                .time_to_live(Duration::from_secs(60))
                .build(),
            categories: Cache::builder()
                .max_capacity(1)
                .time_to_live(Duration::from_secs(300))
                .build(),
            products: Cache::builder()
                .max_capacity(256)
                .time_to_live(Duration::from_secs(30))
                .build(),
            product_detail: Cache::builder()
                .max_capacity(512)
                .time_to_live(Duration::from_secs(60))
                .build(),
            merchant_profile: Cache::builder()
                .max_capacity(512)
                .time_to_live(Duration::from_secs(60))
                .build(),
            merchant_stories: Cache::builder()
                .max_capacity(512)
                .time_to_live(Duration::from_secs(30))
                .build(),
            // ── DIBATASI BYTE, BUKAN JUMLAH ENTRI ────────────────────────
            //
            // `max_capacity` di moka menghitung ENTRI, kecuali diberi
            // `weigher`. Untuk cache yang isinya seragam kecil itu tak masalah;
            // di sini isinya respons JSON yang sudah terserialisasi, dan
            // ukurannya sangat timpang — `/api/banners` beberapa ratus byte,
            // satu halaman `/api/products` berisi 20 produk lengkap dengan
            // deskripsi bisa ratusan kilobyte.
            //
            // Dengan plafon 1024 ENTRI, batas atas memakan tempat yang
            // sebenarnya tak pernah dinyatakan siapa pun: seribu entri gemuk
            // adalah ratusan megabyte, di kotak yang totalnya 4 GB dan juga
            // harus memuat pool Postgres, WebSocket, dan SFU.
            //
            // Yang membuatnya sulit terlihat: cache ini bekerja persis seperti
            // seharusnya sampai pola query cukup beragam untuk mengisi seribu
            // slot — jadi ia tak pernah bermasalah saat diuji, hanya saat
            // ramai.
            //
            // `weigher` mengubah satuannya menjadi byte nyata, dan 32 MB adalah
            // anggaran yang bisa dinalar: cukup menampung puluhan kombinasi
            // query terpanas, dan tak bisa membesar melewatinya apa pun yang
            // terjadi.
            rest: Cache::builder()
                .max_capacity(32 * 1024 * 1024)
                .weigher(|_k: &String, v: &bytes::Bytes| v.len().min(u32::MAX as usize) as u32)
                .time_to_live(Duration::from_secs(30))
                .build(),
        }
    }

    /// Buang setiap entri cache yang bisa memuat versi lama sebuah product.
    ///
    /// Tanpa ini, TTL-lah satu-satunya yang menyegarkan: sesudah merchant
    /// menekan SIMPAN, halaman publik dan REST tetap menyajikan versi lama
    /// sampai 30–60 detik. Bagi yang baru saja menyimpan, itu tak terbaca
    /// sebagai "cache" melainkan sebagai "perubahan saya tidak masuk" — lalu
    /// ia menyimpan ulang berkali-kali, dan setiap kali tampak gagal lagi.
    ///
    /// Daftar (`products`, dan kunci `products|…` di `rest`) di-cache per kombinasi
    /// query, jadi tak ada cara menyasar hanya yang memuat product ini —
    /// seluruhnya dibuang. Ongkosnya satu query ulang per kombinasi yang masih
    /// aktif; jauh lebih murah daripada data yang salah.
    pub async fn invalidate_product(&self, slug: &str, merchant_id: &str) {
        self.product_detail.invalidate(slug).await;
        self.rest.invalidate(&format!("product|{slug}")).await;
        self.rest.invalidate(&format!("loc|{slug}")).await;
        self.products.invalidate_all();
        self.rest.invalidate_all();
        // Kategori bisa bertambah/hilang saat product diubah.
        self.categories.invalidate(&()).await;
        // Profil publik merchant memuat `products_count` (hanya status 'active'),
        // yang ikut berubah begitu status product berpindah.
        self.merchant_profile.invalidate(merchant_id).await;
    }
}

pub struct AppState {
    #[allow(dead_code)]
    pub pool: Pool,
    pub jwt: JwtService,
    pub internal_jwt_secret: String,

    pub auth_svc: Arc<AuthService>,
    /// Daur hidup refresh token: penerbitan, rotasi, pencabutan.
    pub refresh_svc: Arc<RefreshService>,
    pub merchant_svc: Arc<MerchantService>,
    pub product_svc: Arc<ProductService>,
    pub order_svc: Arc<OrderService>,
    /// Keranjang belanja DB-backed (menggantikan localStorage browser).
    pub cart_svc: Arc<CartService>,
    /// Kanal pembayaran & kode promo — datanya di tabel, bukan konstanta kode.
    pub payment_svc: Arc<PaymentService>,
    pub ticket_svc: Arc<TicketService>,
    pub group_chat_svc: Arc<GroupChatService>,
    pub ws_mgr: Arc<WsManager>,
    pub storage: Arc<StorageService>,
    pub banner_svc: Arc<DefaultBannerSvc>,
    pub notification_store_svc: Arc<NotificationStoreService>,
    /// Service untuk story & premium subscription.
    pub story_svc: Arc<DefaultStorySvc>,
    /// In-process cache untuk data publik (banners, categories).
    pub pub_cache: Arc<PublicCache>,
    /// Live streaming service (WebRTC SFU).
    pub live_svc: Arc<LiveStreamService>,
    /// Meet service (konferensi P2P mesh, signaling + waiting room).
    pub meet_svc: Arc<MeetService>,
    /// Behavior tracking (afinitas kategori): buffer in-memory + batch flush.
    pub affinity_svc: Arc<AffinityService>,
    /// Plafon upload media serentak (auto-skala dari kapasitas VPS). Permit
    /// diambil di handler upload; penuh → 503 (fail-fast).
    pub upload_limit: Arc<Semaphore>,
    /// Direktori file temp untuk streaming upload. Harus disk-backed (bukan
    /// tmpfs) agar streaming benar-benar mengurangi RAM. Divalidasi saat startup.
    pub upload_tmp_dir: PathBuf,
    /// CPU/RAM efektif VPS + plafon turunannya (deteksi saat start).
    pub capacity: crate::utils::capacity::Capacity,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        pool: Pool,
        jwt_secret: &str,
        internal_jwt_secret: String,
        bcrypt_cost: u32,
        jwt_expiry_hours: i64,
        waha: Arc<WahaConfig>,
        redis: ConnectionManager,
        redis_client: redis::Client,
        rustfs: RustFsConfig,
        sfu_bind_addr: String,
        upload_tmp_dir: PathBuf,
        capacity: crate::utils::capacity::Capacity,
    ) -> Self {
        let http = HttpClient::builder()
            .pool_idle_timeout(Some(Duration::from_secs(30)))
            .timeout(Duration::from_secs(15))
            .build()
            .expect("http client");

        let jwt = JwtService::new(jwt_secret, jwt_expiry_hours);

        // ── Repositories ──────────────────────────────────────────────────────
        let user_repo = Arc::new(PgUserRepository::new(pool.clone()));
        let refresh_repo = Arc::new(PgRefreshTokenRepository::new(pool.clone()));
        let banner_repo = Arc::new(PgBannerRepository::new(pool.clone()));
        let merchant_repo = Arc::new(PgMerchantRepository::new(pool.clone()));
        let product_repo = Arc::new(PgProductRepository::new(pool.clone()));
        let order_repo = Arc::new(PgOrderRepository::new(pool.clone()));
        let cart_repo = Arc::new(PgCartRepository::new(pool.clone()));
        let payment_repo = Arc::new(PgPaymentRepository::new(pool.clone()));
        let ticket_repo = Arc::new(PgTicketRepository::new(pool.clone()));
        let group_chat_repo = Arc::new(PgGroupChatRepository::new(pool.clone()));
        let notification_repo = Arc::new(PgNotificationRepository::new(pool.clone()));
        let story_repo = Arc::new(PgStoryRepository::new(pool.clone())); // ← NEW

        // ── WS Manager ────────────────────────────────────────────────────────
        let ws_mgr = WsManager::new(redis_client, capacity.recommended_max_ws)
            .await
            .expect("WsManager init failed");

        // ── Services ──────────────────────────────────────────────────────────
        let auth_svc = Arc::new(AuthService::new(
            user_repo.clone(),
            jwt.clone(),
            bcrypt_cost,
            // `jwt_expiry_hours` tak lagi dioper terpisah: JwtService yang
            // memegangnya, dan `expires_in` diambil dari sana — supaya angka
            // yang dijanjikan ke klien mustahil berbeda dari klaim `exp`.
            waha.clone(),
            redis.clone(),
        ));
        let refresh_svc = Arc::new(RefreshService::new(
            refresh_repo,
            user_repo.clone(),
            jwt.clone(),
        ));
        let notif_service = Arc::new(NotificationService::new(
            http,
            waha,
            user_repo,
            redis.clone(),
        ));
        let merchant_svc = Arc::new(MerchantService::new(merchant_repo));
        let product_svc = Arc::new(ProductService::new(product_repo));
        let notification_store_svc = Arc::new(NotificationStoreService::new(notification_repo));
        let ticket_svc = Arc::new(TicketService::new(ticket_repo.clone()));
        let group_chat_svc = Arc::new(GroupChatService::new(group_chat_repo, ws_mgr.clone()));
        // Kanal pembayaran & keranjang dibuat SEBELUM order: checkout membaca
        // keranjang dan menghitung biaya kanal dari sana.
        let payment_svc = Arc::new(PaymentService::new(payment_repo));
        let cart_svc = Arc::new(CartService::new(cart_repo, payment_svc.clone()));
        let order_svc = Arc::new(OrderService::new(
            order_repo,
            redis,
            pool.clone(),
            notif_service,
            notification_store_svc.clone(),
            ticket_repo,
            group_chat_svc.clone(),
            cart_svc.clone(),
            payment_svc.clone(),
        ));
        let banner_svc = Arc::new(BannerService::new(banner_repo));
        let storage = Arc::new(StorageService::new(&rustfs));

        let story_svc = Arc::new(StoryService::new(
            story_repo,
            storage.clone(),
            notification_store_svc.clone(),
        ));

        let sfu_addr: std::net::SocketAddr = sfu_bind_addr
            .parse()
            .unwrap_or_else(|_| "0.0.0.0:4000".parse().expect("default SFU addr"));
        let live_svc = LiveStreamService::new(sfu_addr);
        let meet_svc = MeetService::new();
        let affinity_svc = AffinityService::new(pool.clone());
        let upload_limit = Arc::new(Semaphore::new(capacity.recommended_upload_concurrency));

        Self {
            pool,
            jwt,
            internal_jwt_secret,
            auth_svc,
            refresh_svc,
            merchant_svc,
            product_svc,
            order_svc,
            cart_svc,
            payment_svc,
            ticket_svc,
            group_chat_svc,
            ws_mgr,
            storage,
            banner_svc,
            notification_store_svc,
            story_svc,
            pub_cache: Arc::new(PublicCache::new()),
            live_svc,
            meet_svc,
            affinity_svc,
            upload_limit,
            upload_tmp_dir,
            capacity,
        }
    }
}