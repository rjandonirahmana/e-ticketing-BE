//! service/storage_ext.rs
//!
//! Ekstensi StorageService untuk mendukung upload video (stories).
//! Tambahkan method `upload_media` ke StorageService yang sudah ada.
//!
//! ── Cara integrasi ────────────────────────────────────────────────────────────
//! Tambahkan impl block ini ke file `service/storage.rs` yang sudah ada,
//! atau jadikan module terpisah lalu re-export dari storage.rs.

use bytes::Bytes;
use uuid::Uuid;

use crate::{
    service::storage::StorageService,
    utils::error::{AppError, AppResult},
};

/// Max ukuran video: 50 MB
const MAX_VIDEO_SIZE: usize = 50 * 1024 * 1024;
/// Max ukuran gambar: 5 MB (sama dengan upload_image)
const MAX_IMAGE_SIZE: usize = 5 * 1024 * 1024;

impl StorageService {
    /// Upload media (gambar ATAU video) ke RustFS.
    ///
    /// Berbeda dengan `upload_image` yang hanya gambar, method ini juga
    /// menerima video (mp4/webm/mov). Tidak ada re-validasi magic bytes di sini
    /// karena service pemanggil (StoryService) sudah melakukan deteksi sebelumnya.
    pub async fn upload_media(
        &self,
        data: Bytes,
        folder_name: &str,
        content_type: &str,
    ) -> AppResult<String> {
        // Cek ukuran berdasarkan tipe
        let max = if content_type.starts_with("video/") {
            MAX_VIDEO_SIZE
        } else {
            MAX_IMAGE_SIZE
        };

        if data.len() > max {
            return Err(AppError::BadRequest(format!(
                "File terlalu besar, maksimal {}MB",
                max / 1024 / 1024
            )));
        }

        let ext = mime_to_ext(content_type);
        let filename = format!("{}.{}", Uuid::new_v4(), ext);
        let key = if folder_name.is_empty() {
            filename
        } else {
            let folder = folder_name.trim_matches('/');
            format!("{}/{}", folder, filename)
        };

        tracing::debug!("Uploading media: bucket={}, key={}, type={}", self.bucket, key, content_type);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .content_type(content_type)
            .body(aws_sdk_s3::primitives::ByteStream::from(data))
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
