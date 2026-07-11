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
        header_url: p.header_url,
        verified: p.verified,
        followers: p.followers,
        events_count: p.events_count,
        rating_avg: p.rating_avg,
        rating_count: p.rating_count,
        is_following,
    })
}

/// SEMUA data halaman /m/{id} dalam satu panggilan: profil + events page-1 +
/// ulasan (ringkasan + 20 pertama) + story. Satu round-trip HTTP dari klien
/// (bukan 4 POST /api-fn terpisah) dan satu `futures::join!` di server —
/// memanggil LAPISAN SERVICE langsung, bukan server-fn lain (server-fn yang
/// memanggil server-fn menambah overhead ekstraksi context per panggilan).
/// Profil = penentu halaman (gagal → error); events/ulasan/story best-effort
/// (gagal → kosong) agar satu bagian sekunder tak menjatuhkan seluruh halaman.
#[server(GetMerchantPublicPage, "/api-fn")]
pub async fn get_merchant_public_page(
    merchant_id: String,
) -> Result<MerchantPublicPageData, ServerFnError> {
    use crate::models::events::EventListQuery;
    let state = app_state().await?;
    let viewer = auth_claims().await.ok().map(|c| c.user_id);

    let q = EventListQuery {
        page: Some(1),
        per_page: Some(12),
        city: None,
        category: None,
        search: None,
        // Publik: hanya event aktif — jangan bocorkan draft/cancelled merchant.
        status: Some("active".into()),
    };

    let (profile_res, follow_res, events_res, summary_res, items_res, stories_res) = futures::join!(
        state.merchant_svc.public_profile(&merchant_id),
        async {
            match &viewer {
                Some(uid) => state.merchant_svc.is_following(&merchant_id, uid).await,
                None => Ok(false),
            }
        },
        state.event_svc.list(q, Some(&merchant_id)),
        state.merchant_svc.review_summary(&merchant_id),
        state.merchant_svc.list_reviews(&merchant_id, 1, 20),
        state.story_svc.list_my_group(&merchant_id),
    );

    let p = profile_res.map_err(map_app_error)?;
    let profile = MerchantPublicProfile {
        merchant_id: p.merchant_id,
        store_name: p.store_name,
        description: p.description,
        logo_url: p.logo_url,
        header_url: p.header_url,
        verified: p.verified,
        followers: p.followers,
        events_count: p.events_count,
        rating_avg: p.rating_avg,
        rating_count: p.rating_count,
        is_following: follow_res.unwrap_or(false),
    };

    let events = events_res
        .map(srv_paginated_events_to_web)
        .unwrap_or_default();

    let reviews = match (summary_res, items_res) {
        (Ok(summary), Ok(items)) => MerchantReviewsData {
            store_name: summary
                .store_name
                .unwrap_or_else(|| profile.store_name.clone()),
            avg: summary.avg,
            total: summary.total,
            dist: summary.dist,
            items: items
                .into_iter()
                .map(|i| MerchantReviewItem {
                    user_id: i.user_id,
                    user_name: i.user_name,
                    rating: i.rating,
                    comment: i.comment,
                    created_at: i.created_at,
                })
                .collect(),
        },
        _ => MerchantReviewsData {
            store_name: profile.store_name.clone(),
            avg: 0.0,
            total: 0,
            dist: [0; 5],
            items: Vec::new(),
        },
    };

    let stories = stories_res.map(srv_story_groups_to_web).unwrap_or_default();

    Ok(MerchantPublicPageData {
        profile,
        events,
        reviews,
        stories,
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
                user_id: i.user_id,
                user_name: i.user_name,
                rating: i.rating,
                comment: i.comment,
                created_at: i.created_at,
            })
            .collect(),
    })
}

/// Profil publik user biasa (/u/{id}) — nama + jumlah following/reviews/stories.
#[server(GetUserPublic, "/api-fn")]
pub async fn get_user_public(user_id: String) -> Result<UserPublicProfile, ServerFnError> {
    let state = app_state().await?;
    let p = state
        .merchant_svc
        .user_public(&user_id)
        .await
        .map_err(map_app_error)?;
    Ok(UserPublicProfile {
        user_id: p.user_id,
        name: p.name,
        following: p.following,
        reviews: p.reviews,
        stories: p.stories,
    })
}

/// Ulasan yang DITULIS user (ke berbagai merchant). Publik. 20 per halaman.
#[server(GetUserReviews, "/api-fn")]
pub async fn get_user_reviews(
    user_id: String,
    page: Option<i64>,
) -> Result<UserReviewsData, ServerFnError> {
    let state = app_state().await?;
    let items = state
        .merchant_svc
        .list_user_reviews(&user_id, page.unwrap_or(1), 20)
        .await
        .map_err(map_app_error)?;
    Ok(UserReviewsData {
        total: items.len() as i64,
        items: items
            .into_iter()
            .map(|i| UserReviewItem {
                merchant_id: i.merchant_id,
                store_name: i.store_name,
                rating: i.rating,
                comment: i.comment,
                created_at: i.created_at,
            })
            .collect(),
    })
}

/// Merchant acak — ditampilkan di overlay pencarian SEBELUM user mengetik
/// (discovery). Publik.
#[server(GetRandomMerchants, "/api-fn")]
pub async fn get_random_merchants(limit: i64) -> Result<Vec<MerchantSearchItem>, ServerFnError> {
    let state = app_state().await?;
    let items = state
        .merchant_svc
        .random(limit)
        .await
        .map_err(map_app_error)?;
    Ok(items
        .into_iter()
        .map(|m| MerchantSearchItem {
            merchant_id: m.merchant_id,
            store_name: m.store_name,
            logo_url: m.logo_url,
            verified: m.verified,
        })
        .collect())
}

/// Cari merchant berdasarkan nama (autocomplete search /explore). Publik.
#[server(SearchMerchants, "/api-fn")]
pub async fn search_merchants(query: String) -> Result<Vec<MerchantSearchItem>, ServerFnError> {
    let state = app_state().await?;
    let items = state
        .merchant_svc
        .search(&query, 8)
        .await
        .map_err(map_app_error)?;
    Ok(items
        .into_iter()
        .map(|m| MerchantSearchItem {
            merchant_id: m.merchant_id,
            store_name: m.store_name,
            logo_url: m.logo_url,
            verified: m.verified,
        })
        .collect())
}

/// Daftar follower merchant (publik). 30 per halaman, terbaru dulu.
#[server(GetMerchantFollowers, "/api-fn")]
pub async fn get_merchant_followers(
    merchant_id: String,
    page: Option<i64>,
) -> Result<MerchantFollowersData, ServerFnError> {
    let state = app_state().await?;
    let (total, items) = state
        .merchant_svc
        .list_followers(&merchant_id, page.unwrap_or(1), 30)
        .await
        .map_err(map_app_error)?;
    Ok(MerchantFollowersData {
        total,
        items: items
            .into_iter()
            .map(|f| FollowerItem {
                user_id: f.user_id,
                name: f.name,
                role: f.role,
                created_at: f.created_at,
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

/// Apakah viewer (login) berhak menulis ulasan untuk merchant — yaitu punya ≥1
/// pesanan 'paid'. Anonim → false (form ulasan digate login lebih dulu).
#[server(CanReviewMerchant, "/api-fn")]
pub async fn can_review_merchant(merchant_id: String) -> Result<bool, ServerFnError> {
    let claims = match auth_claims().await {
        Ok(c) => c,
        Err(_) => return Ok(false),
    };
    let state = app_state().await?;
    state
        .merchant_svc
        .can_review(&merchant_id, &claims.user_id)
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
