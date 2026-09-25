//! server_fns/post.rs — Marketplace C2C (Pasar): posting, like, komentar.
//!
//! Semua user terdaftar boleh posting/like/komentar (BUKAN cuma merchant) —
//! server fn di sini gerbangnya `auth_claims()`, bukan `require_roles`.

use leptos::prelude::*;
#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use super::helpers::*;
use crate::web::models::{PaginatedComments, PaginatedPosts, Post, PostComment};

#[server(CreatePost, "/api-fn")]
pub async fn create_post(
    kind: String,
    title: String,
    description: String,
    price: Option<i64>,
    condition: Option<String>,
    category: Option<String>,
    city: Option<String>,
    images: Vec<String>,
) -> Result<Post, ServerFnError> {
    use crate::models::post::CreatePostRequest;
    let claims = auth_claims().await?;
    let state = app_state().await?;

    let images_json = images
        .into_iter()
        .map(|url| serde_json::json!({ "url": url }))
        .collect();

    let req = CreatePostRequest {
        kind,
        title,
        description,
        price,
        condition,
        category,
        city,
        images: images_json,
    };

    let post = state
        .post_svc
        .create(&claims.user_id, req)
        .await
        .map_err(map_app_error)?;
    Ok(srv_post_to_web(post))
}

#[server(ListPosts, "/api-fn")]
pub async fn list_posts(
    kind: Option<String>,
    category: Option<String>,
    city: Option<String>,
    q: Option<String>,
    page: Option<i64>,
) -> Result<PaginatedPosts, ServerFnError> {
    let viewer = auth_claims().await.ok().map(|c| c.user_id);
    let state = app_state().await?;

    let result = state
        .post_svc
        .list_feed(
            viewer.as_deref(),
            kind.as_deref(),
            category.as_deref(),
            city.as_deref(),
            q.as_deref(),
            page.unwrap_or(1),
            20,
        )
        .await
        .map_err(map_app_error)?;
    Ok(srv_paginated_posts_to_web(result))
}

#[server(GetPostDetail, "/api-fn")]
pub async fn get_post_detail(id: String) -> Result<Post, ServerFnError> {
    let viewer = auth_claims().await.ok().map(|c| c.user_id);
    let state = app_state().await?;

    let post = state
        .post_svc
        .get_detail(&id, viewer.as_deref())
        .await
        .map_err(map_app_error)?;
    Ok(srv_post_to_web(post))
}

/// Toggle like (bukan target eksplisit — lihat komentar `PostService::toggle_like`).
#[server(TogglePostLike, "/api-fn")]
pub async fn toggle_like(post_id: String) -> Result<(bool, i32), ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;

    state
        .post_svc
        .toggle_like(&post_id, &claims.user_id)
        .await
        .map_err(map_app_error)
}

#[server(AddPostComment, "/api-fn")]
pub async fn add_comment(post_id: String, body: String) -> Result<PostComment, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;

    let comment = state
        .post_svc
        .add_comment(&post_id, &claims.user_id, &body)
        .await
        .map_err(map_app_error)?;
    Ok(srv_post_comment_to_web(comment))
}

#[server(ListPostComments, "/api-fn")]
pub async fn list_comments(post_id: String, page: Option<i64>) -> Result<PaginatedComments, ServerFnError> {
    let state = app_state().await?;

    let result = state
        .post_svc
        .list_comments(&post_id, page.unwrap_or(1), 20)
        .await
        .map_err(map_app_error)?;
    Ok(srv_paginated_comments_to_web(result))
}

#[server(DeletePostComment, "/api-fn")]
pub async fn delete_comment(id: String) -> Result<(), ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;

    state
        .post_svc
        .delete_comment(&id, &claims.user_id)
        .await
        .map_err(map_app_error)
}

#[server(UpdatePostStatus, "/api-fn")]
pub async fn update_post_status(id: String, status: String) -> Result<(), ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;

    state
        .post_svc
        .update_status(&id, &claims.user_id, &status)
        .await
        .map_err(map_app_error)
}

#[server(DeletePost, "/api-fn")]
pub async fn delete_post(id: String) -> Result<(), ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;

    state
        .post_svc
        .delete(&id, &claims.user_id)
        .await
        .map_err(map_app_error)
}
