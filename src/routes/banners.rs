//! routes/banners.rs
//!
//! Public:
//!   GET  /api/banners                — list banner aktif
//!
//! Admin (require_auth + role = "admin"):
//!   POST   /api/admin/banners        — buat banner (multipart: "data" JSON + opsional "image")
//!   PUT    /api/admin/banners/:id    — update banner (multipart: "data" JSON + opsional "image")
//!   DELETE /api/admin/banners/:id    — soft-delete banner

use std::sync::Arc;

use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use bytes::Bytes;
use serde_json::json;

use crate::{
    middleware::auth::AuthUser,
    models::banners::{Banner, CreateBannerRequest, ListBannersQuery, UpdateBannerRequest},
    state::AppState,
    utils::error::{AppError, AppResult},
};

// ── Public ────────────────────────────────────────────────────────────────────

/// GET /api/banners?event_id=<ulid>
pub async fn list_active(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListBannersQuery>,
) -> AppResult<Json<Vec<Banner>>> {
    let banners = state.banner_svc.list_active(q.event_id.as_deref()).await?;
    Ok(Json(banners))
}

// ── Admin helpers ─────────────────────────────────────────────────────────────

/// Parse multipart body: ekstrak field "data" (JSON string) dan "image" (file bytes + mime).
async fn parse_banner_multipart(
    mut multipart: Multipart,
) -> AppResult<(Option<String>, Option<(Bytes, String)>)> {
    let mut req_json: Option<String> = None;
    let mut image_bytes: Option<(Bytes, String)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        match field.name().unwrap_or("") {
            "data" => {
                req_json = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| AppError::BadRequest(e.to_string()))?,
                );
            }
            "image" => {
                let ct = field.content_type().unwrap_or("image/jpeg").to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                image_bytes = Some((data, ct));
            }
            _ => {}
        }
    }

    Ok((req_json, image_bytes))
}

// ── Admin: Create ─────────────────────────────────────────────────────────────

/// POST /api/admin/banners
///
/// Multipart fields:
///   - `data`  (text) : JSON → `CreateBannerRequest`
///   - `image` (file) : opsional — jika ada, di-upload dan override `image_url`
///
/// Jika tidak ada field `image`, `image_url` di dalam `data` JSON wajib diisi.
pub async fn admin_create(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    multipart: Multipart,
) -> AppResult<Json<Banner>> {
    user.require_role("admin")?;

    let (req_json, image_bytes) = parse_banner_multipart(multipart).await?;

    let json =
        req_json.ok_or_else(|| AppError::BadRequest("Field 'data' (JSON) wajib ada".into()))?;
    let req: CreateBannerRequest = serde_json::from_str(&json)
        .map_err(|e| AppError::BadRequest(format!("JSON tidak valid: {e}")))?;

    // Tentukan image_url final: upload baru > image_url dari JSON body
    let image_url = if let Some((data, ct)) = image_bytes {
        // Upload ke storage, dapatkan public URL
        state.storage.upload_image(data, "banners", &ct).await?
    } else if !req.image_url.is_empty() {
        req.image_url.clone()
    } else {
        return Err(AppError::BadRequest(
            "Wajib menyertakan file 'image' atau mengisi 'image_url' di body JSON".into(),
        ));
    };

    Ok(Json(state.banner_svc.create(image_url, req).await?))
}

// ── Admin: Update ─────────────────────────────────────────────────────────────

/// PUT /api/admin/banners/:id
///
/// Multipart fields:
///   - `data`  (text) : JSON → `UpdateBannerRequest` (semua field opsional)
///   - `image` (file) : opsional — jika ada, di-upload dan override image_url
///
/// Field yang tidak disertakan dalam JSON tidak diubah di DB (COALESCE pattern).
pub async fn admin_update(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    multipart: Multipart,
) -> AppResult<Json<Banner>> {
    user.require_role("admin")?;

    let (req_json, image_bytes) = parse_banner_multipart(multipart).await?;

    let json =
        req_json.ok_or_else(|| AppError::BadRequest("Field 'data' (JSON) wajib ada".into()))?;
    let req: UpdateBannerRequest = serde_json::from_str(&json)
        .map_err(|e| AppError::BadRequest(format!("JSON tidak valid: {e}")))?;

    // Upload image baru jika ada — hasilnya override URL lama
    let new_image_url: Option<String> = if let Some((data, ct)) = image_bytes {
        Some(state.storage.upload_image(data, "banner", &ct).await?)
    } else {
        None
    };

    Ok(Json(state.banner_svc.update(id, new_image_url, req).await?))
}

// ── Admin: Delete ─────────────────────────────────────────────────────────────

/// DELETE /api/admin/banners/:id
///
/// Soft-delete: set `deleted_at = now()`.
/// Banner yang sudah di-delete tidak muncul di list_active.
pub async fn admin_delete(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    user.require_role("admin")?;
    state.banner_svc.soft_delete(id).await?;
    Ok((
        StatusCode::OK,
        Json(json!({ "message": format!("Banner id={id} berhasil dihapus") })),
    ))
}
