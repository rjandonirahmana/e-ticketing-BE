use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ── Domain model ──────────────────────────────────────────────────────────────

/// Row dari tabel `public.banners`.
/// `id` adalah `bigserial` (i64), bukan ULID.
/// `event_id` adalah `bytea` ULID — FK ke tabel `products`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Banner {
    pub id: i64, // bigserial
    pub image_url: String,
    pub click_url: Option<String>,
    pub start_date: DateTime<Utc>,
    pub end_date: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub event_id: Option<String>, // ULID string dari bytea FK products
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── Query params ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListBannersQuery {
    /// Filter berdasarkan product (ULID string).
    pub event_id: Option<String>,
}

// ── Admin request types ───────────────────────────────────────────────────────

/// Payload untuk membuat banner baru (admin).
/// `image_url` opsional di JSON body — bisa diisi lewat upload multipart.
/// Salah satu dari keduanya wajib ada; validasi dilakukan di route handler.
#[derive(Debug, Deserialize, Validate)]
pub struct CreateBannerRequest {
    /// URL gambar. Kosong ("") jika image dikirim via field multipart "image".
    #[serde(default)]
    pub image_url: String,

    /// URL tujuan klik banner (opsional).
    pub click_url: Option<String>,

    /// Tanggal mulai tayang banner (wajib).
    pub start_date: DateTime<Utc>,

    /// Tanggal berakhir tayang (opsional — None = tayang selamanya).
    pub end_date: Option<DateTime<Utc>>,

    /// ULID product yang dikaitkan (opsional).
    pub event_id: Option<String>,
}

/// Payload untuk update banner (admin).
/// Semua field opsional — hanya field yang disertakan yang di-update.
/// `image_url` di-override jika file baru dikirim via multipart.
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateBannerRequest {
    /// URL gambar baru (opsional — tetap URL lama jika tidak diisi dan tidak ada upload).
    pub image_url: Option<String>,

    /// URL tujuan klik baru.
    pub click_url: Option<String>,

    /// Tanggal mulai tayang baru.
    pub start_date: Option<DateTime<Utc>>,

    /// Tanggal berakhir baru.
    pub end_date: Option<DateTime<Utc>>,

    /// ULID product baru (Some("...") = ganti, None = biarkan tanpa perubahan).
    pub event_id: Option<String>,
}
