//! ws/handler.rs — upgrade HTTP → WebSocket, satu task per connection.
//!
//! Auth: JWT di query param `?token=<JWT>`
//! Protocol: JSON (WsClientMsg / WsEvent)

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

use crate::{
    models::auth::Claims,
    service::group_chat::GroupChatService,
    utils::jwt::JwtService,
    ws::{
        manager::WsManager,
        proto::{WsClientMsg, WsEvent, WsMessage},
    },
};

// ── Shared state ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct WsAppState {
    pub jwt: JwtService,
    pub ws_mgr: Arc<WsManager>,
    pub group_svc: Arc<GroupChatService>,
}

// ── Query params ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct WsQuery {
    pub token: String,
}

// ── HTTP upgrade handler ──────────────────────────────────────────────────────

/// GET /ws/chat?token=<JWT>
pub async fn ws_chat(
    ws: WebSocketUpgrade,
    Query(q): Query<WsQuery>,
    State(state): State<Arc<WsAppState>>,
) -> Response {
    let claims = match state.jwt.verify(&q.token) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error=%e, "WS rejected: invalid JWT");
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                "Invalid or expired token",
            )
                .into_response();
        }
    };

    let state_c = state.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, state_c, claims))
}

// ── Socket handler ────────────────────────────────────────────────────────────

async fn handle_socket(socket: WebSocket, state: Arc<WsAppState>, claims: Claims) {
    let user_id = claims.user_id.clone();
    let user_name = claims.name.clone();
    let role = claims.role.clone();

    tracing::info!(user_id, role, "WS opened");

    let mut outbound_rx = state.ws_mgr.connect(&user_id);

    // Auto-track rooms
    let rooms: Vec<String> = state
        .group_svc
        .get_user_rooms(&user_id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.id)
        .collect();

    let (mut sink, mut stream) = socket.split();

    // Send Hello
    let hello = WsEvent::Hello {
        user_id: user_id.clone(),
        rooms: rooms.clone(),
    };
    if sink
        .send(Message::Text(hello.to_json().into()))
        .await
        .is_err()
    {
        state.ws_mgr.disconnect(&user_id);
        return;
    }

    // Write task: outbound_rx → ws sink
    let uid_w = user_id.clone();
    let write_task = tokio::spawn(async move {
        while let Some(json) = outbound_rx.recv().await {
            if sink.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
        tracing::debug!(user_id = uid_w, "WS write task ended");
    });

    // Read loop
    while let Some(Ok(msg)) = stream.next().await {
        match msg {
            Message::Text(text) => {
                dispatch(&state, &user_id, &user_name, &role, &text).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    write_task.abort();
    state.ws_mgr.disconnect(&user_id);
    tracing::info!(user_id, "WS closed");
}

// ── Message dispatcher ────────────────────────────────────────────────────────

async fn dispatch(state: &Arc<WsAppState>, user_id: &str, user_name: &str, role: &str, raw: &str) {
    let msg: WsClientMsg = match serde_json::from_str(raw) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(user_id, error=%e, "WS parse error");
            send_err(state, user_id, "PARSE_ERROR", &e.to_string()).await;
            return;
        }
    };

    match msg {
        WsClientMsg::Ping => {
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
                        .await;
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
                        .await;
                }
                Err(e) => send_err(state, user_id, "SHARE_FAILED", &e.to_string()).await,
            }
        }

        WsClientMsg::GetHistory {
            room_id,
            limit,
            before_id,
        } => {
            let limit = limit.unwrap_or(30);
            match state
                .group_svc
                .get_history(&room_id, user_id, limit, before_id.as_deref())
                .await
            {
                Ok((msgs, has_more)) => {
                    let messages = msgs.iter().map(WsMessage::from_model).collect();
                    state
                        .ws_mgr
                        .send_to(
                            user_id,
                            WsEvent::History {
                                room_id,
                                messages,
                                has_more,
                            },
                        )
                        .await;
                }
                Err(e) => send_err(state, user_id, "HISTORY_FAILED", &e.to_string()).await,
            }
        }
    }
}

async fn send_err(state: &WsAppState, user_id: &str, code: &str, msg: &str) {
    state.ws_mgr.send_to(user_id, WsEvent::err(code, msg)).await;
}

// Needed for the 401 response
use axum::response::IntoResponse;
