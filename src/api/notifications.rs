//! api/notifications.rs
//!
//! GET  /api/notifications
//! POST /api/notifications/:id/read
//! POST /api/notifications/read-all

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;

use crate::state::AppState;
use super::extractor::{app_err, AuthUser};

async fn list_notifications(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let notifs = state
        .notification_store_svc
        .list(&claims.user_id, 1, 100)
        .await
        .map_err(|e| app_err(e.into()))?;
    Ok(Json(serde_json::json!({ "notifications": serde_json::to_value(notifs).unwrap_or_default() })))
}

async fn mark_read(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    state
        .notification_store_svc
        .mark_read(&id, &claims.user_id)
        .await
        .map_err(|e| app_err(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn mark_all_read(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    state
        .notification_store_svc
        .mark_all_read(&claims.user_id)
        .await
        .map_err(|e| app_err(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/notifications", get(list_notifications))
        .route("/notifications/{id}/read", post(mark_read))
        .route("/notifications/read-all", post(mark_all_read))
}
