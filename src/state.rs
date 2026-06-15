//! state.rs — AppState: shared application state untuk seluruh handler.
//!
//! UPDATED: tambah StoryService + PgStoryRepository

use std::sync::Arc;
use std::time::Duration;

use crate::config::config::{RustFsConfig, WahaConfig};
use crate::repository::{
    banner::PgBannerRepository, event::PgEventRepository, group_chat::PgGroupChatRepository,
    merchant::PgMerchantRepository, notification::PgNotificationRepository,
    order::PgOrderRepository, story::PgStoryRepository, ticket::PgTicketRepository,
    user::PgUserRepository,
};
use crate::service::norifications::NotificationService;
use crate::service::notification_store::NotificationStoreService;
use crate::service::{
    auth::AuthService, banners::BannerService, event::EventService, group_chat::GroupChatService,
    merchant::MerchantService, order::OrderService, storage::StorageService, story::StoryService,
    ticket::TicketService,
};
use crate::utils::jwt::JwtService;
use crate::ws::manager::WsManager;
use deadpool_postgres::Pool;
use redis::aio::ConnectionManager;
use reqwest::Client as HttpClient;

pub type DefaultBannerSvc = BannerService<PgBannerRepository>;
pub type DefaultStorySvc = StoryService<PgStoryRepository>;

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
        let ws_mgr = WsManager::new(redis_client)
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
        )); // ← NEW



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
            story_svc, // ← NEW
        }
    }
}