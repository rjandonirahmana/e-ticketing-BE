//! merchant_public.rs — Server functions profil merchant PUBLIK (sisi user).
//!
//! Halaman /m/{id} (profil + event) dan /m/{id}/reviews (rating & ulasan).
//! Semua endpoint baca bersifat publik; tulis (review/follow) butuh login.

use crate::web::models::*;
use leptos::prelude::*;
#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use super::helpers::*;

#[server(GetMerchantPublicProfile, "/api-fn")]
pub async fn get_merchant_public_profile(
    merchant_id: String,
) -> Result<MerchantPublicProfile, ServerFnError> {
    let state = app_state().await?;

    // user_id viewer (JWT cookie, tanpa DB) — untuk status follow.
    let viewer = auth_claims().await.ok().map(|c| c.user_id);

    // Profil + status follow jalan paralel: satu latensi round-trip. Follow
    // best-effort (anonim / gagal = false), jadi jangan gagalkan seluruh profil.
    let (profile, follow_res) = futures::join!(
        state.merchant_svc.public_profile(&merchant_id),
        async {
            match &viewer {
                Some(uid) => state.merchant_svc.is_following(&merchant_id, uid).await,
                None => Ok(false),
            }
        },
    );
    let p = profile.map_err(map_app_error)?;
    let is_following = follow_res.unwrap_or(false);

    Ok(MerchantPublicProfile {
        merchant_id: p.merchant_id,
        store_name: p.store_name,
        description: p.description,
        logo_url: p.logo_url,
        verified: p.verified,
        followers: p.followers,
        events_count: p.events_count,
        rating_avg: p.rating_avg,
        rating_count: p.rating_count,
        is_following,
    })
}

#[server(GetMerchantPublicEvents, "/api-fn")]
pub async fn get_merchant_public_events(
    merchant_id: String,
    page: Option<i64>,
) -> Result<PaginatedEvents, ServerFnError> {
    use crate::models::events::EventListQuery;
    let state = app_state().await?;
    let q = EventListQuery {
        page,
        per_page: Some(12),
        city: None,
        category: None,
        search: None,
        // Publik: hanya event aktif — jangan bocorkan draft/cancelled merchant.
        status: Some("active".into()),
    };
    let result = state
        .event_svc
        .list(q, Some(&merchant_id))
        .await
        .map_err(map_app_error)?;
    Ok(srv_paginated_events_to_web(result))
}

/// Story milik merchant (profil publik /m/{id}). `merchant_id` == `user_id`
/// pemilik merchant (lihat `merchant_details.user_id`), sehingga story merchant
/// = story user tersebut. Satu grup berisi story aktif + arsip, terbaru dulu.
/// Publik (tak butuh login) untuk MELIHAT daftar; membuka viewer digate login
/// di client — konsisten dengan StoryBar.
#[server(GetMerchantStories, "/api-fn")]
pub async fn get_merchant_stories(
    merchant_id: String,
) -> Result<Vec<crate::web::state::stories::StoryGroup>, ServerFnError> {
    let state = app_state().await?;
    let groups = state
        .story_svc
        .list_my_group(&merchant_id)
        .await
        .map_err(map_app_error)?;
    Ok(srv_story_groups_to_web(groups))
}

#[server(GetMerchantReviews, "/api-fn")]
pub async fn get_reviews(
    merchant_id: String,
    page: Option<i64>,
) -> Result<MerchantReviewsData, ServerFnError> {
    let state = app_state().await?;
    // Ringkasan (store_name + rating) & daftar ulasan jalan paralel — satu
    // latensi round-trip, bukan tiga. store_name ikut di summary sehingga header
    // tak butuh fetch profil lengkap (yang berat: 4 sub-query followers/events/…).
    let (summary, items) = futures::try_join!(
        state.merchant_svc.review_summary(&merchant_id),
        state
            .merchant_svc
            .list_reviews(&merchant_id, page.unwrap_or(1), 20),
    )
    .map_err(map_app_error)?;

    // store_name None → merchant tidak ada.
    let store_name = match summary.store_name {
        Some(n) => n,
        None => return Err(ServerFnError::ServerError("Merchant tidak ditemukan".into())),
    };

    Ok(MerchantReviewsData {
        store_name,
        avg: summary.avg,
        total: summary.total,
        dist: summary.dist,
        items: items
            .into_iter()
            .map(|i| MerchantReviewItem {
                user_name: i.user_name,
                rating: i.rating,
                comment: i.comment,
                created_at: i.created_at,
            })
            .collect(),
    })
}

#[server(SubmitMerchantReview, "/api-fn")]
pub async fn submit_merchant_review(
    merchant_id: String,
    rating: i32,
    comment: String,
) -> Result<(), ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    state
        .merchant_svc
        .submit_review(&merchant_id, &claims.user_id, rating, &comment)
        .await
        .map_err(map_app_error)
}

#[server(SetFollowMerchant, "/api-fn")]
pub async fn set_follow_merchant(merchant_id: String, follow: bool) -> Result<(), ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    state
        .merchant_svc
        .set_follow(&merchant_id, &claims.user_id, follow)
        .await
        .map_err(map_app_error)
}
