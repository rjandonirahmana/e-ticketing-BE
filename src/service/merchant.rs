use std::sync::Arc;
use validator::Validate;

use crate::models::merchant::{
    CreateMerchantDetailRequest, MerchantDetailResponse, UpdateMerchantDetailRequest,
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
            )
            .await?;
        self.get_profile(user_id).await
    }
}
