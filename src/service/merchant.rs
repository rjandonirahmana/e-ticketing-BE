use std::sync::Arc;
use validator::Validate;

use crate::models::merchant::{
    CreateMerchantDetailRequest, MerchantDetailResponse, MerchantPublicProfile,
    MerchantReviewItem, MerchantReviewSummary, UpdateMerchantDetailRequest,
};
use crate::repository::merchant::MerchantRepository;
use crate::utils::error::{AppError, AppResult};

pub struct MerchantService {
    repo: Arc<dyn MerchantRepository>,
}

impl MerchantService {
    pub fn new(repo: Arc<dyn MerchantRepository>) -> Self {
        Self { repo }
    }

    pub async fn get_profile(&self, user_id: &str) -> AppResult<MerchantDetailResponse> {
        let m = self
            .repo
            .find(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Merchant profile not set up yet".into()))?;
        Ok(m.into())
    }

    pub async fn create_profile(
        &self,
        user_id: &str,
        req: CreateMerchantDetailRequest,
    ) -> AppResult<MerchantDetailResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        if self.repo.find(user_id).await?.is_some() {
            return Err(AppError::Conflict("Merchant profile already exists".into()));
        }

        // logo_url is NOT NULL in the schema; default to empty string when absent
        let logo = req.logo_url.unwrap_or_default();
        let m = self
            .repo
            .create(user_id, &req.store_name, req.description.as_deref(), &logo)
            .await?;
        Ok(m.into())
    }

    pub async fn update_profile(
        &self,
        user_id: &str,
        req: UpdateMerchantDetailRequest,
    ) -> AppResult<MerchantDetailResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        // make sure it exists first so we can return 404 instead of silently no-op
        self.repo
            .find(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Merchant profile not set up yet".into()))?;

        self.repo
            .update(
                user_id,
                req.store_name.as_deref(),
                req.description.as_deref(),
                req.logo_url.as_deref(),
                req.header_url.as_deref(),
            )
            .await?;
        self.get_profile(user_id).await
    }

    // ── Profil publik + rating & follow (sisi user) ───────────────────────────

    pub async fn public_profile(&self, merchant_id: &str) -> AppResult<MerchantPublicProfile> {
        self.repo
            .public_profile(merchant_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Merchant tidak ditemukan".into()))
    }

    pub async fn review_summary(&self, merchant_id: &str) -> AppResult<MerchantReviewSummary> {
        self.repo
            .review_summary(merchant_id)
            .await
            .map_err(AppError::Internal)
    }

    pub async fn list_reviews(
        &self,
        merchant_id: &str,
        page: i64,
        per_page: i64,
    ) -> AppResult<Vec<MerchantReviewItem>> {
        let per_page = per_page.clamp(1, 50);
        let offset = (page.max(1) - 1) * per_page;
        self.repo
            .list_reviews(merchant_id, per_page, offset)
            .await
            .map_err(AppError::Internal)
    }

    pub async fn submit_review(
        &self,
        merchant_id: &str,
        user_id: &str,
        rating: i32,
        comment: &str,
    ) -> AppResult<()> {
        if !(1..=5).contains(&rating) {
            return Err(AppError::BadRequest("Rating harus 1–5.".into()));
        }
        if merchant_id == user_id {
            return Err(AppError::BadRequest(
                "Tidak bisa mengulas toko sendiri.".into(),
            ));
        }
        // Batasi panjang komentar — jangan percaya klien.
        let comment: String = comment.trim().chars().take(1000).collect();
        let affected = self
            .repo
            .upsert_review(merchant_id, user_id, rating as i16, &comment)
            .await
            .map_err(AppError::Internal)?;
        // 0 baris = tak ada order 'paid' user↔merchant → belum berhak mengulas.
        if affected == 0 {
            return Err(AppError::BadRequest(
                "Kamu perlu menyelesaikan minimal 1 pesanan dengan penyelenggara ini sebelum menulis ulasan.".into(),
            ));
        }
        Ok(())
    }

    /// Apakah user berhak menulis ulasan (punya ≥1 pesanan 'paid' dgn merchant).
    pub async fn can_review(&self, merchant_id: &str, user_id: &str) -> AppResult<bool> {
        if merchant_id == user_id {
            return Ok(false);
        }
        self.repo
            .has_purchased(merchant_id, user_id)
            .await
            .map_err(AppError::Internal)
    }

    /// Profil publik user biasa (halaman /u/{id}). `None` → user tidak ada.
    pub async fn user_public(
        &self,
        user_id: &str,
    ) -> AppResult<crate::models::merchant::UserPublicProfile> {
        self.repo
            .user_public(user_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("User tidak ditemukan".into()))
    }

    /// Ulasan yang ditulis user + total, paginasi per halaman.
    pub async fn list_user_reviews(
        &self,
        user_id: &str,
        page: i64,
        per_page: i64,
    ) -> AppResult<Vec<crate::models::merchant::UserReviewItem>> {
        let per_page = per_page.clamp(1, 50);
        let offset = (page.max(1) - 1) * per_page;
        self.repo
            .list_user_reviews(user_id, per_page, offset)
            .await
            .map_err(AppError::Internal)
    }

    /// Cari merchant berdasarkan nama (autocomplete search /explore). Query
    /// terlalu pendek (<2) → kosong (hindari scan luas untuk 1 huruf).
    pub async fn search(
        &self,
        query: &str,
        limit: i64,
    ) -> AppResult<Vec<crate::models::merchant::MerchantSearchItem>> {
        let q = query.trim();
        if q.len() < 2 {
            return Ok(Vec::new());
        }
        self.repo
            .search(q, limit.clamp(1, 20))
            .await
            .map_err(AppError::Internal)
    }

    /// Daftar follower merchant + total, paginasi per halaman.
    pub async fn list_followers(
        &self,
        merchant_id: &str,
        page: i64,
        per_page: i64,
    ) -> AppResult<(i64, Vec<crate::models::merchant::MerchantFollower>)> {
        let per_page = per_page.clamp(1, 50);
        let offset = (page.max(1) - 1) * per_page;
        let total = self
            .repo
            .count_followers(merchant_id)
            .await
            .map_err(AppError::Internal)?;
        let items = self
            .repo
            .list_followers(merchant_id, per_page, offset)
            .await
            .map_err(AppError::Internal)?;
        Ok((total, items))
    }

    pub async fn set_follow(
        &self,
        merchant_id: &str,
        follower_id: &str,
        follow: bool,
    ) -> AppResult<()> {
        if merchant_id == follower_id {
            return Err(AppError::BadRequest("Tidak bisa follow diri sendiri.".into()));
        }
        self.repo
            .set_follow(merchant_id, follower_id, follow)
            .await
            .map_err(AppError::Internal)
    }

    pub async fn is_following(&self, merchant_id: &str, follower_id: &str) -> AppResult<bool> {
        self.repo
            .is_following(merchant_id, follower_id)
            .await
            .map_err(AppError::Internal)
    }
}
