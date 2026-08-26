use crate::web::models::*;
use leptos::prelude::*;
#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use super::helpers::*;

/// Nama cookie. Ditulis sekali di sini supaya middleware silent-refresh
/// (`middleware/silent_refresh.rs`) dan server function tak mungkin berselisih
/// soal ejaan.
pub const ACCESS_COOKIE: &str = "pulse_token";
pub const REFRESH_COOKIE: &str = "pulse_refresh";

/// Umur cookie refresh — samakan dengan `REFRESH_TTL_DAYS` di service.
const REFRESH_COOKIE_MAX_AGE: i64 = 30 * 24 * 3600;

/// Ambil satu cookie dari header `Cookie`.
pub fn cookie_from_header(cookie_hdr: &str, nama: &str) -> Option<String> {
    let awalan = format!("{nama}=");
    cookie_hdr.split(';').map(|p| p.trim()).find_map(|part| {
        part.strip_prefix(&awalan)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from)
    })
}

#[cfg(feature = "ssr")]
pub async fn get_auth_token() -> Option<String> {
    use axum::http::{header::COOKIE, HeaderMap};
    use leptos_axum::extract;

    let headers: HeaderMap = extract().await.ok()?;
    let cookie_hdr = headers.get(COOKIE)?.to_str().ok()?;
    cookie_from_header(cookie_hdr, ACCESS_COOKIE)
}

#[cfg(feature = "ssr")]
pub async fn get_refresh_token() -> Option<String> {
    use axum::http::{header::COOKIE, HeaderMap};
    use leptos_axum::extract;

    let headers: HeaderMap = extract().await.ok()?;
    let cookie_hdr = headers.get(COOKIE)?.to_str().ok()?;
    cookie_from_header(cookie_hdr, REFRESH_COOKIE)
}

/// Pasang satu cookie pada respons.
///
/// `use_context`, BUKAN `expect_context`: `ResponseOptions` bisa absen pada
/// sebagian jalur SSR, dan `expect_context` di sana berubah menjadi panic yang
/// muncul ke pengguna sebagai 500 — untuk hal yang sebenarnya cuma "cookie tak
/// bisa dipasang".
#[cfg(feature = "ssr")]
fn set_cookie(value: String) {
    use axum::http::{header::SET_COOKIE, HeaderValue};
    use leptos_axum::ResponseOptions;

    let Some(resp) = use_context::<ResponseOptions>() else {
        tracing::warn!("ResponseOptions tidak tersedia — cookie dilewati");
        return;
    };
    if let Ok(hv) = HeaderValue::from_str(&value) {
        resp.append_header(SET_COOKIE, hv);
    }
}

/// Cookie access token. Umurnya sengaja MENGIKUTI umur token itu sendiri, bukan
/// 7 hari seperti sebelumnya: cookie yang hidup lebih lama dari tokennya hanya
/// menghasilkan permintaan yang membawa token mati, lalu ditolak.
///
/// Sesi tetap panjang karena cookie refresh yang memperpanjangnya diam-diam.
#[cfg(feature = "ssr")]
pub fn set_auth_cookie(token: &str) {
    set_cookie(format!(
        "{ACCESS_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
        crate::utils::jwt::access_cookie_max_age()
    ));
}

#[cfg(feature = "ssr")]
pub fn set_refresh_cookie(token: &str) {
    set_cookie(format!(
        "{REFRESH_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={REFRESH_COOKIE_MAX_AGE}"
    ));
}

/// Hapus KEDUA cookie. Menghapus hanya yang access akan membuat middleware
/// silent-refresh menerbitkan yang baru pada permintaan berikutnya — pengguna
/// menekan logout dan tetap masuk.
#[cfg(feature = "ssr")]
pub fn clear_auth_cookie() {
    for nama in [ACCESS_COOKIE, REFRESH_COOKIE] {
        set_cookie(format!(
            "{nama}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0; \
             Expires=Thu, 01 Jan 1970 00:00:00 GMT"
        ));
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
