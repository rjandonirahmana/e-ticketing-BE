//! service/storage.rs — RustFS S3-compatible image upload service.

use aws_sdk_s3::{
    config::{BehaviorVersion, Builder, Credentials, Region},
    error::ProvideErrorMetadata,
    primitives::ByteStream,
    Client,
};
use bytes::Bytes;
use uuid::Uuid;

use crate::{
    config::config::RustFsConfig,
    utils::error::{AppError, AppResult},
};

const MAX_SIZE: usize = 5 * 1024 * 1024;
const ALLOWED_MIME: &[&str] = &["image/jpeg", "image/png", "image/webp", "image/gif"];

/// FIX: Validasi magic bytes untuk mencegah spoofing Content-Type dari client.
/// Client bisa kirim header "image/jpeg" tapi body-nya executable/script.
/// Fungsi ini inspeksi bytes pertama untuk memverifikasi format sebenarnya.
fn detect_image_mime(data: &[u8]) -> Option<&'static str> {
    match data {
        // JPEG: FF D8 FF
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        // PNG: 89 50 4E 47 0D 0A 1A 0A
        [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, ..] => Some("image/png"),
        // GIF: GIF87a atau GIF89a
        [0x47, 0x49, 0x46, 0x38, 0x37, 0x61, ..] => Some("image/gif"),
        [0x47, 0x49, 0x46, 0x38, 0x39, 0x61, ..] => Some("image/gif"),
        // WebP: RIFF....WEBP
        [0x52, 0x49, 0x46, 0x46, _, _, _, _, 0x57, 0x45, 0x42, 0x50, ..] => Some("image/webp"),
        _ => None,
    }
}

#[derive(Clone)]
pub struct StorageService {
    pub client: Client,
    pub bucket: String,
    /// Base public URL tanpa trailing slash, e.g. "https://image.ulalaapi.store"
    pub public_url: String,
}

impl StorageService {
    pub fn new(cfg: &RustFsConfig) -> Self {
        let creds = Credentials::new(&cfg.access_key, &cfg.secret_key, None, None, "rustfs");

        let config = Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .endpoint_url(&cfg.endpoint)
            .credentials_provider(creds)
            .force_path_style(true)
            .build();

        Self {
            client: Client::from_conf(config),
            bucket: cfg.bucket.clone(),
            public_url: cfg.public_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn init(&self) -> AppResult<()> {
        self.ensure_bucket_exists().await
    }

    async fn ensure_bucket_exists(&self) -> AppResult<()> {
        match self.client.list_buckets().send().await {
            Ok(list) => {
                let exists = list
                    .buckets()
                    .iter()
                    .any(|b| b.name() == Some(&self.bucket));
                if exists {
                    tracing::info!("✅ Bucket '{}' already exists", self.bucket);
                    return Ok(());
                }
                tracing::info!("Creating bucket '{}'...", self.bucket);
                self.client
                    .create_bucket()
                    .bucket(&self.bucket)
                    .send()
                    .await
                    .map_err(|e| {
                        AppError::Internal(anyhow::anyhow!("Failed to create bucket: {}", e))
                    })?;
                tracing::info!("✅ Bucket '{}' created", self.bucket);
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to list buckets: {}", e);
                tracing::warn!("Assuming bucket '{}' exists", self.bucket);
                Ok(())
            }
        }
    }

    pub async fn check_health(&self) -> AppResult<()> {
        // FIX: return AppResult agar caller bisa decide apakah fatal atau tidak.
        // Sebelumnya void — storage down saat startup tidak terdeteksi sampai upload pertama gagal.
        self.client
            .head_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| {
                let svc_err = e.into_service_error();
                AppError::Internal(anyhow::anyhow!(
                    "RustFS health check failed: {}",
                    svc_err.code().unwrap_or("unknown")
                ))
            })?;
        tracing::info!("✅ RustFS connected. Bucket '{}' accessible.", self.bucket);
        Ok(())
    }

    /// Upload bytes ke RustFS, return public URL.
    ///
    /// Public URL: `{public_url}/{bucket}/{folder}/{uuid}.{ext}`
    /// e.g. `https://image.ulalaapi.store/image/products/550e.jpg`
    ///
    /// FIX: removed unused `set_bucket_public_read` method and
    /// removed `.acl(PublicRead)` which RustFS silently ignores or rejects.
    /// Use `mc anonymous set public local/image` on the server instead.
    pub async fn upload_image(
        &self,
        data: Bytes,
        folder_name: &str,
        _content_type: &str,
    ) -> AppResult<String> {
        if data.len() > MAX_SIZE {
            return Err(AppError::BadRequest(format!(
                "File terlalu besar, max {}MB",
                MAX_SIZE / 1024 / 1024
            )));
        }

        // FIX: Validasi magic bytes — jangan percaya Content-Type dari client.
        // Client bisa spoof header; inspeksi byte pertama memastikan format sebenarnya.
        let actual_mime = detect_image_mime(&data).ok_or_else(|| {
            AppError::BadRequest(
                "Format file tidak valid atau tidak didukung (harus JPEG/PNG/WebP/GIF)".into(),
            )
        })?;

        // Juga tetap check whitelist untuk defence in depth
        if !ALLOWED_MIME.contains(&actual_mime) {
            return Err(AppError::BadRequest(format!(
                "Format tidak didukung: {actual_mime}"
            )));
        }

        // Gunakan mime type dari magic bytes, bukan dari client — lebih aman
        let content_type = actual_mime;

        let ext = match content_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/webp" => "webp",
            "image/gif" => "gif",
            _ => "bin",
        };

        let filename = format!("{}.{}", Uuid::new_v4(), ext);
        let key = if folder_name.is_empty() {
            filename
        } else {
            let folder = folder_name.trim_matches('/');
            format!("{}/{}", folder, filename)
        };

        tracing::debug!("Uploading: bucket={}, key={}", self.bucket, key);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .content_type(content_type)
            // FIX: no ACL here — set bucket policy publicly via `mc anonymous set public`
            // Per-object ACL is unsupported/ignored by RustFS
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("RustFS upload gagal: {e}")))?;

        // FIX: include bucket name in URL path
        // Pingora strips /image prefix → RustFS receives /{key} directly
        // But URL seen by client includes bucket: /image/{key}
        Ok(format!("{}/{}/{}", self.public_url, self.bucket, key))
    }

    /// Hapus satu object dari bucket berdasarkan public URL hasil upload
    /// (`{public_url}/{bucket}/{key}`). URL yang bukan milik bucket ini
    /// (mis. asset eksternal) diabaikan tanpa error.
    pub async fn delete_by_url(&self, media_url: &str) -> AppResult<()> {
        let prefix = format!("{}/{}/", self.public_url, self.bucket);
        let Some(key) = media_url.strip_prefix(&prefix) else {
            tracing::debug!(
                "delete_by_url: '{}' bukan object bucket '{}', dilewati",
                media_url,
                self.bucket
            );
            return Ok(());
        };
        if key.is_empty() {
            return Ok(());
        }
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                AppError::Internal(anyhow::anyhow!("RustFS delete_object gagal: {e}"))
            })?;
        tracing::debug!("Deleted object: bucket={}, key={}", self.bucket, key);
        Ok(())
    }
}
