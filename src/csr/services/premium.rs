// src/services/premium.rs
//
// API calls untuk fitur Premium Subscription.
// Endpoint: GET /premium/status, POST /premium/activate

use serde::{Deserialize, Serialize};

use crate::csr::services::client::{get_private, post_private, ApiError};

// ── Response shapes ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PremiumStatus {
    pub is_premium: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivatePremiumResponse {
    pub plan: String,
    pub expires_at: String,
    pub is_active: bool,
}

#[derive(Debug, Serialize)]
struct ActivateRequest {
    days: i64,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Cek apakah user yang login saat ini adalah premium subscriber.
/// Endpoint: GET /premium/status
pub async fn fetch_premium_status() -> Result<PremiumStatus, ApiError> {
    get_private("/premium/status").await
}

/// Aktifkan (atau perpanjang) premium subscription.
/// `days` = durasi dalam hari (30 / 90 / 365).
/// Endpoint: POST /premium/activate
pub async fn activate_premium(days: i64) -> Result<ActivatePremiumResponse, ApiError> {
    post_private("/premium/activate", &ActivateRequest { days }).await
}
