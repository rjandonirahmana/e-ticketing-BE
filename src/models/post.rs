//! models/post.rs
//!
//! Data model untuk Marketplace C2C (jual-beli COD): posting oleh SEMUA user
//! terdaftar (bukan cuma merchant), bisa disukai & dikomentari siapa saja.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
    pub user_avatar: String,
    pub kind: String,        // "jual" | "cari"
    pub title: String,
    pub description: String,
    pub price: Option<i64>,
    pub condition: Option<String>, // "baru" | "bekas"
    pub category: Option<String>,
    pub city: Option<String>,
    pub images: Vec<JsonValue>,    // [{"url": "..."}]
    pub status: String,            // "active" | "sold" | "archived"
    pub like_count: i32,
    pub comment_count: i32,
    /// Hanya diisi untuk pemanggil yang login (detail view) — apakah viewer
    /// sudah menyukai post ini. `None` di listing (tak dihitung per-baris).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liked_by_viewer: Option<bool>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostComment {
    pub id: String,
    pub post_id: String,
    pub user_id: String,
    pub user_name: String,
    pub user_avatar: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PaginatedPosts {
    pub data: Vec<Post>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

#[derive(Debug, Clone)]
pub struct PaginatedComments {
    pub data: Vec<PostComment>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

/// Input pembuatan post — divalidasi di service layer (bukan derive
/// `validator::Validate` di sini karena aturannya SALING BERGANTUNG:
/// `condition` wajib ada bila kind="jual" dan wajib kosong bila kind="cari",
/// sesuatu yang tak bisa dinyatakan lewat validator per-field biasa).
#[derive(Debug, Clone)]
pub struct CreatePostRequest {
    pub kind: String,
    pub title: String,
    pub description: String,
    pub price: Option<i64>,
    pub condition: Option<String>,
    pub category: Option<String>,
    pub city: Option<String>,
    pub images: Vec<JsonValue>,
}
