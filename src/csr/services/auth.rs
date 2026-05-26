use super::backend::{
    auth_to_login, user_to_profile, BeAuthResponse, BeLoginPayload, BeRegisterPayload,
    BeUpdateProfilePayload, BeUserResponse, BeVerifyRegisterPayload,
};
use super::client::{get_private, post_public, put_private, ApiError};
use crate::csr::models::*;

/// POST /auth/login — backend uses **phone + password**, NOT email.
pub async fn login(req: &LoginRequest) -> Result<LoginResponse, ApiError> {
    let payload = BeLoginPayload {
        phone: &req.phone,
        password: &req.password,
    };
    let resp: BeAuthResponse = post_public("/auth/login", &payload).await?;
    Ok(auth_to_login(resp))
}

/// POST /auth/register — initiates registration. Backend will:
///   1. Generate a random password,
///   2. Send WhatsApp OTP + the password to `phone`,
///   3. Return empty body.
///
// The user must then call `verify_register(phone, otp)` to actually create
// the account and receive their JWT.

pub async fn register(req: &RegisterRequest) -> Result<RegisterResponse, ApiError> {
    let email_opt = (!req.email.is_empty()).then_some(req.email.as_str());
    let payload = BeRegisterPayload {
        email: email_opt,
        name: &req.full_name,
        phone: &req.phone,
        role: Some("customer"),
    };
    let _: () = post_public("/auth/register", &payload).await?; // ← ganti Value → ()
    Ok(RegisterResponse {
        success: true,
        message: "OTP terkirim ke WhatsApp kamu.".into(),
    })
}

/// POST /auth/verify-register — confirms OTP, creates user, returns auth.
pub async fn verify_register(req: &VerifyOtpRequest) -> Result<LoginResponse, ApiError> {
    let payload = BeVerifyRegisterPayload {
        phone: &req.phone,
        otp: &req.otp,
    };
    let resp: BeAuthResponse = post_public("/auth/verify", &payload).await?;
    Ok(auth_to_login(resp))
}

/// POST /auth/register — call again to trigger a new OTP for the same phone.
pub async fn resend_register_otp(
    full_name: &str,
    phone: &str,
    email: Option<&str>,
) -> Result<(), ApiError> {
    let payload = BeRegisterPayload {
        email,
        name: full_name,
        phone,
        role: Some("customer"),
    };
    let _: () = post_public("/auth/register", &payload).await?; // ← ganti Value → ()
    Ok(())
}

/// The backend has no logout endpoint — sessions are stateless JWTs. We just
/// resolve immediately so the UI can clear local storage.
pub async fn logout(_req: &LogoutRequest) -> Result<(), ApiError> {
    // ← ganti Value → ()
    Ok(())
}

/// The backend does not yet expose a password reset endpoint. We surface
/// that explicitly so the UI can show a meaningful message instead of
/// silently succeeding.
pub async fn request_password_reset(_email: &str) -> Result<(), ApiError> {
    // ← ganti Value → ()
    Err(ApiError::unsupported(
        "Password reset is not available yet. Please contact support.",
    ))
}

/// GET /auth/me — returns the current authenticated user's profile.
pub async fn me() -> Result<UserProfile, ApiError> {
    let resp: BeUserResponse = get_private("/auth/me").await?;
    Ok(user_to_profile(resp))
}

/// PUT /auth/me — update name/phone.
pub async fn update_me(
    name: Option<String>,
    phone: Option<String>,
) -> Result<UserProfile, ApiError> {
    let payload = BeUpdateProfilePayload { name, phone };
    let resp: BeUserResponse = put_private("/auth/me", &payload).await?;
    Ok(user_to_profile(resp))
}
