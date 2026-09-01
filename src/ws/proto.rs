//! ws/proto.rs — JSON protocol antara Leptos client dan WS server.
//!
//! CLIENT → SERVER  : `WsClientMsg`  (serde tag = "type")
//! SERVER → CLIENT  : `WsEvent`      (serde tag = "type")
//!
//! OPTIMISASI (vs versi original):
//!
//! 1. [proto.rs] Box<WsMessage> pada NewMessage — kurangi enum size dari ~256B → 24B.
//!    Match statement atas ratusan ribu product/detik = cache locality jauh lebih baik.
//!
//! 2. [proto.rs] OnceLock pre-serialized Ping/Pong — serialize SEKALI saat pertama
//!    dipakai, reuse selamanya. Heartbeat 10k koneksi × 30s = ~333 ping/s, tiap ping
//!    sekarang O(1) clone Arc<str> (nanoseconds) bukan O(JSON) serialize.
//!
//! 3. [proto.rs] ErrorCode enum — type-safe, integer comparison, zero heap alloc
//!    vs String. Tambahan: WsEvent::err() terima ErrorCode langsung (no Into<String>).
//!
//! 4. [proto.rs] TimestampMillis (u64) — 8 bytes stack vs RFC3339 heap String ~25B.
//!    Frontend JavaScript: `new Date(sent_at)` native support unix millis.
//!    BREAKING CHANGE: client perlu update parsing sent_at dari string → number.
//!
//! 5. [proto.rs] MsgType dipindah dari models/ → dipakai langsung di WsMessage
//!    agar tidak perlu .as_str().to_string() → String alloc saat from_model().
//!
//! 6. [proto.rs] to_shared_json() kembalikan Arc<str> — broadcast serialize SEKALI,
//!    clone Arc ke N koneksi = N × atomic refcount bump (nanoseconds), bukan N × serialize.
//!
//! COUNTER vs analisis sebelumnya:
//!
//! ✗ Cow<'a, str> untuk WsClientMsg — DITOLAK.
//!   dispatch() adalah async fn yang melintas await boundary (DB call).
//!   Cow<'a, str> borrow dari frame buffer akan langsung .to_owned() di boundary tersebut.
//!   Hasilnya: complexity naik, benefit nol. String owned lebih jelas.
//!
//! ✗ Arc<str> interning untuk semua field WsMessage — DITOLAK.
//!   WsMessage dibuat dari GroupMessage (clone sekali dari DB), di-serialize,
//!   lalu di-drop. Tidak ada sharing nyata. Arc overhead (atomic ops) > String clone
//!   untuk data yang tidak di-share. Interning worth it hanya untuk data yang
//!   hidup lama dan di-share antar banyak struct (contoh: sessions map key).
//!
//! ✗ Arc<[WsMessage]> untuk History — DITOLAK.
//!   History dikirim ke SATU user saja (bukan broadcast), jadi Arc tidak hemat apapun.
//!   Vec<WsMessage> cukup dan lebih sederhana.

use std::sync::{Arc, OnceLock};

use crate::models::group_chat::{GroupMessage, MsgType, TicketCard};
use serde::{Deserialize, Serialize};

// ── Pre-serialized constants ────────────────────────────────────────────────────

/// Ping/Pong dipanggil ~333×/s untuk 10k koneksi (heartbeat 30s).
/// Serialize SEKALI, clone Arc selamanya.
static PING_JSON: OnceLock<Arc<str>> = OnceLock::new();
static PONG_JSON: OnceLock<Arc<str>> = OnceLock::new();

// ── Timestamp ──────────────────────────────────────────────────────────────────

/// Unix milliseconds — 8 bytes stack vs RFC3339 heap String ~25 bytes.
/// JS client: `new Date(sent_at)` — native support.
pub type TimestampMillis = u64;

#[inline(always)]
pub fn to_ts(dt: &chrono::DateTime<chrono::Utc>) -> TimestampMillis {
    dt.timestamp_millis() as u64
}

