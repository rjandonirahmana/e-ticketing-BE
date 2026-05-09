use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Satu foto detail event (denah, seat map, info harga, dll.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailImageEntry {
    pub url: String,
    /// "map" | "seat" | "price" | "other"
    pub image_type: String,
    pub caption: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    /// Array foto detail event (denah, seat map, dll.)
    #[serde(default)]
    pub detail_images: Vec<DetailImageEntry>,
    /// Harga base termurah dari variant aktif.
    pub price: f64,
    /// Harga sale termurah yang sedang aktif (None jika tidak ada sale).
    pub sale_price: Option<f64>,
    pub sale_price_start_date: Option<DateTime<Utc>>,
    pub sale_price_end_date: Option<DateTime<Utc>>,
    /// Harga efektif untuk ditampilkan di list:
    /// sale_price jika aktif, else price.
    pub display_price: f64,
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
    pub total_sold: i32,
    pub total_quota: i32,
}

#[derive(Debug, Serialize)]
pub struct EventWithVariants {
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    /// Array foto detail event (denah, seat map, dll.)
    #[serde(default)]
    pub detail_images: Vec<DetailImageEntry>,
    pub venue: Option<String>,
    pub city: Option<String>,
    #[serde(default)]
    pub category: Vec<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    /// Harga base variant termurah (berdasarkan effective price).
    pub price: f64,
    /// Harga sale aktif variant termurah (None jika tidak ada sale aktif).
    pub sale_price: Option<f64>,
    pub sale_price_start_date: Option<DateTime<Utc>>,
    pub sale_price_end_date: Option<DateTime<Utc>>,
    /// Harga efektif untuk ditampilkan: sale_price jika aktif, else price.
    pub display_price: f64,
    pub total_sold: i32,
    pub total_quota: i32,
    pub event_variants: Vec<crate::models::event_variants::EventVariantResponse>,
}

/// Variant inline di dalam CreateEventRequest.
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateVariantInline {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    pub description: Option<String>,
    #[validate(range(min = 0.0))]
    pub price: f64,
    pub sale_price: Option<f64>,
    pub sale_price_start_date: Option<DateTime<Utc>>,
    pub sale_price_end_date: Option<DateTime<Utc>>,
    #[validate(range(min = 1))]
    pub quota: i32,
    pub max_per_order: Option<i32>,
    pub sort_order: Option<i32>,
}

/// Satu hit: event + variants + image sekaligus.
/// FE kirim multipart: field "data" (JSON) + field "image" (file opsional).
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateEventRequest {
    /// Nama merchant — dipakai untuk generate slug.
    #[validate(length(min = 1, max = 80))]
    pub merchant_name: String,
    #[validate(length(min = 2, max = 255))]
    pub name: String,
    pub description: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    /// Flat Vec<String> — FE kirim sebagai ["Musik", "Festival"]
    #[serde(default)]
    pub category: Vec<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    /// Min 1 variant wajib ada.
    #[validate(length(min = 1))]
    pub variants: Vec<CreateVariantInline>,
    /// Foto detail event yang sudah di-upload ke storage.
    #[serde(default)]
    pub detail_images: Vec<DetailImageEntry>,
}

/// Variant inline untuk update — id ada = update, id None = tambah baru.
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateVariantInline {
    pub id: Option<String>,
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub price: Option<f64>,
    pub sale_price: Option<f64>,
    pub sale_price_start_date: Option<DateTime<Utc>>,
    pub sale_price_end_date: Option<DateTime<Utc>>,
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
    /// Hanya admin yang boleh set status selain "edited".
    /// Merchant: field ini di-ignore oleh route handler (di-hardcode "edited").
    /// Admin: dikirim via PUT /api/admin/events/:id/status
    pub status: Option<String>,
    /// Foto detail — None = tidak berubah, Some(vec) = replace seluruhnya.
    pub detail_images: Option<Vec<DetailImageEntry>>,
    /// Opsional — kirim jika mau update/tambah variant sekaligus.
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
