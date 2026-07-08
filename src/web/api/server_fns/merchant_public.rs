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
    let p = state
        .merchant_svc
        .public_profile(&merchant_id)
        .await
        .map_err(map_app_error)?;

    // Status follow viewer: best-effort — viewer anonim sah (false).
    let is_following = match auth_claims().await {
        Ok(c) => state
            .merchant_svc
            .is_following(&merchant_id, &c.user_id)
            .await
            .unwrap_or(false),
        Err(_) => false,
    };

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

#[server(GetMerchantReviews, "/api-fn")]
pub async fn get_reviews(
    merchant_id: String,
    page: Option<i64>,
) -> Result<MerchantReviewsData, ServerFnError> {
    let state = app_state().await?;
    // store_name ikut dikirim agar header halaman reviews tak butuh fetch kedua.
    let profile = state
        .merchant_svc
        .public_profile(&merchant_id)
        .await
        .map_err(map_app_error)?;
    let summary = state
        .merchant_svc
        .review_summary(&merchant_id)
        .await
        .map_err(map_app_error)?;
    let items = state
        .merchant_svc
        .list_reviews(&merchant_id, page.unwrap_or(1), 20)
        .await
        .map_err(map_app_error)?;

    Ok(MerchantReviewsData {
        store_name: profile.store_name,
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
