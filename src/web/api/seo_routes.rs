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
    // ── KENAPA TIDAK `Allow: /` SAJA ──────────────────────────────────────
    // Setiap halaman produk adalah render SSR yang menyentuh basis data, dan
    // katalog ini berisi ratusan ribu produk. `Allow: /` tanpa batas berarti
    // mengundang perayap menyusuri semuanya, secepat yang ia mau, ke mesin yang
    // sama yang melayani pembeli sungguhan. Perayap tidak menemukan halaman itu
    // secara kebetulan — kita yang menyodorkannya lewat sitemap.
    //
    // Yang dilarang di bawah dipilih dengan satu ukuran: apakah halaman ini
    // punya nilai bagi orang yang datang dari mesin pencari? Halaman peta lokasi
    // sebuah produk tidak — ia hanya berguna bagi yang SUDAH membuka produknya,
    // sementara ongkos merayapinya sama persis dengan halaman produk itu
    // sendiri. Begitu pula keranjang, checkout, dan seluruh halaman yang
    // menuntut masuk: perayap tak bisa melihat apa pun di sana selain
    // pengalihan, tapi tetap membayar penuh untuk mengetahuinya.
    //
    // `Crawl-delay` tidak dihormati Googlebot (ia memakai setelan di Search
    // Console), tetapi dihormati Bing, Yandex, dan sebagian besar perayap kecil
    // — dan justru perayap kecil yang datang tanpa pengaturan laju sama sekali.
    let body = format!(
        "User-agent: *\n\
         Allow: /\n\
         Disallow: /products/*/location\n\
         Disallow: /cart\n\
         Disallow: /checkout\n\
         Disallow: /orders\n\
         Disallow: /profile\n\
         Disallow: /pulse\n\
         Disallow: /meet\n\
         Disallow: /admin\n\
         Disallow: /merchant\n\
         Disallow: /api-fn/\n\
         Crawl-delay: 10\n\
         \n\
         Sitemap: {SITE_BASE}/sitemap.xml\n"
    );
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

    // ── Produk aktif ──────────────────────────────────────────────────────
    // Data seed DIKECUALIKAN. Sitemap adalah undangan: setiap URL di dalamnya
    // akan benar-benar diminta perayap, dan tiap permintaan adalah render SSR
    // ke basis data yang sama yang melayani pembeli. Mengundang puluhan ribu
    // kunjungan ke produk PALSU berarti membayar penuh ongkosnya tanpa satu pun
    // pengunjung yang mungkin membeli.
    //
    // Batasnya juga diturunkan 45.000 → 5.000. Pada mesin 4 vCPU, angka yang
    // lebih besar bukan menambah jangkauan melainkan menambah antrean: perayap
    // yang menyusuri empat puluh lima ribu halaman dinamis akan menghabiskan
    // kolam koneksi jauh sebelum ia selesai. Naikkan lagi setelah katalognya
    // benar-benar berisi produk sungguhan.
    if let Ok(rows) = exec_rows(
        &state.pool,
        "SELECT slug FROM products \
          WHERE status = 'active' AND slug <> '' AND slug NOT LIKE 'seed-%' \
          ORDER BY event_date DESC LIMIT 5000",
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
