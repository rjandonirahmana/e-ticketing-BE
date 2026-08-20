//! web/services/event.rs — Tipe payload event (pengganti `csr::services::event`).
//!
//! Tipe-tipe ini dipakai `web::components::detail_image_section` untuk
//! mengumpulkan foto detail event sebelum dikirim ke server function
//! create/update event. Tidak ada network call di sini.

use serde::{Deserialize, Serialize};

/// Metadata satu foto detail event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetailImageMeta {
    /// Jenis foto, mis. "venue", "lineup", "stage", dll. (bebas, sesuai UI).
    pub image_type: String,
    /// Caption opsional.
    pub caption: String,
}

/// Foto detail yang SUDAH ada di backend (punya URL) — dikirim ulang via JSON
/// agar dipertahankan saat update (tanpa re-upload).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetailImagePayload {
    pub url: String,
    pub image_type: String,
    pub caption: String,
    /// Titik fokus `object-position` — dikirim balik ke server saat simpan.
    /// `serde(default)` supaya payload lama (tanpa field ini) tetap terbaca.
    #[serde(default)]
    pub focus: String,
}

/// Foto detail BARU yang perlu di-upload (bytes mentah dari `File`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DetailImageUploadItem {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub meta: DetailImageMeta,
}
