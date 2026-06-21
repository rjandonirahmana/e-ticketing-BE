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
//!   Events & Banners (public):
//!     GET  /api/events
//!     GET  /api/events/:slug
//!     GET  /api/events/:slug/location
//!     GET  /api/banners
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
//!     GET  /api/merchant/events
//!     GET  /api/merchant/events/:slug
//!
//!   Admin (private, role=admin):
//!     GET  /api/admin/events
//!     PUT  /api/admin/events/:id
//!
//!   Stories (private):
//!     GET    /api/stories
//!     POST   /api/stories/:id/view
//!     DELETE /api/stories/:id
//!     POST   /api/stories       (multipart — via /upload/story alias)
//!
//!   Chat & WebSocket (handled in ws/routes.rs):
//!     GET  /api/chat/rooms
//!     GET  /api/chat/events/:event_id/room
//!     POST /api/chat/rooms/:room_id/join
//!     GET  /api/chat/rooms/:room_id/history
//!     GET  /api/chat/rooms/:room_id/sent_count
//!     WS   /api/ws/chat?token=<JWT>

use axum::Router;
use std::sync::Arc;

use crate::state::AppState;

mod extractor;
mod auth;
mod events;
mod orders;
mod notifications;
mod merchant;
mod admin;
mod stories;

pub fn rest_router() -> Router<Arc<AppState>> {
    Router::new()
        .nest(
            "/api",
            Router::new()
                .merge(auth::router())
                .merge(events::router())
                .merge(orders::router())
                .merge(notifications::router())
                .merge(merchant::router())
                .merge(admin::router())
                .merge(stories::router()),
        )
}
