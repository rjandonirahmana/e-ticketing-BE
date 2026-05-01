//! ws/routes.rs — REST routes untuk group room management
//!
//! GET  /api/ws/chat?token=...               WebSocket upgrade (auth via token)
//! GET  /api/chat/rooms                      List rooms user yang sudah join
//! GET  /api/chat/events/{event_id}/room     Get/buat room untuk event
//! POST /api/chat/rooms/{room_id}/join       Join room
//! GET  /api/chat/rooms/{room_id}/history    History pesan
//! GET  /api/chat/rooms/{room_id}/sent_count Berapa pesan user sudah kirim

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    middleware::from_fn_with_state,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    middleware::auth::{AuthUser, require_auth},
    models::group_chat::HistoryQuery,
    service::group_chat::GroupChatService,
    state::AppState,
    utils::error::AppError,
    ws::handler::{WsAppState, ws_chat},
};

// ── Response wrapper ──────────────────────────────────────────────────────────

fn ok<T: Serialize>(data: T) -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "data": data })))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn list_rooms(
    auth: AuthUser,
    State(state): State<Arc<WsAppState>>,
) -> Result<impl IntoResponse, AppError> {
    let rooms = state
        .group_svc
        .get_user_rooms(auth.id())
        .await
        .map_err(|e| AppError::Internal(e))?;
    Ok(ok(rooms))
}

/// GET /chat/events/{event_id}/room — merchant memanggil ini saat create event
/// agar room dibuat sebelum ada buyer.
async fn get_or_create_event_room(
    auth: AuthUser,
    State(state): State<Arc<WsAppState>>,
    Path(event_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Hanya merchant/admin yang bisa init room
    auth.require_role("merchant")
        .or_else(|_| auth.require_role("admin"))?;

    // Nama default: akan di-update saat event detail diambil
    let room = state
        .group_svc
        .get_or_create_room(&event_id, "Event Group", None, auth.id())
        .await
        .map_err(|e| AppError::Internal(e))?;

    Ok(ok(room))
}

/// POST /chat/rooms/{room_id}/join — dipanggil manual atau dari frontend setelah bayar
async fn join_room(
    auth: AuthUser,
    State(state): State<Arc<WsAppState>>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Cek room exist
    let room = state
        .group_svc
        .repo
        .find_by_id(&room_id)
        .await
        .map_err(|e| AppError::Internal(e))?
        .ok_or_else(|| AppError::NotFound("Room not found".into()))?;

    // Add member
    use crate::models::group_chat::MemberRole;
    state
        .group_svc
        .repo
        .add_member(&room_id, auth.id(), MemberRole::Member)
        .await
        .map_err(|e| AppError::Internal(e))?;

    Ok(ok(json!({ "room_id": room_id, "joined": true })))
}

/// GET /chat/rooms/{room_id}/history
async fn get_history(
    auth: AuthUser,
    State(state): State<Arc<WsAppState>>,
    Path(room_id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (msgs, has_more) = state
        .group_svc
        .get_history(&room_id, auth.id(), q.limit, q.before_id.as_deref())
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    Ok(ok(json!({ "messages": msgs, "has_more": has_more })))
}

/// GET /chat/rooms/{room_id}/sent_count — berapa pesan user sudah kirim (untuk UI)
async fn sent_count(
    auth: AuthUser,
    State(state): State<Arc<WsAppState>>,
    Path(room_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let count = state
        .group_svc
        .sent_count(&room_id, auth.id())
        .await
        .map_err(|e| AppError::Internal(e))?;

    Ok(ok(
        json!({ "count": count, "limit": 1, "is_merchant": false }),
    ))
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn chat_router(ws_state: Arc<WsAppState>, app_state: Arc<AppState>) -> Router {
    // WS — auth via token query param, tidak pakai require_auth middleware
    let ws_route = Router::new()
        .route("/api/ws/chat", get(ws_chat))
        .with_state(ws_state.clone());

    // REST chat routes — WAJIB require_auth agar AuthUser extractor dapat claims
    let chat_routes = Router::new()
        .route("/api/chat/rooms", get(list_rooms))
        .route(
            "/api/chat/events/{event_id}/room",
            get(get_or_create_event_room),
        )
        .route("/api/chat/rooms/{room_id}/join", post(join_room))
        .route("/api/chat/rooms/{room_id}/history", get(get_history))
        .route("/api/chat/rooms/{room_id}/sent_count", get(sent_count))
        .route_layer(from_fn_with_state(app_state, require_auth))
        .with_state(ws_state);

    Router::new().merge(ws_route).merge(chat_routes)
}
