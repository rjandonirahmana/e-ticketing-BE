use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Status product yang sedang menunggu review admin.
///
/// Product baru dan setiap suntingan merchant masuk ke antrean ini
/// (`ProductService::list_cancelled_products` membacanya), dan hanya admin yang
/// memindahkannya ke `"active"`. Ditulis sekali di sini karena nilainya harus
/// sama persis di tiga tempat — INSERT, UPDATE, dan kueri antrean review;
/// satu huruf beda dan product menghilang dari antrean tanpa jejak galat.
pub const STATUS_MENUNGGU_REVIEW: &str = "edited";

/// Satu foto detail product (denah, seat map, info harga, dll.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailImageEntry {
    pub url: String,
    /// "map" | "seat" | "price" | "other"
    pub image_type: String,
    pub caption: String,
    /// Titik fokus untuk `object-position`, mis. `"50% 50%"`.
    ///
    /// Tak perlu migrasi: kolom `products.detail_images` sudah JSONB, dan
    /// `serde(default)` membuat seluruh entri LAMA (yang tak punya field ini)
    /// tetap terbaca — nilainya jatuh ke tengah, persis perilaku sebelum fitur
    /// ini ada. Jadi tak ada satu pun foto yang berubah tampilannya sampai
    /// pemiliknya sendiri menggeser titiknya.
    #[serde(default = "fokus_tengah")]
    pub focus: String,
}

/// Titik fokus bawaan — tengah, sama dengan perilaku `object-position` bawaan.
pub fn fokus_tengah() -> String {
    "50% 50%".to_string()
}

/// Bersihkan nilai titik fokus dari luar menjadi bentuk yang aman dipakai
/// langsung di CSS.
///
/// Nilai ini masuk ke atribut `style`, jadi ia TIDAK boleh dipercaya apa adanya:
/// tanpa penyaringan, sebuah "titik fokus" bisa membawa deklarasi CSS lain
/// sekaligus. Karena itu bentuknya tak diperbaiki-perbaiki — yang tak cocok
/// dibuang seluruhnya dan diganti nilai tengah.
///
/// Rentangnya dijepit 0–100: di luar itu foto justru tergeser keluar bingkai,
/// dan tak ada gunanya menyimpan angka yang hasilnya pasti salah.
pub fn normalisasi_fokus(raw: &str) -> String {
    let mut bagian = raw.trim().split_whitespace();
    let (Some(x), Some(y), None) = (bagian.next(), bagian.next(), bagian.next()) else {
        return fokus_tengah();
    };
    let angka = |s: &str| -> Option<u32> {
        s.strip_suffix('%')?
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0.0, 100.0).round() as u32)
    };
    match (angka(x), angka(y)) {
        (Some(x), Some(y)) => format!("{x}% {y}%"),
        _ => fokus_tengah(),
    }
}

#[cfg(test)]
mod tests_fokus {
    use super::{fokus_tengah, normalisasi_fokus};

    #[test]
    fn nilai_wajar_dipertahankan() {
        assert_eq!(normalisasi_fokus("30% 70%"), "30% 70%");
        assert_eq!(normalisasi_fokus("  0%   100%  "), "0% 100%");
    }

    /// Pecahan dibulatkan — editor seret menghasilkan angka seperti 33.7%.
    #[test]
    fn pecahan_dibulatkan() {
        assert_eq!(normalisasi_fokus("33.7% 66.2%"), "34% 66%");
    }

    /// Di luar bingkai dijepit, bukan ditolak: hasil seret yang meleset sedikit
    /// tak boleh membuat penyimpanan gagal.
    #[test]
    fn di_luar_rentang_dijepit() {
        assert_eq!(normalisasi_fokus("-20% 300%"), "0% 100%");
    }

    /// Apa pun yang bukan "X% Y%" jatuh ke tengah — termasuk percobaan
    /// menyelipkan deklarasi CSS lain lewat field ini.
    #[test]
    fn bentuk_asing_jadi_tengah() {
        for jahat in [
            "center",
            "50%",
            "50% 50% 50%",
            "50%;background:url(http://x)",
            "red",
            "",
        ] {
            assert_eq!(normalisasi_fokus(jahat), fokus_tengah(), "gagal untuk: {jahat}");
        }
    }
}

