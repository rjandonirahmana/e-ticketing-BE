//! meet/api.rs — Control plane "meet" (konferensi P2P mesh).
//!
//! - `POST /api/meet/rooms`        (auth merchant/admin) → buat room.
//! - `GET  /api/meet/rooms/{id}`   (publik)              → info ringkas room.
//! - `GET  /ws/meet/{room_id}`     (publik)              → signaling + admit.
//!
//! WS-nya publik supaya tamu pemegang link undangan bisa connect tanpa login.
//! Otorisasi HOST diverifikasi di dalam handler (cookie JWT `pulse_token`):
//! hanya merchant/admin yang `user_id`-nya == `host_id` room yang jadi host.

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::middleware::auth::AuthUser;
use crate::state::AppState;

fn ok<T: Serialize>(data: T) -> Response {
    (StatusCode::OK, Json(json!({ "data": data }))).into_response()
}

fn err(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

// ── HTTP: buat room (host = merchant/admin) ────────────────────────────────────

async fn create_room(auth: AuthUser, State(state): State<Arc<AppState>>) -> Response {
    if auth.require_role("merchant").is_err() && auth.require_role("admin").is_err() {
        return err(StatusCode::FORBIDDEN, "Hanya merchant yang bisa membuat meet");
    }
    let info = match state.meet_svc.create_room(auth.id(), auth.name()) {
        Ok(i) => i,
        Err(e) => return err(StatusCode::SERVICE_UNAVAILABLE, &e),
    };
    let join_path = format!("/meet/{}", info.room_id);
    ok(json!({ "room_id": info.room_id, "join_path": join_path, "host_name": info.host_name }))
}

async fn get_room(Path(room_id): Path<String>, State(state): State<Arc<AppState>>) -> Response {
    match state.meet_svc.info(&room_id) {
        Some(info) => ok(info),
        None => err(StatusCode::NOT_FOUND, "Meet tidak ditemukan atau sudah berakhir"),
    }
}

// ── Cookie JWT (salinan lokal dari middleware::auth — fn-nya privat) ───────────

fn cookie_token(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').map(str::trim).find_map(|p| {
        p.strip_prefix(&format!("{name}="))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from)
    })
}

// ── WS signaling ──────────────────────────────────────────────────────────────

async fn meet_ws(
    ws: WebSocketUpgrade,
    Path(room_id): Path<String>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Response {
    // Verifikasi host SEBELUM upgrade: ambil klaim dari cookie bila ada.
    // Disimpan sebagai (user_id, name) — dipakai hanya jika klien minta `as_host`.
    let claims = cookie_token(&headers, "pulse_token")
        .and_then(|t| state.jwt.verify(&t).ok())
        .map(|c| (c.user_id, c.name, c.role));
    ws.on_upgrade(move |socket| meet_ws_loop(socket, room_id, state, claims))
}

/// Apakah pesan keluar bersifat terminal (klien harus menutup koneksi sesudahnya)?
fn is_terminal(msg: &str) -> bool {
    serde_json::from_str::<Value>(msg)
        .ok()
        .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(String::from))
        .map(|t| t == "denied" || t == "meeting_ended")
        .unwrap_or(false)
}

