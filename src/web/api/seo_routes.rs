//! seo_routes.rs — /robots.txt & /sitemap.xml (server-only).
//!
//! Sitemap dibangun dinamis dari DB: halaman statis + product aktif + profil
//! merchant. Dibatasi jauh di bawah 50.000 URL (batas satu file sitemap).
#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use axum::{
    extract::Extension,
    http::header,
    response::{IntoResponse, Response},
};

use crate::repository::db::exec_rows;
use crate::state::AppState;
use crate::utils::ulid::bin_to_ulid;
use crate::web::seo::SITE_BASE;

/// GET /robots.txt — izinkan semua + tunjuk sitemap.
pub async fn robots_txt() -> Response {
    let body = format!("User-agent: *\nAllow: /\n\nSitemap: {SITE_BASE}/sitemap.xml\n");
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

/// GET /sitemap.xml — halaman statis + product aktif + merchant.
pub async fn sitemap_xml(Extension(state): Extension<Arc<AppState>>) -> Response {
    let mut urls: Vec<String> = [
        "/", "/explore", "/lives", "/stories", "/pulse-landing",
    ]
    .iter()
    .map(|p| format!("{SITE_BASE}{p}"))
    .collect();

    // Produk aktif (terbaru dulu). Cap 45k → sisakan ruang utk statis + merchant.
    if let Ok(rows) = exec_rows(
        &state.pool,
        "SELECT slug FROM products WHERE status = 'active' AND slug <> '' \
         ORDER BY event_date DESC LIMIT 45000",
        &[],
    )
    .await
    {
        for r in &rows {
            if let Ok(slug) = r.try_get::<_, String>("slug") {
                urls.push(format!("{SITE_BASE}/products/{slug}"));
            }
        }
    }

    // Profil merchant.
    if let Ok(rows) = exec_rows(
        &state.pool,
        "SELECT user_id FROM merchant_details LIMIT 5000",
        &[],
    )
    .await
    {
        for r in &rows {
            if let Ok(id) = r.try_get::<_, Vec<u8>>("user_id") {
                if let Ok(ulid) = bin_to_ulid(id) {
                    urls.push(format!("{SITE_BASE}/m/{ulid}"));
                }
            }
        }
    }

    let mut xml = String::with_capacity(urls.len() * 80 + 128);
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push_str(r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">"#);
    for u in &urls {
        xml.push_str("<url><loc>");
        xml.push_str(&u.replace('&', "&amp;"));
        xml.push_str("</loc></url>");
    }
    xml.push_str("</urlset>");

    (
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        xml,
    )
        .into_response()
}
