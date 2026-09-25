//! service/post.rs
//!
//! Business logic Marketplace C2C: validasi posting, like/comment + notifikasi
//! best-effort. Transaksi COD murni — tak menyentuh payment/order sama sekali.

use std::sync::Arc;

use crate::models::notification::CreateNotificationInput;
use crate::models::post::{CreatePostRequest, PaginatedComments, PaginatedPosts, Post, PostComment};
use crate::repository::post::PostRepository;
use crate::service::notification_store::NotificationStoreService;
use crate::service::storage::StorageService;
use crate::utils::error::{AppError, AppResult};
// Daftar kategori TUNGGAL untuk seluruh aplikasi (lihat komentar di
// web/models.rs — sudah kategori barang umum, bukan lagi kategori event).
// Dipakai ulang di sini, bukan ditulis daftar baru, supaya kategori posting
// marketplace & kategori produk merchant tak bisa berselisih. Import lintas-
// layer `web::models` dari `service` SUDAH menjadi pola di proyek ini
// (lihat `service/story.rs` yang mengimpor `web::models::PendingSubOrder`).
use crate::web::models::PRODUCT_CATEGORIES;

const MAX_TITLE_LEN: usize = 150;
const MAX_DESC_LEN: usize = 3000;
const MAX_COMMENT_LEN: usize = 1000;
const MAX_IMAGES: usize = 6;
const ALLOWED_KIND: &[&str] = &["jual", "cari"];
const ALLOWED_CONDITION: &[&str] = &["baru", "bekas"];
const ALLOWED_STATUS: &[&str] = &["active", "sold", "archived"];

pub struct PostService {
    repo: Arc<dyn PostRepository>,
    notif_store: Arc<NotificationStoreService>,
    storage: Arc<StorageService>,
}

impl PostService {
    pub fn new(
        repo: Arc<dyn PostRepository>,
        notif_store: Arc<NotificationStoreService>,
        storage: Arc<StorageService>,
    ) -> Self {
        Self { repo, notif_store, storage }
    }

    /// Aturan `condition` SALING BERGANTUNG pada `kind` — itu sebabnya validasi
    /// hidup di sini, bukan derive `validator::Validate` per-field.
    ///
    /// `images` divalidasi terhadap bucket storage KITA (pola sama dengan
    /// `send_image` di `group_chat.rs`) — tanpa ini, klien bisa menempelkan
    /// URL luar mana pun sebagai "foto barang" dan memuatnya di layar siapa
    /// pun yang membuka postingan itu (tracking pixel, konten berbahaya, dll).
    fn validate_create(&self, req: &CreatePostRequest) -> AppResult<()> {
        if !ALLOWED_KIND.contains(&req.kind.as_str()) {
            return Err(AppError::BadRequest(format!(
                "kind harus salah satu dari {ALLOWED_KIND:?}"
            )));
        }
        let title = req.title.trim();
        if title.is_empty() || title.chars().count() > MAX_TITLE_LEN {
            return Err(AppError::BadRequest(format!(
                "Judul wajib diisi, maksimal {MAX_TITLE_LEN} karakter"
            )));
        }
        let desc = req.description.trim();
        if desc.is_empty() || desc.chars().count() > MAX_DESC_LEN {
            return Err(AppError::BadRequest(format!(
                "Deskripsi wajib diisi, maksimal {MAX_DESC_LEN} karakter"
            )));
        }
        if let Some(p) = req.price {
            if p < 0 {
                return Err(AppError::BadRequest("Harga tidak boleh negatif".into()));
            }
        }
        match req.kind.as_str() {
            "jual" => {
                let Some(c) = req.condition.as_deref() else {
                    return Err(AppError::BadRequest(
                        "condition wajib diisi untuk kind='jual'".into(),
                    ));
                };
                if !ALLOWED_CONDITION.contains(&c) {
                    return Err(AppError::BadRequest(format!(
                        "condition harus salah satu dari {ALLOWED_CONDITION:?}"
                    )));
                }
            }
            "cari" => {
                if req.condition.is_some() {
                    return Err(AppError::BadRequest(
                        "condition harus kosong untuk kind='cari'".into(),
                    ));
                }
            }
            _ => unreachable!("sudah divalidasi di atas"),
        }
        if req.images.len() > MAX_IMAGES {
            return Err(AppError::BadRequest(format!("Maksimal {MAX_IMAGES} foto")));
        }
        for img in &req.images {
            let url = img.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if url.is_empty() || !self.storage.milik_bucket(url) {
                return Err(AppError::BadRequest(
                    "Foto harus diunggah lewat /upload/post-image, bukan tautan luar".into(),
                ));
            }
        }
        if let Some(cat) = req.category.as_deref() {
            if !PRODUCT_CATEGORIES.contains(&cat) {
                return Err(AppError::BadRequest("Kategori tidak dikenal".into()));
            }
        }
        Ok(())
    }

