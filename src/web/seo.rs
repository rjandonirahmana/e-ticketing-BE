//! seo.rs — Helper SEO untuk halaman publik ber-SSR: canonical, OpenGraph,
//! Twitter Card, dan util JSON-LD (schema.org). JSON-LD sendiri bersifat
//! per-halaman (Product, Organization, …) — komponen di sini hanya menyiapkan
//! meta standar + util penanaman <script> yang aman.

use leptos::prelude::*;
use leptos_meta::{Link, Meta, Title};

/// Base URL publik (untuk canonical & `og:url`). Di-set saat build via
/// `PUBLIC_BASE_URL`; fallback ke domain produksi. Compile-time → sama di server
/// & wasm sehingga canonical konsisten.
pub const SITE_BASE: &str = match option_env!("PUBLIC_BASE_URL") {
    Some(v) => v,
    None => "https://ulala.space",
};

/// Path relatif → URL absolut (URL yang sudah absolut dibiarkan).
pub fn abs_url(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        format!("{SITE_BASE}{path}")
    }
}

/// Serialize JSON agar aman ditanam di `<script>`: `< > &` → escape unicode JSON
/// yang valid. Mencegah break-out `</script>` DAN korupsi bila renderer meng-
/// HTML-escape teks (hasilnya identik & valid di kedua kasus).
pub fn safe_ld(value: &serde_json::Value) -> String {
    value
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
}

/// Meta SEO standar satu halaman: `<title>`, description, canonical, OpenGraph,
/// Twitter Card. JSON-LD ditambahkan terpisah oleh halaman (lihat `safe_ld`).
#[component]
pub fn SeoMeta(
    #[prop(into)] title: String,
    #[prop(into)] description: String,
    /// Path relatif halaman, mis. "/products/slug".
    #[prop(into)]
    path: String,
    /// URL gambar absolut untuk og:image/twitter:image. Kosong = tanpa gambar.
    #[prop(into, optional)]
    image: String,
    /// og:type — "website" (default) | "article" | "profile" | dst.
    #[prop(into, optional)]
    og_type: String,
) -> impl IntoView {
    let url = abs_url(&path);
    let og_type = if og_type.is_empty() {
        "website".to_string()
    } else {
        og_type
    };
    let has_image = !image.is_empty();
    let tw_card = if has_image {
        "summary_large_image"
    } else {
        "summary"
    };
    view! {
        <Title text=title.clone() />
        <Meta name="description" content=description.clone() />
        <Link rel="canonical" href=url.clone() />

        <Meta property="og:site_name" content="PULSE" />
        <Meta property="og:type" content=og_type />
        <Meta property="og:title" content=title.clone() />
        <Meta property="og:description" content=description.clone() />
        <Meta property="og:url" content=url />
        {has_image.then(|| view! { <Meta property="og:image" content=image.clone() /> })}

        <Meta name="twitter:card" content=tw_card />
        <Meta name="twitter:title" content=title />
        <Meta name="twitter:description" content=description />
        {has_image.then(|| view! { <Meta name="twitter:image" content=image /> })}
    }
}
