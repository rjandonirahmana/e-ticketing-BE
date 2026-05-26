//! Wire-level types that mirror the *backend* (Axum) JSON shapes.
//!
//! These are intentionally `pub(crate)` and only used inside the `services`
//! module. We deserialize the backend's snake_case payloads here, then map
//! them onto the frontend-facing types in `crate::models::*`.
//!
//! Keeping a separate set of structs means the rest of the app does not have
//! to change when the backend's shape evolves — only the mappers do.

use serde::{Deserialize, Deserializer, Serialize};

/// Deserialise a field that the backend may send as either a JSON number
/// (`150000`) or a quoted decimal string (`"150000.00"`).
fn de_f64_or_str<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    use serde::de::{self, Visitor};
    struct F64OrStr;
    impl<'de> Visitor<'de> for F64OrStr {
        type Value = f64;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a number or numeric string")
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
            Ok(v)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
            v.parse::<f64>().map_err(de::Error::custom)
        }
    }
    d.deserialize_any(F64OrStr)
}

// ─── Auth ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct BeRegisterPayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<&'a str>,
    pub name: &'a str,
    pub phone: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<&'a str>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BeVerifyRegisterPayload<'a> {
    pub phone: &'a str,
    pub otp: &'a str,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BeLoginPayload<'a> {
    pub phone: &'a str,
    pub password: &'a str,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BeUserResponse {
    pub id: String,
    /// Backend treats `email` as nullable (it's `Option<String>` in Rust BE).
    /// We MUST accept `null` here — `#[serde(default)]` alone only handles
    /// a missing field, not an explicit `null`, so the field type itself
    /// has to be `Option<String>`.
    #[serde(default)]
    pub email: Option<String>,
    pub name: String,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BeAuthResponse {
    pub access_token: String,
    #[serde(default)]
    pub token_type: String,
    #[serde(default)]
    pub expires_in: i64,
    pub user: BeUserResponse,
}

#[derive(Debug, Serialize, Default)]
pub struct BeUpdateProfilePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}

// ─── Events ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BeEvent {
    pub id: String,
    #[serde(default)]
    pub merchant_id: String,
    #[serde(default)]
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub price: f64,
    #[serde(default)]
    pub sale_price: Option<f64>,
    #[serde(default)]
    pub venue: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    pub event_date: String,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub total_sold: i32,
    #[serde(default)]
    pub total_quota: i32,
    /// BE returns category sebagai array: ["Musik", "Piknik"]
    #[serde(default)]
    pub category: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct BeEventVariant {
    pub id: String,
    pub event_id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub price: f64,
    #[serde(default)]
    pub sale_price: Option<f64>,
    pub quota: i32,
    pub sold: i32,
    #[serde(default)]
    pub available: i32,
    #[serde(default)]
    pub max_per_order: Option<i32>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default)]
    pub effective_price: f64,
    #[serde(default)]
    pub is_sale_active: bool,
    #[serde(default)]
    pub sale_price_start_date: Option<String>,
    #[serde(default)]
    pub sale_price_end_date: Option<String>,
}

fn default_true() -> bool {
    true
}

// Alias lama untuk backward compat kalau ada yang masih pakai
pub type BeTicketVariant = BeEventVariant;

#[derive(Debug, Deserialize)]
pub struct BeEventWithVariants {
    pub id: String,
    #[serde(default)]
    pub merchant_id: String,
    #[serde(default)]
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub venue: Option<String>,
    #[serde(default)]
    pub city: Option<String>,
    pub event_date: String,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub total_sold: i32,
    #[serde(default)]
    pub total_quota: i32,
    /// BE returns category sebagai array: ["Musik", "Piknik"]
    #[serde(default)]
    pub category: Vec<String>,
    /// BE sekarang return `event_variants` (rename dari ticket_variants)
    #[serde(default, alias = "ticket_variants")]
    pub event_variants: Vec<BeEventVariant>,
    /// Foto detail event (denah, seat map, info harga, dll)
    #[serde(default)]
    pub detail_images: Vec<crate::csr::models::DetailImagePayload>,
}

#[derive(Debug, Deserialize)]
pub struct BePaginatedEvents {
    #[serde(default)]
    pub data: Vec<BeEvent>,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub page: i64,
    #[serde(default)]
    pub per_page: i64,
    #[serde(default)]
    pub total_pages: i64,
}

// ─── Orders ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct BeCreateOrderItem<'a> {
    pub ticket_variant_id: &'a str,
    pub quantity: i32,
}

#[derive(Debug, Serialize)]
pub struct BeCreateOrderPayload<'a> {
    pub idempotency_key: String,
    pub items: Vec<BeCreateOrderItem<'a>>,
}

