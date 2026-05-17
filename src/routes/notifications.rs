use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::middleware::auth::AuthUser;
use crate::models::notification::Notification;
use crate::state::AppState;
use crate::utils::error::AppResult;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct UnreadCountResponse {
    pub count: i64,
}

/// GET /api/notifications — list notifikasi milik user yang sedang login.
pub async fn list(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Notification>>> {
    Ok(Json(
        state
            .notification_store_svc
            .list(user.id(), q.page.unwrap_or(1), q.per_page.unwrap_or(20))
            .await?,
    ))
}

/// GET /api/notifications/unread-count — badge counter.
pub async fn unread_count(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> AppResult<Json<UnreadCountResponse>> {
    let count = state
        .notification_store_svc
        .unread_count(user.id())
        .await?;
    Ok(Json(UnreadCountResponse { count }))
}

/// POST /api/notifications/:id/read — tandai satu notif sebagai dibaca.
pub async fn mark_read(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    state
        .notification_store_svc
        .mark_read(&id, user.id())
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /api/notifications/read-all — tandai semua notif sebagai dibaca.
pub async fn mark_all_read(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> AppResult<Json<serde_json::Value>> {
    state
        .notification_store_svc
        .mark_all_read(user.id())
        .await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
