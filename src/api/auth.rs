//! api/auth.rs — Auth REST endpoints.
//!
//! POST /api/auth/login
//! POST /api/auth/register
//! POST /api/auth/verify-register
//! POST /api/auth/forgot-password
//! POST /api/auth/refresh
//! POST /api/auth/logout        (private, invalidate token client-side)

use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;
use super::extractor::{app_err, AuthUser};

// ── Request types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginReq {
    pub phone: String,
    pub password: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterReq {
    pub full_name: String,
    pub phone: String,
    pub email: Option<String>,
    pub role: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyOtpReq {
    pub phone: String,
    pub otp: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgotPasswordReq {
    #[allow(dead_code)]
    pub email: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshReq {
    pub refresh_token: String,
}

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOut {
    pub id: String,
    pub full_name: String,
    pub email: Option<String>,
    pub phone: String,
    pub role: String,
    pub membership_tier: String,
    pub active_tickets: i32,
    pub points: i32,
    pub avatar_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthOut {
    pub user: UserOut,
    pub access_token: String,
    pub refresh_token: String,
}

fn into_user_out(u: crate::models::users::UserResponse) -> UserOut {
    UserOut {
        id: u.id,
        full_name: u.name,
        email: u.email,
        phone: u.phone,
        role: u.role,
        membership_tier: "STANDARD".into(),
        active_tickets: 0,
        points: 0,
        avatar_url: String::new(),
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginReq>,
) -> Result<Json<AuthOut>, (StatusCode, Json<serde_json::Value>)> {
    use crate::models::users::LoginRequest;
    let req = LoginRequest { phone: body.phone, password: body.password };
    let auth = state.auth_svc.login(req).await.map_err(app_err)?;
    // Backend belum implement refresh token; kirim access_token di kedua field.
    // Next.js api.ts menyimpan keduanya dan pakai refreshToken untuk retry 401.
    let token = auth.access_token.clone();
    Ok(Json(AuthOut {
        user: into_user_out(auth.user),
        access_token: auth.access_token,
        refresh_token: token,
    }))
}

async fn register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    use crate::models::users::RegisterRequest;
    let req = RegisterRequest {
        name: body.full_name,
        phone: body.phone,
        email: body.email,
        role: Some(body.role.unwrap_or_else(|| "customer".into())),
    };
    state.auth_svc.initiate_register(req).await.map_err(app_err)?;
    Ok(Json(serde_json::json!({ "success": true, "message": "OTP dikirim ke nomor kamu" })))
}

async fn verify_register(
    State(state): State<Arc<AppState>>,
    Json(body): Json<VerifyOtpReq>,
) -> Result<Json<AuthOut>, (StatusCode, Json<serde_json::Value>)> {
    let auth = state
        .auth_svc
        .verify_register(&body.phone, &body.otp)
        .await
        .map_err(app_err)?;
    let token = auth.access_token.clone();
    Ok(Json(AuthOut {
        user: into_user_out(auth.user),
        access_token: auth.access_token,
        refresh_token: token,
    }))
}

async fn forgot_password(
    State(_state): State<Arc<AppState>>,
    Json(_body): Json<ForgotPasswordReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // TODO: wire up password reset service
    Ok(Json(serde_json::json!({ "message": "Jika email terdaftar, link reset akan dikirim" })))
}

async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(body): Json<RefreshReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Verifikasi refresh token (sama formatnya dengan access token).
    // Jika valid, keluarkan access token baru dengan TTL segar.
    let claims = state.jwt.verify(&body.refresh_token).map_err(|_| (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({ "message": "Refresh token tidak valid" })),
    ))?;
    let new_token = state.jwt.sign(&claims.user_id, &claims.name, &claims.phone, &claims.role)
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({ "message": e.to_string() })),
        ))?;
    Ok(Json(serde_json::json!({ "accessToken": new_token })))
}

async fn logout(
    _auth: AuthUser,
) -> StatusCode {
    // JWT is stateless; client should discard the token.
    StatusCode::NO_CONTENT
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/register", post(register))
        .route("/auth/verify-register", post(verify_register))
        .route("/auth/forgot-password", post(forgot_password))
        .route("/auth/refresh", post(refresh))
        .route("/auth/logout", post(logout))
}