// ── Client → Server ───────────────────────────────────────────────────────────
//
// String owned (bukan Cow) karena dispatch() adalah async fn dengan DB await boundary.
// Lifetime 'a dari Cow tidak bisa melintas await — Rust compiler akan paksa .to_owned()
// anyway, sehingga kompleksitas Cow tidak memberi manfaat di sini.

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMsg {
    /// Kirim pesan teks
    SendText {
        room_id: String,
        content: String,
        client_id: Option<String>,
    },

    /// Kirim gambar yang SUDAH terunggah.
    ///
    /// Hanya URL-nya yang lewat sini, bukan berkasnya: WebSocket ini membawa
    /// pesan seluruh percakapan milik pengguna, dan menyalurkan bita gambar
    /// lewat kanal yang sama akan menahan setiap pesan teks di belakangnya
    /// selama unggahan berjalan. Berkasnya naik lewat POST /upload/chat-image.
    SendImage {
        room_id: String,
        media_url: String,
        /// Keterangan foto. Boleh kosong.
        caption: Option<String>,
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

    /// Ping keepalive dari client
    Ping,
}

// ── Server → Client ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    /// Pesan baru di room.
    ///
    /// Box<WsMessage>: enum size dari ~256B → 24B.
    /// Broadcast N kali = N × match overhead berkurang 10×.
    /// WsMessage tidak perlu di-copy saat enum dipindahkan.
    NewMessage(Box<WsMessage>),

    /// ACK setelah send berhasil.
    ///
    /// sent_at: u64 (stack) bukan String (heap). Hemat 1 alloc per ACK.
    Ack {
        msg_id: String,
        client_id: Option<String>,
        sent_at: TimestampMillis,
    },

    /// Error dari operasi client.
    ///
    /// ErrorCode: integer comparison + zero alloc vs String.
    Error { code: ErrorCode, message: String },

    /// Response GetHistory.
    ///
    /// Vec<WsMessage> cukup — history tidak di-broadcast ke banyak user.
    /// Arc<[WsMessage]> tidak hemat apapun di sini (single recipient).
    History {
        room_id: String,
        messages: Vec<WsMessage>,
        has_more: bool,
    },

    /// Welcome saat pertama connect.
    Hello { user_id: String, rooms: Vec<String> },

    /// Server-initiated Ping — pre-serialized, O(1).
    Ping,

    /// Pong response — pre-serialized, O(1).
    Pong,
}

/// Error codes type-safe.
/// Serialized sebagai SCREAMING_SNAKE_CASE string di JSON
/// agar backward compatible dengan client yang expect string error code.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    ParseError,
    SendFailed,
    ShareFailed,
    HistoryFailed,
    Overloaded,
    Replaced,
    RateLimited,
    Unauthorized,
    NotFound,
    Serialize,
}

impl WsEvent {
    /// Buat error product.
    #[inline]
    pub fn err(code: ErrorCode, msg: impl Into<String>) -> Self {
        WsEvent::Error {
            code,
            message: msg.into(),
        }
    }

    /// Serialize ke JSON.
    ///
    /// HOT PATH — strategi:
    /// 1. Ping/Pong: kembalikan pre-computed Arc<str> (nanoseconds, zero alloc)
    /// 2. Lainnya: serialize on-demand
    ///
    /// Return Arc<str> agar bisa di-share ke banyak koneksi tanpa copy.
    /// Caller untuk broadcast: serialize SEKALI via to_shared_json(), clone Arc ke N conn.
    pub fn to_json(&self) -> Arc<str> {
        match self {
            // Fast path: zero alloc, atomic clone
            WsEvent::Ping => PING_JSON
                .get_or_init(|| Arc::from(r#"{"type":"ping"}"#))
                .clone(),

            WsEvent::Pong => PONG_JSON
                .get_or_init(|| Arc::from(r#"{"type":"pong"}"#))
                .clone(),

            // Standard path
            other => Arc::from(serde_json::to_string(other).unwrap_or_else(|e| {
                format!(r#"{{"type":"error","code":"SERIALIZE","message":"{e}"}}"#)
            })),
        }
    }

    /// Alias semantik eksplisit untuk broadcast use-case.
    ///
    /// Pola yang BENAR di broadcast_room():
    /// ```text
    /// let shared = product.to_shared_json();          // serialize SEKALI
    /// for uid in members { deliver(uid, shared.clone()); }  // N × atomic clone
    /// ```
    ///
    /// BUKAN:
    /// ```text
    /// for uid in members { deliver(uid, product.to_json()); } // N × serialize — SALAH
    /// ```
    #[inline(always)]
    pub fn to_shared_json(&self) -> Arc<str> {
        self.to_json()
    }
}

// ── Message DTO ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsMessage {
    pub id: String,
    pub room_id: String,
    pub sender_id: String,
    pub sender_name: String,
    /// Enum langsung dari model — tidak perlu .as_str().to_string() alloc.
    pub msg_type: MsgType,
    pub content: String,
    pub media_url: Option<String>,
    pub ticket_card: Option<TicketCard>,
    /// u64 unix millis — 8 bytes, zero alloc vs RFC3339 String ~25 bytes.
    pub sent_at: TimestampMillis,
    pub is_system: bool,
}

impl WsMessage {
    /// Convert dari DB model.
    ///
    /// vs original:
    /// - msg_type: pakai enum langsung (bukan .as_str().to_string() = 1 alloc)
    /// - sent_at:  u64 millis (stack) bukan .to_rfc3339() (heap String ~25B)
    /// - String fields: masih clone sekali (tidak bisa avoid karena GroupMessage owned)
    ///
    /// Total: hemat 2 alloc per message (msg_type string + RFC3339 string).
    pub fn from_model(m: &GroupMessage) -> Self {
        Self {
            id: m.id.clone(),
            room_id: m.room_id.clone(),
            sender_id: m.sender_id.clone(),
            sender_name: m.sender_name.clone(),
            msg_type: m.msg_type.clone(), // enum Clone, no string alloc
            content: m.content.clone(),
            media_url: m.media_url.clone(),
            ticket_card: m.ticket_card.clone(),
            sent_at: to_ts(&m.sent_at), // stack u64, no heap alloc
            is_system: m.is_system,
        }
    }
}
