use std::sync::Arc;

use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
};
use bytes::Bytes;

use crate::middleware::auth::AuthUser;
use crate::models::event_variants::{EventVariantResponse, UpdateEventVariantRequest};
use crate::models::events::{
    CreateEventRequest, EventListQuery, EventWithVariants, PaginatedEvents, UpdateEventRequest,
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
    Path(id): Path<String>,
) -> AppResult<Json<EventWithVariants>> {
    Ok(Json(state.event_svc.get(&id).await?))
}

/// POST /api/events — multipart/form-data
///
/// Fields:
///   - `data`  (text) : JSON string → CreateEventRequest (termasuk variants)
///   - `image` (file) : opsional, max 5MB, JPEG/PNG/WebP/GIF
///
/// Response: EventWithVariants (event + semua variants sekaligus)
pub async fn create(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    mut multipart: Multipart,
) -> AppResult<Json<EventWithVariants>> {
    user.require_role("merchant")?;

    let mut image_bytes: Option<(Bytes, String)> = None;
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
                image_bytes = Some((data, ct));
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
    let req: CreateEventRequest = serde_json::from_str(&json)
        .map_err(|e| AppError::BadRequest(format!("JSON tidak valid: {e}")))?;

    // Upload image dulu jika ada — cover_url wajib sebelum insert event
    let cover_url: Option<String> = match image_bytes {
        Some((data, ct)) => {
            let storage = state.storage.clone();
            Some(storage.upload_image(data, &ct).await?)
        }
        None => None,
    };

    Ok(Json(
        state
            .event_svc
            .create(user.id(), req, cover_url.as_deref())
            .await?,
    ))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<UpdateEventRequest>,
) -> AppResult<Json<EventWithVariants>> {
    user.require_role("merchant")?;
    Ok(Json(state.event_svc.update(&id, user.id(), body).await?))
}

pub async fn delete_event(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    user.require_role("merchant")?;
    state.event_svc.delete(&id, user.id()).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
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

pub async fn delete_variant(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(variant_id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    user.require_role("merchant")?;
    state
        .event_svc
        .delete_variant(&variant_id, user.id())
        .await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}
