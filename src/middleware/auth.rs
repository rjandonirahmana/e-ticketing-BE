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

/// Axum middleware that validates Bearer tokens and attaches the Claims to
/// the request extensions. Routes that need auth nest under a layer that
/// applies this middleware; handlers then use the `AuthUser` extractor.
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
        .ok_or_else(|| AppError::Unauthorized("Missing Bearer token".into()))?;

    let claims = state
        .jwt
        .verify(token)
        .map_err(|_| AppError::Unauthorized("Invalid or expired token".into()))?;

    req.extensions_mut().insert(AuthUser(claims));
    Ok(next.run(req).await)
}

#[axum::async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthUser>()
            .cloned()
            .ok_or((StatusCode::UNAUTHORIZED, "Missing auth context"))
    }
}
