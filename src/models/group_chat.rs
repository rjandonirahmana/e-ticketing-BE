use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── GroupRoom ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupRoom {
    pub id: String,          // ULID hex
    pub event_id: String,    // event yang punya room ini
    pub name: String,
    pub cover_url: Option<String>,
    pub created_by: String,  // user_id merchant
    pub created_at: DateTime<Utc>,
    pub member_count: i64,
}

// ── GroupMember ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemberRole {
    Owner,
    Member,
}

impl MemberRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemberRole::Owner  => "owner",
            MemberRole::Member => "member",
        }
    }
    pub fn from_str(s: &str) -> Self {
        if s == "owner" { MemberRole::Owner } else { MemberRole::Member }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMember {
    pub room_id:    String,
    pub user_id:    String,
    pub user_name:  String,
    pub role:       MemberRole,
    pub joined_at:  DateTime<Utc>,
}

// ── GroupMessage ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MsgType {
    Text,
    Image,
    SharedTicket,
    System,
}

impl MsgType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MsgType::Text         => "text",
            MsgType::Image        => "image",
            MsgType::SharedTicket => "shared_ticket",
            MsgType::System       => "system",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "image"         => MsgType::Image,
            "shared_ticket" => MsgType::SharedTicket,
            "system"        => MsgType::System,
            _               => MsgType::Text,
        }
    }
}

/// Embedded ticket card di dalam pesan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketCard {
    pub event_name: String,
    pub venue:      String,
    pub price:      String,
    pub tier:       String,
    pub ticket_id:  String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMessage {
    pub id:           String,
    pub room_id:      String,
    pub sender_id:    String,
    pub sender_name:  String,
    pub msg_type:     MsgType,
    pub content:      String,
    pub media_url:    Option<String>,
    pub ticket_card:  Option<TicketCard>,
    pub sent_at:      DateTime<Utc>,
    pub is_system:    bool,
}

// ── REST request/response bodies ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateRoomReq {
    pub name:      String,
    pub cover_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SendTextReq {
    pub content:   String,
    pub client_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ShareTicketReq {
    pub ticket:    TicketCard,
    pub message:   Option<String>,
    pub client_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    pub limit:     i64,
    pub before_id: Option<String>,
}

fn default_limit() -> i64 { 30 }
