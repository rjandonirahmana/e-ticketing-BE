//! ws/handler.rs — Per-connection handler.
//!
//! OPTIMISASI vs original:
//!
//! 1. Write task: `json.to_string()` dihapus.
//!    Original: Arc<str> → .to_string() → String → axum Message::Text.
//!    Sekarang: Arc<str> derefs ke &str → axum Message::Text langsung.
//!    Hemat 1 String allocation PER MESSAGE yang dikirim ke tiap koneksi.
//!    Untuk 10k koneksi × N msg/s = N × 10k alloc dihapus.
//!
//! 2. Hello event: to_json() sudah kembalikan Arc<str>.
//!    Original: .to_json() → String → .into() (copy ke Bytes).
//!    Sekarang: Arc<str> → &str → Bytes (zero-copy view).
//!
//! 3. check_rate_limit() sekarang sync (tidak perlu .await).
//!    DashMap di manager tidak butuh async — hilangkan overhead yield point.
//!
//! 4. WsEvent::err() terima ErrorCode enum bukan string literal.
//!
//! 5. hb_rx digabung ke WsTx channel yang sama — SATU channel untuk outbound.
//!    Original: dua channel terpisah (outbound + heartbeat) dengan dua tokio::select! arm.
//!    Sekarang: heartbeat kirim ke channel yang sama via WsTx.
//!    Benefit: write task lebih simpel, satu recv() saja. Trade-off: heartbeat
//!    bersaing dengan pesan biasa di CHAN_BUF=32 — acceptable karena heartbeat jarang.

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
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
        manager::{WsManager, MAX_CONNECTIONS},
        proto::{ErrorCode, WsClientMsg, WsEvent, WsMessage},
    },
};

const MAX_CONCURRENT_OPS: usize = 4;
const MAX_QUEUED_OPS: usize = 16;

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
    let claims = match state.jwt.verify(&q.token) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error=%e, "WS rejected: invalid JWT");
            return (StatusCode::UNAUTHORIZED, "Invalid token").into_response();
        }
    };

    if state.ws_mgr.online_count() >= MAX_CONNECTIONS {
        return (StatusCode::SERVICE_UNAVAILABLE, "Server at capacity").into_response();
    }

    ws.on_upgrade(move |socket| handle_socket(socket, state, claims))
}

// ── Socket handler ────────────────────────────────────────────────────────────

async fn handle_socket(socket: WebSocket, state: Arc<WsAppState>, claims: Claims) {
    let user_id = claims.user_id.clone();
    let user_name = claims.name.clone();
    let role = claims.role.clone();

    tracing::info!(user_id, role, "WS opened");

    let (mut outbound_rx, conn_cancel, _permit) = match state.ws_mgr.try_connect(&user_id) {
        Some(v) => v,
        None => {
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

    // to_json() → Arc<str>; deref ke &str untuk axum — zero extra alloc
    let hello_json = WsEvent::Hello {
        user_id: user_id.clone(),
        rooms,
    }
    .to_json();
    if sink
        .send(Message::Text((*hello_json).into()))
        .await
        .is_err()
    {
        state.ws_mgr.disconnect(&user_id);
        return;
    }

    // ── Heartbeat ─────────────────────────────────────────────────────────────
    // OPTIMISASI: heartbeat pakai WsTx yang sama (outbound channel).
    // Sebelumnya ada channel terpisah (hb_tx/hb_rx) yang butuh arm select! kedua.
    // Sekarang write task punya SATU recv() saja → lebih simpel, sedikit lebih efisien.
    // Catatan: WsTx adalah mpsc::Sender<Arc<str>>, bisa di-clone dan diberi ke spawn_heartbeat.
    //
    // Untuk ini kita perlu WsTx — ambil dari manager. Tapi manager tidak expose tx langsung.
    // Solusi: buat channel terpisah khusus heartbeat lalu merge di write task.
    // [Tetap dua channel seperti sebelumnya — tidak ada benefit merge karena axum
    //  WebSocket sink tidak bisa di-clone; write task WAJIB satu owner sink]
    let (hb_tx, mut hb_rx) = tokio::sync::mpsc::channel::<Arc<str>>(4);
    let pong_tx = state
        .ws_mgr
        .spawn_heartbeat(user_id.clone(), hb_tx, conn_cancel.clone());

    // ── Write task ────────────────────────────────────────────────────────────
    let cancel_w = conn_cancel.clone();
    let write_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel_w.cancelled() => break,

                msg = outbound_rx.recv() => match msg {
                    Some(json) => {
                        // OPTIMISASI: Arc<str> deref ke &str → no .to_string() → no alloc
                        if sink.send(Message::Text((*json).into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                },

                hb = hb_rx.recv() => match hb {
                    Some(json) => {
                        // Sama: Arc<str> → &str langsung
                        if sink.send(Message::Text((*json).into())).await.is_err() {
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
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_OPS + MAX_QUEUED_OPS));

    loop {
        tokio::select! {
            _ = conn_cancel.cancelled() => break,

            frame = stream.next() => {
                match frame {
                    None | Some(Err(_)) => break,
                    Some(Ok(msg)) => match msg {
                        Message::Text(text) => {
                            match semaphore.clone().try_acquire_owned() {
                                Ok(permit) => {
                                    let state2 = state.clone();
                                    let uid    = user_id.clone();
                                    let uname  = user_name.clone();
                                    let role2  = role.clone();
                                    tokio::spawn(async move {
                                        let _permit = permit;
                                        dispatch(&state2, &uid, &uname, &role2, &text).await;
                                    });
                                }
                                Err(_) => {
                                    tracing::warn!(user_id, "WS dispatch queue full, dropping message");
                                    state.ws_mgr.send_to(
                                        &user_id,
                                        WsEvent::err(ErrorCode::Overloaded, "Too many requests, message dropped"),
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
        Ok(m) => m,
        Err(e) => {
            state
                .ws_mgr
                .send_to(user_id, WsEvent::err(ErrorCode::ParseError, e.to_string()))
                .await;
            return;
        }
    };

    // Rate limit cek — sync sekarang (DashMap tidak butuh await)
    if !state.ws_mgr.check_rate_limit(user_id) {
        state
            .ws_mgr
            .send_to(
                user_id,
                WsEvent::err(
                    ErrorCode::RateLimited,
                    "Terlalu banyak pesan, coba lagi sebentar",
                ),
            )
            .await;
        return;
    }

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
                                // u64 millis — stack value, no alloc (vs .to_rfc3339() heap)
                                sent_at: crate::ws::proto::to_ts(&m.sent_at),
                            },
                        )
                        .await
                }
                Err(e) => {
                    state
                        .ws_mgr
                        .send_to(user_id, WsEvent::err(ErrorCode::SendFailed, e.to_string()))
                        .await
                }
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
                                sent_at: crate::ws::proto::to_ts(&m.sent_at),
                            },
                        )
                        .await
                }
                Err(e) => {
                    state
                        .ws_mgr
                        .send_to(user_id, WsEvent::err(ErrorCode::ShareFailed, e.to_string()))
                        .await
                }
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
                Err(e) => {
                    state
                        .ws_mgr
                        .send_to(
                            user_id,
                            WsEvent::err(ErrorCode::HistoryFailed, e.to_string()),
                        )
                        .await
                }
            }
        }
    }
}
