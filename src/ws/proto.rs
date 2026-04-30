//! ws/proto.rs — JSON protocol antara Leptos client dan WS server.
//!
//! CLIENT → SERVER  : `WsClientMsg`  (serde tag = "type")
//! SERVER → CLIENT  : `WsEvent`      (serde tag = "type")

use crate::models::group_chat::{GroupMessage, TicketCard};
use serde::{Deserialize, Serialize};

// ── Client → Server ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMsg {
    /// Kirim pesan teks
    SendText {
        room_id: String,
        content: String,
        client_id: Option<String>,
    },

    /// Share ticket card
    ShareTicket {
        room_id: String,
        ticket: TicketCard,
        caption: Option<String>,
        client_id: Option<String>,
    },

    /// Minta history pesan (cursor-based)
    GetHistory {
        room_id: String,
        limit: Option<i64>,
        before_id: Option<String>,
    },

    /// Ping keepalive
    Ping,
}

// ── Server → Client ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    /// Pesan baru di salah satu room user
    NewMessage(WsMessage),

    /// ACK setelah send berhasil
    Ack {
        msg_id: String,
        client_id: Option<String>,
        sent_at: String,
    },

    /// Error dari operasi client
    Error { code: String, message: String },

    /// Response GetHistory
    History {
        room_id: String,
        messages: Vec<WsMessage>,
        has_more: bool,
    },

    /// Welcome saat pertama connect
    Hello { user_id: String, rooms: Vec<String> },

    /// Server-initiated Ping untuk heartbeat
    Ping,

    /// Pong response ke client Ping
    Pong,
}

impl WsEvent {
    pub fn err(code: impl Into<String>, msg: impl Into<String>) -> Self {
        WsEvent::Error {
            code: code.into(),
            message: msg.into(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|e| format!(r#"{{"type":"error","code":"SERIALIZE","message":"{e}"}}"#))
    }
}

// ── Message DTO ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsMessage {
    pub id: String,
    pub room_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub msg_type: String,
    pub content: String,
    pub media_url: Option<String>,
    pub ticket_card: Option<TicketCard>,
    pub sent_at: String,
    pub is_system: bool,
}

impl WsMessage {
    pub fn from_model(m: &GroupMessage) -> Self {
        Self {
            id: m.id.clone(),
            room_id: m.room_id.clone(),
            sender_id: m.sender_id.clone(),
            sender_name: m.sender_name.clone(),
            msg_type: m.msg_type.as_str().to_string(),
            content: m.content.clone(),
            media_url: m.media_url.clone(),
            ticket_card: m.ticket_card.clone(),
            sent_at: m.sent_at.to_rfc3339(),
            is_system: m.is_system,
        }
    }
}
