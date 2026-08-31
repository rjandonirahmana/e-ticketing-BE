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
    /// Nomor HP, BUKAN email.
    ///
    /// Aplikasi ini mendaftarkan dan memasukkan orang lewat nomor HP + WhatsApp;
    /// email opsional dan sebagian besar akun tak punya. Bentuk lama meminta
    /// email, dan itu tak mungkin bisa memulihkan akun mana pun.
    pub phone: String,
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
    headers: axum::http::HeaderMap,
    Json(body): Json<LoginReq>,
) -> Result<Json<AuthOut>, (StatusCode, Json<serde_json::Value>)> {
    use crate::models::users::LoginRequest;
    let req = LoginRequest { phone: body.phone, password: body.password };
    let auth = state.auth_svc.login(req).await.map_err(app_err)?;

    let refresh = state
        .refresh_svc
        .issue(&auth.user.id, None, user_agent(&headers))
        .await
        .map_err(app_err)?;

    Ok(Json(AuthOut {
        user: into_user_out(auth.user),
        access_token: auth.access_token,
        refresh_token: refresh,
    }))
}

/// User-Agent apa adanya, dipakai hanya untuk membantu pemilik akun mengenali
/// perangkat pada daftar sesi. Tak pernah dipercaya sebagai identitas.
fn user_agent(h: &axum::http::HeaderMap) -> &str {
    h.get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
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
    headers: axum::http::HeaderMap,
    Json(body): Json<VerifyOtpReq>,
) -> Result<Json<AuthOut>, (StatusCode, Json<serde_json::Value>)> {
    let auth = state
        .auth_svc
        .verify_register(&body.phone, &body.otp)
        .await
        .map_err(app_err)?;

    let refresh = state
        .refresh_svc
        .issue(&auth.user.id, None, user_agent(&headers))
        .await
        .map_err(app_err)?;

    Ok(Json(AuthOut {
        user: into_user_out(auth.user),
        access_token: auth.access_token,
        refresh_token: refresh,
    }))
}

/// POST /api/auth/forgot-password — kirim password baru lewat WhatsApp.
///
/// Sandi LAMA tidak disentuh di sini. Ia baru berganti saat seseorang benar-
/// benar masuk memakai sandi barunya — lihat `AuthService::pakai_sandi_menunggu`.
async fn forgot_password(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ForgotPasswordReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let pesan = state
        .auth_svc
        .forgot_password(&body.phone)
        .await
        .map_err(app_err)?;
    Ok(Json(serde_json::json!({ "message": pesan })))
}

/// Tukar refresh token dengan sepasang token baru.
///
/// Refresh token LAMA dicabut di sini (rotasi), jadi klien WAJIB menyimpan
/// `refreshToken` yang dikembalikan. Memakai token lama sekali lagi dianggap
/// pencurian dan mencabut seluruh keluarga sesi — lihat `service/refresh.rs`.
///
/// Peran (`role`) di access token baru diambil ULANG dari database, bukan
/// disalin dari token lama. Itulah yang membatasi umur hak akses yang sudah
/// dicabut: paling lama sepanjang sisa umur access token, bukan sampai refresh
/// berikutnya kedaluwarsa berhari-hari kemudian.
async fn refresh(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(body): Json<RefreshReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ua = user_agent(&headers).to_string();
    let hasil = state
        .refresh_svc
        .rotate(&body.refresh_token, &ua)
        .await
        .map_err(app_err)?;

    // Refresh token kosong = `rotate` menempuh jalur rotasi-bersamaan: peminta
    // lain sudah merotasi lebih dulu, dan token barunya hanya ada di tangan
    // peminta itu (yang tersimpan di sini cuma hash-nya, jadi tak mungkin
    // dikembalikan dari sini).
    //
    // Jalur itu memang untuk MIDDLEWARE, yang cukup butuh access token karena
    // cookie refresh-nya diurus permintaan saudara di halaman yang sama. Klien
    // native yang memanggil endpoint ini menyimpan refresh token sendiri, dan
    // memberinya string kosong akan menghapus sesinya. Jadi di sini: minta ia
    // mencoba lagi — TANPA mencabut keluarga, karena tak ada yang salah.
    if hasil.refresh_token.is_empty() {
        return Err(app_err(crate::utils::error::AppError::Unauthorized(
            "Refresh sedang berjalan di permintaan lain, coba lagi".into(),
        )));
    }

    Ok(Json(serde_json::json!({
        "accessToken": hasil.access_token,
        "refreshToken": hasil.refresh_token,
        "expiresIn": hasil.expires_in,
        "user": into_user_out(hasil.user),
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogoutReq {
    /// Opsional supaya klien lama yang tak mengirimkannya tetap jalan —
    /// bagi mereka logout tetap sekadar membuang token di sisi klien.
    #[serde(default)]
    pub refresh_token: Option<String>,
}

/// Logout: cabut seluruh keluarga refresh token yang ditunjukkan.
///
/// Access token tetap berlaku sampai kedaluwarsa — itu sifat JWT, dan itulah
/// alasan umurnya harus pendek. Yang dijamin di sini: sesi tak bisa lagi
/// diperpanjang, jadi ia berakhir paling lama sepanjang sisa umur access token.
async fn logout(
    _auth: AuthUser,
    State(state): State<Arc<AppState>>,
    body: Option<Json<LogoutReq>>,
) -> StatusCode {
    if let Some(Json(b)) = body {
        if let Some(rt) = b.refresh_token.filter(|s| !s.is_empty()) {
            if let Err(e) = state.refresh_svc.revoke(&rt).await {
                tracing::warn!(error = %e, "logout: gagal mencabut refresh token");
            }
        }
    }
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
