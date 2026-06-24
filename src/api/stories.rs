//! api/stories.rs
//!
//! GET    /api/stories
//! POST   /api/stories           (multipart: file + metadata)
//! POST   /api/stories/:id/view
//! DELETE /api/stories/:id

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use std::sync::Arc;

use crate::state::AppState;
use super::extractor::{app_err, AuthUser};

async fn list_stories(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let groups = state.story_svc.list_groups(&claims.user_id).await.map_err(app_err)?;
    Ok(Json(serde_json::to_value(groups).unwrap_or_default()))
}

async fn view_story(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    state.story_svc.mark_viewed(&id, &claims.user_id).await.map_err(app_err)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_story(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<serde_json::Value>)> {
    state.story_svc.delete(&id, &claims.user_id).await.map_err(app_err)?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/stories", get(list_stories))
        .route("/stories/{id}/view", post(view_story))
        .route("/stories/{id}", delete(delete_story))
    // POST /stories (multipart upload) ditangani oleh upload_router di main.rs
    // yang sudah punya /upload/story — cukup alias di Next.js
}
