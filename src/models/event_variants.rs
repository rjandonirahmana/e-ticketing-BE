use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::utils::ulid::hex_to_ulid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventVariant {
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

#[derive(serde::Deserialize)]
pub struct EventVariantJson {
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

impl EventVariantJson {
    pub fn into_variant(self) -> Result<EventVariant> {
        // NaiveDate → DateTime<Utc> midnight
        let to_dt = |d: NaiveDate| -> DateTime<Utc> {
            Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
        };

        Ok(EventVariant {
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

#[derive(Debug, Serialize, Clone)]
pub struct EventVariantResponse {
    pub id: String,
    pub event_id: String,
    pub name: String,
    pub description: Option<String>,
    pub price: f64,
    pub quota: i32,
    pub sold: i32,
    pub available: i32,
    pub max_per_order: Option<i32>,
    pub is_active: bool,
    pub sort_order: i32,
}

impl From<EventVariant> for EventVariantResponse {
    fn from(v: EventVariant) -> Self {
        let available = v.quota - v.sold;
        Self {
            id: v.id,
            event_id: v.event_id,
            name: v.name,
            description: v.description,
            price: v.price,
            quota: v.quota,
            sold: v.sold,
            available,
            max_per_order: v.max_per_order,
            is_active: v.is_active,
            sort_order: v.sort_order,
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateEventVariantRequest {
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

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateEventVariantRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub price: Option<f64>,
    pub quota: Option<i32>,
    pub max_per_order: Option<i32>,
    pub is_active: Option<bool>,
    pub sort_order: Option<i32>,
}
