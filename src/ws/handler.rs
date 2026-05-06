//! ws/handler.rs — Per-connection handler.

use std::sync::Arc;

use axum::{
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
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
        manager::{MAX_CONNECTIONS, WsManager},
        proto::{WsClientMsg, WsEvent, WsMessage},
    },
};

/// Max concurrent DB ops per koneksi.
const MAX_CONCURRENT_OPS: usize = 4;

/// Max tasks queued waiting for DB semaphore per connection.
/// FIX: prevent unbounded task accumulation under message flood.
/// If client sends faster than we can process, drop excess messages
/// rather than queuing unlimited goroutines.
const MAX_QUEUED_OPS: usize = 16;

#[derive(Clone)]
pub struct WsAppState {
    pub jwt:       JwtService,
    pub ws_mgr:    Arc<WsManager>,
    pub group_svc: Arc<GroupChatService>,
}

#[derive(Deserialize)]
pub struct WsQuery {
    pub token: String,
}

// ── HTTP upgrade handler ──────────────────────────────────────────────────────

pub async fn ws_chat(
    ws:                 WebSocketUpgrade,
    Query(q):           Query<WsQuery>,
    State(state):       State<Arc<WsAppState>>,
) -> Response {
    let claims = match state.jwt.verify(&q.token) {
        Ok(c)  => c,
        Err(e) => {
            tracing::warn!(error=%e, "WS rejected: invalid JWT");
            return (StatusCode::UNAUTHORIZED, "Invalid token").into_response();
        }
    };

    // FIX: use MAX_CONNECTIONS constant instead of hardcoded 10_000
    if state.ws_mgr.online_count() >= MAX_CONNECTIONS {
        return (StatusCode::SERVICE_UNAVAILABLE, "Server at capacity").into_response();
    }

    ws.on_upgrade(move |socket| handle_socket(socket, state, claims))
}

// ── Socket handler ────────────────────────────────────────────────────────────

