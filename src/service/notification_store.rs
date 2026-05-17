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
