//! api/mod.rs — REST API router untuk Next.js frontend.
//!
//! Semua endpoint di-prefix /api/* dan di-merge ke main Axum router.
//! Auth: Bearer JWT di header Authorization (kecuali endpoint public).
//!
//! Endpoint summary:
//!   Auth (public):
//!     POST /api/auth/login
//!     POST /api/auth/register
//!     POST /api/auth/verify-register
//!     POST /api/auth/forgot-password
//!     POST /api/auth/refresh
//!     POST /api/auth/logout
//!
//!   Products & Banners (public):
//!     GET  /api/products
//!     GET  /api/products/:slug
//!     GET  /api/products/:slug/location
//!     GET  /api/banners
//!
//!   Cart & Payment & Checkout (private):
//!     GET    /api/cart/view
//!     GET    /api/cart/count
//!     POST   /api/cart/create
//!     POST   /api/cart/add
//!     PUT    /api/cart/quantity
//!     DELETE /api/cart/item/:variant_id
//!     DELETE /api/cart/clear
//!     POST   /api/cart/promo
//!     POST   /api/cart/payment
//!     GET    /api/payments
//!     POST   /api/checkout
//!     POST   /api/orders/:id/pay
//!     POST   /api/orders/:id/cancel
//!
//!   Orders & Tickets & Promos (private):
//!     GET  /api/orders
//!     POST /api/orders
//!     GET  /api/orders/:id
//!     GET  /api/orders/:id/tickets
//!     GET  /api/tickets
//!     GET  /api/tickets/:id
//!     POST /api/promos/validate
//!
//!   Notifications (private):
//!     GET  /api/notifications
//!     POST /api/notifications/:id/read
//!     POST /api/notifications/read-all
//!
//!   Merchant (private):
//!     GET  /api/merchant/products
//!     GET  /api/merchant/products/:slug
//!
//!   Admin (private, role=admin):
//!     GET  /api/admin/products
//!     PUT  /api/admin/products/:id
//!
//!   Stories (private):
//!     GET    /api/stories
//!     POST   /api/stories/:id/view
//!     DELETE /api/stories/:id
//!     POST   /api/stories       (multipart — via /upload/story alias)
//!
//!   Subscriptions (private):
//!     POST /api/subscriptions/order   — buat subscription order (weekly/monthly/yearly/lifetime)
//!     GET  /api/subscriptions/status  — cek status premium user
//!
//!   Chat & WebSocket (handled in ws/routes.rs):
//!     GET  /api/chat/rooms
//!     GET  /api/chat/products/:event_id/room
//!     POST /api/chat/rooms/:room_id/join
//!     GET  /api/chat/rooms/:room_id/history
//!     GET  /api/chat/rooms/:room_id/sent_count
//!     WS   /api/ws/chat?token=<JWT>

use axum::Router;
use std::sync::Arc;

use crate::state::AppState;

mod extractor;
mod auth;
mod cart;
mod products;
mod orders;
mod notifications;
mod merchant;
mod admin;
mod stories;
mod subscriptions;

pub fn rest_router() -> Router<Arc<AppState>> {
    Router::new()
        .nest(
            "/api",
            Router::new()
                .merge(auth::router())
                .merge(products::router())
                .merge(orders::router())
                .merge(cart::router())
                .merge(notifications::router())
                .merge(merchant::router())
                .merge(admin::router())
                .merge(stories::router())
                .merge(subscriptions::router()),
        )
}
