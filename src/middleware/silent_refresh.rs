//! middleware/silent_refresh.rs — memperpanjang sesi web tanpa mengganggu.
//!
//! ── MASALAH YANG DISELESAIKAN ──────────────────────────────────────────────
//! Jalur web memakai cookie berisi access token. Selama tak ada cara
//! memperpanjangnya, umur access token harus panjang supaya pengguna tak
//! terlempar keluar tiap setengah jam — dan umur panjang itulah yang membuat
//! peran yang sudah dicabut tetap berlaku berjam-jam, karena peran ikut
//! dibawa di dalam JWT.
//!
//! Middleware ini memutus tarik-menarik itu. Saat access token mati tetapi
//! cookie refresh masih ada, ia menukarnya diam-diam: pengguna tak melihat
//! apa-apa, dan access token boleh berumur pendek.
//!
//! ── DUA HAL YANG DILAKUKAN SEKALIGUS ───────────────────────────────────────
//! 1. Menyuntikkan token baru ke header `Cookie` **permintaan ini**, supaya
//!    handler di belakangnya langsung melihat pengguna yang sudah masuk —
//!    tanpa perlu satu putaran redirect.
//! 2. Memasang `Set-Cookie` pada respons, supaya permintaan berikutnya sudah
//!    membawa token baru.
//!
//! Tanpa langkah pertama, permintaan yang memicu refresh akan tetap dianggap
//! anonim, dan pengguna melihat halaman "silakan masuk" sekali setiap kali
//! token kedaluwarsa.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::header::{COOKIE, SET_COOKIE},
    http::HeaderValue,
    middleware::Next,
    response::Response,
};

use crate::state::AppState;
use crate::web::api::server_fns::session::{cookie_from_header, ACCESS_COOKIE, REFRESH_COOKIE};

pub async fn silent_refresh(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Response {
    let cookie_hdr = req
        .headers()
        .get(COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if cookie_hdr.is_empty() {
        return next.run(req).await;
    }

    // Access token masih sah → tak ada yang perlu dikerjakan. Ini jalur
    // mayoritas permintaan, jadi ia harus semurah mungkin: satu verifikasi
    // tanda tangan, tanpa menyentuh database.
    let access_ok = cookie_from_header(&cookie_hdr, ACCESS_COOKIE)
        .map(|t| state.jwt.verify(&t).is_ok())
        .unwrap_or(false);

    if access_ok {
        return next.run(req).await;
    }

    let Some(refresh) = cookie_from_header(&cookie_hdr, REFRESH_COOKIE) else {
        return next.run(req).await;
    };

    let ua = req
        .headers()
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("web")
        .to_string();

    let hasil = match state.refresh_svc.rotate(&refresh, &ua).await {
        Ok(h) => h,
        Err(e) => {
            // Termasuk deteksi pemakaian ulang, yang MEMANG mencabut sesi.
            // Bukan kondisi luar biasa dari sisi middleware: permintaan
            // diteruskan sebagai anonim, dan halaman yang memerlukan login
            // akan mengarahkan sendiri.
            tracing::debug!(error = %e, "silent refresh gagal — lanjut sebagai anonim");
            return next.run(req).await;
        }
    };

    // 1) Permintaan INI ikut melihat token barunya.
    let cookie_baru = ganti_cookie(&cookie_hdr, ACCESS_COOKIE, &hasil.access_token);
    let cookie_baru = ganti_cookie(&cookie_baru, REFRESH_COOKIE, &hasil.refresh_token);
    if let Ok(hv) = HeaderValue::from_str(&cookie_baru) {
        req.headers_mut().insert(COOKIE, hv);
    }

    let mut resp = next.run(req).await;

    // 2) Permintaan BERIKUTNYA membawa token baru.
    //
    // Refresh token ikut diganti karena `rotate` mencabut yang lama. Kalau
    // hanya cookie access yang diperbarui, permintaan berikutnya akan
    // menunjukkan refresh token yang sudah dicabut — dan itu terbaca sebagai
    // pemakaian ulang, yang mencabut seluruh sesi. Pengguna akan terlempar
    // keluar justru oleh mekanisme yang seharusnya menjaganya tetap masuk.
    let max_age_access = crate::utils::jwt::access_cookie_max_age();
    let pasang = [
        format!(
            "{ACCESS_COOKIE}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_access}",
            hasil.access_token
        ),
        format!(
            "{REFRESH_COOKIE}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
            hasil.refresh_token,
            30 * 24 * 3600
        ),
    ];
    for v in pasang {
        if let Ok(hv) = HeaderValue::from_str(&v) {
            resp.headers_mut().append(SET_COOKIE, hv);
        }
    }

    resp
}

/// Ganti nilai satu cookie di dalam header `Cookie`, atau tambahkan bila belum
/// ada. Cookie lain dibiarkan apa adanya — header itu juga membawa milik
/// fitur lain (tema, dsb).
fn ganti_cookie(header: &str, nama: &str, nilai: &str) -> String {
    let awalan = format!("{nama}=");
    let mut bagian: Vec<String> = header
        .split(';')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty() && !p.starts_with(&awalan))
        .map(String::from)
        .collect();
    bagian.push(format!("{nama}={nilai}"));
    bagian.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mengganti satu cookie tak boleh menjatuhkan cookie lain — header itu
    /// juga membawa preferensi tema dan lainnya.
    #[test]
    fn ganti_cookie_menjaga_yang_lain() {
        let h = "theme=dark; pulse_token=lama; lang=id";
        let hasil = ganti_cookie(h, "pulse_token", "baru");
        assert!(hasil.contains("theme=dark"));
        assert!(hasil.contains("lang=id"));
        assert!(hasil.contains("pulse_token=baru"));
        assert!(!hasil.contains("pulse_token=lama"));
    }

    /// Cookie yang belum ada ditambahkan, bukan diabaikan.
    #[test]
    fn ganti_cookie_menambah_bila_belum_ada() {
        let hasil = ganti_cookie("theme=dark", "pulse_refresh", "abc");
        assert!(hasil.contains("theme=dark"));
        assert!(hasil.contains("pulse_refresh=abc"));
    }
}