#[derive(Debug, Serialize)]
pub struct BePayOrderPayload<'a> {
    pub payment_method: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct BeOrderItem {
    pub id: String,
    #[serde(default)]
    pub ticket_variant_id: String,
    #[serde(default)]
    pub variant_name: String,
    #[serde(default)]
    pub event_id: String,
    #[serde(default)]
    pub event_name: String,
    pub quantity: i32,
    #[serde(deserialize_with = "de_f64_or_str")]
    pub unit_price: f64,
    #[serde(deserialize_with = "de_f64_or_str")]
    pub subtotal: f64,
}

#[derive(Debug, Deserialize)]
pub struct BeOrderDetail {
    pub id: String,
    #[serde(default)]
    pub customer_id: String,
    #[serde(default)]
    pub order_code: String,
    #[serde(default)]
    pub status: String,
    #[serde(deserialize_with = "de_f64_or_str", default)]
    pub total_amount: f64,
    #[serde(default)]
    pub payment_method: Option<String>,
    #[serde(default)]
    pub paid_at: Option<String>,
    #[serde(default)]
    pub expired_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub items: Vec<BeOrderItem>,
}

// ─── Tickets ─────────────────────────────────────────────────────────────
// backend.rs — BeTicketResponse: tambah field cover_url
#[derive(Debug, Deserialize, Clone)]
pub struct BeTicketResponse {
    pub id: String,
    #[serde(default)]
    pub ticket_code: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub used_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub order_id: String,
    #[serde(default)]
    pub order_code: String,
    #[serde(default)]
    pub event_id: String,
    #[serde(default)]
    pub event_name: String,
    #[serde(default)]
    pub event_date: Option<String>,
    #[serde(default)]
    pub event_venue: Option<String>,
    #[serde(default)]
    pub event_city: Option<String>,
    #[serde(default)]
    pub variant_id: String,
    #[serde(default)]
    pub variant_name: String,
    #[serde(default)]
    pub unit_price: f64,
    #[serde(default)]
    pub cover_url: Option<String>, // ← tambah ini
}
// ─── Mappers ─────────────────────────────────────────────────────────────

use crate::csr::models::{
    Artist, Event as FeEvent, IssuedTicket, LoginResponse, OrderRef, TicketTier, UserProfile, Venue,
};

/// Backend role/created_at → frontend membership fields.
pub fn user_to_profile(u: BeUserResponse) -> UserProfile {
    let role = u.role.clone();
    UserProfile {
        id: u.id,
        full_name: u.name,
        email: u.email.unwrap_or_default(),
        phone: u.phone.unwrap_or_default(),
        avatar_url: String::new(),
        active_tickets: 0,
        points: 0,
        membership_tier: match role.as_str() {
            "merchant" => "MERCHANT".to_string(),
            "admin" => "ADMIN".to_string(),
            _ => "STANDARD".to_string(),
        },
        role: u.role,
    }
}

pub fn auth_to_login(a: BeAuthResponse) -> LoginResponse {
    LoginResponse {
        access_token: a.access_token,
        // Backend currently issues only a single access token; the refresh
        // token slot is kept in the response for UI compatibility.
        refresh_token: String::new(),
        user: user_to_profile(a.user),
    }
}

fn variant_to_tier(v: BeEventVariant) -> TicketTier {
    let available = if v.available > 0 {
        v.available
    } else {
        (v.quota - v.sold).max(0)
    };
    TicketTier {
        id: v.id,
        r#type: "GA".to_string(),
        name: v.name,
        description: v.description.unwrap_or_default(),
        price_idr: v.price.round() as i64,
        sale_price_idr: v.sale_price.map(|p| p.round() as i64),
        sale_start_date: v.sale_price_start_date.clone(),
        sale_end_date: v.sale_price_end_date.clone(),
        effective_price_idr: if v.effective_price > 0.0 {
            v.effective_price.round() as i64
        } else {
            v.price.round() as i64
        },
        is_sale_active: v.is_sale_active,
        sold: v.sold,
        available,
        total: v.quota,
        max_per_order: v.max_per_order,
        is_active: v.is_active,
        sort_order: v.sort_order,
        perks: Vec::new(),
        zone: String::new(),
    }
}

pub fn event_to_fe(e: BeEvent) -> FeEvent {
    let venue_name = e.venue.clone().unwrap_or_default();
    let city = e.city.clone().unwrap_or_default();
    // Slug dipakai untuk URL — fallback ke id jika slug kosong (data lama)
    let slug = if e.slug.is_empty() {
        e.id.clone()
    } else {
        e.slug.clone()
    };
    FeEvent {
        id: e.id,
        slug,
        title: e.name,
        subtitle: String::new(),
        category: e.category,
        description: e.description.unwrap_or_default(),
        status: normalize_event_status(&e.status),
        start_time: e.start_time.unwrap_or(e.event_date),
        duration_minutes: 0,
        total_sold: e.total_sold,
        total_quota: e.total_quota,
        venue: Venue {
            name: venue_name,
            address: String::new(),
            city,
            latitude: 0.0,
            longitude: 0.0,
        },
        lineup: Vec::<Artist>::new(),
        tiers: Vec::new(),
        cover_url: e.cover_url.unwrap_or_default(),
        base_price_idr: e.price.round() as i64,
        detail_images: Vec::new(),
        end_time: e.end_time,
    }
}

