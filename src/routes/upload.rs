//! routes/upload.rs — Image upload endpoint.
//!
//! POST /api/upload/image   multipart/form-data, field "file"
//! Response: { "url": "https://image.ulalaapi.store/image/events/uuid.jpg" }

use std::sync::Arc;

use axum::{
    extract::{Multipart, State},
    Json,
};
use serde_json::{json, Value};

use crate::state::AppState;
use crate::utils::error::{AppError, AppResult};

pub async fn upload_image(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> AppResult<Json<Value>> {
    let storage = state.storage.clone();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name != "file" {
            continue;
        }

        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;

        // FIX: pass folder_name — was missing, causing compile error.
        // Use "events" as default folder so URL = /image/events/uuid.jpg
        let url = storage.upload_image(data, "events", &content_type).await?;
        return Ok(Json(json!({ "url": url })));
    }

    Err(AppError::BadRequest("Field 'file' tidak ditemukan".into()))
}
