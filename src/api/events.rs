//! api/events.rs — Events & Banners REST endpoints.
//!
//! GET  /api/events               (public, query: query, category, city, page, page_size)
//! GET  /api/events/:slug         (public)
//! GET  /api/events/:slug/location (public)
//! GET  /api/banners              (public)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::state::AppState;
use super::extractor::app_err;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EventsQuery {
    pub query: Option<String>,
    pub category: Option<String>,
    pub city: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

async fn list_events(
    State(state): State<Arc<AppState>>,
    Query(q): Query<EventsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    use crate::models::events::EventListQuery;
    let query = EventListQuery {
        page: q.page,
        per_page: q.page_size,
        city: q.city,
        category: q.category,
        search: q.query,
        status: Some("active".into()),
    };
    let result = state.event_svc.list(query, None).await.map_err(app_err)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

async fn get_event(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let event = state.event_svc.get(&slug).await.map_err(app_err)?;
    Ok(Json(serde_json::to_value(event).unwrap_or_default()))
}

async fn get_event_location(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let event = state.event_svc.get(&slug).await.map_err(app_err)?;
    Ok(Json(serde_json::json!({
        "slug": event.slug,
        "venue": event.venue,
        "city": event.city,
    })))
}

async fn list_banners(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let banners = state.banner_svc.list_active(None).await.map_err(app_err)?;
    Ok(Json(serde_json::to_value(banners).unwrap_or_default()))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/events", get(list_events))
        .route("/events/{slug}", get(get_event))
        .route("/events/location/{slug}", get(get_event_location))
        .route("/banners", get(list_banners))
}
