//! ws/handler.rs — HTTP upgrade → WebSocket per-connection handler.
//!
//! ## Fixes dari versi sebelumnya
//! 1. write_task di-abort tanpa `await` → task bisa linger setelah socket tutup.
//!    Sekarang pakai `CancellationToken` + `select!` agar bersih.
//! 2. Tidak ada heartbeat → stale connections tidak terdeteksi.
//!    Sekarang spawn heartbeat task via `WsManager::spawn_heartbeat`.
//! 3. Pong dari client tidak dihandle → heartbeat tidak bisa konfirmasi hidup.
//!    Sekarang `Message::Pong` forward ke pong_tx.
//! 4. Semua dispatch di-await di read loop → jika DB lambat, message backlog
//!    menumpuk. Sekarang dispatch di-spawn sebagai task terpisah dengan
//!    semaphore untuk limit concurrency.

use std::sync::Arc;

use axum::{
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use crate::{
    models::auth::Claims,
    service::group_chat::GroupChatService,
    utils::jwt::JwtService,
    ws::{
        manager::WsManager,
        proto::{WsClientMsg, WsEvent, WsMessage},
    },
};

// ── Concurrency limit untuk dispatch ─────────────────────────────────────────
/// Max concurrent DB operations per connection.
const MAX_CONCURRENT_OPS: usize = 4;

// ── Shared state ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct WsAppState {
    pub jwt: JwtService,
    pub ws_mgr: Arc<WsManager>,
    pub group_svc: Arc<GroupChatService>,
}

#[derive(Deserialize)]
pub struct WsQuery {
    pub token: String,
}

// ── HTTP upgrade handler ──────────────────────────────────────────────────────

pub async fn ws_chat(
    ws: WebSocketUpgrade,
    Query(q): Query<WsQuery>,
    State(state): State<Arc<WsAppState>>,
) -> Response {
    use axum::response::IntoResponse;

    let claims = match state.jwt.verify(&q.token) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error=%e, "WS rejected: invalid JWT");
            return (axum::http::StatusCode::UNAUTHORIZED, "Invalid token").into_response();
        }
    };

    ws.on_upgrade(move |socket| handle_socket(socket, state, claims))
}

// ── Socket handler ────────────────────────────────────────────────────────────