async fn handle_socket(socket: WebSocket, state: Arc<WsAppState>, claims: Claims) {
    let user_id   = claims.user_id.clone();
    let user_name = claims.name.clone();
    let role      = claims.role.clone();

    tracing::info!(user_id, role, "WS opened");

    let (mut outbound_rx, conn_cancel, _permit) = match state.ws_mgr.try_connect(&user_id) {
        Some(v) => v,
        None    => {
            tracing::warn!(user_id, "WS rejected: connection limit reached");
            return;
        }
    };

    let (mut sink, mut stream) = socket.split();

    // ── Hello ─────────────────────────────────────────────────────────────────
    let rooms: Vec<String> = state
        .group_svc
        .get_user_rooms(&user_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.id)
        .collect();

    let hello = WsEvent::Hello { user_id: user_id.clone(), rooms };
    if sink.send(Message::Text(hello.to_json().into())).await.is_err() {
        state.ws_mgr.disconnect(&user_id);
        return;
    }

    // ── Heartbeat ─────────────────────────────────────────────────────────────
    let (hb_tx, mut hb_rx) = tokio::sync::mpsc::channel::<Arc<str>>(4);
    let pong_tx = state.ws_mgr.spawn_heartbeat(user_id.clone(), hb_tx, conn_cancel.clone());

    // ── Write task ────────────────────────────────────────────────────────────
    let cancel_w = conn_cancel.clone();
    let write_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_w.cancelled() => break,

                msg = outbound_rx.recv() => match msg {
                    Some(json) => {
                        if sink.send(Message::Text(json.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                },

                hb = hb_rx.recv() => match hb {
                    Some(json) => {
                        if sink.send(Message::Text(json.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                },
            }
        }
        let _ = sink.close().await;
    });

    // ── Read loop ─────────────────────────────────────────────────────────────
    // FIX: bounded semaphore with try_acquire to prevent unbounded task accumulation.
    // Previously: every message spawned a task that awaited sem.acquire_owned().
    // Under flood, unlimited tasks piled up in memory waiting for the semaphore.
    // Now: if MAX_QUEUED_OPS tasks are already queued, drop excess messages
    // rather than queuing more tasks.
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_OPS + MAX_QUEUED_OPS));

    loop {
        tokio::select! {
            _ = conn_cancel.cancelled() => break,

            frame = stream.next() => {
                match frame {
                    None | Some(Err(_)) => break,
                    Some(Ok(msg)) => match msg {
                        Message::Text(text) => {
                            // FIX: non-blocking acquire — drop message if queue full
                            // rather than spawning unlimited waiting tasks
                            match semaphore.clone().try_acquire_owned() {
                                Ok(permit) => {
                                    let state2 = state.clone();
                                    let uid    = user_id.clone();
                                    let uname  = user_name.clone();
                                    let role2  = role.clone();
                                    tokio::spawn(async move {
                                        let _permit = permit; // released when task completes
                                        dispatch(&state2, &uid, &uname, &role2, &text).await;
                                    });
                                }
                                Err(_) => {
                                    // Queue full — drop this message, send error back
                                    tracing::warn!(user_id, "WS dispatch queue full, dropping message");
                                    state.ws_mgr.send_to(
                                        &user_id,
                                        WsEvent::err("OVERLOADED", "Too many requests, message dropped"),
                                    ).await;
                                }
                            }
                        }
                        Message::Pong(_) => { let _ = pong_tx.try_send(()); }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
            }
        }
    }

    // ── Cleanup ───────────────────────────────────────────────────────────────
    conn_cancel.cancel();
    let _ = write_task.await;
    state.ws_mgr.disconnect(&user_id);
    tracing::info!(user_id, "WS closed");
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

async fn dispatch(state: &WsAppState, user_id: &str, user_name: &str, role: &str, raw: &str) {
    let msg: WsClientMsg = match serde_json::from_str(raw) {
        Ok(m)  => m,
        Err(e) => {
            send_err(state, user_id, "PARSE_ERROR", &e.to_string()).await;
            return;
        }
    };

    match msg {
        WsClientMsg::Ping => {
            state.ws_mgr.send_to(user_id, WsEvent::Pong).await;
        }

        WsClientMsg::SendText { room_id, content, client_id } => {
            match state.group_svc.send_text(&room_id, user_id, user_name, role, &content).await {
                Ok(m) => {
                    state.ws_mgr.send_to(
                        user_id,
                        WsEvent::Ack {
                            msg_id:   m.id,
                            client_id,
                            sent_at:  m.sent_at.to_rfc3339(),
                        },
                    ).await
                }
                Err(e) => send_err(state, user_id, "SEND_FAILED", &e.to_string()).await,
            }
        }

        WsClientMsg::ShareTicket { room_id, ticket, caption, client_id } => {
            let cap = caption.as_deref().unwrap_or("");
            match state.group_svc.share_ticket(&room_id, user_id, user_name, role, ticket, cap).await {
                Ok(m) => {
                    state.ws_mgr.send_to(
                        user_id,
                        WsEvent::Ack {
                            msg_id:   m.id,
                            client_id,
                            sent_at:  m.sent_at.to_rfc3339(),
                        },
                    ).await
                }
                Err(e) => send_err(state, user_id, "SHARE_FAILED", &e.to_string()).await,
            }
        }

        WsClientMsg::GetHistory { room_id, limit, before_id } => {
            let limit = limit.unwrap_or(30).clamp(1, 100);
            match state.group_svc.get_history(&room_id, user_id, limit, before_id.as_deref()).await {
                Ok((msgs, has_more)) => {
                    state.ws_mgr.send_to(
                        user_id,
                        WsEvent::History {
                            room_id,
                            messages: msgs.iter().map(WsMessage::from_model).collect(),
                            has_more,
                        },
                    ).await
                }
                Err(e) => send_err(state, user_id, "HISTORY_FAILED", &e.to_string()).await,
            }
        }
    }
}

async fn send_err(state: &WsAppState, user_id: &str, code: &str, msg: &str) {
    state.ws_mgr.send_to(user_id, WsEvent::err(code, msg)).await;
}
