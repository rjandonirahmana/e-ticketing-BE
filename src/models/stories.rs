//! models/stories.rs
//!
//! Data models untuk fitur Stories dan Premium Subscription.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ── Story ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Story {
    pub id: String,
    pub user_id: String,
    pub media_url: String,
    pub media_type: String, // "image" | "video"
    pub filter: Option<String>,
    pub overlays: Vec<JsonValue>,
    pub event_id: Option<String>,
    pub event_slug: Option<String>,
    pub event_title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// Response item satu story — dikirim ke FE sebagai bagian dari StoryGroup.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryItemResponse {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub avatar_url: String,
    pub media_url: String,
    pub media_type: String,
    pub filter: Option<String>,
    pub overlays: Vec<JsonValue>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub viewed: bool,
    pub event_id: Option<String>,
    pub event_slug: Option<String>,
    pub event_title: Option<String>,
}

/// Response grup stories per user — sesuai ApiStoryGroup di FE.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoryGroupResponse {
    pub user_id: String,
    pub username: String,
    pub avatar_url: String,
    pub stories: Vec<StoryItemResponse>,
}

/// Response setelah upload story berhasil — sesuai UploadStoryResponse di FE.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadStoryResponse {
    pub story_id: String,
    pub media_url: String,
}

// ── Subscription ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSubscription {
    pub id: String,
    pub user_id: String,
    pub plan: String,
    pub started_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}
