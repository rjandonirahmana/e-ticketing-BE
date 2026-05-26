use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub event_date: DateTime<Utc>,
    pub event_venue: Option<String>,
    pub event_city: Option<String>,

    pub variant_id: String,
    pub variant_name: String,
    pub unit_price: f64,
    pub cover_url: Option<String>,
}
