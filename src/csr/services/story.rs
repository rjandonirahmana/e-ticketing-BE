// src/services/story.rs — updated
//
// Perubahan dari versi sebelumnya:
//   - Tambah `is_rate_limited()` helper untuk deteksi 409 dari rate limit.
//   - Mode A di StoryCreatorPage sekarang meneruskan slug (jika ada) agar
//     deep-link event tetap bekerja bahkan saat user memilih file sendiri.

use serde::{Deserialize, Serialize};

use crate::csr::services::client::{get_private, post_multipart_private, ApiError};
use crate::csr::state::stories::{StoryGroup, StoryItem, StoryMediaType, StoryOverlay};

// ── API response shapes ───────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiStoryItem {
    id: String,
    user_id: String,
    username: String,
    avatar_url: String,
    media_url: String,
    media_type: String,
    filter: Option<String>,
    overlays: Option<Vec<StoryOverlay>>,
    created_at: String,
    expires_at: String,
    viewed: bool,
    event_id: Option<String>,
    event_slug: Option<String>,
    event_title: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiStoryGroup {
    user_id: String,
    username: String,
    avatar_url: String,
    stories: Vec<ApiStoryItem>,
}

/// Metadata event yang disertakan saat story dibuat dari EventDetailPage.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventStoryMeta {
    pub event_id: String,
    pub event_slug: String,
    pub event_title: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadStoryResponse {
    pub story_id: String,
    pub media_url: String,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_story(s: ApiStoryItem) -> StoryItem {
    use chrono::DateTime;

    let media_type = match s.media_type.as_bytes().first().copied() {
        Some(b'v' | b'V') => StoryMediaType::Video,
        _ => StoryMediaType::Image,
    };

    StoryItem {
        id: s.id,
        user_id: s.user_id,
        username: s.username,
        avatar_url: s.avatar_url,
        media_url: s.media_url,
        media_type,
        filter: s.filter,
        overlays: s.overlays.unwrap_or_default(),
        created_at: DateTime::parse_from_rfc3339(&s.created_at)
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        expires_at: DateTime::parse_from_rfc3339(&s.expires_at)
            .map(|d| d.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        viewed: s.viewed,
        event_id: s.event_id,
        event_slug: s.event_slug,
        event_title: s.event_title,
    }
}

/// Cek apakah error adalah rate-limit (HTTP 409) dari kuota story harian.
///
/// Dipanggil di StoryCreatorPage setelah `upload_story` gagal untuk memutuskan
/// apakah perlu redirect ke /subscription.
pub fn is_rate_limited(err: &ApiError) -> bool {
    err.status == 409
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Fetch semua story group dari backend.
/// Endpoint: GET /stories
pub async fn fetch_story_groups() -> Result<Vec<StoryGroup>, ApiError> {
    let raw: Vec<ApiStoryGroup> = get_private("/stories").await?;

    let groups = raw
        .into_iter()
        .map(|g| {
            let stories: Vec<StoryItem> = g.stories.into_iter().map(parse_story).collect();
            let all_viewed = stories.iter().all(|s| s.viewed);
            StoryGroup {
                user_id: g.user_id,
                username: g.username,
                avatar_url: g.avatar_url,
                stories,
                all_viewed,
            }
        })
        .collect();

    Ok(groups)
}

/// Upload media story ke backend (multipart/form-data).
///
/// Fields:
///   - `media`  (required) — file gambar atau video.
///   - `slug`   (optional) — slug event untuk deep-link.
///
/// Endpoint: POST /stories
///
/// Error 409 → rate limit (free user sudah buat story hari ini).
/// Gunakan `is_rate_limited()` untuk cek dan redirect ke /subscription.
pub async fn upload_story(
    file: &web_sys::File,
    slug: Option<String>,
) -> Result<UploadStoryResponse, ApiError> {
    let form = web_sys::FormData::new().map_err(|_| ApiError::network("gagal buat FormData"))?;

    form.append_with_blob("media", file)
        .map_err(|_| ApiError::network("gagal append file"))?;

    if let Some(s) = slug {
        form.append_with_str("slug", &s)
            .map_err(|_| ApiError::network("gagal append slug"))?;
    }

    post_multipart_private("/stories", form).await
}

/// Tandai story sudah dilihat.
/// Endpoint: POST /stories/:id/view
pub async fn mark_viewed(story_id: &str) -> Result<(), ApiError> {
    use crate::csr::services::client::post_private;

    #[derive(Serialize)]
    struct Empty {}

    post_private::<Empty, serde_json::Value>(&format!("/stories/{}/view", story_id), &Empty {})
        .await?;
    Ok(())
}

/// Hapus story milik sendiri.
/// Endpoint: DELETE /stories/:id
pub async fn delete_story(story_id: &str) -> Result<(), ApiError> {
    use crate::csr::services::client::delete_private;
    delete_private::<serde_json::Value>(&format!("/stories/{}", story_id)).await?;
    Ok(())
}
