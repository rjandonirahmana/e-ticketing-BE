use crate::web::models::*;
use leptos::prelude::*;
#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use super::helpers::*;

#[cfg(feature = "ssr")]
pub async fn get_auth_token() -> Option<String> {
    use axum::http::{header::COOKIE, HeaderMap};
    use leptos_axum::extract;

    let headers: HeaderMap = extract().await.ok()?;
    let cookie_hdr = headers.get(COOKIE)?.to_str().ok()?;

    cookie_hdr.split(';').map(|p| p.trim()).find_map(|part| {
        part.strip_prefix("pulse_token=")
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from)
    })
}

#[cfg(feature = "ssr")]
pub fn set_auth_cookie(token: &str) {
    use axum::http::{header::SET_COOKIE, HeaderValue};
    use leptos_axum::ResponseOptions;

    let resp = expect_context::<ResponseOptions>();
    let value = format!("pulse_token={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800");
    if let Ok(hv) = HeaderValue::from_str(&value) {
        resp.append_header(SET_COOKIE, hv);
    }
}

#[cfg(feature = "ssr")]
pub fn clear_auth_cookie() {
    use axum::http::{header::SET_COOKIE, HeaderValue};
    use leptos_axum::ResponseOptions;

    let resp = expect_context::<ResponseOptions>();
    let value = "pulse_token=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0; \
                 Expires=Thu, 01 Jan 1970 00:00:00 GMT";
    if let Ok(hv) = HeaderValue::from_str(value) {
        resp.append_header(SET_COOKIE, hv);
    }
}

#[server(GetSession, "/api-fn")]
pub async fn get_session() -> Result<Option<UserResponse>, ServerFnError> {
    let Some(token) = get_auth_token().await else {
        return Ok(None);
    };
    let state = app_state().await?;
    let claims = match state.jwt.verify(&token) {
        Ok(c) => c,
        Err(_) => {
            clear_auth_cookie();
            return Ok(None);
        }
    };
    // JWT sudah diverifikasi secara kriptografis — tidak perlu hit DB.
    // Reconstruct UserResponse langsung dari claims (zero network round-trip).
    return Ok(Some(UserResponse {
        id: claims.user_id,
        name: claims.name,
        phone: claims.phone,
        role: claims.role,
        email: None,
    }));
}