async fn meet_ws_loop(
    mut socket: WebSocket,
    room_id: String,
    state: Arc<AppState>,
    claims: Option<(String, String, String)>,
) {
    let peer_id = uuid::Uuid::new_v4().to_string();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();

    let mut is_host = false;

    // ── Tahap 1: tunggu pesan `join` pertama ──────────────────────────────────
    loop {
        match socket.recv().await {
            Some(Ok(Message::Text(txt))) => {
                let v: Value = match serde_json::from_str(&txt) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if v.get("type").and_then(|t| t.as_str()) != Some("join") {
                    continue;
                }
                let want_host = v.get("as_host").and_then(|b| b.as_bool()).unwrap_or(false);
                let req_name = v
                    .get("name")
                    .and_then(|n| n.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from);
                let photo = v
                    .get("photo")
                    .and_then(|p| p.as_str())
                    .filter(|s| !s.is_empty())
                    .map(String::from);

                let Some(room) = state.meet_svc.get_room(&room_id) else {
                    let _ = socket
                        .send(Message::Text(
                            json!({ "type": "error", "message": "Meet tidak ditemukan" })
                                .to_string()
                                .into(),
                        ))
                        .await;
                    return;
                };

                if want_host {
                    // Host wajib: punya cookie valid, role merchant/admin, dan
                    // user_id == host_id room.
                    let authorized = claims
                        .as_ref()
                        .map(|(uid, _, role)| {
                            uid == &room.host_id && (role == "merchant" || role == "admin")
                        })
                        .unwrap_or(false);
                    if !authorized {
                        let _ = socket
                            .send(Message::Text(
                                json!({ "type": "error", "message": "Bukan host meet ini" })
                                    .to_string()
                                    .into(),
                            ))
                            .await;
                        return;
                    }
                    let name = claims
                        .as_ref()
                        .map(|(_, n, _)| n.clone())
                        .filter(|n| !n.trim().is_empty())
                        .unwrap_or_else(|| "Host".to_string());
                    is_host = true;
                    if !state.meet_svc.register_peer(
                        &room_id, &peer_id, &name, photo, true, out_tx.clone(),
                    ) {
                        let _ = socket
                            .send(Message::Text(
                                json!({ "type": "error", "message": "Meet tidak ditemukan" })
                                    .to_string()
                                    .into(),
                            ))
                            .await;
                        return;
                    }
                    // Beri host snapshot: peserta admitted lain + waiting room.
                    let _ = socket
                        .send(Message::Text(
                            json!({
                                "type": "joined",
                                "self_id": peer_id,
                                "as_host": true,
                                "peers": room.admitted_peers(Some(&peer_id)),
                                "pending": room.pending_peers(),
                            })
                            .to_string()
                            .into(),
                        ))
                        .await;
                } else {
                    // Tamu → waiting room. Beri tahu host ada permintaan masuk.
                    let name = req_name.unwrap_or_else(|| "Tamu".to_string());
                    if !state.meet_svc.register_peer(
                        &room_id,
                        &peer_id,
                        &name,
                        photo.clone(),
                        false,
                        out_tx.clone(),
                    ) {
                        let _ = socket
                            .send(Message::Text(
                                json!({ "type": "error", "message": "Ruang meet penuh" })
                                    .to_string()
                                    .into(),
                            ))
                            .await;
                        return;
                    }
                    let _ = socket
                        .send(Message::Text(
                            json!({ "type": "waiting", "self_id": peer_id })
                                .to_string()
                                .into(),
                        ))
                        .await;
                    if let Some(htx) = room.host_tx() {
                        let _ = htx.send(
                            json!({
                                "type": "join_request",
                                "peer_id": peer_id,
                                "name": name,
                                "photo": photo,
                            })
                            .to_string(),
                        );
                    }
                }
                break;
            }
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
            _ => {}
        }
    }

    // ── Tahap 2: loop signaling + kontrol ─────────────────────────────────────
    loop {
        tokio::select! {
            // Pesan dari server/peer lain → teruskan ke browser ini.
            out = out_rx.recv() => {
                match out {
                    Some(msg) => {
                        let terminal = is_terminal(&msg);
                        if socket.send(Message::Text(msg.into())).await.is_err() {
                            break;
                        }
                        if terminal {
                            break;
                        }
                    }
                    None => break,
                }
            }
            // Pesan dari browser ini.
            inbound = socket.recv() => {
                match inbound {
                    None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                    Some(Ok(Message::Text(txt))) => {
                        let v: Value = match serde_json::from_str(&txt) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                            // ── Host meng-admit tamu ──────────────────────────
                            "admit" if is_host => {
                                let Some(target) = v.get("peer_id").and_then(|p| p.as_str()) else { continue };
                                if let Some((info, others)) = state.meet_svc.admit(&room_id, target) {
                                    if let Some(room) = state.meet_svc.get_room(&room_id) {
                                        // Tamu yang baru diizinkan: kirim daftar peer
                                        // lain → dia yang initiate offer (anti-glare).
                                        room.send_to(
                                            target,
                                            &json!({
                                                "type": "admitted",
                                                "self_id": target,
                                                "peers": others,
                                            })
                                            .to_string(),
                                        );
                                        // Peserta lain (termasuk host) diberi tahu.
                                        room.broadcast_admitted(
                                            &json!({
                                                "type": "peer_joined",
                                                "peer_id": info.id,
                                                "name": info.name,
                                                "photo": info.photo,
                                            })
                                            .to_string(),
                                            Some(target),
                                        );
                                    }
                                }
                            }
                            // ── Host menolak tamu ─────────────────────────────
                            "deny" if is_host => {
                                let Some(target) = v.get("peer_id").and_then(|p| p.as_str()) else { continue };
                                if let Some(room) = state.meet_svc.get_room(&room_id) {
                                    room.send_to(target, &json!({ "type": "denied" }).to_string());
                                    // Peer target akan menutup koneksinya sendiri saat
                                    // menerima "denied" (cleanup-nya menghapus dari room).
                                }
                            }
                            // ── Relay signaling mesh (SDP / ICE) ──────────────
                            "signal" => {
                                let Some(target) = v.get("to").and_then(|p| p.as_str()) else { continue };
                                let data = v.get("data").cloned().unwrap_or(Value::Null);
                                if let Some(room) = state.meet_svc.get_room(&room_id) {
                                    // Hanya peer admitted yang boleh saling signal.
                                    let sender_ok = room.peers.get(&peer_id).map(|p| p.admitted).unwrap_or(false);
                                    let target_ok = room.peers.get(target).map(|p| p.admitted).unwrap_or(false);
                                    if sender_ok && target_ok {
                                        room.send_to(
                                            target,
                                            &json!({ "type": "signal", "from": peer_id, "data": data })
                                                .to_string(),
                                        );
                                    }
                                }
                            }
                            // ── Status media (mic/kamera on-off) → broadcast ──
                            "state" => {
                                if let Some(room) = state.meet_svc.get_room(&room_id) {
                                    let admitted = room
                                        .peers
                                        .get(&peer_id)
                                        .map(|p| p.admitted)
                                        .unwrap_or(false);
                                    if admitted {
                                        let mic = v.get("mic").and_then(|b| b.as_bool()).unwrap_or(true);
                                        let cam = v.get("cam").and_then(|b| b.as_bool()).unwrap_or(true);
                                        room.broadcast_admitted(
                                            &json!({
                                                "type": "peer_state",
                                                "peer_id": peer_id,
                                                "mic": mic,
                                                "cam": cam,
                                            })
                                            .to_string(),
                                            Some(&peer_id),
                                        );
                                    }
                                }
                            }
                            // ── Chat teks di dalam meet → broadcast ──────────
                            "chat" => {
                                if let Some(room) = state.meet_svc.get_room(&room_id) {
                                    let admitted = room
                                        .peers
                                        .get(&peer_id)
                                        .map(|p| p.admitted)
                                        .unwrap_or(false);
                                    let text = v
                                        .get("text")
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("")
                                        .trim();
                                    if admitted && !text.is_empty() {
                                        // Batasi panjang agar tidak bisa disalahgunakan.
                                        let text: String = text.chars().take(1000).collect();
                                        let name = room
                                            .peers
                                            .get(&peer_id)
                                            .map(|p| p.name.clone())
                                            .unwrap_or_default();
                                        room.broadcast_admitted(
                                            &json!({
                                                "type": "chat",
                                                "peer_id": peer_id,
                                                "name": name,
                                                "text": text,
                                            })
                                            .to_string(),
                                            None, // termasuk pengirim → dia lihat pesannya sendiri
                                        );
                                    }
                                }
                            }
                            "leave" => break,
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // ── Cleanup: lepas peer & beri tahu yang lain ─────────────────────────────
    let Some(room) = state.meet_svc.get_room(&room_id) else {
        return;
    };
    let was_admitted = room.peers.get(&peer_id).map(|p| p.admitted).unwrap_or(false);
    room.peers.remove(&peer_id);

    if is_host {
        // Host pergi → bubarkan meet: kabari semua, lalu hapus room.
        room.broadcast_admitted(&json!({ "type": "meeting_ended" }).to_string(), None);
        // Tamu yang masih waiting juga ditutup.
        for p in room.pending_peers() {
            room.send_to(&p.id, &json!({ "type": "meeting_ended" }).to_string());
        }
        state.meet_svc.end_room(&room_id);
    } else if was_admitted {
        room.broadcast_admitted(
            &json!({ "type": "peer_left", "peer_id": peer_id }).to_string(),
            None,
        );
    } else {
        // Tamu batal dari waiting room → beri tahu host agar daftar diperbarui.
        if let Some(htx) = room.host_tx() {
            let _ = htx.send(json!({ "type": "pending_left", "peer_id": peer_id }).to_string());
        }
    }
}

// ── Router ─────────────────────────────────────────────────────────────────────

pub fn meet_router(state: Arc<AppState>) -> Router {
    use crate::middleware::auth::require_auth;
    use axum::middleware::from_fn_with_state;

    let protected = Router::new()
        .route("/api/meet/rooms", post(create_room))
        .layer(from_fn_with_state(state.clone(), require_auth));

    let public = Router::new()
        .route("/api/meet/rooms/{room_id}", get(get_room))
        .route("/ws/meet/{room_id}", get(meet_ws));

    Router::new().merge(protected).merge(public).with_state(state)
}
