//! upload.rs — Axum multipart handler for story media upload.
//!
//! POST /upload/story   — multipart/form-data
//!   fields: file (required), slug (optional event slug), title (optional event title)
//!
//! Auth: HttpOnly cookie `pulse_token`.
//!
//! Media di-STREAM ke file temp di disk (chunk demi chunk), bukan dibaca penuh
//! ke RAM. Dengan begitu N upload paralel hanya menahan ~satu chunk kecil di RAM
//! masing-masing (bukan N × ukuran-file) — aman dari OOM di VPS kecil, dan batas
//! konkurensinya (lihat `capacity::recommended_upload_concurrency`) bisa jauh
//! lebih longgar karena kini dibatasi I/O disk, bukan memori.

#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Extension, Multipart},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;

use crate::state::AppState;

/// Batas ukuran media story (samakan dengan `MAX_FILE_SIZE` di StoryService).
const MAX_MEDIA_BYTES: usize = 50 * 1024 * 1024;
/// Byte awal yang ditangkap untuk deteksi magic bytes (cukup untuk semua format).
const HEADER_LEN: usize = 16;

/// RAII: hapus file temp saat handler selesai — sukses, error, maupun panic.
struct TempFileGuard(PathBuf);
impl Drop for TempFileGuard {
    fn drop(&mut self) {
        // Best-effort; NotFound (file belum sempat dibuat) diabaikan.
        let _ = std::fs::remove_file(&self.0);
    }
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(json!({ "error": msg.into() }))).into_response()
}

fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get("cookie")?.to_str().ok()?;
    raw.split(';').map(str::trim).find_map(|p| {
        p.strip_prefix(&format!("{name}="))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from)
    })
}

pub async fn story_upload(
    Extension(state): Extension<Arc<AppState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<Value>, Response> {
    let token = cookie_value(&headers, "pulse_token")
        .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "Tidak terautentikasi"))?;

    let claims = state
        .jwt
        .verify(&token)
        .map_err(|e| err(StatusCode::UNAUTHORIZED, e.to_string()))?;

    // Gate konkurensi: batasi jumlah upload serentak (plafon auto-skala dari
    // kapasitas VPS). Penuh → 503 fail-fast agar klien retry, bukan antre yang
    // menahan file descriptor & I/O. Permit dipegang sampai handler selesai.
    let _permit = state
        .upload_limit
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            err(
                StatusCode::SERVICE_UNAVAILABLE,
                "Server sedang sibuk memproses upload, coba lagi sebentar",
            )
        })?;

    // File temp + guard dibuat SEBELUM loop agar path apa pun yang tercipta
    // dijamin dibersihkan saat fungsi keluar (termasuk jalur error). Direktori
    // disk-backed sudah divalidasi saat startup (lihat prepare_upload_tmp_dir).
    let tmp_path = state
        .upload_tmp_dir
        .join(format!("story-upload-{}.tmp", uuid::Uuid::new_v4()));
    let _tmp_guard = TempFileGuard(tmp_path.clone());

    let mut size: usize = 0;
    let mut header: Vec<u8> = Vec::with_capacity(HEADER_LEN);
    let mut has_file = false;
    let mut slug: Option<String> = None;
    let mut title: Option<String> = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| err(StatusCode::BAD_REQUEST, format!("Multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" | "media" => {
                let mut file = tokio::fs::File::create(&tmp_path)
                    .await
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("Temp file: {e}")))?;

                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|e| err(StatusCode::BAD_REQUEST, format!("Read error: {e}")))?
                {
                    size += chunk.len();
                    if size > MAX_MEDIA_BYTES {
                        return Err(err(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            format!("File terlalu besar, maksimal {}MB", MAX_MEDIA_BYTES / 1024 / 1024),
                        ));
                    }
                    // Tangkap byte awal untuk magic-detection tanpa membaca ulang file.
                    if header.len() < HEADER_LEN {
                        let need = HEADER_LEN - header.len();
                        header.extend_from_slice(&chunk[..need.min(chunk.len())]);
                    }
                    file.write_all(&chunk)
                        .await
                        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("Write error: {e}")))?;
                }
                file.flush()
                    .await
                    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("Flush error: {e}")))?;
                has_file = true;
            }
            "slug" => {
                let text = field.text().await.unwrap_or_default();
                if !text.is_empty() {
                    slug = Some(text);
                }
            }
            "title" => {
                let text = field.text().await.unwrap_or_default();
                if !text.is_empty() {
                    title = Some(text);
                }
            }
            _ => {}
        }
    }

    if !has_file || size == 0 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "Field 'file' tidak ada dalam request",
        ));
    }

    let res = state
        .story_svc
        .upload_streamed(&claims.user_id, &tmp_path, size, &header, slug, title)
        .await
        .map_err(IntoResponse::into_response)?;

    Ok(Json(json!({
        "story_id": res.story_id,
        "media_url": res.media_url,
    })))
}
