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

/// Story milik user yang login (aktif + arsip) sebagai satu grup — untuk section
/// "Story Saya" di profil (thumbnail + buka viewer). `None` bila belum ada story.
/// Wajib login.
#[server(GetMyStoryGroup, "/api-fn")]
pub async fn get_my_story_group(
) -> Result<Option<crate::web::state::stories::StoryGroup>, ServerFnError> {
    let state = app_state().await?;
    let claims = auth_claims().await?;
    let groups = state
        .story_svc
        .list_my_group(&claims.user_id)
        .await
        .map_err(map_app_error)?;
    Ok(srv_story_groups_to_web(groups).into_iter().next())
}

/// Hapus satu story milik user. Owner-enforced di repo (DELETE ... AND user_id).
#[server(DeleteMyStory, "/api-fn")]
pub async fn delete_my_story(story_id: String) -> Result<(), ServerFnError> {
    let state = app_state().await?;
    let claims = auth_claims().await?;
    state
        .story_svc
        .delete(&story_id, &claims.user_id)
        .await
        .map_err(map_app_error)?;
    Ok(())
}

/// Tandai satu story sudah ditonton oleh pengguna yang sedang masuk.
///
/// ── KENAPA BARU ADA SEKARANG ────────────────────────────────────────────
/// Seluruh sisi servernya sudah lama lengkap: tabel `story_views`,
/// `StoryService::mark_viewed`, dan REST `POST /api/stories/:id/view`. Yang
/// tak pernah ada hanyalah jalur untuk web Leptos.
///
/// Akibatnya penanda `sudah ditonton` hanya hidup di memori tab yang sedang
/// terbuka — `StoriesStore::mark_current_viewed` menulis ke sinyal lokal dan
/// berhenti di situ. Cincin warnanya memudar seperti seharusnya, lalu muncul
/// kembali utuh begitu halaman dimuat ulang, karena server tak pernah diberi
/// tahu apa pun. Yang dilihat pengguna adalah story yang sudah ia tonton terus
/// menerus menandai dirinya belum ditonton.
///
/// Anonim mendapat `Err` dari `auth_claims` dan pemanggilnya memang
/// mengabaikannya — tak ada riwayat tontonan untuk disimpan bagi orang yang
/// belum punya akun, dan itu bukan kegagalan yang perlu ditampilkan.
#[server(MarkStoryViewed, "/api-fn")]
pub async fn mark_story_viewed(story_id: String) -> Result<(), ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    state
        .story_svc
        .mark_viewed(&story_id, &claims.user_id)
        .await
        .map_err(map_app_error)?;
    Ok(())
}
