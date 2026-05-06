//! service/storage.rs — RustFS S3-compatible image upload service.
//!
//! Upload flow:
//!   1. FE kirim multipart POST /api/upload/image
//!   2. Backend validasi (size, mime type)
//!   3. Upload ke RustFS bucket via S3 API (aws_sdk_s3, path-style)
//!   4. Return public URL → FE simpan ke cover_url field
//!
//! RustFS adalah S3-compatible storage, jadi aws_sdk_s3 tetap dipakai.
//! Bedanya dari Garage: endpoint, env vars, dan region string.

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

/// Max ukuran file upload — 5 MB.
const MAX_SIZE: usize = 5 * 1024 * 1024;

/// MIME type yang diizinkan.
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

        let config_builder = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .endpoint_url(&cfg.endpoint)
            .force_path_style(true);

        let config_with_creds = config_builder.credentials_provider(creds).build();
        let client = Client::from_conf(config_with_creds);

        Self {
            client,
            bucket: cfg.bucket.clone(),
            public_url: cfg.public_url.trim_end_matches('/').to_string(),
        }
    }

    /// Initialize storage - ensure bucket exists
    pub async fn init(&self) -> AppResult<()> {
        self.ensure_bucket_exists().await
    }

    /// Create bucket if it doesn't exist
    async fn ensure_bucket_exists(&self) -> AppResult<()> {
        // Method 1: List all buckets first
        let buckets = self.client.list_buckets().send().await;

        match buckets {
            Ok(list) => {
                let bucket_exists = list
                    .buckets()
                    .iter()
                    .any(|b| b.name() == Some(&self.bucket));

                if bucket_exists {
                    tracing::info!("✅ Bucket '{}' already exists", self.bucket);
                    return Ok(());
                }

                // Bucket doesn't exist, create it
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
                // Fallback: assume bucket exists
                tracing::warn!("Assuming bucket '{}' exists", self.bucket);
                Ok(())
            }
        }
    }

    /// Make bucket publicly readable
    async fn set_bucket_public_read(&self) -> AppResult<()> {
        use aws_sdk_s3::types::{BucketCannedAcl, Grant, Grantee, Permission, Type};

        // Set ACL to public-read
        self.client
            .put_bucket_acl()
            .bucket(&self.bucket)
            .acl(BucketCannedAcl::PublicRead)
            .send()
            .await
            .map_err(|e| {
                tracing::warn!("Failed to set public ACL: {}", e);
                // Non-fatal error
            })
            .ok();

        Ok(())
    }

    pub async fn check_health(&self) {
        match self.client.head_bucket().bucket(&self.bucket).send().await {
            Ok(_) => tracing::info!("✅ RustFS connected. Bucket '{}' accessible.", self.bucket),
            Err(e) => {
                let service_err = e.into_service_error();
                tracing::error!(
                    "❌ RustFS health check gagal: {}",
                    service_err.code().unwrap_or("Unknown error")
                );
            }
        }
    }
    /// Upload bytes ke RustFS, return public URL.
    ///
    /// Public URL format: `{public_url}/{folderName}/{key}`
    /// e.g. `https://image.ulalaapi.store/events/550e8400-e29b-41d4-a716.jpg`
    pub async fn upload_image(
        &self,
        data: Bytes,
        folder_name: &str, // Rubah ke snake_case sesuai Rust convention
        content_type: &str,
    ) -> AppResult<String> {
        // Validasi size
        if data.len() > MAX_SIZE {
            return Err(AppError::BadRequest(format!(
                "File terlalu besar, max {}MB",
                MAX_SIZE / 1024 / 1024
            )));
        }

        // Validasi mime
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

        // Generate UUID filename
        let filename = format!("{}.{}", Uuid::new_v4(), ext);

        // Build key dengan folder: "folder_name/filename"
        let key = if folder_name.is_empty() {
            filename
        } else {
            // Normalize folder name: remove leading/trailing slashes
            let normalized_folder = folder_name.trim_start_matches('/').trim_end_matches('/');
            format!("{}/{}", normalized_folder, filename)
        };

        tracing::debug!("Uploading to S3: bucket={}, key={}", self.bucket, key);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .content_type(content_type)
            .acl(aws_sdk_s3::types::ObjectCannedAcl::PublicRead)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("RustFS upload gagal: {e}")))?;

        // Return public URL dengan folder
        Ok(format!("{}/{}/{}", self.public_url, self.bucket, key))
    }
}
