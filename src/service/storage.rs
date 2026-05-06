//! service/storage.rs — RustFS S3-compatible image upload service.

use std::sync::Arc;

use aws_sdk_s3::{
    Client,
    config::{BehaviorVersion, Builder, Credentials, Region},
    error::ProvideErrorMetadata,
    primitives::ByteStream,
};
use bytes::Bytes;
use uuid::Uuid;

use crate::{
    config::config::RustFsConfig,
    utils::error::{AppError, AppResult},
};

const MAX_SIZE: usize = 5 * 1024 * 1024;
const ALLOWED_MIME: &[&str] = &["image/jpeg", "image/png", "image/webp", "image/gif"];

#[derive(Clone)]
pub struct StorageService {
    client: Client,
    bucket: String,
    /// Base public URL tanpa trailing slash, e.g. "https://image.ulalaapi.store"
    public_url: String,
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

    pub async fn check_health(&self) {
        match self.client.head_bucket().bucket(&self.bucket).send().await {
            Ok(_) => tracing::info!("✅ RustFS connected. Bucket '{}' accessible.", self.bucket),
            Err(e) => {
                let svc_err = e.into_service_error();
                tracing::error!(
                    "❌ RustFS health check gagal: {}",
                    svc_err.code().unwrap_or("Unknown error")
                );
            }
        }
    }

    /// Upload bytes ke RustFS, return public URL.
    ///
    /// Public URL: `{public_url}/{bucket}/{folder}/{uuid}.{ext}`
    /// e.g. `https://image.ulalaapi.store/image/events/550e.jpg`
    ///
    /// FIX: removed unused `set_bucket_public_read` method and
    /// removed `.acl(PublicRead)` which RustFS silently ignores or rejects.
    /// Use `mc anonymous set public local/image` on the server instead.
    pub async fn upload_image(
        &self,
        data: Bytes,
        folder_name: &str,
        content_type: &str,
    ) -> AppResult<String> {
        if data.len() > MAX_SIZE {
            return Err(AppError::BadRequest(format!(
                "File terlalu besar, max {}MB",
                MAX_SIZE / 1024 / 1024
            )));
        }

        if !ALLOWED_MIME.contains(&content_type) {
            return Err(AppError::BadRequest(format!(
                "Format tidak didukung: {content_type}"
            )));
        }

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
}
