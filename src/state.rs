//! state.rs — AppState: shared application state untuk seluruh handler.
//!
//! Desain generics:
//! - `BannerService<R: BannerRepository>` di-monomorphize ke `DefaultBannerSvc`
//!   (alias konkret) pada layer ini.
//! - Layer bawah (repository, service) tetap generic → zero-cost, full inlining.
//! - `AppState` sendiri tidak perlu generic parameter sehingga Axum State extractor
//!   dan semua route handler bisa pakai `Arc<AppState>` tanpa turbofish.

use std::sync::Arc;
use std::time::Duration;

use crate::config::config::{RustFsConfig, WahaConfig};
use crate::repository::{
    banner::PgBannerRepository, event::PgEventRepository, group_chat::PgGroupChatRepository,
    merchant::PgMerchantRepository, order::PgOrderRepository, ticket::PgTicketRepository,
    user::PgUserRepository,
};
use crate::service::norifications::NotificationService;
use crate::service::{
    auth::AuthService, banners::BannerService, event::EventService, group_chat::GroupChatService,
    merchant::MerchantService, order::OrderService, storage::StorageService, ticket::TicketService,
};
use crate::utils::jwt::JwtService;
use crate::ws::manager::WsManager;
use deadpool_postgres::Pool;
use redis::aio::ConnectionManager;
use reqwest::Client as HttpClient;

// ── Type alias ────────────────────────────────────────────────────────────────

/// Alias konkret untuk BannerService yang di-pakai di production.
/// Generic parameter `R = PgBannerRepository` di-monomorphize di sini:
/// compiler menghasilkan kode spesifik tanpa vtable.
pub type DefaultBannerSvc = BannerService<PgBannerRepository>;

// ── AppState ──────────────────────────────────────────────────────────────────

pub struct AppState {
    #[allow(dead_code)]
    pub pool: Pool,
    pub jwt: JwtService,

    pub auth_svc: Arc<AuthService>,
    pub merchant_svc: Arc<MerchantService>,
    pub event_svc: Arc<EventService>,
    pub order_svc: Arc<OrderService>,
    pub ticket_svc: Arc<TicketService>,
    pub group_chat_svc: Arc<GroupChatService>,
    pub ws_mgr: Arc<WsManager>,
    pub storage: Arc<StorageService>,
    /// Banner service — monomorphized ke `BannerService<PgBannerRepository>`.
    pub banner_svc: Arc<DefaultBannerSvc>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        pool: Pool,
        jwt_secret: &str,
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
        // FIX: NotificationService kini menerima Redis untuk dedup WA notification.
        // redis.clone() murah — ConnectionManager adalah Arc internal.
        let notif_service = Arc::new(NotificationService::new(
            http,
            waha,
            user_repo,
            redis.clone(),
        ));
        let merchant_svc = Arc::new(MerchantService::new(merchant_repo));
        let event_svc = Arc::new(EventService::new(event_repo));

        let ticket_svc = Arc::new(TicketService::new(ticket_repo));
        let group_chat_svc = Arc::new(GroupChatService::new(group_chat_repo, ws_mgr.clone()));

        let order_svc = Arc::new(OrderService::new(
            order_repo,
            redis,
            pool.clone(),
            notif_service,
            group_chat_svc.clone(), // FIX: inject group_svc untuk auto-join after payment
        ));
        // BannerService<PgBannerRepository> — dikoncretkan via type alias DefaultBannerSvc
        let banner_svc = Arc::new(BannerService::new(banner_repo));

        let storage = Arc::new(StorageService::new(&rustfs));

        // Health check storage — log warning tapi tidak panic (storage mungkin lazy-start)
        let _ = storage.init().await.map_err(|e| {
            tracing::error!("Storage init failed: {:?}", e);
            e
        });
        // FIX: check_health kini return AppResult — log error jelas jika bucket tidak accessible
        if let Err(e) = storage.check_health().await {
            tracing::error!("❌ RustFS health check failed at startup: {:?}", e);
            // Tidak panic — app tetap berjalan, upload endpoint akan return error jika dipanggil
        }

        Self {
            pool,
            jwt,
            auth_svc,
            merchant_svc,
            event_svc,
            order_svc,
            ticket_svc,
            group_chat_svc,
            ws_mgr,
            storage,
            banner_svc,
        }
    }
}
