use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use validator::Validate;

use crate::utils::ulid::hex_to_ulid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductVariant {
    pub id: String,
    pub event_id: String,
    pub name: String,
    pub description: Option<String>,
    pub price: f64,
    pub sale_price: Option<f64>,
    pub sale_price_start_date: Option<DateTime<Utc>>,
    pub sale_price_end_date: Option<DateTime<Utc>>,
    pub quota: i32,
    pub sold: i32,
    pub max_per_order: Option<i32>,
    pub is_active: bool,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProductVariant {
    /// Harga efektif: gunakan sale_price jika sedang dalam periode sale,
    /// fallback ke price biasa.
    pub fn effective_price(&self) -> f64 {
        let now = Utc::now();
        if let Some(sp) = self.sale_price {
            let start_ok = self.sale_price_start_date.map_or(true, |d| now >= d);
            let end_ok = self.sale_price_end_date.map_or(true, |d| now <= d);
            if start_ok && end_ok {
                return sp;
            }
        }
        self.price
    }

    /// Apakah sedang dalam periode sale aktif sekarang?
    pub fn is_sale_active(&self) -> bool {
        let now = Utc::now();
        self.sale_price.is_some()
            && self.sale_price_start_date.map_or(true, |d| now >= d)
            && self.sale_price_end_date.map_or(true, |d| now <= d)
    }
}

/// Komparator effective_price ASC — untuk menemukan variant termurah
/// sebagai representasi harga product di list.
pub fn cmp_by_effective_price(a: &ProductVariant, b: &ProductVariant) -> Ordering {
    a.effective_price()
        .partial_cmp(&b.effective_price())
        .unwrap_or(Ordering::Equal)
}

// ── JSON helper untuk deserialisasi variants_json dari jsonb_agg ──────────────

#[derive(serde::Deserialize)]
pub struct ProductVariantJson {
    id: String,
    event_id: String,
    name: String,
    description: Option<String>,
    price: f64,
    sale_price: Option<f64>,
    sale_price_start_date: Option<NaiveDate>,
    sale_price_end_date: Option<NaiveDate>,
    quota: i32,
    sold: i32,
    max_per_order: Option<i32>,
    is_active: bool,
    sort_order: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ProductVariantJson {
    pub fn into_variant(self) -> Result<ProductVariant> {
        let to_dt = |d: NaiveDate| -> DateTime<Utc> {
            Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
        };

        Ok(ProductVariant {
            id: hex_to_ulid(&self.id).context("variant id hex→ulid")?,
            event_id: hex_to_ulid(&self.event_id).context("variant event_id hex→ulid")?,
            name: self.name,
            description: self.description,
            price: self.price,
            sale_price: self.sale_price,
            sale_price_start_date: self.sale_price_start_date.map(to_dt),
            sale_price_end_date: self.sale_price_end_date.map(to_dt),
            quota: self.quota,
            sold: self.sold,
            max_per_order: self.max_per_order,
            is_active: self.is_active,
            sort_order: self.sort_order,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

// ── Response ──────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct ProductVariantResponse {
    pub id: String,
    pub event_id: String,
    pub name: String,
    pub description: Option<String>,
    /// Harga normal (base price).
    pub price: f64,
    /// Harga diskon jika ada.
    pub sale_price: Option<f64>,
    pub sale_price_start_date: Option<DateTime<Utc>>,
    pub sale_price_end_date: Option<DateTime<Utc>>,
    /// Harga efektif sekarang: sale_price jika periode aktif, else price.
    pub effective_price: f64,
    /// true jika sale sedang berjalan saat ini.
    pub is_sale_active: bool,
    pub quota: i32,
    pub sold: i32,
    pub available: i32,
    pub max_per_order: Option<i32>,
    pub is_active: bool,
    pub sort_order: i32,
}

impl From<ProductVariant> for ProductVariantResponse {
    fn from(v: ProductVariant) -> Self {
        let effective_price = v.effective_price();
        let is_sale_active = v.is_sale_active();
        let available = v.quota - v.sold;
        Self {
            id: v.id,
            event_id: v.event_id,
            name: v.name,
            description: v.description,
            price: v.price,
            sale_price: v.sale_price,
            sale_price_start_date: v.sale_price_start_date,
            sale_price_end_date: v.sale_price_end_date,
            effective_price,
            is_sale_active,
            quota: v.quota,
            sold: v.sold,
            available,
            max_per_order: v.max_per_order,
            is_active: v.is_active,
            sort_order: v.sort_order,
        }
    }
}

// ── Request DTOs ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Validate)]
pub struct CreateProductVariantRequest {
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

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateProductVariantRequest {
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
