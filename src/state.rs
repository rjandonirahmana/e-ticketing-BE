//! state.rs — AppState: shared application state untuk seluruh handler.
//!
//! UPDATED: tambah StoryService + PgStoryRepository

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

use crate::config::config::{RustFsConfig, WahaConfig};
use crate::repository::{
    banner::PgBannerRepository, event::PgEventRepository, group_chat::PgGroupChatRepository,
    merchant::PgMerchantRepository, notification::PgNotificationRepository,
    order::PgOrderRepository, story::PgStoryRepository, ticket::PgTicketRepository,
    user::PgUserRepository,
};
use crate::service::affinity::AffinityService;
use crate::service::norifications::NotificationService;
use crate::service::notification_store::NotificationStoreService;
use crate::service::{
    auth::AuthService, banners::BannerService, event::EventService, group_chat::GroupChatService,
    merchant::MerchantService, order::OrderService, storage::StorageService, story::StoryService,
    ticket::TicketService,
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
    pub events: Cache<String, crate::web::models::PaginatedEvents>,
    /// Key: event slug. 60 s TTL — event detail jarang berubah.
    pub event_detail: Cache<String, crate::web::models::EventWithVariants>,
    /// Key: merchant_id. Profil publik /m/{id} — sub-query TERBERAT di halaman
    /// (followers + events_count + rating agg). Viewer-invariant (is_following
    /// dihitung terpisah per-viewer), jadi aman di-cache. 60 s TTL — konsisten
    /// dgn event_detail; follower/rating boleh stale sesaat saat traffic tinggi.
    pub merchant_profile: Cache<String, crate::models::merchant::MerchantPublicProfile>,
    /// Key: merchant_id (== user_id pemilik). Grup story profil merchant —
    /// dipakai story-ring di SETIAP buka event detail DAN panel STORY /m/{id}.
    /// Viewer-invariant (`list_my_group` mengembalikan `viewed`=FALSE konstan),
    /// jadi aman lintas-viewer. TTL 30 s (story lebih dinamis dari profil):
    /// story baru muncul ≤30 s. Menghapus 1 query DB per buka detail.
    pub merchant_stories: Cache<String, Vec<crate::web::state::stories::StoryGroup>>,
    /// Respons JSON REST publik (/api/events*, /api/banners) yang SUDAH
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
            events: Cache::builder()
                .max_capacity(256)
                .time_to_live(Duration::from_secs(30))
                .build(),
            event_detail: Cache::builder()
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
            rest: Cache::builder()
                .max_capacity(1024)
                .time_to_live(Duration::from_secs(30))
                .build(),
        }
    }
}

pub struct AppState {
    #[allow(dead_code)]
    pub pool: Pool,
    pub jwt: JwtService,
    pub internal_jwt_secret: String,

    pub auth_svc: Arc<AuthService>,
    pub merchant_svc: Arc<MerchantService>,
    pub event_svc: Arc<EventService>,
    pub order_svc: Arc<OrderService>,
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

        let jwt = JwtService::new(jwt_secret);

        // ── Repositories ──────────────────────────────────────────────────────
        let user_repo = Arc::new(PgUserRepository::new(pool.clone()));
        let banner_repo = Arc::new(PgBannerRepository::new(pool.clone()));
        let merchant_repo = Arc::new(PgMerchantRepository::new(pool.clone()));
        let event_repo = Arc::new(PgEventRepository::new(pool.clone()));
        let order_repo = Arc::new(PgOrderRepository::new(pool.clone()));
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
            jwt_expiry_hours,
            waha.clone(),
            redis.clone(),
        ));
        let notif_service = Arc::new(NotificationService::new(
            http,
            waha,
            user_repo,
            redis.clone(),
        ));
        let merchant_svc = Arc::new(MerchantService::new(merchant_repo));
        let event_svc = Arc::new(EventService::new(event_repo));
        let notification_store_svc = Arc::new(NotificationStoreService::new(notification_repo));
        let ticket_svc = Arc::new(TicketService::new(ticket_repo.clone()));
        let group_chat_svc = Arc::new(GroupChatService::new(group_chat_repo, ws_mgr.clone()));
        let order_svc = Arc::new(OrderService::new(
            order_repo,
            redis,
            pool.clone(),
            notif_service,
            notification_store_svc.clone(),
            ticket_repo,
            group_chat_svc.clone(),
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
            merchant_svc,
            event_svc,
            order_svc,
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