pub mod auth;
pub mod events;
pub mod merchant;
pub mod orders;
pub mod tickets;

use std::sync::Arc;

use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{delete, get, post, put},
};
use tower_http::trace::TraceLayer;

use crate::middleware::auth::require_auth;
use crate::state::AppState;

pub fn build_router(state: Arc<AppState>) -> Router {
    // Public routes — no auth required
    let public = Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(auth::register))
        .route("/auth/verify", post(auth::verify_register))
        .route("/auth/login", post(auth::login))
        .route("/events", get(events::list))
        .route("/events/{id}", get(events::get_one));

    // Protected routes — JWT required
    let protected = Router::new()
        .route("/auth/me", get(auth::me))
        .route("/auth/me", put(auth::update_me))
        // merchant profile
        .route("/merchant/profile", get(merchant::get_profile))
        .route("/merchant/profile", post(merchant::create_profile))
        .route("/merchant/profile", put(merchant::update_profile))
        // event management (merchant)
        .route("/events", post(events::create))
        .route("/events/{id}", put(events::update))
        .route("/events/{id}", delete(events::delete_event))
        .route("/merchant/events", get(events::list_mine))
        // ticket variants
        .route("/events/{id}/variants", post(events::create_variant))
        .route("/variants/{id}", put(events::update_variant))
        .route("/variants/{id}", delete(events::delete_variant))
        // orders (customer)
        .route("/orders", post(orders::create))
        .route("/orders", get(orders::list_mine))
        .route("/orders/{id}", get(orders::get_one))
        .route("/orders/{id}/pay", post(orders::pay))
        .route("/orders/{id}/cancel", post(orders::cancel))
        // tickets
        .route("/tickets", get(tickets::list_mine))
        .route("/tickets/{id}", get(tickets::get_one))
        .route("/tickets/validate", post(tickets::validate))
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
