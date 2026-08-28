use std::sync::Arc;

use crate::models::notification::{CreateNotificationInput, Notification};
use crate::repository::notification::NotificationRepository;
use crate::utils::error::AppError;

pub struct NotificationStoreService {
    repo: Arc<dyn NotificationRepository>,
}

impl NotificationStoreService {
    pub fn new(repo: Arc<dyn NotificationRepository>) -> Self {
        Self { repo }
    }

    /// List notifikasi milik user, dengan pagination.
    pub async fn list(
        &self,
        user_id: &str,
        page: i64,
        per_page: i64,
    ) -> Result<Vec<Notification>, AppError> {
        let limit = per_page.clamp(1, 100);
        let offset = (page - 1).max(0) * limit;
        self.repo
            .list_for_user(user_id, limit, offset)
            .await
            .map_err(AppError::Internal)
    }

    /// Satu notifikasi milik user, beserta data target-nya (join sesuai `kind`).
    ///
    /// Halaman detail dulu memanggil `list(user, 1, 1000)` lalu mencari id-nya
    /// di dalam hasil. Itu keliru dua kali. Yang pertama halus: `list` menahan
    /// `per_page` pada 100 (lihat di atas), jadi permintaan 1000 diam-diam
    /// dilayani 100 — notifikasi ke-101 dan seterusnya TIDAK PERNAH ada di
    /// dalam hasil, dan halaman detailnya menjawab "tidak ditemukan" untuk
    /// baris yang jelas-jelas masih ada di basis data.
    ///
    /// Yang kedua lebih merugikan: `mark_read` dipanggil LEBIH DULU dan tetap
    /// berhasil. Jadi notifikasi itu berpindah ke status sudah-dibaca — hilang
    /// dari hitungan lonceng — tanpa isinya pernah bisa dibuka satu kali pun.
    ///
    /// Query yang benar sudah lama ada di repository dan ikut mengambil data
    /// target-nya dalam satu perjalanan; ia hanya tak pernah dipanggil.
    pub async fn detail(&self, id: &str, user_id: &str) -> Result<Notification, AppError> {
        self.repo
            .find_detail(id, user_id)
            .await
            .map_err(AppError::Internal)
    }

    /// Buat notifikasi baru. Dipanggil dari service lain (OrderService, dll).
    pub async fn create(
        &self,
        input: CreateNotificationInput,
    ) -> Result<Notification, AppError> {
        self.repo.create(input).await.map_err(AppError::Internal)
    }

    /// Tandai satu notifikasi sebagai sudah dibaca.
    pub async fn mark_read(&self, id: &str, user_id: &str) -> Result<(), AppError> {
        self.repo
            .mark_read(id, user_id)
            .await
            .map_err(AppError::Internal)
    }

    /// Tandai semua notifikasi user sebagai sudah dibaca.
    pub async fn mark_all_read(&self, user_id: &str) -> Result<(), AppError> {
        self.repo
            .mark_all_read(user_id)
            .await
            .map_err(AppError::Internal)
    }

    /// Jumlah notifikasi belum dibaca — untuk badge di UI.
    pub async fn unread_count(&self, user_id: &str) -> Result<i64, AppError> {
        self.repo
            .unread_count(user_id)
            .await
            .map_err(AppError::Internal)
    }
}