    pub async fn create(&self, user_id: &str, req: CreatePostRequest) -> AppResult<Post> {
        self.validate_create(&req)?;
        self.repo.create(user_id, &req).await.map_err(AppError::Internal)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list_feed(
        &self,
        viewer_id: Option<&str>,
        kind: Option<&str>,
        category: Option<&str>,
        city: Option<&str>,
        q: Option<&str>,
        page: i64,
        per_page: i64,
    ) -> AppResult<PaginatedPosts> {
        self.repo
            .list_feed(viewer_id, kind, category, city, q, page, per_page)
            .await
            .map_err(AppError::Internal)
    }

    pub async fn get_detail(&self, id: &str, viewer_id: Option<&str>) -> AppResult<Post> {
        self.repo
            .get_by_id(id, viewer_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("Postingan tidak ditemukan".into()))
    }

    /// Toggle like SENDIRI (bukan target eksplisit dari klien — beda dengan
    /// `merchant_svc.set_follow` yang menerima `follow: bool`). Return
    /// `(liked_baru, like_count_terbaru)`.
    pub async fn toggle_like(&self, post_id: &str, user_id: &str) -> AppResult<(bool, i32)> {
        let liked = self
            .repo
            .toggle_like(post_id, user_id)
            .await
            .map_err(AppError::Internal)?;

        // Trigger DB sudah menaikkan/menurunkan like_count; ambil ulang baris
        // supaya angka yang dibalas ke klien akurat (bukan dihitung manual di
        // sisi Rust, yang bisa melenceng dari DB).
        let post = self
            .repo
            .get_by_id(post_id, None)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("Postingan tidak ditemukan".into()))?;

        // Notifikasi best-effort, HANYA saat like (bukan unlike), dan tak
        // pernah ke diri sendiri.
        if liked && post.user_id != user_id {
            let notif_store = self.notif_store.clone();
            let owner_id = post.user_id.clone();
            let post_id_owned = post.id.clone();
            let title = post.title.clone();
            tokio::spawn(async move {
                let input = CreateNotificationInput::post_like(
                    owner_id,
                    post_id_owned.clone(),
                    "Postingan Disukai",
                    format!("Seseorang menyukai postingan \"{title}\" kamu."),
                );
                if let Err(e) = notif_store.create(input).await {
                    tracing::error!(
                        post_id = %post_id_owned,
                        error = %e,
                        "Background notifikasi like gagal disimpan"
                    );
                }
            });
        }

        Ok((liked, post.like_count))
    }

    pub async fn add_comment(&self, post_id: &str, user_id: &str, body: &str) -> AppResult<PostComment> {
        let body_trim = body.trim();
        if body_trim.is_empty() || body_trim.chars().count() > MAX_COMMENT_LEN {
            return Err(AppError::BadRequest(format!(
                "Komentar wajib diisi, maksimal {MAX_COMMENT_LEN} karakter"
            )));
        }

        let post = self
            .repo
            .get_by_id(post_id, None)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("Postingan tidak ditemukan".into()))?;

        let comment = self
            .repo
            .add_comment(post_id, user_id, body_trim)
            .await
            .map_err(AppError::Internal)?;

        if post.user_id != user_id {
            let notif_store = self.notif_store.clone();
            let owner_id = post.user_id.clone();
            let post_id_owned = post.id.clone();
            let title = post.title.clone();
            let commenter = comment.user_name.clone();
            tokio::spawn(async move {
                let input = CreateNotificationInput::post_comment(
                    owner_id,
                    post_id_owned.clone(),
                    "Komentar Baru",
                    format!("{commenter} mengomentari postingan \"{title}\" kamu."),
                );
                if let Err(e) = notif_store.create(input).await {
                    tracing::error!(
                        post_id = %post_id_owned,
                        error = %e,
                        "Background notifikasi komentar gagal disimpan"
                    );
                }
            });
        }

        Ok(comment)
    }

    pub async fn list_comments(&self, post_id: &str, page: i64, per_page: i64) -> AppResult<PaginatedComments> {
        self.repo
            .list_comments(post_id, page, per_page)
            .await
            .map_err(AppError::Internal)
    }

    pub async fn delete_comment(&self, comment_id: &str, user_id: &str) -> AppResult<()> {
        let ok = self
            .repo
            .soft_delete_comment(comment_id, user_id)
            .await
            .map_err(AppError::Internal)?;
        if !ok {
            return Err(AppError::Forbidden(
                "Komentar bukan milikmu atau sudah dihapus".into(),
            ));
        }
        Ok(())
    }

    pub async fn update_status(&self, post_id: &str, user_id: &str, status: &str) -> AppResult<()> {
        if !ALLOWED_STATUS.contains(&status) {
            return Err(AppError::BadRequest(format!(
                "status harus salah satu dari {ALLOWED_STATUS:?}"
            )));
        }
        let ok = self
            .repo
            .update_status(post_id, user_id, status)
            .await
            .map_err(AppError::Internal)?;
        if !ok {
            return Err(AppError::Forbidden(
                "Postingan bukan milikmu atau sudah dihapus".into(),
            ));
        }
        Ok(())
    }

    pub async fn delete(&self, post_id: &str, user_id: &str) -> AppResult<()> {
        let ok = self
            .repo
            .soft_delete_post(post_id, user_id)
            .await
            .map_err(AppError::Internal)?;
        if !ok {
            return Err(AppError::Forbidden(
                "Postingan bukan milikmu atau sudah dihapus".into(),
            ));
        }
        Ok(())
    }
}
