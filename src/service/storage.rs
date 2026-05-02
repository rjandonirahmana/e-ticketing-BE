//! service/storage.rs — Garage S3 image upload service.
//!
//! Upload flow:
//!   1. FE kirim multipart POST /api/upload/image
//!   2. Backend validasi (size, mime type)
//!   3. Upload ke Garage bucket via S3 API
//!   4. Return public URL → FE simpan ke cover_url field

use std::sync::Arc;

use aws_sdk_s3::{
    Client,
    config::{BehaviorVersion, Builder, Credentials, Region},
    primitives::ByteStream,
};
use bytes::Bytes;
use uuid::Uuid;

use crate::{
    config::config::GarageConfig,
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
    public_url: String,
}

impl StorageService {
    pub fn new(cfg: &GarageConfig) -> Self {
        let creds = Credentials::new(&cfg.access_key, &cfg.secret_key, None, None, "garage");

        let config = Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("garage"))
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

    pub async fn check_health(&self) {
        match self.client.list_buckets().send().await {
            Ok(r) => {
                for b in r.buckets() {
                    println!("  bucket: {}", b.name().unwrap_or("?"));
                }
            }
            Err(e) => {
                panic!("  ERROR: {e}");
                println!("  ERROR: {e}");
            }
        }
    }

    /// Upload bytes ke Garage, return public URL.
    pub async fn upload_image(&self, data: Bytes, content_type: &str) -> AppResult<String> {
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

        // Generate key unik — pakai UUID agar tidak bisa ditebak
        let ext = match content_type {
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/webp" => "webp",
            "image/gif" => "gif",
            _ => "bin",
        };
        let key = format!("{}.{}", Uuid::new_v4(), ext);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .content_type(content_type)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("S3 upload failed: {e}")))?;

        Ok(format!("{}/{}", self.public_url, key))
    }
}
