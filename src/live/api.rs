use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;
use crate::middleware::auth::{AuthUser, require_auth};
use axum::middleware::from_fn_with_state;

fn ok<T: Serialize>(data: T) -> Response {
    (StatusCode::OK, Json(serde_json::json!({ "data": data }))).into_response()
}

fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
}

#[derive(Debug, Deserialize)]
pub struct CreateRoomReq {
    pub event_slug: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SdpReq {
    pub sdp: String,
}

#[derive(Debug, Deserialize)]
pub struct SubscribeReq {
    pub sdp: String,
    // Identitas penonton (opsional — penonton bisa anonim / belum login).
    #[serde(default)]
    pub viewer_id: Option<String>,
    #[serde(default)]
    pub viewer_name: Option<String>,
    #[serde(default)]
    pub viewer_photo: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct IceReq {
    pub candidate: String,
    pub sdp_mid: String,
    pub sdp_mline_index: u32,
}

#[derive(Debug, Deserialize)]
pub struct SubscribeIceReq {
    pub subscriber_id: String,
    pub candidate: String,
    pub sdp_mid: String,
    pub sdp_mline_index: u32,
}

#[derive(Debug, Serialize)]
pub struct SdpRes {
    pub sdp: String,
}

#[derive(Debug, Serialize)]
pub struct SubscribeSdpRes {
    pub sdp: String,
    pub subscriber_id: String,
}

async fn create_room(
    auth: AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateRoomReq>,
) -> Response {
    if auth.require_role("merchant").is_err() && auth.require_role("admin").is_err() {
        return err(StatusCode::FORBIDDEN, "Only merchants can go live");
    }

    match state
        .live_svc
        .create_room(&auth.id(), &auth.name(), body.event_slug.as_deref())
        .await
    {
        Ok(info) => ok(info),
        Err(e) => err(StatusCode::CONFLICT, &e),
    }
}

async fn list_rooms(State(state): State<Arc<AppState>>) -> Response {
    ok(state.live_svc.list_rooms())
}

async fn get_room(
    Path(room_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    match state.live_svc.get_room(&room_id) {
        Some(info) => ok(info),
        None => err(StatusCode::NOT_FOUND, "Room not found"),
    }
}

async fn publish_sdp(
    _auth: AuthUser,
    Path(room_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<SdpReq>,
) -> impl IntoResponse {
    match state.live_svc.publish_sdp(&room_id, &body.sdp).await {
        Ok(answer) => ok(SdpRes { sdp: answer }),
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

async fn publish_ice(
    _auth: AuthUser,
    Path(room_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<IceReq>,
) -> impl IntoResponse {
    match state
        .live_svc
        .publish_ice(&room_id, &body.candidate, &body.sdp_mid, body.sdp_mline_index)
        .await
    {
        Ok(()) => ok(serde_json::json!({ "ok": true })),
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

async fn subscribe_sdp(
    Path(room_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<SubscribeReq>,
) -> impl IntoResponse {
    let sub_id = uuid::Uuid::new_v4().to_string();
    let viewer = crate::live::room::ViewerInfo {
        id: body.viewer_id.unwrap_or_else(|| sub_id.clone()),
        name: body
            .viewer_name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "Anonim".to_string()),
        photo_url: body.viewer_photo.filter(|p| !p.trim().is_empty()),
    };
    match state
        .live_svc
        .subscribe_sdp(&room_id, &sub_id, &body.sdp, viewer)
        .await
    {
        // Kembalikan subscriber_id agar klien bisa mengirim trickle ICE
        // dan memanggil endpoint leave saat keluar (viewer count akurat).
        Ok(answer) => ok(SubscribeSdpRes {
            sdp: answer,
            subscriber_id: sub_id,
        }),
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

async fn subscribe_ice(
    Path(room_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<SubscribeIceReq>,
) -> impl IntoResponse {
    match state
        .live_svc
        .subscribe_ice(
            &room_id,
            &body.subscriber_id,
            &body.candidate,
            &body.sdp_mid,
            body.sdp_mline_index,
        )
        .await
    {
        Ok(()) => ok(serde_json::json!({ "ok": true })),
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

async fn leave_room(
    Path((room_id, subscriber_id)): Path<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match state
        .live_svc
        .remove_subscriber(&room_id, &subscriber_id)
        .await
    {
        Ok(()) => ok(serde_json::json!({ "ok": true })),
        Err(e) => err(StatusCode::BAD_REQUEST, &e),
    }
}

async fn stop_room(
    _auth: AuthUser,
    Path(room_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.live_svc.stop_room(&room_id).await {
        Ok(()) => ok(serde_json::json!({ "ok": true })),
        Err(e) => err(StatusCode::NOT_FOUND, &e),
    }
}

pub fn live_router(state: Arc<AppState>) -> Router {
    let protected = Router::new()
        .route("/api/live/rooms", post(create_room))
        .route("/api/live/rooms/{room_id}/publish/sdp", post(publish_sdp))
        .route("/api/live/rooms/{room_id}/publish/ice", post(publish_ice))
        .route("/api/live/rooms/{room_id}", delete(stop_room))
        .layer(from_fn_with_state(state.clone(), require_auth));

    let public = Router::new()
        .route("/api/live/rooms", get(list_rooms))
        .route("/api/live/rooms/{room_id}", get(get_room))
        .route("/api/live/rooms/{room_id}/subscribe/sdp", post(subscribe_sdp))
        .route("/api/live/rooms/{room_id}/subscribe/ice", post(subscribe_ice))
        .route(
            "/api/live/rooms/{room_id}/subscribe/{subscriber_id}",
            delete(leave_room),
        );

    Router::new().merge(protected).merge(public).with_state(state)
}