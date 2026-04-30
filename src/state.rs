use std::sync::Arc;

use deadpool_postgres::Pool;
use redis::aio::ConnectionManager;

use crate::config::config::WahaConfig;
use crate::repository::{
    event::PgEventRepository, group_chat::GroupChatRepository, merchant::PgMerchantRepository,
    order::PgOrderRepository, ticket::PgTicketRepository, user::PgUserRepository,
};
use crate::service::{
    auth::AuthService, event::EventService, group_chat::GroupChatService,
    merchant::MerchantService, order::OrderService, ticket::TicketService,
};
use crate::utils::jwt::JwtService;
use crate::ws::manager::WsManager;

pub struct AppState {
    #[allow(dead_code)]
    pub pool: Pool,
    pub jwt: JwtService,

    pub auth_svc:       Arc<AuthService>,
    pub merchant_svc:   Arc<MerchantService>,
    pub event_svc:      Arc<EventService>,
    pub order_svc:      Arc<OrderService>,
    pub ticket_svc:     Arc<TicketService>,
    pub group_chat_svc: Arc<GroupChatService>,
    pub ws_mgr:         Arc<WsManager>,
}

impl AppState {
    pub fn new(
        pool:             Pool,
        jwt_secret:       &str,
        bcrypt_cost:      u32,
        jwt_expiry_hours: i64,
        waha:             Arc<WahaConfig>,
        redis:            ConnectionManager,
        redis_client:     redis::Client,
    ) -> Self {
        let jwt = JwtService::new(jwt_secret);

        let user_repo       = Arc::new(PgUserRepository::new(pool.clone()));
        let merchant_repo   = Arc::new(PgMerchantRepository::new(pool.clone()));
        let event_repo      = Arc::new(PgEventRepository::new(pool.clone()));
        let order_repo      = Arc::new(PgOrderRepository::new(pool.clone()));
        let ticket_repo     = Arc::new(PgTicketRepository::new(pool.clone()));
        let group_chat_repo = Arc::new(GroupChatRepository::new(pool.clone()));

        let ws_mgr = WsManager::new(redis_client);

        let auth_svc     = Arc::new(AuthService::new(user_repo.clone(), jwt.clone(), bcrypt_cost, jwt_expiry_hours, waha, redis));
        let merchant_svc = Arc::new(MerchantService::new(merchant_repo));
        let event_svc    = Arc::new(EventService::new(event_repo));
        let order_svc    = Arc::new(OrderService::new(order_repo));
        let ticket_svc   = Arc::new(TicketService::new(ticket_repo));
        let group_chat_svc = Arc::new(GroupChatService::new(group_chat_repo, ws_mgr.clone()));

        Self { pool, jwt, auth_svc, merchant_svc, event_svc, order_svc, ticket_svc, group_chat_svc, ws_mgr }
    }
}
