use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub price: f64,
    pub sale_price: Option<f64>,
    pub sale_price_start_date: Option<DateTime<Utc>>,
    pub sale_price_end_date: Option<DateTime<Utc>>,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// With ticket_variants embedded (for detail view)
#[derive(Debug, Serialize)]
pub struct EventWithVariants {
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub ticket_variants: Vec<crate::models::event_variant::TicketVariantResponse>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateEventRequest {
    #[validate(length(min = 2, max = 255))]
    pub name: String,
    pub description: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateEventRequest {
    #[validate(length(min = 2, max = 255))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub event_date: Option<DateTime<Utc>>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EventListQuery {
    pub city: Option<String>,
    pub status: Option<String>,
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
