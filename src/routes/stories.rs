//! routes/stories.rs
//!
//! Endpoints:
//!   POST   /api/stories              — upload story baru (multipart)
//!   GET    /api/stories              — list semua story group
//!   POST   /api/stories/:id/view     — tandai story sudah dilihat
//!   DELETE /api/stories/:id          — hapus story milik sendiri
//!
//! ── Premium ───────────────────────────────────────────────────────────────────
//!   POST   /api/premium/activate     — aktifkan premium (admin/payment callback)
//!   GET    /api/premium/status       — cek status premium user yang login

use std::sync::Arc;

use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use bytes::Bytes;
use serde::Deserialize;
use serde_json::json;

use crate::{
    middleware::auth::AuthUser,
    state::AppState,
    utils::error::{AppError, AppResult},
};

// ── POST /api/stories ─────────────────────────────────────────────────────────

/// Upload story baru.
///
/// Multipart fields:
///   - `media`  (required) — file gambar atau video
///   - `slug`   (optional) — slug event (misal "electronic-oasis-abc123")
///
/// Rate limit:
///   - User biasa : 1x per hari
///   - Premium    : unlimited
pub async fn create(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    let mut media_bytes: Option<Bytes> = None;
    let mut slug: Option<String> = None;
    let mut title: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        match field.name().unwrap_or("") {
            "media" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Gagal baca file: {e}")))?;
                if bytes.is_empty() {
                    return Err(AppError::BadRequest("Field 'media' kosong".into()));
                }
                media_bytes = Some(bytes);
            }
            "slug" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Gagal baca slug: {e}")))?;
                if !text.is_empty() {
                    slug = Some(text);
                }
            }
            "title" => {
                let text = field.text().await.unwrap_or_default();
                if !text.is_empty() {
                    title = Some(text);
                }
            }
            _ => {} // abaikan field lain
        }
    }

    let bytes =
        media_bytes.ok_or_else(|| AppError::BadRequest("Field 'media' wajib diisi".into()))?;

    let resp = state
        .story_svc
        .upload(user.id(), bytes, slug, title)
        .await?;

    Ok((StatusCode::CREATED, Json(resp)))
}

// ── GET /api/stories ──────────────────────────────────────────────────────────

/// Ambil semua story group yang aktif (belum expired 24 jam).
/// Story milik viewer sendiri muncul di posisi pertama.
pub async fn list(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> AppResult<Json<serde_json::Value>> {
    let groups = state.story_svc.list_groups(user.id()).await?;
    Ok(Json(json!(groups)))
}

// ── POST /api/stories/:id/view ────────────────────────────────────────────────

/// Tandai story sudah dilihat oleh user yang login.
/// Idempotent — aman dipanggil berkali-kali.
pub async fn mark_viewed(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(story_id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    state.story_svc.mark_viewed(&story_id, user.id()).await?;
    Ok(Json(json!({ "ok": true })))
}

// ── DELETE /api/stories/:id ───────────────────────────────────────────────────

/// Hapus story milik sendiri.
pub async fn delete(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(story_id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    state.story_svc.delete(&story_id, user.id()).await?;
    Ok(Json(json!({ "deleted": true })))
}

// ── GET /api/premium/status ───────────────────────────────────────────────────

pub async fn premium_status(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> AppResult<Json<serde_json::Value>> {
    let is_premium = state.story_svc.is_premium(user.id()).await?;
    Ok(Json(json!({ "is_premium": is_premium })))
}

// ── POST /api/premium/activate ────────────────────────────────────────────────

/// Aktifkan premium untuk user yang login.
/// Body JSON: `{ "days": 30 }`
///
/// Catatan: Endpoint ini harus dipanggil oleh payment webhook / admin.
/// Tambahkan middleware role check ("admin") jika perlu.
#[derive(Debug, Deserialize)]
pub struct ActivatePremiumRequest {
    pub days: Option<i64>,
}

pub async fn activate_premium(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<ActivatePremiumRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let days = body.days.unwrap_or(30);
    let sub = state.story_svc.activate_premium(user.id(), days).await?;
    Ok(Json(json!({
        "plan": sub.plan,
        "expires_at": sub.expires_at,
        "is_active": sub.is_active,
    })))
}
