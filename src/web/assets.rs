//! CSS ter-embed ke binary.
//!
//! Semua stylesheet `/styles/*.css` yang di-`<link>` di shell disajikan dari
//! sini lewat `include_str!` (di-embed saat compile). Pendekatan ini kebal
//! terhadap masalah working-directory / penyalinan aset cargo-leptos, sehingga
//! `/styles/x.css` TIDAK PERNAH 404 — baik via `cargo leptos watch`, `cargo run`,
//! maupun di dalam container.
//!
//! Catatan: sejak fix FOUC v4, shell() sudah meng-inline semua CSS langsung ke
//! `<head>` via `<style>` tag (bukan `<link>`), sehingga assets router ini hanya
//! dipakai jika ada fetch CSS manual atau saat debug. Tetap dipertahankan agar
//! bisa fallback.

use std::hash::{Hash, Hasher};
use std::sync::LazyLock;

use axum::{
    extract::Path,
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

/// Tabel nama-file → isi CSS (embedded). Path relatif terhadap file ini
/// (`src/web/assets.rs` → `../../styles/`).
macro_rules! css_table {
    ($($name:literal),+ $(,)?) => {
        &[ $( ($name, include_str!(concat!("../../styles/", $name))) ),+ ]
    };
}

/// Daftar semua CSS yang di-embed ke binary.
static STYLES: &[(&str, &str)] = css_table![
    "tokens.css",
    "base.css",
    "components.css",
    "page-home.css",
    "page-explore.css",
    "page-event-detail.css",
    "page-auth.css",
    "page-tickets.css",
    "page-ticket-detail.css",
    "page-orders.css",
    "page-order-detail.css",
    "page-order-tickets.css",
    "page-profile.css",
    "page-merchant.css",
    "page-merchant-event.css",
    "page-lives.css",
    "page-merchant-landing.css",
    "page-merchant-live.css",
    "page-meet.css",
    "page-admin.css",
    "page-messages.css",
    "page-notifications.css",
    "notifications.css",
    "page-cart.css",
    "page-scan.css",
    "page-story.css",
    "page-misc.css",
    "subscription.css",
    "profile_premium.css",
    "message_stories.css",
];

/// Bundled stylesheet — semua CSS di-embed ke binary dan disajikan sebagai
/// satu file `/styles/app.css` dengan cache 24 jam. Browser hanya download
/// sekali; kunjungan berikutnya menggunakan cache → HTML jauh lebih kecil.
static APP_BUNDLE: &str = include_str!("../../styles/app.bundle.css");

// Leaflet di-self-host (di-embed ke binary) alih-alih dari CDN unpkg. unpkg
// tidak untuk produksi & sering lambat/diblokir di sebagian jaringan/ekstensi
// browser → peta gagal load (kotak abu-abu kosong). Disajikan dari binary
// menjamin peta selalu tersedia tanpa bergantung pihak ketiga.
static LEAFLET_JS: &str = include_str!("../../styles/leaflet.js");
static LEAFLET_CSS: &str = include_str!("../../styles/leaflet.css");

async fn serve_leaflet_js() -> Response {
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("application/javascript; charset=utf-8")),
            (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=604800, immutable")),
        ],
        LEAFLET_JS,
    )
        .into_response()
}

async fn serve_leaflet_css() -> Response {
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/css; charset=utf-8")),
            (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=604800, immutable")),
        ],
        LEAFLET_CSS,
    )
        .into_response()
}

/// ETag = hash isi bundle. Berubah setiap CSS berubah → URL/identitas berbeda,
/// jadi browser tidak menyajikan CSS basi setelah deploy/edit.
static BUNDLE_ETAG: LazyLock<String> = LazyLock::new(|| {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    APP_BUNDLE.hash(&mut h);
    format!("\"{:x}\"", h.finish())
});

async fn serve_app_bundle(headers: HeaderMap) -> Response {
    let etag = BUNDLE_ETAG.as_str();
    // `no-cache` = browser WAJIB revalidasi tiap load (kirim If-None-Match).
    // Bila ETag cocok → 304 (tak kirim ulang CSS). Bila CSS berubah → ETag beda
    // → 200 dengan CSS baru. Hasilnya: perubahan SELALU tampil, tanpa download
    // ulang saat tidak berubah. (Sebelumnya max-age 24 jam → perubahan "tak
    // muncul" sampai cache kedaluwarsa / hard refresh.)
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag)
    {
        return (
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, HeaderValue::from_str(etag).unwrap())],
        )
            .into_response();
    }
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("text/css; charset=utf-8")),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
            (header::ETAG, HeaderValue::from_str(etag).unwrap()),
        ],
        APP_BUNDLE,
    )
        .into_response()
}

async fn serve_css(Path(file): Path<String>) -> Response {
    match STYLES.iter().find(|(name, _)| *name == file) {
        Some((_, body)) => (
            [
                (header::CONTENT_TYPE,  HeaderValue::from_static("text/css; charset=utf-8")),
                (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400, stale-while-revalidate=86400")),
            ],
            *body,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "/* stylesheet not found */").into_response(),
    }
}

/// Router untuk `/styles/*` — gabungkan ke router utama.
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/styles/app.css", get(serve_app_bundle))
        .route("/styles/{file}", get(serve_css))
        .route("/vendor/leaflet.js", get(serve_leaflet_js))
        .route("/vendor/leaflet.css", get(serve_leaflet_css))
}
