use std::sync::Arc;

use axum::{
    extract::{Multipart, Path, Query, State},
    Json,
};
use bytes::Bytes;
use futures::future::try_join_all;

use crate::middleware::auth::AuthUser;
use crate::models::event_variants::{EventVariantResponse, UpdateEventVariantRequest};
use crate::models::events::{
    CreateEventRequest, DetailImageEntry, DetailImageMeta, EventListQuery, EventWithVariants,
    PaginatedEvents, UpdateEventRequest,
};
use crate::state::AppState;
use crate::utils::error::{AppError, AppResult};

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<EventListQuery>,
) -> AppResult<Json<PaginatedEvents>> {
    Ok(Json(state.event_svc.list(q, None).await?))
}

pub async fn list_mine(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<EventListQuery>,
) -> AppResult<Json<PaginatedEvents>> {
    user.require_role("merchant")?;
    Ok(Json(state.event_svc.list(q, Some(user.id())).await?))
}

pub async fn get_one(
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> AppResult<Json<EventWithVariants>> {
    Ok(Json(state.event_svc.get(&slug).await?))
}

/// POST /api/events — multipart/form-data
///
/// Fields:
///   - `data`              (text) : JSON string → CreateEventRequest (termasuk variants).
///                                  Field `detail_images` di JSON **tidak perlu diisi**;
///                                  akan di-override dari file upload jika ada.
///   - `image`             (file) : opsional, cover event, max 5MB, JPEG/PNG/WebP/GIF
///   - `detail_image`      (file) : bisa banyak (field name sama berulang), max 5MB each
///   - `detail_image_meta` (text) : JSON array of `{ image_type, caption }`, satu entry
///                                  per file `detail_image`, dicocokkan by index.
///                                  Contoh: `[{"image_type":"map","caption":"Denah Venue"},
///                                            {"image_type":"seat","caption":"Seat Map"}]`
///                                  Jika tidak dikirim, semua gambar pakai
///                                  image_type="other" dan caption="".
///
/// Response: EventWithVariants
pub async fn create(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    mut multipart: Multipart,
) -> AppResult<Json<EventWithVariants>> {
    user.require_role("merchant")?;

    let mut cover_bytes: Option<(Bytes, String)> = None;
    let mut detail_image_bytes: Vec<(Bytes, String)> = Vec::new();
    let mut detail_image_meta_json: Option<String> = None;
    let mut req_json: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        match field.name().unwrap_or("") {
            "image" => {
                let ct = field.content_type().unwrap_or("image/jpeg").to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                cover_bytes = Some((data, ct));
            }
            "detail_image" => {
                let ct = field.content_type().unwrap_or("image/jpeg").to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                if !data.is_empty() {
                    detail_image_bytes.push((data, ct));
                }
            }
            "detail_image_meta" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                detail_image_meta_json = Some(text);
            }
            "data" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                req_json = Some(text);
            }
            _ => {}
        }
    }

    let json = req_json.ok_or_else(|| AppError::BadRequest("Field 'data' wajib ada".into()))?;
    let mut req: CreateEventRequest = serde_json::from_str(&json)
        .map_err(|e| AppError::BadRequest(format!("JSON tidak valid: {e}")))?;

    // Ambil store_name merchant untuk slug generation
    let merchant = state
        .merchant_svc
        .get_profile(user.id())
        .await
        .map_err(|_| AppError::BadRequest("Merchant profile belum dibuat".into()))?;

    // Upload cover image jika ada
    let cover_url: Option<String> = match cover_bytes {
        Some((data, ct)) => {
            let storage = state.storage.clone();
            Some(storage.upload_image(data, "/event", &ct).await?)
        }
        None => None,
    };

    // Upload semua detail images secara paralel, lalu override req.detail_images
    if !detail_image_bytes.is_empty() {
        let metas =
            parse_detail_image_meta(detail_image_meta_json.as_deref(), detail_image_bytes.len())?;

        let upload_futs: Vec<_> = detail_image_bytes
            .into_iter()
            .zip(metas.into_iter())
            .map(|((data, ct), meta)| {
                let storage = state.storage.clone();
                async move {
                    let url = storage.upload_image(data, "/event/detail", &ct).await?;
                    Ok::<DetailImageEntry, AppError>(DetailImageEntry {
                        url,
                        image_type: meta.image_type,
                        caption: meta.caption,
                    })
                }
            })
            .collect();

        req.detail_images = try_join_all(upload_futs).await?;
    }

    Ok(Json(
        state
            .event_svc
            .create(user.id(), &merchant.store_name, req, cover_url.as_deref())
            .await?,
    ))
}

