//! service/story.rs
//!
//! Business logic untuk fitur Stories:
//!   - Upload media (image / video) ke storage
//!   - Enforce rate limit: 1x/hari untuk user biasa, unlimited untuk premium
//!   - CRUD story & mark viewed

use std::sync::Arc;

use bytes::Bytes;

use crate::{
    models::{
        notification::CreateNotificationInput,
        stories::{StoryGroupResponse, SubscriptionActivation, UploadStoryResponse},
    },
    repository::story::StoryRepository,
    service::{notification_store::NotificationStoreService, storage::StorageService},
    utils::error::{AppError, AppResult},
    web::models::PendingSubOrder,
};

// ── Konfigurasi ───────────────────────────────────────────────────────────────

/// Batas upload story per hari untuk user non-premium.
const FREE_DAILY_LIMIT: i64 = 1;

/// Max ukuran file story: 50 MB (untuk video).
const MAX_FILE_SIZE: usize = 50 * 1024 * 1024;

/// MIME type yang diizinkan untuk story media.
const ALLOWED_IMAGE_MIME: &[&str] = &["image/jpeg", "image/png", "image/webp", "image/gif"];
const ALLOWED_VIDEO_MIME: &[&str] = &["video/mp4", "video/quicktime", "video/webm"];

// ── Magic byte detector (gambar + video) ─────────────────────────────────────

fn detect_media_mime(data: &[u8]) -> Option<(&'static str, &'static str)> {
    // Returns (mime_type, media_kind) where media_kind = "image" | "video"
    match data {
        // JPEG
        [0xFF, 0xD8, 0xFF, ..] => Some(("image/jpeg", "image")),
        // PNG
        [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, ..] => Some(("image/png", "image")),
        // GIF
        [0x47, 0x49, 0x46, 0x38, 0x37, 0x61, ..] | [0x47, 0x49, 0x46, 0x38, 0x39, 0x61, ..] => {
            Some(("image/gif", "image"))
        }
        // WebP: RIFF....WEBP
        [0x52, 0x49, 0x46, 0x46, _, _, _, _, 0x57, 0x45, 0x42, 0x50, ..] => {
            Some(("image/webp", "image"))
        }
        // MP4 / MOV (ftyp box): byte 4-7 = "ftyp"
        d if d.len() >= 8 && &d[4..8] == b"ftyp" => Some(("video/mp4", "video")),
        // WebM: 0x1A 0x45 0xDF 0xA3
        [0x1A, 0x45, 0xDF, 0xA3, ..] => Some(("video/webm", "video")),
        _ => None,
    }
}
// ── Service ───────────────────────────────────────────────────────────────────

pub struct StoryService<R: StoryRepository> {
    repo: Arc<R>,
    storage: Arc<StorageService>,
    notif_store: Arc<NotificationStoreService>,
}

impl<R: StoryRepository> StoryService<R> {
    pub fn new(
        repo: Arc<R>,
        storage: Arc<StorageService>,
        notif_store: Arc<NotificationStoreService>,
    ) -> Self {
        Self {
            repo,
            storage,
            notif_store,
        }
    }

    // ── Upload story ──────────────────────────────────────────────────────────

    /// Upload story baru.
    ///
    /// `media_bytes`: raw bytes file.
    /// `slug`: slug event (opsional, dikirim jika story dibuat dari event detail).
    ///
    /// Alur:
    ///   1. Validasi ukuran & tipe file (magic bytes).
    ///   2. Cek rate limit (1/hari untuk free, unlimited untuk premium).
    ///   3. Upload ke storage.
    ///   4. Simpan record ke DB.
    pub async fn upload(
        &self,
        user_id: &str,
        media_bytes: Bytes,
        slug: Option<String>,
        title: Option<String>,
    ) -> AppResult<UploadStoryResponse> {
        // ── 1. Validasi ukuran ────────────────────────────────────────────────
        if media_bytes.len() > MAX_FILE_SIZE {
            return Err(AppError::BadRequest(format!(
                "File terlalu besar, maksimal {}MB",
                MAX_FILE_SIZE / 1024 / 1024
            )));
        }

        // ── 2. Deteksi tipe via magic bytes ───────────────────────────────────
        let (mime_type, media_kind) = detect_media_mime(&media_bytes).ok_or_else(|| {
            AppError::BadRequest(
                "Format file tidak didukung. Gunakan JPEG/PNG/WebP/GIF untuk gambar \
                 atau MP4/WebM/MOV untuk video."
                    .into(),
            )
        })?;

        let all_allowed: Vec<&&str> = ALLOWED_IMAGE_MIME
            .iter()
            .chain(ALLOWED_VIDEO_MIME.iter())
            .collect();
        if !all_allowed.iter().any(|&&m| m == mime_type) {
            return Err(AppError::BadRequest(format!(
                "Format tidak didukung: {mime_type}"
            )));
        }

        // ── 3. Rate limit ─────────────────────────────────────────────────────
        let is_premium = self
            .repo
            .is_premium(user_id)
            .await
            .map_err(AppError::Internal)?;

        if !is_premium {
            let count = self
                .repo
                .count_today(user_id)
                .await
                .map_err(AppError::Internal)?;
            if count >= FREE_DAILY_LIMIT {
                return Err(AppError::Conflict(
                    "Kamu sudah membuat story hari ini. \
                     Upgrade ke Premium untuk upload tanpa batas."
                        .into(),
                ));
            }
        }

        // ── 4. Resolve event info dari slug (opsional) ────────────────────────
        let (event_id, event_slug, event_title): (Option<String>, Option<String>, Option<String>) =
            match slug {
                Some(s) if !s.is_empty() => (None, Some(s), title.filter(|t| !t.is_empty())),
                _ => (None, None, None),
            };

        // ── 5. Upload ke storage ──────────────────────────────────────────────
        let folder = if media_kind == "video" {
            "stories/video"
        } else {
            "stories/image"
        };

        let media_url = self
            .storage
            .upload_media(media_bytes, folder, mime_type)
            .await?;

        // ── 6. Simpan ke DB ───────────────────────────────────────────────────
        let story = self
            .repo
            .create(
                user_id,
                &media_url,
                media_kind,
                None, // filter & overlays ditambahkan FE sebelum kirim (via canvas/render)
                &[],
                event_id.as_deref(),
                event_slug.as_deref(),
                event_title.as_deref(),
            )
            .await
            .map_err(|e| AppError::Internal(e))?;

        // ── 7. Background: notifikasi in-app (fire-and-forget) ────────────────
        // Clone semua data yang dibutuhkan SEBELUM spawn agar future 'static.
        let notif_store = self.notif_store.clone();
        let owner_id = user_id.to_string(); // owned String, 'static
        let story_id = story.id.clone(); // owned String, 'static
        let media_label = if media_kind == "video" {
            "video"
        } else {
            "foto"
        };

        tokio::spawn(async move {
            let input = CreateNotificationInput {
                user_id: owner_id,
                title: "Story Berhasil Diunggah".into(),
                body: format!(
                    "Story {} kamu sudah aktif dan bisa dilihat selama 24 jam.",
                    media_label
                ),
                kind: "story".into(),
                target_id: story_id.clone().into(),
            };

            if let Err(e) = notif_store.create(input).await {
                tracing::error!(
                    story_id = %story_id,
                    error = %e,
                    "Background notifikasi story gagal disimpan"
                );
            } else {
                tracing::info!(
                    story_id = %story_id,
                    "Notifikasi story uploaded tersimpan"
                );
            }
        });

        Ok(UploadStoryResponse {
            story_id: story.id,
            media_url: story.media_url,
        })
    }

