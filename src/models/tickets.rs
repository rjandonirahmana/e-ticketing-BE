use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    pub id: String,
    pub order_item_id: String,
    pub ticket_code: String,
    pub status: String,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Ticket enriched with the event/variant context the customer typically wants
/// to see in their wallet view.
#[derive(Debug, Serialize)]
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
}

#[derive(Debug, Deserialize, Validate)]
pub struct ValidateTicketRequest {
    #[validate(length(min = 1, max = 100))]
    pub ticket_code: String,
}
