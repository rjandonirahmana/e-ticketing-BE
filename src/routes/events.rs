use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
};

use crate::middleware::auth::AuthUser;
use crate::models::event_variant::{
    CreateTicketVariantRequest, TicketVariantResponse, UpdateTicketVariantRequest,
};
use crate::models::events::{
    CreateEventRequest, Event, EventListQuery, EventWithVariants, PaginatedEvents,
    UpdateEventRequest,
};
use crate::state::AppState;
use crate::utils::error::AppResult;

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
    Ok(Json(state.event_svc.get_with_variants(&id).await?))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<CreateEventRequest>,
) -> AppResult<Json<Event>> {
    user.require_role("merchant")?;
    Ok(Json(state.event_svc.create(user.id(), body).await?))
}

pub async fn update(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<UpdateEventRequest>,
) -> AppResult<Json<Event>> {
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

// ── Variants ────────────────────────────────────────────────────────────────

pub async fn create_variant(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(event_id): Path<String>,
    Json(body): Json<CreateTicketVariantRequest>,
) -> AppResult<Json<TicketVariantResponse>> {
    user.require_role("merchant")?;
    Ok(Json(
        state
            .event_svc
            .create_variant(&event_id, user.id(), body)
            .await?,
    ))
}

pub async fn update_variant(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(variant_id): Path<String>,
    Json(body): Json<UpdateTicketVariantRequest>,
) -> AppResult<Json<TicketVariantResponse>> {
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
