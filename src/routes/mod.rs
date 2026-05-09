//! routes/mod.rs — Router composition

pub mod auth;
pub mod banners;
pub mod events;
pub mod merchant;
pub mod orders;
pub mod tickets;

use std::sync::Arc;

use axum::{
    middleware::from_fn_with_state,
    routing::{delete, get, post, put},
    Router,
};
use tower_http::trace::TraceLayer;

use crate::middleware::auth::require_auth;
use crate::state::AppState;

pub fn build_router(state: Arc<AppState>) -> Router {
    // ── Public routes — no auth required ─────────────────────────────────────
    let public = Router::new()
        .route("/api/health", get(health))
        // auth
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/verify", post(auth::verify_register))
        .route("/api/auth/login", post(auth::login))
        // events (read-only public)
        .route("/api/events/categories", get(events::list_categories))
        .route("/api/events", get(events::list))
        .route("/api/events/{id}", get(events::get_one))
        // banners (public feed)
        .route("/api/banners", get(banners::list_active));

    // ── Protected routes — valid JWT required ─────────────────────────────────
    let protected = Router::new()
        // profile
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/me", put(auth::update_me))
        // merchant profile
        .route("/api/merchant/profile", get(merchant::get_profile))
        .route("/api/merchant/profile", post(merchant::create_profile))
        .route("/api/merchant/profile", put(merchant::update_profile))
        // event management (role: merchant)
        .route("/api/events", post(events::create))
        .route("/api/events/{id}", put(events::update))
        .route("/api/merchant/events", get(events::list_mine))
        // ticket variants
        .route("/api/variants/{id}", put(events::update_variant))
        // orders (role: customer)
        .route("/api/orders", post(orders::create))
        .route("/api/orders", get(orders::list_mine))
        .route("/api/orders/{id}", get(orders::get_one))
        .route("/api/orders/{id}/pay", post(orders::pay))
        .route("/api/orders/{id}/cancel", post(orders::cancel))
        // tickets (role: customer)
        .route("/api/tickets", get(tickets::list_mine))
        .route("/api/tickets/{id}", get(tickets::get_one))
        .route("/api/tickets/validate", post(tickets::validate))
        // ── Admin: banner management ─────────────────────────────────────────
        // Semua handler di bawah memanggil user.require_role("admin") secara internal.
        .route("/api/admin/banners", post(banners::admin_create))
        .route("/api/admin/banners/{id}", put(banners::admin_update))
        .route("/api/admin/banners/{id}", delete(banners::admin_delete))
        // ── Admin: event management ──────────────────────────────────────────
        // GET /api/admin/events       — list semua event (bisa filter by status)
        // PUT /api/admin/events/:id/status — update status event (approve/reject)
        .route("/api/admin/events", get(events::admin_list_events))
        .route(
            "/api/admin/events/{id}/status",
            put(events::admin_update_status),
        )
        // Apply JWT middleware ke seluruh protected group
        .route_layer(from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .merge(public)
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health() -> &'static str {
    "/ok"
}