async fn handle_socket(socket: WebSocket, state: Arc<WsAppState>, claims: Claims) {
    use axum::response::IntoResponse;

    let user_id = claims.user_id.clone();
    let user_name = claims.name.clone();
    let role = claims.role.clone();

    tracing::info!(user_id, role, "WS opened");

    // CancellationToken per koneksi — dicancel oleh heartbeat timeout atau close
    let conn_cancel = CancellationToken::new();

    // Register session → dapat outbound receiver
    let (mut outbound_rx, _) = state.ws_mgr.connect(&user_id);

    // Split socket
    let (mut sink, mut stream) = socket.split();

    // ── 1. Send Hello ─────────────────────────────────────────────────────────
    let rooms: Vec<String> = state
        .group_svc
        .get_user_rooms(&user_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.id)
        .collect();

    let hello = WsEvent::Hello {
        user_id: user_id.clone(),
        rooms,
    };
    if sink
        .send(Message::Text(hello.to_json().into()))
        .await
        .is_err()
    {
        state.ws_mgr.disconnect(&user_id);
        return;
    }

    // ── 2. Clone tx untuk heartbeat ───────────────────────────────────────────
    // Kita butuh tx untuk heartbeat task tapi manager sudah simpan tx.
    // Buat channel kedua kecil khusus heartbeat.
    let (hb_tx, mut hb_rx) = tokio::sync::mpsc::channel::<String>(4);
    let pong_tx = state
        .ws_mgr
        .spawn_heartbeat(user_id.clone(), hb_tx, conn_cancel.clone());

    // ── 3. Write task ─────────────────────────────────────────────────────────
    // Forward outbound_rx + heartbeat channel → WS sink.
    let cancel_w = conn_cancel.clone();
    let write_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_w.cancelled() => break,

                // Pesan dari manager (fanout/direct)
                msg = outbound_rx.recv() => {
                    match msg {
                        Some(json) => {
                            if sink.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                        None => break, // channel closed
                    }
                }

                // Pesan dari heartbeat task (Ping)
                hb_msg = hb_rx.recv() => {
                    match hb_msg {
                        Some(json) => {
                            if sink.send(Message::Text(json.into())).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
        // Pastikan sink di-close dengan bersih
        let _ = sink.close().await;
    });

    // ── 4. Read loop ──────────────────────────────────────────────────────────
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_OPS));

    loop {
        tokio::select! {
            _ = conn_cancel.cancelled() => break,

            frame = stream.next() => {
                match frame {
                    None | Some(Err(_)) => break,
                    Some(Ok(msg)) => {
                        match msg {
                            Message::Text(text) => {
                                // Spawn dispatch agar read loop tidak blocking
                                let state2    = state.clone();
                                let uid       = user_id.clone();
                                let uname     = user_name.clone();
                                let role2     = role.clone();
                                let sem       = semaphore.clone();

                                tokio::spawn(async move {
                                    let _permit = sem.acquire_owned().await;
                                    dispatch(&state2, &uid, &uname, &role2, &text).await;
                                });
                            }
                            Message::Pong(_) => {
                                // Client balas Pong → kasih tahu heartbeat task
                                let _ = pong_tx.try_send(());
                            }
                            Message::Close(_) => break,
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // ── 5. Cleanup ────────────────────────────────────────────────────────────
    conn_cancel.cancel(); // stop heartbeat task
    write_task.abort(); // stop write task
    state.ws_mgr.disconnect(&user_id);
    tracing::info!(user_id, "WS closed and cleaned up");
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

async fn dispatch(state: &WsAppState, user_id: &str, user_name: &str, role: &str, raw: &str) {
    let msg: WsClientMsg = match serde_json::from_str(raw) {
        Ok(m) => m,
        Err(e) => {
            send_err(state, user_id, "PARSE_ERROR", &e.to_string()).await;
            return;
        }
    };

    match msg {
        WsClientMsg::Ping => {
            // Client explicit Ping (berbeda dari frame-level Ping)
            state.ws_mgr.send_to(user_id, WsEvent::Pong).await;
        }

        WsClientMsg::SendText {
            room_id,
            content,
            client_id,
        } => {
            match state
                .group_svc
                .send_text(&room_id, user_id, user_name, role, &content)
                .await
            {
                Ok(m) => {
                    state
                        .ws_mgr
                        .send_to(
                            user_id,
                            WsEvent::Ack {
                                msg_id: m.id,
                                client_id,
                                sent_at: m.sent_at.to_rfc3339(),
                            },
                        )
                        .await
                }
                Err(e) => send_err(state, user_id, "SEND_FAILED", &e.to_string()).await,
            }
        }

        WsClientMsg::ShareTicket {
            room_id,
            ticket,
            caption,
            client_id,
        } => {
            let cap = caption.as_deref().unwrap_or("");
            match state
                .group_svc
                .share_ticket(&room_id, user_id, user_name, role, ticket, cap)
                .await
            {
                Ok(m) => {
                    state
                        .ws_mgr
                        .send_to(
                            user_id,
                            WsEvent::Ack {
                                msg_id: m.id,
                                client_id,
                                sent_at: m.sent_at.to_rfc3339(),
                            },
                        )
                        .await
                }
                Err(e) => send_err(state, user_id, "SHARE_FAILED", &e.to_string()).await,
            }
        }

        WsClientMsg::GetHistory {
            room_id,
            limit,
            before_id,
        } => {
            let limit = limit.unwrap_or(30).clamp(1, 100);
            match state
                .group_svc
                .get_history(&room_id, user_id, limit, before_id.as_deref())
                .await
            {
                Ok((msgs, has_more)) => {
                    state
                        .ws_mgr
                        .send_to(
                            user_id,
                            WsEvent::History {
                                room_id,
                                messages: msgs.iter().map(WsMessage::from_model).collect(),
                                has_more,
                            },
                        )
                        .await
                }
                Err(e) => send_err(state, user_id, "HISTORY_FAILED", &e.to_string()).await,
            }
        }
    }
}

async fn send_err(state: &WsAppState, user_id: &str, code: &str, msg: &str) {
    state.ws_mgr.send_to(user_id, WsEvent::err(code, msg)).await;
}
