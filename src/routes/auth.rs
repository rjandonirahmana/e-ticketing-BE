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
    #[validate(email(message = "Invalid email format"))]
    pub email: String,
    #[validate(length(min = 5, message = "Password must be at least 5 characters"))]
    pub password: String,
    #[validate(length(min = 2, max = 255))]
    pub name: String,
    pub phone: Option<String>,
    pub role: Option<String>,
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterPayload>,
) -> AppResult<Json<AuthResponse>> {
    body.validate()
        .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
    let req = RegisterRequest {
        email: body.email,
        name: body.name,
        phone: body.phone,
        role: body.role,
    };
    let resp = state.auth_svc.register(req, &body.password).await?;
    Ok(Json(resp))
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
