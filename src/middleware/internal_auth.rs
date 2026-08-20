use axum::{extract::State, middleware::Next, response::Response};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::sync::Arc;

use crate::state::AppState;
use crate::utils::error::AppError;

/// Claims yang di-sign oleh FE Leptos menggunakan shared INTERNAL_JWT_SECRET.
/// Isi minimal: iss harus "kinetic-fe", exp wajib valid.
#[derive(Debug, Deserialize)]
struct InternalClaims {
    pub iss: String,
    // iat dan exp sudah divalidasi oleh jsonwebtoken (validate_exp = true)
    #[allow(dead_code)]
    pub iat: i64,
}

/// Axum middleware: wajibkan `X-App-Token` header di setiap request.
///
/// Token harus merupakan JWT HS256 yang di-sign dengan `INTERNAL_JWT_SECRET`
/// yang sama di FE dan BE. Ini memastikan hanya aplikasi Leptos FE yang bisa
/// memanggil API — bukan scraper atau client lain yang tidak memiliki secret.
///
/// Cara pemakaian (di build_router):
/// ```text
/// .route_layer(from_fn_with_state(state.clone(), require_internal_jwt))
/// ```
pub async fn require_internal_jwt(
    State(state): State<Arc<AppState>>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AppError> {
    let token = req
        .headers()
        .get("X-App-Token")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("Missing X-App-Token".into()))?;

    verify_internal_token(token, &state.internal_jwt_secret)?;

    Ok(next.run(req).await)
}

/// Verifikasi token internal. `pub(crate)` supaya ekstraktor di
/// `api::extractor` memakai implementasi YANG SAMA — bukan menuliskan
/// versinya sendiri yang bisa menyimpang diam-diam.
pub(crate) fn verify_internal_token(token: &str, secret: &str) -> Result<(), AppError> {
    let dec_key = DecodingKey::from_secret(secret.as_bytes());

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    // iss harus "kinetic-fe"
    validation.set_issuer(&["kinetic-fe"]);

    let data = decode::<InternalClaims>(token, &dec_key, &validation).map_err(|e| {
        tracing::warn!("Internal JWT verify failed: {:?}", e);
        AppError::Unauthorized("Invalid internal token".into())
    })?;

    if data.claims.iss != "kinetic-fe" {
        return Err(AppError::Unauthorized("Invalid token issuer".into()));
    }

    Ok(())
}
