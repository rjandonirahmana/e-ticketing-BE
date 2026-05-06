use std::sync::Arc;

use axum::{Json, extract::State};
use serde::Deserialize;
use validator::Validate;

use crate::middleware::auth::AuthUser;
use crate::models::auth::AuthResponse;
use crate::models::users::{LoginRequest, RegisterRequest, UpdateProfileRequest, UserResponse};
use crate::state::AppState;
use crate::utils::error::{AppError, AppResult};

/// Register payload — register DTO + the password (kept out of the User model
/// on purpose so the server-side User struct never carries it).
#[derive(Debug, Deserialize, Validate)]
pub struct RegisterPayload {
    pub email: Option<String>,
    pub name: String,
    #[validate(length(min = 8, message = "phone must be at least 8 characters"))]
    pub phone: String,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct VerifyRegisterPayload {
    #[validate(length(min = 9, message = "phone must be at least 9 characters"))]
    pub phone: String,
    #[validate(length(min = 6, max = 10))]
    pub otp: String,
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterPayload>,
) -> AppResult<Json<()>> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
    let req: RegisterRequest = RegisterRequest {
        email: body.email,
        name: body.name,
        phone: body.phone,
        role: body.role,
    };
    state.auth_svc.initiate_register(req).await?;
    Ok(Json(()))
}

pub async fn verify_register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<VerifyRegisterPayload>,
) -> AppResult<Json<AuthResponse>> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

    let data = state
        .auth_svc
        .verify_register(&body.phone, &body.otp)
        .await?;
    Ok(Json(data))
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginRequest>,
) -> AppResult<Json<AuthResponse>> {
    let resp = state.auth_svc.login(body).await?;
    Ok(Json(resp))
}

pub async fn me(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> AppResult<Json<UserResponse>> {
    Ok(Json(state.auth_svc.me(user.id()).await?))
}

pub async fn update_me(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<UpdateProfileRequest>,
) -> AppResult<Json<UserResponse>> {
    Ok(Json(state.auth_svc.update_profile(user.id(), body).await?))
}
