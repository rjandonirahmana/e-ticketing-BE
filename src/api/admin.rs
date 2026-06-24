//! api/admin.rs
//!
//! GET /api/admin/events          (private, query: page, status)
//! PUT /api/admin/events/:id      (private, body: { status })

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::state::AppState;
use super::extractor::{app_err, AuthUser};

#[derive(Deserialize, Default)]
pub struct AdminEventsQuery {
    pub page: Option<i64>,
    pub status: Option<String>,
}

async fn list_admin_events(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<AdminEventsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if claims.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "message": "Akses ditolak" })),
        ));
    }
    use crate::models::events::EventListQuery;
    let query = EventListQuery {
        page: q.page,
        per_page: Some(50),
        city: None,
        category: None,
        search: None,
        status: q.status,
    };
    let result = state.event_svc.list(query, None).await.map_err(app_err)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

#[derive(Deserialize)]
pub struct UpdateStatusReq {
    pub status: String,
}

async fn update_event_status(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateStatusReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if claims.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "message": "Akses ditolak" })),
        ));
    }
    let result = state
        .event_svc
        .admin_update_status(&id, &body.status)
        .await
        .map_err(app_err)?;
    Ok(Json(serde_json::json!({ "id": result.id, "status": result.status })))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/admin/events", get(list_admin_events))
        .route("/admin/events/{id}", put(update_event_status))
}