    // ── List groups ───────────────────────────────────────────────────────────

    pub async fn list_groups(&self, viewer_id: &str) -> AppResult<Vec<StoryGroupResponse>> {
        self.repo
            .list_groups(viewer_id)
            .await
            .map_err(|e| AppError::Internal(e))
    }

    /// Daftar story untuk pengunjung anonim (belum login): semua story aktif
    /// tanpa status viewed. Membuka story tetap digate login di sisi client.
    pub async fn list_groups_public(&self) -> AppResult<Vec<StoryGroupResponse>> {
        self.repo
            .list_groups_public()
            .await
            .map_err(|e| AppError::Internal(e))
    }

    /// Arsip story publik untuk halaman /stories: satu grup per user (termasuk
    /// story expired), user dengan story terbaru dulu. Paginasi per user.
    pub async fn list_user_groups(
        &self,
        page: i64,
        per_page: i64,
    ) -> AppResult<Vec<StoryGroupResponse>> {
        let page = page.max(1);
        let per_page = per_page.clamp(1, 48);
        let offset = (page - 1) * per_page;
        self.repo
            .list_user_groups_paged(per_page, offset)
            .await
            .map_err(|e| AppError::Internal(e))
    }

    /// Story milik user sendiri sebagai satu grup (aktif + arsip), terbaru dulu —
    /// dipakai daftar + viewer "Story Saya" di halaman profil.
    pub async fn list_my_group(&self, user_id: &str) -> AppResult<Vec<StoryGroupResponse>> {
        self.repo
            .list_my_group(user_id)
            .await
            .map_err(|e| AppError::Internal(e))
    }

    // ── Mark viewed ───────────────────────────────────────────────────────────

    pub async fn mark_viewed(&self, story_id: &str, viewer_id: &str) -> AppResult<()> {
        self.repo
            .mark_viewed(story_id, viewer_id)
            .await
            .map_err(|e| AppError::Internal(e))
    }

    // ── Delete story ──────────────────────────────────────────────────────────

    pub async fn delete(&self, story_id: &str, user_id: &str) -> AppResult<()> {
        let deleted = self
            .repo
            .delete(story_id, user_id)
            .await
            .map_err(|e| AppError::Internal(e))?;
        if !deleted {
            return Err(AppError::NotFound(format!(
                "Story {story_id} tidak ditemukan atau bukan milikmu"
            )));
        }
        Ok(())
    }

    // ── Premium status ────────────────────────────────────────────────────────

    pub async fn is_premium(&self, user_id: &str) -> AppResult<bool> {
        self.repo
            .is_premium(user_id)
            .await
            .map_err(|e| AppError::Internal(e))
    }

    // ── Activate premium ──────────────────────────────────────────────────────

    pub async fn activate_premium(&self, user_id: &str, plan: &str, days: i64) -> AppResult<SubscriptionActivation> {
        if days < 1 || days > 3650 {
            return Err(AppError::BadRequest(
                "Durasi premium harus antara 1–3650 hari".into(),
            ));
        }
        self.repo
            .activate_premium(user_id, plan, days)
            .await
            .map_err(AppError::Internal)
    }

    pub async fn create_pending_subscription_order(&self, user_id: &str, plan: &str) -> AppResult<PendingSubOrder> {
        self.repo
            .create_pending_subscription_order(user_id, plan)
            .await
            .map_err(AppError::Internal)
    }

    pub async fn confirm_subscription_order(
        &self,
        order_id: &str,
        user_id: &str,
        plan: &str,
        days: i64,
    ) -> AppResult<SubscriptionActivation> {
        self.repo
            .confirm_subscription_order(order_id, user_id, plan, days)
            .await
            .map_err(AppError::Internal)
    }
}