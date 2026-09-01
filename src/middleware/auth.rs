use std::sync::Arc;

use axum::{
    extract::{FromRequestParts, State},
    http::{StatusCode, header, request::Parts},
    middleware::Next,
    response::Response,
};

use crate::models::auth::Claims;
use crate::state::AppState;
use crate::utils::error::AppError;

/// Extracted by handlers to access the current authenticated user.
#[derive(Debug, Clone)]
pub struct AuthUser(pub Claims);

impl AuthUser {
    pub fn id(&self) -> &str {
        &self.0.user_id
    }
    pub fn name(&self) -> &str {
        &self.0.name
    }
    #[allow(dead_code)]
    pub fn role(&self) -> &str {
        &self.0.role
    }
    pub fn require_role(&self, required: &str) -> Result<(), AppError> {
        if self.0.role != required {
            Err(AppError::Forbidden(format!(
                "Endpoint requires role '{}'",
                required
            )))
        } else {
            Ok(())
        }
    }
}

/// Extracts the JWT from the `pulse_token` cookie. Same-origin browser
/// requests (Leptos WASM) can't read the HttpOnly cookie to build an
/// `Authorization` header, but the cookie *is* sent automatically — so we
/// read it server-side as a fallback to the Bearer header.


/// Axum middleware that validates the JWT and attaches the Claims to the
/// request extensions. Accepts either an `Authorization: Bearer <token>`
/// header (the separate Next.js frontend / REST API) or the `pulse_token`
/// HttpOnly cookie (same-origin Leptos WASM, which cannot set the header
/// itself). Routes that need auth nest under a layer that applies this
/// middleware; handlers then use the `AuthUser` extractor.
pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    mut req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AppError> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(String::from)
        .or_else(|| crate::utils::cookie::nilai(req.headers(), "pulse_token"))
        .ok_or_else(|| AppError::Unauthorized("Missing auth token".into()))?;

    let claims = state
        .jwt
        .verify(&token)
        .map_err(|_| AppError::Unauthorized("Invalid or expired token".into()))?;

    req.extensions_mut().insert(AuthUser(claims));
    Ok(next.run(req).await)
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    // Hier kein Makro nötig, einfach async fn
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "Missing auth context"))
    }
}
