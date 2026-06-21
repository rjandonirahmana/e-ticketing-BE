//! api/merchant.rs
//!
//! GET  /api/merchant/events      (private, query: page)
//! GET  /api/merchant/events/:slug (private)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::state::AppState;
use super::extractor::{app_err, AuthUser};

#[derive(Deserialize, Default)]
pub struct PageQuery {
    pub page: Option<i64>,
}

async fn list_merchant_events(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<PageQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    use crate::models::events::EventListQuery;
    let query = EventListQuery {
        page: q.page,
        per_page: Some(20),
        city: None,
        category: None,
        search: None,
        status: None,
    };
    let result = state
        .event_svc
        .list(query, Some(&claims.user_id))
        .await
        .map_err(app_err)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

async fn get_merchant_event(
    AuthUser(_claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let event = state.event_svc.get(&slug).await.map_err(app_err)?;
    Ok(Json(serde_json::to_value(event).unwrap_or_default()))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/merchant/events", get(list_merchant_events))
        .route("/merchant/events/{slug}", get(get_merchant_event))
}
