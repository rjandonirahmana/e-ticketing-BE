pub mod auth;
pub mod events;
pub mod merchant;
pub mod orders;
pub mod tickets;

use std::sync::Arc;

use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{get, post, put},
};
use tower_http::trace::TraceLayer;

use crate::middleware::auth::require_auth;
use crate::state::AppState;

pub fn build_router(state: Arc<AppState>) -> Router {
    // Public routes — no auth required
    let public = Router::new()
        .route("/api/health", get(health))
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/verify", post(auth::verify_register))
        .route("/api/auth/login", post(auth::login))
        .route("/api/events/categories", get(events::list_categories))
        .route("/api/events", get(events::list))
        .route("/api/events/{id}", get(events::get_one));

    // Protected routes — JWT required
    let protected = Router::new()
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/me", put(auth::update_me))
        // merchant profile
        .route("/api/merchant/profile", get(merchant::get_profile))
        .route("/api/merchant/profile", post(merchant::create_profile))
        .route("/api/merchant/profile", put(merchant::update_profile))
        // event management (merchant)
        .route("/api/events", post(events::create))
        .route("/api/events/{id}", put(events::update))
        .route("/api/merchant/events", get(events::list_mine))
        // ticket variants
        .route("/api/variants/{id}", put(events::update_variant))
        // orders (customer)
        .route("/api/orders", post(orders::create))
        .route("/api/orders", get(orders::list_mine))
        .route("/api/orders/{id}", get(orders::get_one))
        .route("/api/orders/{id}/pay", post(orders::pay))
        .route("/api/orders/{id}/cancel", post(orders::cancel))
        // tickets
        .route("/api/tickets", get(tickets::list_mine))
        .route("/api/tickets/{id}", get(tickets::get_one))
        .route("/api/tickets/validate", post(tickets::validate))
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