pub fn event_with_variants_to_fe(e: BeEventWithVariants) -> FeEvent {
    let venue_name = e.venue.clone().unwrap_or_default();
    let city = e.city.clone().unwrap_or_default();
    let slug = if e.slug.is_empty() {
        e.id.clone()
    } else {
        e.slug.clone()
    };
    let tiers: Vec<TicketTier> = e
        .event_variants
        .iter()
        .filter(|v| v.is_active)
        .map(|v| {
            variant_to_tier(BeEventVariant {
                id: v.id.clone(),
                event_id: v.event_id.clone(),
                name: v.name.clone(),
                description: v.description.clone(),
                price: v.price,
                sale_price: v.sale_price,
                quota: v.quota,
                sold: v.sold,
                available: v.available,
                max_per_order: v.max_per_order,
                is_active: v.is_active,
                sort_order: v.sort_order,
                effective_price: v.effective_price,
                is_sale_active: v.is_sale_active,
                sale_price_end_date: v.sale_price_end_date.clone(),
                sale_price_start_date: v.sale_price_start_date.clone(),
            })
        })
        .collect();
    // Compute total_sold/total_quota dari variants jika tidak ada di response root
    let total_sold = if e.total_sold > 0 {
        e.total_sold
    } else {
        e.event_variants.iter().map(|v| v.sold).sum()
    };
    let total_quota = if e.total_quota > 0 {
        e.total_quota
    } else {
        e.event_variants.iter().map(|v| v.quota).sum()
    };
    let base_price = tiers
        .iter()
        .map(|t| t.effective_price_idr)
        .min()
        .unwrap_or(0);
    FeEvent {
        id: e.id,
        slug,
        title: e.name,
        subtitle: String::new(),
        category: e.category,
        description: e.description.unwrap_or_default(),
        status: normalize_event_status(&e.status),
        start_time: e.start_time.unwrap_or(e.event_date),
        duration_minutes: 0,
        total_sold,
        total_quota,
        venue: Venue {
            name: venue_name,
            address: String::new(),
            city,
            latitude: 0.0,
            longitude: 0.0,
        },
        lineup: Vec::<Artist>::new(),
        tiers,
        cover_url: e.cover_url.unwrap_or_default(),
        base_price_idr: base_price,
        detail_images: e.detail_images,
        end_time: e.end_time,
    }
}

fn normalize_event_status(s: &str) -> String {
    match s.to_lowercase().as_str() {
        "live" | "active" | "ongoing" => "LIVE".to_string(),
        "upcoming" | "scheduled" | "draft" => "UPCOMING".to_string(),
        "ended" | "completed" | "finished" | "cancelled" | "canceled" => "ENDED".to_string(),
        "" => "UPCOMING".to_string(),
        other => other.to_uppercase(),
    }
}

// backend.rs — ticket_to_issued: map cover_url ke event_cover
pub fn ticket_to_issued(t: BeTicketResponse) -> IssuedTicket {
    IssuedTicket {
        id: t.id,
        order_id: t.order_id,
        event_id: t.event_id,
        event_title: t.event_name,
        event_cover: t.cover_url.unwrap_or_default(), // ← ubah dari String::new()
        venue_name: t.event_venue.unwrap_or_default(),
        tier_name: t.variant_name,
        tier_type: "GA".to_string(),
        zone: String::new(),
        row_seat: String::new(),
        price_idr: t.unit_price.round() as i64,
        status: normalize_ticket_status(&t.status),
        qr_code: t.ticket_code.clone(),
        ticket_ref: t.ticket_code,
        event_time: t.event_date.unwrap_or_default(),
        attendee_avatars: Vec::new(),
        attendee_count: 1,
    }
}

fn normalize_ticket_status(s: &str) -> String {
    match s.to_lowercase().as_str() {
        "active" | "issued" | "valid" => "ACTIVE".to_string(),
        "used" | "checked_in" | "scanned" => "PAST".to_string(),
        "shared" | "transferred" => "SHARED".to_string(),
        "" => "ACTIVE".to_string(),
        other => other.to_uppercase(),
    }
}

pub fn order_detail_to_ref(o: BeOrderDetail) -> OrderRef {
    let items = o
        .items
        .iter()
        .map(|i| crate::csr::models::OrderItemRef {
            event_name: i.event_name.clone(),
            variant_name: i.variant_name.clone(),
            quantity: i.quantity,
            subtotal: i.subtotal.round() as i64,
        })
        .collect();
    OrderRef {
        id: o.id,
        order_code: o.order_code,
        status: o.status,
        total_amount: o.total_amount.round() as i64,
        expired_at: o.expired_at,
        created_at: o.created_at,
        items,
    }
}
