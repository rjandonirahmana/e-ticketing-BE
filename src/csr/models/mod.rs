use serde::{Deserialize, Serialize};

// ─── Auth ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub id: String,
    pub full_name: String,
    pub email: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub avatar_url: String,
    #[serde(default)]
    pub active_tickets: i32,
    #[serde(default)]
    pub points: i32,
    #[serde(default = "default_tier")]
    pub membership_tier: String,
    /// Role dari backend: "customer" | "merchant" | "admin"
    #[serde(default)]
    pub role: String,
}

fn default_tier() -> String {
    "STANDARD".into()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub phone: String,
    pub password: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub user: UserProfile,
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    pub full_name: String,
    /// Email is optional on the backend.
    #[serde(default)]
    pub email: String,
    pub phone: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterResponse {
    pub success: bool,
    #[serde(default)]
    pub message: String,
}

/// OTP verification payload (used after `register`).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyOtpRequest {
    pub phone: String,
    pub otp: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoutRequest {
    pub access_token: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct EmailRequest {
    pub email: String,
}

// ─── Events / Tickets ────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Venue {
    pub name: String,
    #[serde(default)]
    pub address: String,
    pub city: String,
    #[serde(default)]
    pub latitude: f64,
    #[serde(default)]
    pub longitude: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub role: String,
    #[serde(default)]
    pub image_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TicketTier {
    pub id: String,
    #[serde(default = "default_tier")]
    pub r#type: String, // "VIP" | "GA" | "SEATED" | "BACKSTAGE"
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Harga normal (base price).
    pub price_idr: i64,
    /// Harga diskon (jika ada sale).
    #[serde(default)]
    pub sale_price_idr: Option<i64>,
    /// Tanggal mulai sale (ISO8601 string, untuk pre-fill form).
    #[serde(default)]
    pub sale_start_date: Option<String>,
    /// Tanggal akhir sale (ISO8601 string, untuk pre-fill form).
    #[serde(default)]
    pub sale_end_date: Option<String>,
    /// Harga efektif saat ini — sale_price jika aktif, else price_idr.
    #[serde(default)]
    pub effective_price_idr: i64,
    /// true jika sedang dalam periode sale aktif.
    #[serde(default)]
    pub is_sale_active: bool,
    /// Tiket terjual untuk tier ini.
    #[serde(default)]
    pub sold: i32,
    /// Sisa tiket tersedia (quota - sold).
    #[serde(default)]
    pub available: i32,
    /// Total quota tier ini.
    #[serde(default)]
    pub total: i32,
    /// Batas pembelian per order (None = tidak dibatasi).
    #[serde(default)]
    pub max_per_order: Option<i32>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub perks: Vec<String>,
    #[serde(default)]
    pub zone: String,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    /// Slug unik dari BE — dipakai untuk URL dan navigasi
    #[serde(default)]
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub subtitle: String,
    #[serde(default)]
    pub category: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub status: String,
    pub start_time: String,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default)]
    pub duration_minutes: i32,
    /// Total tiket terjual (semua variant).
    #[serde(default)]
    pub total_sold: i32,
    /// Total kuota (semua variant).
    #[serde(default)]
    pub total_quota: i32,
    pub venue: Venue,
    #[serde(default)]
    pub lineup: Vec<Artist>,
    #[serde(default)]
    pub tiers: Vec<TicketTier>,
    #[serde(default)]
    pub cover_url: String,
    #[serde(default)]
    pub base_price_idr: i64,
    #[serde(default)]
    pub detail_images: Vec<DetailImagePayload>,
}

/// Satu foto detail event (denah, seat map, info harga, dll).
/// Disimpan sebagai array JSON di field `detail_images`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DetailImagePayload {
    pub url: String,
    /// "map" | "seat" | "price" | "other"
    pub image_type: String,
    pub caption: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IssuedTicket {
    pub id: String,
    pub order_id: String,
    pub event_id: String,
    pub event_title: String,
    #[serde(default)]
    pub event_cover: String,
    pub venue_name: String,
    pub tier_name: String,
    pub tier_type: String,
    #[serde(default)]
    pub zone: String,
    #[serde(default)]
    pub row_seat: String,
    pub price_idr: i64,
    pub status: String,
    #[serde(default)]
    pub qr_code: String,
    pub ticket_ref: String,
    pub event_time: String,
    #[serde(default)]
    pub attendee_avatars: Vec<String>,
    #[serde(default)]
    pub attendee_count: i32,
}

// ─── Listing requests/responses ──────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListEventsRequest {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub page: i32,
    #[serde(default)]
    pub page_size: i32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListEventsResponse {
    pub events: Vec<Event>,
    #[serde(default)]
    pub total: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetEventRequest {
    pub id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListMyTicketsRequest {
    pub filter: String, // "ACTIVE" | "PAST" | "SHARED"
    pub page: i32,
    pub page_size: i32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListMyTicketsResponse {
    pub tickets: Vec<IssuedTicket>,
}

// ─── Payment ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatePromoRequest {
    pub promo_code: String,
    pub subtotal: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatePromoResponse {
    pub valid: bool,
    #[serde(default)]
    pub discount_idr: i64,
    #[serde(default)]
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOrderRequest {
    pub items: Vec<CartItem>,
    pub payment_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promo_code: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderItemRef {
    pub event_name: String,
    pub variant_name: String,
    pub quantity: i32,
    pub subtotal: i64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRef {
    pub id: String,
    pub order_code: String,
    pub status: String,
    pub total_amount: i64,
    pub expired_at: Option<String>,
    pub created_at: Option<String>,
    pub items: Vec<OrderItemRef>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateOrderResponse {
    pub order: OrderRef,
    #[serde(default)]
    pub requires_redirect: bool,
    #[serde(default)]
    pub payment_url: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmPaymentRequest {
    pub order_id: String,
    pub payment_token: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConfirmPaymentResponse {
    #[serde(default)]
    pub success: bool,
}

pub mod banner;
pub mod categories;
pub mod detail_image;
pub mod tickets;
