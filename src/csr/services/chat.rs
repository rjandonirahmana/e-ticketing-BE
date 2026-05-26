//! services/chat.rs — Group chat: REST API + WebSocket ke backend Axum
//!
//! Backend endpoints (prefix /api):
//!   GET  /chat/rooms                        — list rooms yang di-join user
//!   GET  /chat/events/:event_id/room        — get/init room untuk event
//!   POST /chat/rooms/:room_id/join          — join room
//!   GET  /chat/rooms/:room_id/history       — history pesan
//!   GET  /chat/rooms/:room_id/sent_count    — berapa pesan user sudah kirim
//!   WS   /ws/chat?token=<JWT>              — WebSocket

use gloo_storage::{LocalStorage, Storage};
use serde::{Deserialize, Serialize};

use super::client::{get_private, post_private, ApiError, TOKEN_KEY};

// ── Custom sent_at deserializer ───────────────────────────────────────────────
//
// Backend mengirim sent_at dalam DUA format berbeda:
//   - WS NewMessage  → u64 unix millis  (WsMessage::from_model → to_ts())
//   - REST /history  → ISO 8601 string  (GroupMessage via serde_json default)
//
// FIX: satu deserializer yang handle keduanya → tidak perlu dua struct terpisah.
// Urutan coba: Number → u64 langsung. String → parse RFC3339 → timestamp_millis.
mod sent_at_de {
    use serde::{Deserialize, Deserializer, de::Error};

    pub fn deserialize<'de, D>(de: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let val = serde_json::Value::deserialize(de)?;
        match val {
            // WS path: sudah u64 millis
            serde_json::Value::Number(n) => {
                n.as_u64().ok_or_else(|| D::Error::custom("sent_at number bukan u64"))
            }
            // REST history path: ISO 8601 string → millis
            serde_json::Value::String(s) => {
                // Coba RFC3339 dulu (format standar chrono/serde)
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                    return Ok(dt.timestamp_millis() as u64);
                }
                // Fallback: format tanpa timezone offset (contoh: "2026-04-30T08:42:26.087987Z")
                // sudah di-cover RFC3339 di atas, tapi jaga-jaga
                Err(D::Error::custom(format!("sent_at string tidak bisa di-parse: {s}")))
            }
            other => Err(D::Error::custom(format!(
                "sent_at harus Number atau String, dapat: {other}"
            ))),
        }
    }
}

