use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::middleware::auth::{require_auth, AuthUser};
use crate::state::AppState;
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

/// Daftar ICE server untuk semua klien WebRTC (live + meet). STUN publik selalu
/// disertakan; TURN ditambahkan bila `TURN_URL` di-set di env (lihat
/// `.env.example`). Browser tak bisa baca env server → diambil dari sini.
async fn ice_servers() -> Response {
    let mut servers = vec![serde_json::json!({
        "urls": ["stun:stun.l.google.com:19302", "stun:stun1.l.google.com:19302"]
    })];
    if let Ok(url) = std::env::var("TURN_URL") {
        let url = url.trim();
        if !url.is_empty() {
            let mut turn = serde_json::json!({ "urls": [url] });
            if let Ok(u) = std::env::var("TURN_USER") {
                if !u.trim().is_empty() {
                    turn["username"] = serde_json::json!(u);
                }
            }
            if let Ok(p) = std::env::var("TURN_PASS") {
                if !p.trim().is_empty() {
                    turn["credential"] = serde_json::json!(p);
                }
            }
            servers.push(turn);
        }
    }
    ok(servers)
}

/// WS /ws/lives — push daftar room live realtime (pengganti polling).
/// Kirim snapshot saat connect, lalu tiap ada perubahan.
async fn lives_ws(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> Response {
    ws.on_upgrade(move |socket| lives_ws_loop(socket, state))
}

async fn lives_ws_loop(mut socket: WebSocket, state: Arc<AppState>) {
    let mut rx = state.live_svc.subscribe_changes();

    let initial = serde_json::to_string(&state.live_svc.list_rooms()).unwrap_or_default();
    if socket.send(Message::Text(initial.into())).await.is_err() {
        return;
    }

    loop {
        tokio::select! {
            update = rx.recv() => match update {
                Ok(list) => {
                    let txt = serde_json::to_string(&list).unwrap_or_default();
                    if socket.send(Message::Text(txt.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            },
            msg = socket.recv() => match msg {
                Some(Ok(_)) => {}
                _ => break,
            },
        }
    }
}

async fn get_room(Path(room_id): Path<String>, State(state): State<Arc<AppState>>) -> Response {
    match state.live_svc.get_room(&room_id) {
        Some(info) => ok(info),
        None => err(StatusCode::NOT_FOUND, "Room not found"),
    }
}

// ── HTTP signaling (tetap ada untuk kompatibilitas klien lama) ─────────────────

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
        .publish_ice(
            &room_id,
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

// ── WS signaling: publisher ────────────────────────────────────────────────────
//
// GET /ws/live/publish/{room_id}
//
// Protokol (JSON):
//   Client → Server: { "type": "publish_offer", "sdp": "..." }
//   Server → Client: { "type": "answer",        "sdp": "..." }
//   Server → Client: { "type": "error",         "message": "..." }
//   Client → Server: { "type": "candidate",
//                      "candidate": "...", "sdp_mid": "...", "sdp_mline_index": 0 }
//
// Keunggulan vs HTTP: trickle ICE sejati (kandidat dikirim satu per satu, real-time),
// tidak ada `wait_ice_gathering_complete` polling, setup koneksi 300-500 ms lebih cepat.
// Server otomatis memanggil stop_room saat WS ditutup (kamera mati / tab ditutup).

async fn live_publish_ws(
    _auth: AuthUser,
    ws: WebSocketUpgrade,
    Path(room_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| live_publish_ws_loop(socket, room_id, state))
}

async fn live_publish_ws_loop(mut socket: WebSocket, room_id: String, state: Arc<AppState>) {
    let mut negotiated = false;

    loop {
        match socket.recv().await {
            None | Some(Err(_)) => break,
            Some(Ok(Message::Close(_))) => break,
            Some(Ok(Message::Text(txt))) => {
                let v: serde_json::Value = match serde_json::from_str(&txt) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "publish_offer" => {
                        let sdp = v["sdp"].as_str().unwrap_or_default();
                        let resp = match state.live_svc.publish_sdp(&room_id, sdp).await {
                            Ok(answer) => {
                                negotiated = true;
                                serde_json::json!({ "type": "answer", "sdp": answer })
                            }
                            Err(e) => serde_json::json!({ "type": "error", "message": e }),
                        };
                        if socket
                            .send(Message::Text(resp.to_string().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    "candidate" if negotiated => {
                        let candidate = v["candidate"].as_str().unwrap_or_default();
                        let mid = v["sdp_mid"].as_str().unwrap_or_default();
                        let idx = v["sdp_mline_index"].as_u64().unwrap_or(0) as u32;
                        // Error silently ignored — ICE kadang kirim kandidat duplikat.
                        let _ = state
                            .live_svc
                            .publish_ice(&room_id, candidate, mid, idx)
                            .await;
                    }
                    _ => {}
                }
            }
            Some(Ok(_)) => {} // Binary/ping/pong — abaikan
        }
    }

    // Publisher terputus (tutup tab / klik STOP LIVE / navigasi pergi):
    // hentikan room agar tidak "menggantung". Dilakukan TANPA syarat `negotiated`
    // — bila WS publisher ditutup sebelum sempat mengirim offer (mis. room dibuat
    // via HTTP POST lalu WS gagal/ditutup), room tetap harus dibuang agar tidak
    // menjadi room yatim yang menumpuk di memori.
    tracing::info!(room_id, negotiated, "Publisher WS closed — stopping room");
    let _ = state.live_svc.stop_room(&room_id).await;
}

// ── WS signaling: subscriber (viewer) ─────────────────────────────────────────
//
// GET /ws/live/subscribe/{room_id}
//
// Protokol (JSON):
//   Client → Server: { "type": "subscribe_offer", "sdp": "...",
//                      "viewer_id": "...", "viewer_name": "..." }
//   Server → Client: { "type": "answer", "sdp": "...", "subscriber_id": "..." }
//   Server → Client: { "type": "error",  "message": "..." }
//   Client → Server: { "type": "candidate",
//                      "candidate": "...", "sdp_mid": "...", "sdp_mline_index": 0 }
//   Client → Server: { "type": "leave" }   (opsional, boleh langsung tutup WS)
//
// WS disconnect (apapun sebabnya) secara otomatis memanggil remove_subscriber
// sehingga jumlah penonton selalu akurat, tanpa perlu HTTP DELETE.

async fn live_subscribe_ws(
    ws: WebSocketUpgrade,
    Path(room_id): Path<String>,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| live_subscribe_ws_loop(socket, room_id, state))
}

async fn live_subscribe_ws_loop(mut socket: WebSocket, room_id: String, state: Arc<AppState>) {
    let sub_id = uuid::Uuid::new_v4().to_string();
    let mut subscribed = false;

    // Dengarkan perubahan daftar room. Saat publisher mematikan siaran, room
    // dihapus dari snapshot → kita beri tahu penonton ("stream_ended") lalu tutup
    // koneksi, sehingga seluruh penonton otomatis keluar dari live.
    let mut changes = state.live_svc.subscribe_changes();

    loop {
        tokio::select! {
            // ── Perubahan room: deteksi siaran berakhir ───────────────────────
            changed = changes.recv() => {
                match changed {
                    Ok(list) => {
                        if subscribed && !list.iter().any(|r| r.room_id == room_id) {
                            tracing::info!(room_id, sub_id, "Siaran berakhir → keluarkan penonton");
                            let _ = socket
                                .send(Message::Text(
                                    serde_json::json!({ "type": "stream_ended" }).to_string().into(),
                                ))
                                .await;
                            break;
                        }
                    }
                    // Ketinggalan beberapa update tak masalah: cek room di update berikutnya.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => break,
                }
            }

            // ── Pesan dari klien (offer / candidate / leave) ──────────────────
            msg = socket.recv() => {
        match msg {
            None | Some(Err(_)) => break,
            Some(Ok(Message::Close(_))) => break,
            Some(Ok(Message::Text(txt))) => {
                let v: serde_json::Value = match serde_json::from_str(&txt) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "subscribe_offer" => {
                        let sdp = v["sdp"].as_str().unwrap_or_default();
                        let viewer = crate::live::room::ViewerInfo {
                            id: v["viewer_id"]
                                .as_str()
                                .filter(|s| !s.is_empty())
                                .unwrap_or(&sub_id)
                                .to_string(),
                            name: v["viewer_name"]
                                .as_str()
                                .filter(|s| !s.trim().is_empty())
                                .unwrap_or("Anonim")
                                .to_string(),
                            photo_url: v["viewer_photo"]
                                .as_str()
                                .filter(|s| !s.is_empty())
                                .map(String::from),
                        };
                        let resp =
                            match state.live_svc.subscribe_sdp(&room_id, &sub_id, sdp, viewer).await {
                                Ok(answer) => {
                                    subscribed = true;
                                    serde_json::json!({
                                        "type": "answer",
                                        "sdp": answer,
                                        "subscriber_id": sub_id,
                                    })
                                }
                                Err(e) => serde_json::json!({ "type": "error", "message": e }),
                            };
                        if socket
                            .send(Message::Text(resp.to_string().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    "candidate" if subscribed => {
                        let candidate = v["candidate"].as_str().unwrap_or_default();
                        let mid = v["sdp_mid"].as_str().unwrap_or_default();
                        let idx = v["sdp_mline_index"].as_u64().unwrap_or(0) as u32;
                        let _ = state
                            .live_svc
                            .subscribe_ice(&room_id, &sub_id, candidate, mid, idx)
                            .await;
                    }
                    "leave" => break,
                    _ => {}
                }
            }
            Some(Ok(_)) => {}
        }
            }
        }
    }

    // Penonton putus (tutup tab, klik Keluar, navigasi pergi):
    // lepas dari room agar hitungan penonton akurat.
    if subscribed {
        let _ = state.live_svc.remove_subscriber(&room_id, &sub_id).await;
    }
}

// ── Router ─────────────────────────────────────────────────────────────────────

pub fn live_router(state: Arc<AppState>) -> Router {
    // Route yang memerlukan autentikasi merchant/admin.
    let protected = Router::new()
        .route("/api/live/rooms", post(create_room))
        // HTTP signaling (dipertahankan untuk kompatibilitas)
        .route("/api/live/rooms/{room_id}/publish/sdp", post(publish_sdp))
        .route("/api/live/rooms/{room_id}/publish/ice", post(publish_ice))
        .route("/api/live/rooms/{room_id}", delete(stop_room))
        // WS signaling untuk publisher (auth via cookie — browser kirim otomatis)
        .route("/ws/live/publish/{room_id}", get(live_publish_ws))
        .layer(from_fn_with_state(state.clone(), require_auth));

    // Route publik (tidak perlu login).
    let public = Router::new()
        .route("/api/live/rooms", get(list_rooms))
        .route("/api/rtc/ice", get(ice_servers))
        .route("/ws/lives", get(lives_ws))
        .route("/api/live/rooms/{room_id}", get(get_room))
        // HTTP signaling (dipertahankan)
        .route(
            "/api/live/rooms/{room_id}/subscribe/sdp",
            post(subscribe_sdp),
        )
        .route(
            "/api/live/rooms/{room_id}/subscribe/ice",
            post(subscribe_ice),
        )
        .route(
            "/api/live/rooms/{room_id}/subscribe/{subscriber_id}",
            delete(leave_room),
        )
        // WS signaling untuk penonton (publik — tidak perlu login)
        .route("/ws/live/subscribe/{room_id}", get(live_subscribe_ws));

    Router::new()
        .merge(protected)
        .merge(public)
        .with_state(state)
}
