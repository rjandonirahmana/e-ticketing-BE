use leptos::prelude::*;
#[cfg(feature = "ssr")]
use super::helpers::*;

#[server(GetStoryGroups, "/api-fn")]
pub async fn get_story_groups(
) -> Result<Vec<crate::web::state::stories::StoryGroup>, ServerFnError> {
    let state = app_state().await?;
    // Pengunjung anonim tetap dapat MELIHAT daftar story (semua story aktif
    // bersifat publik) — hanya tanpa status viewed per-user. Membuka story
    // digate login di client (StoryBar redirect ke /login).
    let groups = match auth_claims().await {
        Ok(claims) => state
            .story_svc
            .list_groups(&claims.user_id)
            .await
            .map_err(map_app_error)?,
        Err(_) => state
            .story_svc
            .list_groups_public()
            .await
            .map_err(map_app_error)?,
    };
    return Ok(srv_story_groups_to_web(groups));
}

/// Arsip story publik (halaman /stories): semua story yang pernah ada —
/// termasuk yang sudah expired — terbaru dulu. Tidak butuh login untuk
/// MELIHAT daftar; membuka story digate login di client.
#[server(GetAllStories, "/api-fn")]
pub async fn get_all_stories(
    page: Option<i64>,
) -> Result<Vec<crate::web::state::stories::StoryItem>, ServerFnError> {
    let state = app_state().await?;
    let items = state
        .story_svc
        .list_all(page.unwrap_or(1), 24)
        .await
        .map_err(map_app_error)?;

    // Reuse mapper group→web: bungkus flat list dalam satu grup sementara,
    // lalu ambil kembali stories-nya (field grup tidak dipakai mapper item).
    let mapped = srv_story_groups_to_web(vec![crate::models::stories::StoryGroupResponse {
        user_id: String::new(),
        username: String::new(),
        avatar_url: String::new(),
        stories: items,
    }]);
    return Ok(mapped.into_iter().flat_map(|g| g.stories).collect());
}
