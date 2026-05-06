use std::sync::Arc;

use axum::{Json, extract::State};

use crate::middleware::auth::AuthUser;
use crate::models::merchant::{
    CreateMerchantDetailRequest, MerchantDetailResponse, UpdateMerchantDetailRequest,
};
use crate::state::AppState;
use crate::utils::error::AppResult;

pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> AppResult<Json<MerchantDetailResponse>> {
    user.require_role("merchant")?;
    Ok(Json(state.merchant_svc.get_profile(user.id()).await?))
}

pub async fn create_profile(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<CreateMerchantDetailRequest>,
) -> AppResult<Json<MerchantDetailResponse>> {
    user.require_role("merchant")?;
    Ok(Json(
        state.merchant_svc.create_profile(user.id(), body).await?,
    ))
}

pub async fn update_profile(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<UpdateMerchantDetailRequest>,
) -> AppResult<Json<MerchantDetailResponse>> {
    user.require_role("merchant")?;
    Ok(Json(
        state.merchant_svc.update_profile(user.id(), body).await?,
    ))
}