/// Metadata per file `detail_image` yang dikirim lewat multipart.
/// Dikirim sebagai field `detail_image_meta` (JSON array), dicocokkan by index
/// dengan urutan field `detail_image`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailImageMeta {
    /// "map" | "seat" | "price" | "other" — default "other"
    #[serde(default = "default_image_type")]
    pub image_type: String,
    /// Keterangan singkat gambar — default ""
    #[serde(default)]
    pub caption: String,
}

fn default_image_type() -> String {
    "other".to_string()
}

impl Default for DetailImageMeta {
    fn default() -> Self {
        Self {
            image_type: "other".to_string(),
            caption: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    /// Titik fokus cover untuk `object-position` (mis. "50% 50%"). Satu berkas
    /// asli dipakai di banyak rasio — kartu 1:1, hero lebar, thumbnail tiket —
    /// dan nilai ini yang menentukan bagian mana yang bertahan saat dipotong.
    #[serde(default = "fokus_tengah")]
    pub cover_focus: String,
    /// Array foto detail product (denah, seat map, dll.)
    #[serde(default)]
    pub detail_images: Vec<DetailImageEntry>,
    /// Harga base termurah dari variant aktif.
    pub price: f64,
    /// Harga sale termurah yang sedang aktif (None jika tidak ada sale).
    pub sale_price: Option<f64>,
    pub sale_price_start_date: Option<DateTime<Utc>>,
    pub sale_price_end_date: Option<DateTime<Utc>>,
    /// Harga efektif untuk ditampilkan di list:
    /// sale_price jika aktif, else price.
    pub display_price: f64,
    pub venue: Option<String>,
    #[serde(default)]
    pub category: Vec<String>,
    pub city: Option<String>,
    /// Koordinat lokasi venue (untuk peta). None bila merchant belum mengisi.
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub total_sold: i32,
    pub total_quota: i32,
    /// Nama toko penyelenggara (JOIN merchant_details.store_name). None hanya
    /// pada jalur tanpa join (INSERT RETURNING).
    pub merchant_name: Option<String>,
    /// Ringkasan profil penyelenggara untuk bottom sheet di product detail.
    /// HANYA terisi pada query detail (by slug/id dengan MERCHANT_INFO_COLS) —
    /// list & INSERT RETURNING tak membayar subquery agregatnya (None).
    #[serde(default)]
    pub merchant: Option<MerchantSummary>,
}

/// Ringkasan profil merchant yang di-embed di detail product: satu round-trip
/// (JOIN + subquery agregat) — sheet penyelenggara tak perlu fetch kedua.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantSummary {
    pub logo_url: Option<String>,
    pub header_url: Option<String>,
    pub description: Option<String>,
    pub verified: bool,
    pub followers: i64,
    pub products_count: i64,
    pub rating_avg: f64,
    pub rating_count: i64,
}

#[derive(Debug, Serialize)]
pub struct ProductWithVariants {
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    /// Titik fokus cover untuk `object-position` (mis. "50% 50%"). Satu berkas
    /// asli dipakai di banyak rasio — kartu 1:1, hero lebar, thumbnail tiket —
    /// dan nilai ini yang menentukan bagian mana yang bertahan saat dipotong.
    #[serde(default = "fokus_tengah")]
    pub cover_focus: String,
    /// Array foto detail product (denah, seat map, dll.)
    #[serde(default)]
    pub detail_images: Vec<DetailImageEntry>,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    #[serde(default)]
    pub category: Vec<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    /// Harga base variant termurah (berdasarkan effective price).
    pub price: f64,
    /// Harga sale aktif variant termurah (None jika tidak ada sale aktif).
    pub sale_price: Option<f64>,
    pub sale_price_start_date: Option<DateTime<Utc>>,
    pub sale_price_end_date: Option<DateTime<Utc>>,
    /// Harga efektif untuk ditampilkan: sale_price jika aktif, else price.
    pub display_price: f64,
    pub total_sold: i32,
    pub total_quota: i32,
    /// Nama toko penyelenggara (JOIN merchant_details.store_name).
    pub merchant_name: Option<String>,
    /// Ringkasan profil penyelenggara (bottom sheet product detail).
    pub merchant: Option<MerchantSummary>,
    pub product_variants: Vec<crate::models::product_variants::ProductVariantResponse>,
}

/// Variant inline di dalam CreateProductRequest.
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateVariantInline {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    pub description: Option<String>,
    #[validate(range(min = 0.0))]
    pub price: f64,
    pub sale_price: Option<f64>,
    pub sale_price_start_date: Option<DateTime<Utc>>,
    pub sale_price_end_date: Option<DateTime<Utc>>,
    #[validate(range(min = 1))]
    pub quota: i32,
    pub max_per_order: Option<i32>,
    pub sort_order: Option<i32>,
}

/// Satu hit: product + variants + image sekaligus.
/// FE kirim multipart: field "data" (JSON) + field "image" (file opsional).
#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateProductRequest {
    /// Nama merchant — dipakai untuk generate slug.
    #[validate(length(min = 1, max = 80))]
    pub merchant_name: String,
    #[validate(length(min = 2, max = 255))]
    pub name: String,
    pub description: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    /// Koordinat lokasi venue (opsional).
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// Flat Vec<String> — FE kirim sebagai ["Musik", "Festival"]
    #[serde(default)]
    pub category: Vec<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    /// Min 1 variant wajib ada.
    #[validate(length(min = 1))]
    pub variants: Vec<CreateVariantInline>,
    /// Foto detail product yang sudah di-upload ke storage.
    #[serde(default)]
    pub detail_images: Vec<DetailImageEntry>,
}

