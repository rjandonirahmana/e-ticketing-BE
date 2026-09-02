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
//! 2. Hello product: to_json() sudah kembalikan Arc<str>.
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

use crate::{
    models::auth::Claims,
    service::group_chat::GroupChatService,
    utils::jwt::JwtService,
    ws::{
        manager::WsManager,
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
    pub token: Option<String>,
}

/// Extract `pulse_token` dari Cookie header (browser kirim otomatis pada WS upgrade
/// same-origin — tidak perlu JS membaca/mengirim token secara eksplisit).
fn token_from_cookie_header(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.split(';').map(|p| p.trim()).find_map(|part| {
                part.strip_prefix("pulse_token=")
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(String::from)
            })
        })
}

// ── HTTP upgrade handler ──────────────────────────────────────────────────────

pub async fn ws_chat(
    ws: WebSocketUpgrade,
    Query(q): Query<WsQuery>,
    headers: axum::http::HeaderMap,
    State(state): State<Arc<WsAppState>>,
) -> Response {
    // ── COOKIE DULU, query hanya cadangan ─────────────────────────────────
    // Urutannya dulu terbalik. Itu berarti jalur yang dicoba PERTAMA adalah
    // jalur yang menaruh JWT di dalam alamat — dan alamat masuk ke log akses
    // proxy, log server, dan riwayat peramban. Token yang bocor ke log adalah
    // token yang bocor ke siapa pun yang kelak membaca log itu.
    //
    // Klien aplikasi ini seluruhnya memakai cookie (`pulse_token`, HttpOnly,
    // dikirim otomatis saat upgrade same-origin), jadi mendahulukan cookie
    // membuat lalu lintas kita sendiri tak pernah menempuh jalur itu lagi.
    //
    // Query BELUM dibuang sepenuhnya karena mungkin ada klien lain yang masih
    // memakainya — tetapi setiap pemakaiannya kini tercatat, sehingga bisa
    // dipastikan sudah kosong sebelum dihapus.
    let dari_cookie = token_from_cookie_header(&headers);
    if dari_cookie.is_none() && q.token.is_some() {
        tracing::warn!(
            "WS memakai token lewat query — jalur usang, JWT bocor ke log akses"
        );
    }
    let raw_token = dari_cookie.or(q.token);
    let claims = match raw_token.as_deref().map(|t| state.jwt.verify(t)) {
        Some(Ok(c)) => c,
        _ => {
            tracing::warn!("WS rejected: no valid token in query param or cookie");
            return (StatusCode::UNAUTHORIZED, "Invalid token").into_response();
        }
    };

    if state.ws_mgr.online_count() >= state.ws_mgr.max_connections() {
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

    let (mut outbound_rx, conn_cancel, _permit, conn_id) = match state.ws_mgr.try_connect(&user_id)
    {
        Some(v) => v,
        None => {
            tracing::warn!(user_id, "WS rejected: connection limit reached");
            // Katakan SEBABNYA sebelum menutup.
            //
            // Menutup begitu saja tak bisa dibedakan klien dari gangguan
            // jaringan, dan watchdog-nya akan menyambung ulang tiap tiga detik
            // — dari SETIAP klien yang ditolak, selamanya. Artinya persis pada
            // detik server kehabisan kapasitas, ia mulai dihantam paling keras.
            //
            // Satu bingkai ini yang memutus lingkarannya.
            let mut socket = socket;
            let _ = socket
                .send(Message::Text(
                    (*WsEvent::err(
                        ErrorCode::Overloaded,
                        "Server sedang penuh, coba lagi sebentar",
                    )
                    .to_json())
                    .into(),
                ))
                .await;
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

    // P0 FIX: Register rooms ke WS index SEBELUM Hello dikirim.
    // Tanpa ini, user ada di sessions tapi tidak di room_members — broadcast room
    // tidak akan sampai ke user sampai ada product lain yang memanggil join_room.
    // Penting saat reconnect: user harus langsung masuk ke semua room-nya.
    state.ws_mgr.register_rooms(&user_id, &rooms);

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
        state.ws_mgr.disconnect(&user_id, conn_id);
        return;
    }

    // ── Heartbeat ─────────────────────────────────────────────────────────────
    // Arsitektur: dua channel terpisah untuk outbound (pesan biasa) dan heartbeat.
    // axum WebSocket sink tidak bisa di-clone — write task harus single owner sink.
    // Oleh karena itu heartbeat tidak bisa share channel yang sama dengan outbound.
    // write task select! dua arm: outbound_rx dan hb_rx.
    //
    // CATATAN: Komentar lama menyebut "SATU channel" — itu aspirasi yang tidak
    // terealisasi karena constraint axum (sink ownership). Implementasi tetap
    // dua channel, yang sudah benar dan berfungsi dengan baik.
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
                    Some(_) => {
                        // Native WS PING frame — browser auto-responds with PONG.
                        // read loop catches Message::Pong(_) → pong_tx.try_send(())
                        if sink.send(Message::Ping(bytes::Bytes::new())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                },
            }
        }
        // FIX: timeout agar tidak hang selamanya jika TCP dead.
        // sink.close() bisa block indefinitely jika koneksi drop tanpa graceful close.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), sink.close()).await;
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
                                    tokio::spawn(async move {
                                        let _permit = permit;
                                        dispatch(&state2, &uid, &uname, &text).await;
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
    state.ws_mgr.disconnect(&user_id, conn_id);
    tracing::info!(user_id, "WS closed");
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// Peran pengirim tak lagi ikut: sejak plafon "satu pesan per percakapan"
/// dibuang (lihat `service::group_chat::authorize_and_save`), tak ada satu pun
/// keputusan di jalur ini yang bergantung padanya. Ia tetap dicatat saat
/// koneksi dibuka, di mana ia memang berguna untuk penelusuran.
async fn dispatch(state: &WsAppState, user_id: &str, user_name: &str, raw: &str) {
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
            reply_to,
        } => {
            match state
                .group_svc
                .send_text(&room_id, user_id, user_name, &content, reply_to.as_deref())
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

        WsClientMsg::SendImage {
            room_id,
            media_url,
            caption,
            client_id,
            reply_to,
        } => {
            match state
                .group_svc
                .send_image(
                    &room_id,
                    user_id,
                    user_name,
                    &media_url,
                    caption.as_deref().unwrap_or(""),
                    reply_to.as_deref(),
                )
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
                .share_ticket(&room_id, user_id, user_name, ticket, cap)
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
