//! Model untuk foto detail event (denah, seat map, info harga, dll).
//! Disimpan sebagai JSON array di field `gallery` BE, atau di-embed
//! ke description sementara kalau BE belum punya kolom gallery.

use serde::{Deserialize, Serialize};

/// Tipe foto detail — dipakai untuk label & warna di UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailImageType {
    Map,   // denah lokasi / venue
    Seat,  // peta tempat duduk
    Price, // info harga / tier visual
    Other, // lainnya
}

impl DetailImageType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Map => "Denah Lokasi",
            Self::Seat => "Peta Kursi",
            Self::Price => "Info Harga",
            Self::Other => "Lainnya",
        }
    }

    /// Nilai string untuk serialisasi ke BE.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Map => "map",
            Self::Seat => "seat",
            Self::Price => "price",
            Self::Other => "other",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "map" => Self::Map,
            "seat" => Self::Seat,
            "price" => Self::Price,
            _ => Self::Other,
        }
    }
}

/// Satu foto detail event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailImage {
    /// URL gambar yang sudah di-upload ke storage.
    pub url: String,
    /// Tipe foto untuk label & warna di UI.
    pub image_type: String,
    /// Keterangan singkat yang ditampilkan di bawah foto.
    pub caption: String,
}

/// State lokal per-foto selama proses input di form.
/// Berbeda dari `DetailImage` karena masih mungkin belum punya URL
/// (belum di-upload) — hanya ada preview blob URL sementara.
#[derive(Clone)]
pub struct DetailImageDraft {
    /// Blob URL sementara untuk preview (dari createObjectURL).
    /// Setelah upload sukses, diganti dengan URL permanen.
    pub preview_url: String,
    /// URL permanen setelah upload selesai. None = belum di-upload.
    pub uploaded_url: Option<String>,
    /// File asli — dipakai saat upload.
    pub file: Option<web_sys::File>,
    pub image_type: String,
    pub caption: String,
}

impl DetailImageDraft {
    pub fn new(file: web_sys::File, preview_url: String) -> Self {
        Self {
            preview_url,
            uploaded_url: None,
            file: Some(file),
            image_type: "map".to_string(),
            caption: String::new(),
        }
    }

    /// Buat draft dari data yang sudah ada (saat edit event).
    pub fn from_existing(img: &DetailImage) -> Self {
        Self {
            preview_url: img.url.clone(),
            uploaded_url: Some(img.url.clone()),
            file: None,
            image_type: img.image_type.clone(),
            caption: img.caption.clone(),
        }
    }

    /// Konversi ke `DetailImage` untuk dikirim ke BE.
    /// Mengembalikan None jika upload belum selesai.
    pub fn to_detail_image(&self) -> Option<DetailImage> {
        let url = self.uploaded_url.clone()?;
        Some(DetailImage {
            url,
            image_type: self.image_type.clone(),
            caption: self.caption.clone(),
        })
    }

    pub fn is_uploaded(&self) -> bool {
        self.uploaded_url.is_some()
    }
}