pub async fn list_categories(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<serde_json::Value>> {
    let cats = state.event_svc.list_categories().await?;
    Ok(Json(serde_json::json!({ "data": cats })))
}

/// PUT /api/events/:id — multipart/form-data
///
/// Fields:
///   - `data`              (text) : JSON string → UpdateEventRequest
///   - `image`             (file) : opsional, ganti cover event
///   - `detail_image`      (file) : bisa banyak (field name sama berulang), max 5MB each
///   - `detail_image_meta` (text) : JSON array of `{ image_type, caption }`, satu entry
///                                  per file `detail_image`.
///                                  Jika `detail_image` dikirim → field `detail_images`
///                                  di JSON di-replace sepenuhnya oleh hasil upload.
///                                  Jika tidak ada `detail_image` → field `detail_images`
///                                  di JSON tetap dipakai (kirim URL lama untuk retain).
///
/// Response: EventWithVariants
pub async fn update(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> AppResult<Json<EventWithVariants>> {
    user.require_role("merchant")?;

    let mut cover_bytes: Option<(Bytes, String)> = None;
    let mut detail_image_bytes: Vec<(Bytes, String)> = Vec::new();
    let mut detail_image_meta_json: Option<String> = None;
    let mut req_json: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        match field.name().unwrap_or("") {
            "image" => {
                let ct = field.content_type().unwrap_or("image/jpeg").to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                cover_bytes = Some((data, ct));
            }
            "detail_image" => {
                let ct = field.content_type().unwrap_or("image/jpeg").to_string();
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                if !data.is_empty() {
                    detail_image_bytes.push((data, ct));
                }
            }
            "detail_image_meta" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                detail_image_meta_json = Some(text);
            }
            "data" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                req_json = Some(text);
            }
            _ => {}
        }
    }

    // Upload cover image jika ada
    let cover_url: Option<String> = match cover_bytes {
        Some((data, ct)) => {
            let storage = state.storage.clone();
            Some(storage.upload_image(data, "/event", &ct).await?)
        }
        None => None,
    };

    let json = req_json.ok_or_else(|| AppError::BadRequest("Field 'data' wajib ada".into()))?;
    let mut req: UpdateEventRequest = serde_json::from_str(&json)
        .map_err(|e| AppError::BadRequest(format!("JSON tidak valid: {e}")))?;

    req.cover_url = cover_url;
    req.status = Some("edited".to_string());

    // Upload detail images jika ada — replace seluruh detail_images
    if !detail_image_bytes.is_empty() {
        let metas =
            parse_detail_image_meta(detail_image_meta_json.as_deref(), detail_image_bytes.len())?;

        let upload_futs: Vec<_> = detail_image_bytes
            .into_iter()
            .zip(metas.into_iter())
            .map(|((data, ct), meta)| {
                let storage = state.storage.clone();
                async move {
                    let url = storage.upload_image(data, "/event/detail", &ct).await?;
                    Ok::<DetailImageEntry, AppError>(DetailImageEntry {
                        url,
                        image_type: meta.image_type,
                        caption: meta.caption,
                    })
                }
            })
            .collect();

        req.detail_images = Some(try_join_all(upload_futs).await?);
    }

    Ok(Json(state.event_svc.update(&id, user.id(), req).await?))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse `detail_image_meta` JSON string menjadi Vec<DetailImageMeta>.
/// Jika tidak ada / kosong, return vec berisi `n` entry default
/// (image_type="other", caption="").
fn parse_detail_image_meta(raw: Option<&str>, n: usize) -> AppResult<Vec<DetailImageMeta>> {
    match raw {
        Some(s) if !s.trim().is_empty() => {
            let mut metas: Vec<DetailImageMeta> = serde_json::from_str(s)
                .map_err(|e| AppError::BadRequest(format!("detail_image_meta tidak valid: {e}")))?;
            // Pad dengan default jika jumlah meta < jumlah file; truncate jika lebih
            metas.resize_with(n, DetailImageMeta::default);
            Ok(metas)
        }
        _ => Ok((0..n).map(|_| DetailImageMeta::default()).collect()),
    }
}

// ── Variants (masih tersedia untuk update/delete individual) ─────────────────

pub async fn update_variant(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(variant_id): Path<String>,
    Json(body): Json<UpdateEventVariantRequest>,
) -> AppResult<Json<EventVariantResponse>> {
    user.require_role("merchant")?;
    Ok(Json(
        state
            .event_svc
            .update_variant(&variant_id, user.id(), body)
            .await?,
    ))
}

// ── Admin: update status event ───────────────────────────────────────────────

/// PUT /api/admin/events/:id/status
///
/// Body JSON: `{ "status": "active" | "cancelled" | "completed" | "edited" }`
///
/// Admin menggunakan ini untuk approve (active) atau reject event yang
/// baru dibuat / baru diedit merchant (status = "edited").
pub async fn admin_update_status(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<AdminUpdateStatusBody>,
) -> AppResult<Json<EventWithVariants>> {
    user.require_role("admin")?;
    Ok(Json(
        state
            .event_svc
            .admin_update_status(&id, &body.status)
            .await?,
    ))
}

/// GET /api/admin/events?status=edited&page=1&per_page=20
///
/// Admin: list semua event (opsional filter status).
pub async fn admin_list_events(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<EventListQuery>,
) -> AppResult<Json<PaginatedEvents>> {
    user.require_role("admin")?;
    // Gunakan list biasa tapi tanpa filter merchant_id
    Ok(Json(state.event_svc.list(q, None).await?))
}

#[derive(serde::Deserialize)]
pub struct AdminUpdateStatusBody {
    pub status: String,
}
