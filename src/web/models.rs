use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Events ────────────────────────────────────────────────────────────────────

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
    pub display_price: f64,
    pub venue: Option<String>,
    pub city: Option<String>,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    #[serde(default)]
    pub category: Vec<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: String,
    pub total_sold: i32,
    pub total_quota: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventVariant {
    pub id: String,
    pub event_id: String,
    pub name: String,
    pub description: Option<String>,
    pub price: f64,
    pub sale_price: Option<f64>,
    #[serde(rename = "effective_price")]
    pub display_price: f64,
    pub quota: i32,
    #[serde(rename = "available")]
    pub remaining: i32,
    pub max_per_order: Option<i32>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    #[serde(default)]
    pub category: Vec<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: String,
    pub price: f64,
    pub sale_price: Option<f64>,
    pub display_price: f64,
    pub total_sold: i32,
    pub total_quota: i32,
    #[serde(default)]
    pub event_variants: Vec<EventVariant>,
}

/// Payload varian dari form create/edit event. Dikirim ke server fn sebagai
/// JSON string karena `crate::models` (tipe request server) tidak ter-compile
/// di WASM. `id` Some = update varian lama, None = varian baru.
/// `is_active: Some(false)` = varian lama "dihapus" dari form (dinonaktifkan,
/// bukan DELETE — tiket terjual bisa masih mereferensikannya).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantForm {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub price: f64,
    pub quota: i32,
    #[serde(default)]
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaginatedEvents {
    pub data: Vec<Event>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

// ── Merchant publik (profil + rating & reviews) ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantPublicProfile {
    pub merchant_id: String,
    pub store_name: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub verified: bool,
    pub followers: i64,
    pub events_count: i64,
    pub rating_avg: f64,
    pub rating_count: i64,
    /// Apakah viewer yang sedang login mem-follow merchant ini.
    #[serde(default)]
    pub is_following: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantReviewItem {
    pub user_name: String,
    pub rating: i32,
    pub comment: String,
    pub created_at: DateTime<Utc>,
}

/// Payload halaman reviews: ringkasan + daftar ulasan sekaligus (1 round-trip).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantReviewsData {
    pub store_name: String,
    pub avg: f64,
    pub total: i64,
    /// dist[0] = bintang 1 … dist[4] = bintang 5.
    pub dist: [i64; 5],
    pub items: Vec<MerchantReviewItem>,
}

// ── Banners ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Banner {
    pub id: i64,
    pub image_url: String,
    #[serde(rename = "click_url")]
    pub link_url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
}

// ── Users / Auth ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserResponse {
    pub id: String,
    pub email: Option<String>,
    pub name: String,
    pub phone: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    #[serde(alias = "token")]
    pub access_token: String,
    pub user: UserResponse,
}

// ── Tickets ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketResponse {
    pub id: String,
    pub ticket_code: String,
    pub status: String,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub order_id: String,
    pub order_code: String,
    pub event_id: String,
    pub event_name: String,
    pub event_slug: String,
    pub event_date: DateTime<Utc>,
    pub event_venue: Option<String>,
    pub event_city: Option<String>,
    pub variant_id: String,
    pub variant_name: String,
    pub unit_price: f64,
    pub cover_url: Option<String>,
}

// ── Orders ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderListItem {
    pub id: String,
    pub order_code: String,
    pub status: String,
    pub total_amount: f64,
    pub event_name: Option<String>,
    pub event_date: Option<DateTime<Utc>>,
    pub venue: Option<String>,
    pub cover_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    #[serde(default)]
    pub event_name: String,
    #[serde(default)]
    pub variant_name: String,
    pub quantity: i32,
    pub subtotal: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDetail {
    pub id: String,
    #[serde(default)]
    pub order_code: String,
    #[serde(default)]
    pub status: String,
    pub total_amount: f64,
    #[serde(default)]
    pub payment_method: Option<String>,
    #[serde(default)]
    pub paid_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub expired_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub items: Vec<OrderItem>,
}

// ── Notifications ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationItem {
    pub id: String,
    #[serde(default)]
    pub kind: String,
    pub title: String,
    pub body: String,
    pub is_read: bool,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

// ── Chat / Pulse ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatRoom {
    pub id: String,
    pub event_id: String,
    pub name: String,
    #[serde(default)]
    pub member_count: i32,
    #[serde(default)]
    pub last_message: Option<String>,
    #[serde(default)]
    pub unread_count: i32,
    #[serde(default)]
    pub cover_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub room_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub content: String,
    pub sent_at: u64,
    #[serde(default, alias = "msg_type")]
    pub message_type: String,
}

// ── Merchant Event Form ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MerchantEventForm {
    pub name: String,
    pub description: String,
    pub venue: String,
    pub city: String,
    pub event_date: String,
    pub start_time: String,
    pub end_time: String,
    pub categories: Vec<String>,
    pub cover_url: Option<String>,
}

// ── Cart & Checkout ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CartItem {
    pub event_id: String,
    pub tier_id: String,
    pub event_title: String,
    pub tier_name: String,
    pub venue_name: String,
    #[serde(default)]
    pub event_cover: String,
    pub quantity: i32,
    pub unit_price: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItemRef {
    pub event_name: String,
    pub variant_name: String,
    pub quantity: i32,
    pub subtotal: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRef {
    pub id: String,
    pub order_code: String,
    pub status: String,
    pub total_amount: i64,
    pub expired_at: Option<String>,
    pub created_at: Option<String>,
    pub items: Vec<OrderItemRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrderResponse {
    pub order: OrderRef,
    #[serde(default)]
    pub requires_redirect: bool,
    #[serde(default)]
    pub payment_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatePromoResponse {
    pub valid: bool,
    #[serde(default)]
    pub discount_idr: i64,
    #[serde(default)]
    pub message: String,
}

// ── Subscription ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingSubOrder {
    pub order_id: String,
    pub order_code: String,
    pub plan: String,
    pub amount_idr: i64,
}

// ── Scan ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanValidateResult {
    pub event_title: String,
    pub tier_name: String,
    pub status: String,
    pub ticket_code: String,
}

// ── Admin ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminStats {
    #[serde(default)]
    pub total_users: i64,
    #[serde(default)]
    pub total_events: i64,
    #[serde(default)]
    pub total_orders: i64,
    #[serde(default)]
    pub total_revenue: f64,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn format_price(price: f64) -> String {
    let p = price as i64;
    if p == 0 {
        return "Gratis".to_string();
    }
    let s = p.to_string();
    let chars: Vec<char> = s.chars().rev().collect();
    let grouped: String = chars
        .chunks(3)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(".")
        .chars()
        .rev()
        .collect();
    format!("Rp {}", grouped)
}

pub fn format_date(dt: &DateTime<Utc>) -> String {
    use chrono::Datelike;
    let months = [
        "Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des",
    ];
    format!(
        "{} {} {}",
        dt.day(),
        months[(dt.month() as usize) - 1],
        dt.year()
    )
}

pub fn format_datetime(dt: &DateTime<Utc>) -> String {
    use chrono::Timelike;
    format!(
        "{}, {:02}:{:02} WIB",
        format_date(dt),
        dt.hour(),
        dt.minute()
    )
}
