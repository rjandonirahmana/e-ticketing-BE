//! service/scan.rs — Ticket validation for merchant scanner.
//!
//! POST /api/tickets/validate → { ticket_ref } → ValidateResponse

use super::client::{post_private, ApiError};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct ValidateRequest<'a> {
    ticket_ref: &'a str,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ValidateResponse {
    pub ticket_ref: String,
    pub event_title: String,
    pub tier_name: String,
    pub attendee: String,
    pub status: String, // "VALID" | "ALREADY_USED" | "INVALID"
    pub message: String,
}

pub async fn validate_ticket(ticket_ref: &str) -> Result<ValidateResponse, ApiError> {
    post_private(
        "/tickets/validate",
        &ValidateRequest { ticket_ref },
    )
    .await
}
