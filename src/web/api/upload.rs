//! upload.rs — Axum multipart handler for story media upload.
//!
//! POST /upload/story   — multipart/form-data
//!   fields: file (required), slug (optional event slug)
//!
//! Auth: HttpOnly cookie `pulse_token`.

#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use axum::{
    extract::{Extension, Multipart},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use bytes::Bytes;
use serde_json::{json, Value};

use crate::state::AppState;

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get("cookie")?.to_str().ok()?;
    raw.split(';').map(str::trim).find_map(|p| {
        p.strip_prefix(&format!("{name}="))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from)
    })
}

pub async fn story_upload(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let token = cookie_value(&headers, "pulse_token").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Tidak terautentikasi" })),
        )
    })?;

    let claims = state.jwt.verify(&token).map_err(|e| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    let mut media_bytes: Option<Bytes> = None;
    let mut slug: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Multipart error: {e}") })),
        )
    })? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" | "media" => {
                let data = field.bytes().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(json!({ "error": format!("Read error: {e}") })),
                    )
                })?;
                media_bytes = Some(data);
            }
            "slug" => {
                let text = field.text().await.unwrap_or_default();
                if !text.is_empty() {
                    slug = Some(text);
                }
            }
            _ => {}
        }
    }

    let bytes = media_bytes.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Field 'file' tidak ada dalam request" })),
        )
    })?;

    let res = state
        .story_svc
        .upload(&claims.user_id, bytes, slug)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    Ok(Json(json!({
        "story_id": res.story_id,
        "media_url": res.media_url,
    })))
}
