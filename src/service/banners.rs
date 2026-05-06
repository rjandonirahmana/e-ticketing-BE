//! service/banners.rs
//!
//! `BannerService<R>` — generic atas `BannerRepository`.
//! Monomorphisasi terjadi saat kompilasi → zero virtual-dispatch,
//! seluruh call path di-inline, binary lebih lean.

use std::sync::Arc;

use chrono::Utc;

use crate::{
    models::banners::{Banner, CreateBannerRequest, UpdateBannerRequest},
    repository::banner::BannerRepository,
    utils::error::{AppError, AppResult},
};

// ── Service ───────────────────────────────────────────────────────────────────

pub struct BannerService<R: BannerRepository> {
    repo: Arc<R>,
}

impl<R: BannerRepository> BannerService<R> {
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    // ── Public endpoints ──────────────────────────────────────────────────────

    /// Daftar banner yang sedang aktif saat ini.
    pub async fn list_active(&self, event_id: Option<&str>) -> AppResult<Vec<Banner>> {
        let now = Utc::now();
        Ok(self.repo.list_active(now, event_id).await?)
    }

    /// Detail satu banner by id (termasuk yang soft-deleted untuk admin).
    pub async fn get_by_id(&self, id: i64) -> AppResult<Option<Banner>> {
        Ok(self.repo.find_by_id(id).await?)
    }

    // ── Admin endpoints ───────────────────────────────────────────────────────

    /// Buat banner baru.
    /// `image_url` sudah final — route handler yang menentukan
    /// apakah dari upload file atau dari JSON body.
    pub async fn create(&self, image_url: String, req: CreateBannerRequest) -> AppResult<Banner> {
        Ok(self.repo.create(&image_url, &req).await?)
    }

    /// Update banner. `image_url` override dari upload baru (None = jangan ganti URL lewat ini).
    /// Mengembalikan `NotFound` jika id tidak ada atau sudah di-soft-delete.
    pub async fn update(
        &self,
        id: i64,
        image_url: Option<String>,
        req: UpdateBannerRequest,
    ) -> AppResult<Banner> {
        self.repo
            .update(id, image_url.as_deref(), &req)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Banner id={id} tidak ditemukan")))
    }

    /// Soft-delete banner. Mengembalikan `NotFound` jika tidak ada atau sudah di-delete.
    pub async fn soft_delete(&self, id: i64) -> AppResult<()> {
        let deleted = self.repo.soft_delete(id).await?;
        if !deleted {
            return Err(AppError::NotFound(format!(
                "Banner id={id} tidak ditemukan atau sudah dihapus"
            )));
        }
        Ok(())
    }
}
