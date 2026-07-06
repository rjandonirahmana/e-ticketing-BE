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

/// Arsip story publik (halaman /stories): SATU grup per user — termasuk story
/// yang sudah expired — user dengan story terbaru dulu. Paginasi per user.
/// Tidak butuh login untuk MELIHAT daftar; membuka story digate login di client.
#[server(GetStoryArchiveGroups, "/api-fn")]
pub async fn get_story_archive_groups(
    page: Option<i64>,
) -> Result<Vec<crate::web::state::stories::StoryGroup>, ServerFnError> {
    let state = app_state().await?;
    let groups = state
        .story_svc
        .list_user_groups(page.unwrap_or(1), 24)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_story_groups_to_web(groups));
}