/// Variant inline untuk update — id ada = update, id None = tambah baru.
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateVariantInline {
    pub id: Option<String>,
    #[validate(length(min = 1, max = 255))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub price: Option<f64>,
    pub sale_price: Option<f64>,
    pub sale_price_start_date: Option<DateTime<Utc>>,
    pub sale_price_end_date: Option<DateTime<Utc>>,
    pub quota: Option<i32>,
    pub max_per_order: Option<i32>,
    pub is_active: Option<bool>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateProductRequest {
    #[validate(length(min = 2, max = 255))]
    pub name: Option<String>,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    /// Titik fokus cover (`object-position`). `None` = tak diubah — COALESCE di
    /// repo. Konsisten dengan field lain di struct ini: yang tak dikirim form
    /// tak boleh menimpa nilai lama dengan bawaan.
    #[serde(default)]
    pub cover_focus: Option<String>,
    pub venue: Option<String>,
    pub city: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub event_date: Option<DateTime<Utc>>,
    /// `None` = kategori tak disentuh. `Some(vec![])` = merchant melepas SEMUA
    /// centang kategori, dan itu harus benar-benar tersimpan sebagai kosong.
    ///
    /// Dulu tipenya `Vec<String>` dan repo memperlakukan vec kosong sebagai
    /// "tidak dikirim" (COALESCE), sehingga kategori mustahil dikosongkan:
    /// centang dilepas, SIMPAN, lalu kategori lama muncul lagi.
    #[serde(default)]
    pub category: Option<Vec<String>>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    /// Hanya admin yang boleh set status selain "edited".
    /// Merchant: field ini di-ignore oleh route handler (di-hardcode "edited").
    /// Admin: dikirim via PUT /api/admin/products/:id/status
    pub status: Option<String>,
    /// Foto detail — None = tidak berubah, Some(vec) = replace seluruhnya.
    pub detail_images: Option<Vec<DetailImageEntry>>,
    /// Opsional — kirim jika mau update/tambah variant sekaligus.
    pub variants: Option<Vec<UpdateVariantInline>>,
}

#[derive(Debug, Deserialize)]
pub struct ProductListQuery {
    /// Urutan hasil: `harga_asc` | `harga_desc` | `terlaris` | `terbaru`.
    /// Kosong / tak dikenali → urutan bawaan (lihat `repository::product::urutan_sql`).
    #[serde(default)]
    pub sort: Option<String>,
    pub city: Option<String>,
    pub status: Option<String>,
    pub category: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct PaginatedProducts {
    pub data: Vec<Product>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}
