//! api/extractor.rs — JWT auth extractor untuk REST handlers.

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};

use crate::{models::auth::Claims, state::AppState};
use std::sync::Arc;

/// Extractor yang memverifikasi `Authorization: Bearer <token>` dan
/// meng-inject Claims ke handler. Gagal dengan 401 jika token absen/invalid.
pub struct AuthUser(pub Claims);

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = (StatusCode, axum::Json<serde_json::Value>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({ "message": "Token tidak ditemukan" })),
                )
            })?;

        let claims = state.jwt.verify(token).map_err(|_| {
            (
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({ "message": "Token tidak valid atau kadaluarsa" })),
            )
        })?;

        Ok(AuthUser(claims))
    }
}

/// Verifikasi internal JWT dari header `X-App-Token`.
/// Dipakai untuk endpoint yang dipanggil dari WASM (sudah punya internal secret).
#[allow(dead_code)]
pub struct InternalAuth;

impl FromRequestParts<Arc<AppState>> for InternalAuth {
    type Rejection = (StatusCode, axum::Json<serde_json::Value>);

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get("x-app-token")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({ "message": "x-app-token missing" })),
                )
            })?;

        // TANDA TANGANNYA DIPERIKSA, bukan sekadar ada-tidaknya header.
        //
        // Versi sebelumnya berhenti di `ok_or_else` di atas dengan catatan
        // "validasi HMAC bisa ditambah nanti" — sementara nama tipenya,
        // `InternalAuth`, dan doc-comment-nya menjanjikan verifikasi JWT.
        // Selama ia tak dipasang di rute mana pun, itu tak berbahaya; masalahnya
        // muncul pada hari seseorang memasangnya sambil mengira ia melindungi
        // sesuatu. Yang dijaganya cuma "pengirim tahu ada header bernama
        // x-app-token" — pengetahuan yang bisa didapat siapa pun dengan membuka
        // tab Network sekali.
        //
        // Implementasinya sengaja DIPINJAM dari middleware, bukan disalin:
        // dua salinan aturan verifikasi adalah dua tempat yang bisa berbeda.
        crate::middleware::internal_auth::verify_internal_token(
            token,
            &_state.internal_jwt_secret,
        )
        .map_err(|e| {
            (
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({ "message": e.to_string() })),
            )
        })?;

        Ok(InternalAuth)
    }
}

/// Konversi AppError ke HTTP response.
pub fn app_err(e: crate::utils::error::AppError) -> (StatusCode, axum::Json<serde_json::Value>) {
    let status = match e {
        crate::utils::error::AppError::NotFound(_) => StatusCode::NOT_FOUND,
        crate::utils::error::AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
        crate::utils::error::AppError::Forbidden(_) => StatusCode::FORBIDDEN,
        crate::utils::error::AppError::Conflict(_) => StatusCode::CONFLICT,
        crate::utils::error::AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
        crate::utils::error::AppError::UnprocessableEntity(_) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, axum::Json(serde_json::json!({ "message": e.to_string() })))
}
