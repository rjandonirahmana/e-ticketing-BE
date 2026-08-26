use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantDetail {
    pub user_id: String,
    pub store_name: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub header_url: Option<String>,
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
    pub header_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MerchantDetailResponse {
    pub user_id: String,
    pub store_name: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub header_url: Option<String>,
    pub verified: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Profil merchant untuk halaman publik /m/{id} (sisi user).
#[derive(Debug, Clone, Serialize)]
pub struct MerchantPublicProfile {
    pub merchant_id: String,
    pub store_name: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    pub header_url: Option<String>,
    pub verified: bool,
    pub followers: i64,
    pub products_count: i64,
    pub rating_avg: f64,
    pub rating_count: i64,
}

/// Ringkasan rating (halaman reviews): rata-rata, total, distribusi per bintang.
///
/// `store_name` ikut di sini agar halaman reviews cukup satu round-trip untuk
/// header + ringkasan (DB di VPS — tiap round-trip berbiaya latensi jaringan).
/// `None` = merchant tidak ada → server_fn membalas NotFound.
#[derive(Debug, Clone, Serialize)]
pub struct MerchantReviewSummary {
    pub store_name: Option<String>,
    pub avg: f64,
    pub total: i64,
    /// dist[0] = jumlah bintang 1 … dist[4] = jumlah bintang 5.
    pub dist: [i64; 5],
}

/// Profil publik user biasa (halaman /u/{id}) — hanya data yang ada di skema.
#[derive(Debug, Clone, Serialize)]
pub struct UserPublicProfile {
    pub user_id: String,
    pub name: String,
    /// Jumlah merchant yang di-follow user ini (merchant_follows.follower_id).
    pub following: i64,
    pub reviews: i64,
    pub stories: i64,
}

/// Satu ulasan yang DITULIS user (ke merchant) — halaman /u/{id} tab Reviews.
#[derive(Debug, Clone, Serialize)]
pub struct UserReviewItem {
    pub merchant_id: String,
    pub store_name: String,
    pub rating: i32,
    pub comment: String,
    pub created_at: DateTime<Utc>,
}

/// Hasil pencarian merchant (autocomplete search /explore).
#[derive(Debug, Clone, Serialize)]
pub struct MerchantSearchItem {
    pub merchant_id: String,
    pub store_name: String,
    pub logo_url: Option<String>,
    pub verified: bool,
}

/// Satu follower merchant (untuk halaman daftar follower /m/{id}/followers).
#[derive(Debug, Clone, Serialize)]
pub struct MerchantFollower {
    pub user_id: String,
    pub name: String,
    /// role='merchant' ⟺ punya merchant_details (dijamin trigger migrasi 016),
    /// jadi cukup role untuk memutuskan tautan /m/ vs /u/ di daftar follower.
    pub role: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MerchantReviewItem {
    pub user_id: String,
    pub user_name: String,
    pub rating: i32,
    pub comment: String,
    pub created_at: DateTime<Utc>,
}

impl From<MerchantDetail> for MerchantDetailResponse {
    fn from(m: MerchantDetail) -> Self {
        Self {
            user_id: m.user_id,
            store_name: m.store_name,
            description: m.description,
            logo_url: m.logo_url,
            header_url: m.header_url,
            verified: m.verified,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}
