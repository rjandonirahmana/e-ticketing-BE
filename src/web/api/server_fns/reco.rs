//! reco.rs — Behavior tracking + rekomendasi per-user (server-side, DB).
//!
//! - `record_affinity`  : catat minat user (kategori product yang dibuka). No-op
//!                        diam-diam bila belum login (anonim → localStorage client).
//! - `get_recommended_products` : product dari kategori favorit user (skor tertinggi
//!                        di `user_affinity`). Kosong bila belum login / belum ada
//!                        data / tabel belum dimigrasi (graceful).

use crate::web::models::*;
use leptos::prelude::*;
#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use super::helpers::*;

/// Catat perilaku: user berinteraksi dengan product berkategori `categories`.
/// `signal`: "view" (default) | "cart" | "purchase" — bobot naik sesuai kuatnya
/// niat. Hanya menulis buffer in-memory (flush batch oleh AffinityService) —
/// TIDAK ada round-trip DB di jalur request. No-op bila tak login.
#[server(RecordAffinity, "/api-fn")]
pub async fn record_affinity(
    categories: Vec<String>,
    signal: Option<String>,
) -> Result<(), ServerFnError> {
    use crate::service::affinity::AffinitySignal;

    // Diam-diam berhenti bila belum login (user anonim ditangani localStorage).
    let Ok(claims) = auth_claims().await else {
        return Ok(());
    };
    if categories.is_empty() {
        return Ok(());
    }
    let state = app_state().await?;
    // "purchase" tak diterima dari client (bisa dipalsukan) — sinyal purchase
    // dicatat server-side saat order benar-benar dibuat (checkout.rs).
    let sig = match signal.as_deref() {
        Some("cart") => AffinitySignal::Cart,
        _ => AffinitySignal::View,
    };
    state.affinity_svc.record(&claims.user_id, &categories, sig);
    Ok(())
}

/// Product rekomendasi = product dari kategori favorit user (skor tertinggi).
/// Kosong bila belum login / belum ada perilaku / tabel belum ada (graceful).
#[server(GetRecommendedProducts, "/api-fn")]
pub async fn get_recommended_products() -> Result<PaginatedProducts, ServerFnError> {
    use crate::models::products::ProductListQuery;

    let empty = || PaginatedProducts {
        data: vec![],
        total: 0,
        page: 1,
        per_page: 0,
        total_pages: 0,
    };

    let Ok(claims) = auth_claims().await else {
        return Ok(empty());
    };
    let state = app_state().await?;
    let client = match state.pool.get().await {
        Ok(c) => c,
        Err(_) => return Ok(empty()),
    };

    // Kategori favorit user, diurutkan skor TER-DECAY (minat lama memudar —
    // konsisten dengan decay saat tulis di AffinityService).
    // Graceful: kalau tabel belum dimigrasi → kosong.
    let row = match client
        .query_opt(
            "SELECT category FROM user_affinity WHERE user_id = decode($1,'hex') \
             ORDER BY score * POWER(0.977, \
                 GREATEST(EXTRACT(EPOCH FROM (NOW() - updated_at)), 0) / 86400.0) DESC, \
               updated_at DESC \
             LIMIT 1",
            &[&claims.user_id],
        )
        .await
    {
        Ok(r) => r,
        Err(_) => return Ok(empty()),
    };
    let Some(row) = row else {
        return Ok(empty());
    };
    let top_cat: String = row.get(0);

    // Ambil product kategori itu lewat listing (pakai index GIN @> category).
    let q = ProductListQuery {
        page: Some(1),
        per_page: Some(12),
        city: None,
        category: Some(top_cat),
        search: None,
        status: Some("active".into()),
    };
    let result = state.product_svc.list(q, None).await.map_err(map_app_error)?;
    Ok(srv_paginated_products_to_web(result))
}
