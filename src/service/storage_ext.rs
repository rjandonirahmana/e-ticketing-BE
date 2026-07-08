//! service/storage_ext.rs
//!
//! Ekstensi StorageService untuk upload media story (gambar/video) via STREAMING
//! dari file temp di disk — `upload_media_file`. Ukuran & magic bytes divalidasi
//! caller (handler stream + StoryService), jadi di sini fokus streaming ke RustFS.

use std::path::Path;

use uuid::Uuid;

use crate::{
    service::storage::StorageService,
    utils::error::{AppError, AppResult},
};

impl StorageService {
    /// Upload media dari FILE di disk (streaming) ke RustFS.
    ///
    /// Berbeda dengan `upload_media` yang menerima `Bytes` (file penuh di RAM),
    /// method ini memakai `ByteStream::from_path` — AWS SDK membaca file secara
    /// bertahap dari disk dan mengeset `Content-Length` otomatis dari metadata,
    /// jadi tak pernah memuat seluruh file ke RAM. Ukuran & magic bytes sudah
    /// divalidasi caller (handler stream + StoryService) sebelum sampai sini.
    pub async fn upload_media_file(
        &self,
        path: &Path,
        folder_name: &str,
        content_type: &str,
    ) -> AppResult<String> {
        let ext = mime_to_ext(content_type);
        let filename = format!("{}.{}", Uuid::new_v4(), ext);
        let key = if folder_name.is_empty() {
            filename
        } else {
            let folder = folder_name.trim_matches('/');
            format!("{}/{}", folder, filename)
        };

        tracing::debug!(
            "Streaming media file: bucket={}, key={}, type={}",
            self.bucket,
            key,
            content_type
        );

        let body = aws_sdk_s3::primitives::ByteStream::from_path(path)
            .await
            .map_err(|e| {
                AppError::Internal(anyhow::anyhow!("Gagal membuka file temp upload: {e}"))
            })?;

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .content_type(content_type)
            .body(body)
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("RustFS upload gagal: {e}")))?;

        Ok(format!("{}/{}/{}", self.public_url, self.bucket, key))
    }
}

fn mime_to_ext(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png"  => "png",
        "image/webp" => "webp",
        "image/gif"  => "gif",
        "video/mp4"  => "mp4",
        "video/webm" => "webm",
        "video/quicktime" => "mov",
        _ => "bin",
    }
}
