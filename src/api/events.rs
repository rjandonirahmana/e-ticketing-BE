//! api/events.rs — Events & Banners REST endpoints.
//!
//! GET  /api/events               (public, query: query, category, city, page, page_size)
//! GET  /api/events/:slug         (public)
//! GET  /api/events/:slug/location (public)
//! GET  /api/banners              (public)
//!
//! Semua endpoint di sini publik & read-heavy — dua lapis peredam beban:
//! 1. Cache in-process (moka, TTL 30 dtk) menyimpan JSON yang SUDAH
//!    terserialisasi sebagai `Bytes`: burst ratusan ribu request hanya memicu
//!    satu query DB + satu serialisasi per key per 30 dtk; sisanya clone Bytes
//!    (murah, refcount) langsung jadi body.
//! 2. Header `Cache-Control: public` mengizinkan CDN/proxy/browser ikut
//!    menyerap traffic sebelum menyentuh server.

use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::state::AppState;
use super::extractor::app_err;

type ApiErr = (StatusCode, Json<serde_json::Value>);

/// Bungkus body JSON ter-cache jadi Response dengan Cache-Control publik.
/// max-age pendek (15 dtk) + stale-while-revalidate: data tetap terasa segar,
/// tapi CDN/browser boleh memakai salinan basi sambil revalidasi di belakang.
fn cached_json(body: bytes::Bytes) -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (
                header::CACHE_CONTROL,
                "public, max-age=15, stale-while-revalidate=30",
            ),
        ],
        body,
    )
        .into_response()
}

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
) -> Result<Response, ApiErr> {
    use crate::models::events::EventListQuery;

    let cache_key = format!(
        "events|{}|{}|{}|{}|{}",
        q.page.unwrap_or(1),
        q.city.as_deref().unwrap_or(""),
        q.category.as_deref().unwrap_or(""),
        q.query.as_deref().unwrap_or(""),
        q.page_size.unwrap_or(0),
    );
    if let Some(body) = state.pub_cache.rest.get(&cache_key).await {
        return Ok(cached_json(body));
    }

    let query = EventListQuery {
        page: q.page,
        per_page: q.page_size,
        city: q.city,
        category: q.category,
        search: q.query,
        status: Some("active".into()),
    };
    let result = state.event_svc.list(query, None).await.map_err(app_err)?;
    let body = bytes::Bytes::from(serde_json::to_vec(&result).unwrap_or_default());
    state.pub_cache.rest.insert(cache_key, body.clone()).await;
    Ok(cached_json(body))
}

async fn get_event(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Result<Response, ApiErr> {
    let cache_key = format!("event|{slug}");
    if let Some(body) = state.pub_cache.rest.get(&cache_key).await {
        return Ok(cached_json(body));
    }
    let event = state.event_svc.get(&slug).await.map_err(app_err)?;
    let body = bytes::Bytes::from(serde_json::to_vec(&event).unwrap_or_default());
    state.pub_cache.rest.insert(cache_key, body.clone()).await;
    Ok(cached_json(body))
}

async fn get_event_location(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Result<Response, ApiErr> {
    let cache_key = format!("loc|{slug}");
    if let Some(body) = state.pub_cache.rest.get(&cache_key).await {
        return Ok(cached_json(body));
    }
    let event = state.event_svc.get(&slug).await.map_err(app_err)?;
    let body = bytes::Bytes::from(
        serde_json::to_vec(&serde_json::json!({
            "slug": event.slug,
            "venue": event.venue,
            "city": event.city,
        }))
        .unwrap_or_default(),
    );
    state.pub_cache.rest.insert(cache_key, body.clone()).await;
    Ok(cached_json(body))
}

async fn list_banners(State(state): State<Arc<AppState>>) -> Result<Response, ApiErr> {
    let cache_key = "banners".to_string();
    if let Some(body) = state.pub_cache.rest.get(&cache_key).await {
        return Ok(cached_json(body));
    }
    let banners = state.banner_svc.list_active(None).await.map_err(app_err)?;
    let body = bytes::Bytes::from(serde_json::to_vec(&banners).unwrap_or_default());
    state.pub_cache.rest.insert(cache_key, body.clone()).await;
    Ok(cached_json(body))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/events", get(list_events))
        .route("/events/{slug}", get(get_event))
        .route("/events/location/{slug}", get(get_event_location))
        .route("/banners", get(list_banners))
}
