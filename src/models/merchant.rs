use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantDetail {
    pub user_id: String,
    pub store_name: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub verified: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateMerchantDetailRequest {
    #[validate(length(min = 2, max = 255, message = "Store name must be 2-255 characters"))]
    pub store_name: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMerchantDetailRequest {
    #[validate(length(min = 2, max = 255))]
    pub store_name: Option<String>,
    pub description: Option<String>,
    pub logo_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MerchantDetailResponse {
    pub user_id: String,
    pub store_name: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub verified: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<MerchantDetail> for MerchantDetailResponse {
    fn from(m: MerchantDetail) -> Self {
        Self {
            user_id: m.user_id,
            store_name: m.store_name,
            description: m.description,
            logo_url: m.logo_url,
            verified: m.verified,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
