//! reco.rs — Behavior tracking + rekomendasi per-user (server-side, DB).
//!
//! - `record_affinity`  : catat minat user (kategori event yang dibuka). No-op
//!                        diam-diam bila belum login (anonim → localStorage client).
//! - `get_recommended_events` : event dari kategori favorit user (skor tertinggi
//!                        di `user_affinity`). Kosong bila belum login / belum ada
//!                        data / tabel belum dimigrasi (graceful).

use crate::web::models::*;
use leptos::prelude::*;
#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use super::helpers::*;

/// Catat perilaku: user membuka event dengan kategori `categories`. Skor tiap
/// kategori di-upsert (+1). Aman dipanggil selalu — no-op bila tak login.
#[server(RecordAffinity, "/api-fn")]
pub async fn record_affinity(categories: Vec<String>) -> Result<(), ServerFnError> {
    // Diam-diam berhenti bila belum login (user anonim ditangani localStorage).
    let Ok(claims) = auth_claims().await else {
        return Ok(());
    };
    if categories.is_empty() {
        return Ok(());
    }
    let state = app_state().await?;
    let client = state
        .pool
        .get()
        .await
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;

    for cat in categories.iter().take(3) {
        let c = cat.trim().to_string();
        if c.is_empty() {
            continue;
        }
        // Upsert O(1): +1 pada kategori tsb; recency lewat updated_at.
        let _ = client
            .execute(
                "INSERT INTO user_affinity (user_id, category, score, updated_at) \
                 VALUES (decode($1,'hex'), $2, 1.0, NOW()) \
                 ON CONFLICT (user_id, category) DO UPDATE \
                   SET score = user_affinity.score + 1.0, updated_at = NOW()",
                &[&claims.user_id, &c],
            )
            .await;
    }
    Ok(())
}

/// Event rekomendasi = event dari kategori favorit user (skor tertinggi).
/// Kosong bila belum login / belum ada perilaku / tabel belum ada (graceful).
#[server(GetRecommendedEvents, "/api-fn")]
pub async fn get_recommended_events() -> Result<PaginatedEvents, ServerFnError> {
    use crate::models::events::EventListQuery;

    let empty = || PaginatedEvents {
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

    // Kategori favorit user. Graceful: kalau tabel belum dimigrasi → kosong.
    let row = match client
        .query_opt(
            "SELECT category FROM user_affinity WHERE user_id = decode($1,'hex') \
             ORDER BY score DESC, updated_at DESC LIMIT 1",
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

    // Ambil event kategori itu lewat listing (pakai index GIN @> category).
    let q = EventListQuery {
        page: Some(1),
        per_page: Some(12),
        city: None,
        category: Some(top_cat),
        search: None,
        status: Some("active".into()),
    };
    let result = state.event_svc.list(q, None).await.map_err(map_app_error)?;
    Ok(srv_paginated_events_to_web(result))
}