// ── Models ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GroupRoom {
    pub id: String,
    pub event_id: String,
    pub name: String,
    pub cover_url: Option<String>,
    pub created_by: String,
    pub member_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsGroupMessage {
    pub id: String,
    pub room_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub msg_type: String,
    pub content: String,
    pub media_url: Option<String>,
    pub ticket_card: Option<TicketCard>,
    /// Custom deserializer: handle u64 millis (WS) DAN ISO string (REST history).
    /// Backend mengirim format berbeda dari dua path — deserializer ini
    /// transparently konversi keduanya ke u64 millis untuk fmt_time().
    #[serde(deserialize_with = "sent_at_de::deserialize")]
    pub sent_at: u64,
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketCard {
    pub event_name: String,
    pub venue: String,
    pub price: String,
    pub tier: String,
    pub ticket_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SentCountResponse {
    pub count: i64,
}

// ── Wrapper response dari backend ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct DataWrap<T> {
    data: T,
}

// ── REST API calls ─────────────────────────────────────────────────────────────

/// GET /chat/rooms — ambil semua room yang user sudah join
pub async fn get_my_rooms() -> Result<Vec<GroupRoom>, ApiError> {
    let wrap: DataWrap<Vec<GroupRoom>> = get_private("/chat/rooms").await?;
    Ok(wrap.data)
}

/// GET /chat/events/:event_id/room — init/get room untuk event tertentu
pub async fn get_event_room(event_id: &str) -> Result<GroupRoom, ApiError> {
    let path = format!("/chat/events/{}/room", event_id);
    let wrap: DataWrap<GroupRoom> = get_private(&path).await?;
    Ok(wrap.data)
}

/// POST /chat/rooms/:room_id/join — join room (dipanggil setelah bayar)
pub async fn join_room(room_id: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    struct Empty {}
    let path = format!("/chat/rooms/{}/join", room_id);
    let _: serde_json::Value = post_private(&path, &Empty {}).await?;
    Ok(())
}

/// Auto-join group setelah bayar event.
/// 1. Cari room event → 2. Join
/// Non-fatal: error diabaikan agar tidak block navigasi ke /tickets
pub async fn join_event_group(event_id: &str) -> Result<(), ApiError> {
    let room = get_event_room(event_id).await?;
    join_room(&room.id).await
}

/// GET /chat/rooms/:room_id/history
pub async fn get_history(
    room_id: &str,
    limit: i64,
    before_id: Option<&str>,
) -> Result<(Vec<WsGroupMessage>, bool), ApiError> {
    let mut path = format!("/chat/rooms/{}/history?limit={}", room_id, limit);
    if let Some(bid) = before_id {
        path.push_str(&format!("&before_id={}", bid));
    }

    #[derive(Deserialize)]
    struct HistoryData {
        messages: Vec<WsGroupMessage>,
        has_more: bool,
    }
    let wrap: DataWrap<HistoryData> = get_private(&path).await?;
    Ok((wrap.data.messages, wrap.data.has_more))
}

/// GET /chat/rooms/:room_id/sent_count
pub async fn get_sent_count(room_id: &str) -> Result<i64, ApiError> {
    let path = format!("/chat/rooms/{}/sent_count", room_id);
    let wrap: DataWrap<SentCountResponse> = get_private(&path).await?;
    Ok(wrap.data.count)
}

// ── WebSocket ─────────────────────────────────────────────────────────────────

/// Buat WebSocket URL dengan JWT token sebagai query param.
/// Pakai KINETIC_API_BASE_URL agar konsisten dengan REST API — tidak depend window.location.
pub fn ws_url() -> Option<String> {
    let token = LocalStorage::get::<String>(TOKEN_KEY).ok()?;

    // BUG FIX: KINETIC_API_BASE_URL=/api (relative, default build tanpa env) →
    // setelah trim menghasilkan empty host → URL "ws:///api/ws/chat" (invalid, triple slash).
    //
    // Fix: kalau setelah trim hasilnya bukan absolute URL (tidak mulai "http"),
    // fallback ke window.location untuk dapat proto + host yang benar.
    // Ini juga otomatis handle ws→wss di localhost (http) vs domain (https).
    let _raw_base = option_env!("KINETIC_API_BASE_URL").unwrap_or("/api");
    let _raw_trimmed = _raw_base.trim_end_matches('/');
    // FIX: strip_suffix untuk exact match "/api" — bukan set-of-chars match
    let raw = _raw_trimmed.strip_suffix("/api").unwrap_or(_raw_trimmed);
    // raw: "https://ulalaapi.store", "http://127.0.0.1:8080", atau "" (kalau relative)

    let (ws_proto, host) = if raw.starts_with("http") {
        // Absolute URL — extract protocol dan host
        let proto = if raw.starts_with("https") {
            "wss"
        } else {
            "ws"
        };
        // FIX: strip_prefix untuk exact scheme removal
        let h = raw
            .strip_prefix("https://")
            .or_else(|| raw.strip_prefix("http://"))
            .unwrap_or(raw);
        (proto, h.to_string())
    } else {
        // Relative atau empty — ambil dari window.location (local dev / no env set)
        let window = web_sys::window()?;
        let location = window.location();
        let page_proto = location.protocol().ok()?;
        let page_host = location.host().ok()?;
        let proto = if page_proto == "https:" { "wss" } else { "ws" };
        (proto, page_host)
    };

    Some(format!(
        "{}://{}/api/ws/chat?token={}",
        ws_proto, host, token
    ))
}

// ── WS JSON message types (mirror dari backend ws/proto.rs) ──────────────────

/// Pesan dari client ke server
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMsg<'a> {
    SendText {
        room_id: &'a str,
        content: &'a str,
        client_id: Option<&'a str>,
    },
    ShareTicket {
        room_id: &'a str,
        ticket: &'a TicketCard,
        caption: Option<&'a str>,
        client_id: Option<&'a str>,
    },
    GetHistory {
        room_id: &'a str,
        limit: Option<i64>,
        before_id: Option<&'a str>,
    },
    Ping,
}

impl<'a> WsClientMsg<'a> {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Event dari server ke client
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    NewMessage(WsGroupMessage),
    Ack {
        msg_id: String,
        client_id: Option<String>,
        sent_at: String,
    },
    Error {
        code: String,
        message: String,
    },
    History {
        room_id: String,
        messages: Vec<WsGroupMessage>,
        has_more: bool,
    },
    Hello {
        user_id: String,
        rooms: Vec<String>,
    },
    /// Server-initiated heartbeat ping — wajib dibalas dengan WsClientMsg::Ping
    Ping,
    /// Server balas ping kita — tidak perlu aksi
    Pong,
}
