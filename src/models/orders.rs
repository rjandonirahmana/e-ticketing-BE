use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub customer_id: String,
    pub order_code: String,
    pub status: String,

    /// Angka yang DIBAYAR: `subtotal_amount - discount_amount + payment_charge`.
    pub total_amount: Decimal,
    /// Harga tiket sebelum potongan dan sebelum biaya kanal.
    pub subtotal_amount: Decimal,
    pub discount_amount: Decimal,
    pub promo_code: Option<String>,

    /// Nama kanal lama (dipertahankan untuk jalur REST & data lawas); nilainya
    /// kini selalu sama dengan `payment_code`.
    pub payment_method: Option<String>,
    pub payment_vendor: Option<String>,
    pub payment_code: Option<String>,
    pub payment_charge: Decimal,
    /// Batas waktu dari sisi KANAL — berbeda dari `expired_at` yang menahan stok.
    pub payment_expired_at: Option<DateTime<Utc>>,
    /// Nomor Virtual Account / referensi QRIS yang ditunjukkan ke pembeli.
    pub payment_reference: Option<String>,
    /// Halaman bayar milik gateway, bila kanalnya redirect.
    pub link_pay: Option<String>,

    pub paid_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    pub id: String,
    pub order_id: String,
    pub ticket_variant_id: String,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub subtotal: Decimal,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Clone)]
pub struct OrderItemResponse {
    pub id: String,
    pub ticket_variant_id: String,
    pub variant_name: String,
    pub event_id: String,
    pub event_name: String,
    pub quantity: i32,
    pub unit_price: Decimal,
    pub subtotal: Decimal,
}

#[derive(Debug, Serialize, Clone)]
pub struct OrderDetailResponse {
    pub id: String,
    pub customer_id: String,
    pub order_code: String,
    pub status: String,

    pub total_amount: Decimal,
    pub subtotal_amount: Decimal,
    pub discount_amount: Decimal,
    pub promo_code: Option<String>,

    pub payment_method: Option<String>,
    pub payment_vendor: Option<String>,
    pub payment_code: Option<String>,
    /// Nama kanal yang enak dibaca ("BCA Virtual Account"), diisi jalur checkout.
    pub payment_name: Option<String>,
    pub payment_charge: Decimal,
    pub payment_expired_at: Option<DateTime<Utc>>,
    pub payment_reference: Option<String>,
    pub payment_instruction: Option<String>,
    pub link_pay: Option<String>,

    pub paid_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub items: Vec<OrderItemResponse>,
}

/// Enriched order for list endpoint — includes first product's info.
/// Eliminates the need for a per-order items fetch just to show the product name.
#[derive(Debug, Serialize, Clone)]
pub struct OrderListItem {
    pub id: String,
    pub customer_id: String,
    pub order_code: String,
    pub status: String,
    pub total_amount: Decimal,
    pub payment_method: Option<String>,
    pub paid_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    // Enriched from the first order item's product (NULL if order has no items yet)
    pub event_name: Option<String>,
    pub event_date: Option<DateTime<Utc>>,
    pub venue: Option<String>,
    pub cover_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Validate)]
pub struct CreateOrderItemRequest {
    pub ticket_variant_id: String,
    #[validate(range(min = 1, max = 100, message = "quantity must be 1..=100"))]
    pub quantity: i32,
}

#[derive(Debug, Deserialize, Validate)]
pub struct CreateOrderRequest {
    /// Opsional — di-generate client (UUID/ULID).
    /// Request ulang dengan key yang sama mengembalikan order yang sudah ada
    /// tanpa membuat order baru (mencegah double order akibat retry/double-click).
    pub idempotency_key: Option<String>,

    #[validate(length(min = 1, message = "items must not be empty"))]
    #[validate(nested)]
    pub items: Vec<CreateOrderItemRequest>,
}

/// Checkout dari keranjang yang tersimpan di database.
///
/// Perhatikan yang TIDAK ada di sini: daftar tiket dan harganya. Keduanya
/// dibaca server dari keranjang milik pemanggil — persis seperti
/// `POST /order/create` kiddoapi yang hanya menerima {vendor, code}. Klien
/// tidak pernah bisa menyebut harga.
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CheckoutRequest {
    /// Kode kanal pembayaran dari tabel `payment_methods`.
    #[validate(length(min = 1, max = 50, message = "metode pembayaran wajib dipilih"))]
    pub payment_code: String,
    /// Bila kosong, kode promo yang sudah menempel di keranjang yang dipakai.
    #[serde(default)]
    pub promo_code: Option<String>,
    /// Kunci idempotensi dari klien — mencegah dobel-klik melahirkan dua order.
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct PayOrderRequest {
    #[validate(length(min = 1, max = 50))]
    pub payment_method: String,
}
