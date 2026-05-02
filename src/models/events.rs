use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub price: f64,
    pub sale_price: Option<f64>,
    pub sale_price_start_date: Option<DateTime<Utc>>,
    pub sale_price_end_date: Option<DateTime<Utc>>,
    pub venue: Option<String>,
    #[serde(default)]
    pub category: Vec<String>,
    pub city: Option<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct EventWithVariants {
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    #[serde(default)]
    pub category: Vec<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub event_variants: Vec<crate::models::event_variants::EventVariantResponse>,
}

/// Variant inline di dalam CreateEventRequest — tidak perlu API terpisah.
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateVariantInline {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    pub description: Option<String>,
    #[validate(range(min = 0.0))]
    pub price: f64,
    #[validate(range(min = 1))]
    pub quota: i32,
    pub max_per_order: Option<i32>,
    pub sort_order: Option<i32>,
}

/// Satu hit: event + variants + image sekaligus.
/// FE kirim multipart: field "data" (JSON) + field "image" (file opsional).
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateEventRequest {
    /// Nama merchant — dipakai untuk generate slug, tidak disimpan di events
    #[validate(length(min = 1, max = 80))]
    pub merchant_name: String,
    #[validate(length(min = 2, max = 255))]
    pub name: String,
    pub description: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    #[serde(default)]
    pub category: Vec<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    /// Min 1 variant wajib ada
    #[validate(length(min = 1))]
    pub variants: Vec<CreateVariantInline>,
}

/// Variant inline untuk update — id ada = update, id None = tambah baru.
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateVariantInline {
    pub id: Option<String>,
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub price: Option<f64>,
    pub quota: Option<i32>,
    pub max_per_order: Option<i32>,
    pub is_active: Option<bool>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateEventRequest {
    #[validate(length(min = 2, max = 255))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub event_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub category: Vec<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: Option<String>,
    /// Opsional — kirim jika mau update/tambah variant sekaligus
    pub variants: Option<Vec<UpdateVariantInline>>,
}

#[derive(Debug, Deserialize)]
pub struct EventListQuery {
    pub city: Option<String>,
    pub status: Option<String>,
    pub category: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedEvents {
    pub data: Vec<Event>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}
