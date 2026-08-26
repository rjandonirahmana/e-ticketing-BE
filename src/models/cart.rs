//! models/cart.rs — keranjang belanja yang hidup di database.
//!
//! Bentuknya mengikuti kiddoapi (`cart` + `cart_product_detail`): satu keranjang
//! aktif per user, dan setiap barisnya membawa SNAPSHOT tampilan produk saat
//! dimasukkan. Bedanya, di sini harga yang mengikat selalu dihitung ulang dari
//! `product_variants` ketika keranjang dibaca — snapshot hanya dipakai untuk
//! memperlihatkan perubahan ("harga naik sejak Anda menambahkan").

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use validator::Validate;

// ── Baris database ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cart {
    pub id: String,
    pub user_id: String,
    pub promo_code: Option<String>,
    pub discount_amount: Decimal,
    pub payment_code: Option<String>,
    pub position: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── Tampilan ──────────────────────────────────────────────────────────────────

/// Satu baris keranjang, sudah dikawinkan dengan keadaan varian TERKINI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartItemView {
    pub id: String,
    pub ticket_variant_id: String,
    pub event_id: String,
    pub event_slug: String,
    pub quantity: i32,

    /// Harga saat barang dimasukkan ke keranjang (snapshot).
    pub unit_price_snapshot: Decimal,
    /// Harga berlaku sekarang — inilah yang dipakai menghitung subtotal.
    pub unit_price: Decimal,
    pub subtotal: Decimal,

    pub event_name: String,
    pub variant_name: String,
    pub venue: String,
    pub cover_url: String,
    pub event_date: Option<DateTime<Utc>>,

    /// Sisa stok varian (quota − sold) saat keranjang dibaca.
    pub available: i32,
    pub max_per_order: Option<i32>,
    /// `quantity` melebihi sisa stok → baris ditandai, tombol bayar dikunci.
    pub exceeds_stock: bool,
    /// Harga berubah sejak dimasukkan (naik maupun turun).
    pub price_changed: bool,
    /// Ikut dibayar pada checkout berikutnya.
    pub selected: bool,
}

/// Isi keranjang + ringkasan harga. Semua angka dihitung di server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CartView {
    pub cart_id: String,
    pub items: Vec<CartItemView>,

    pub subtotal: Decimal,
    pub discount: Decimal,
    pub promo_code: Option<String>,
    /// Kosong bila promo dipakai tanpa masalah; berisi alasan bila promo gugur.
    pub promo_message: String,
    pub payment_code: Option<String>,
    pub position: String,

    /// Jumlah tiket yang DIPILIH untuk dibayar. Inilah yang dipakai halaman
    /// checkout dan syarat minimum promo.
    pub total_quantity: i32,
    /// Jumlah SELURUH tiket di keranjang, dipilih maupun tidak. Dipakai lencana
    /// navigasi dan teks "3 dari 5 dipilih".
    pub cart_quantity: i32,
    /// Subtotal − diskon, keduanya dari baris terpilih saja. Biaya kanal
    /// pembayaran BELUM termasuk (baru diketahui setelah user memilih kanal).
    pub total: Decimal,

    /// Pesan bila ada barang yang DIBUANG otomatis saat keranjang dibaca —
    /// padanan `notif` di `GET /cart/view` milik kiddoapi.
    pub notif: String,
}

// ── Request ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CartItemInput {
    pub ticket_variant_id: String,
    #[validate(range(min = 1, max = 100, message = "quantity harus 1..=100"))]
    pub quantity: i32,
}

/// Ganti SELURUH isi keranjang — padanan `POST /cart/create` kiddoapi.
/// Dipakai untuk sinkronisasi keranjang tamu (localStorage) setelah login dan
/// untuk penyimpanan massal dari halaman keranjang.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, Default)]
pub struct SaveCartRequest {
    #[validate(nested)]
    #[serde(default)]
    pub items: Vec<CartItemInput>,
    #[serde(default)]
    pub promo_code: Option<String>,
    #[serde(default)]
    pub payment_code: Option<String>,
    #[serde(default)]
    pub position: Option<String>,
    /// TRUE  → `items` menimpa isi keranjang (perilaku `POST /cart/create`).
    /// FALSE → `items` ditambahkan ke yang sudah ada (penggabungan setelah login).
    #[serde(default)]
    pub replace: bool,
}

/// Tandai satu baris ikut / tidak ikut dibayar. `ticket_variant_id` kosong
/// berarti "semua baris".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectCartItemRequest {
    #[serde(default)]
    pub ticket_variant_id: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateCartItemRequest {
    pub ticket_variant_id: String,
    /// 0 berarti hapus baris.
    #[validate(range(min = 0, max = 100, message = "quantity harus 0..=100"))]
    pub quantity: i32,
}
