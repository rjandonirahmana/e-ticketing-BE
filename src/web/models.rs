use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── Kategori produk ─────────────────────────────────────────────────────────
//
// Satu-satunya daftar kategori di aplikasi ini.
//
// Sebelumnya daftarnya ditulis DUA KALI — satu di formulir buat produk, satu di
// formulir sunting — dengan isi yang kebetulan masih sama. Dua salinan yang
// harus dijaga tetap identik hanya punya satu masa depan: menambah kategori di
// satu formulir dan lupa di formulir lain, lalu merchant yang menyunting
// produknya kehilangan kategori yang tak ada di daftar sebelah.
//
// Isinya juga sudah tak cocok lagi. Daftar lamanya — Musik, Festival, Konser,
// Olahraga, Seni, Hiburan — adalah kategori ACARA, sisa dari masa ketika
// aplikasi ini menjual tiket. Untuk marketplace barang, kategori itu bukan
// sekadar tidak relevan: ia menyesatkan merchant tentang apa yang boleh dijual
// di sini, dan membuat pembeli menyaring dengan kata yang tak pernah cocok
// dengan barang mana pun.
//
// CATATAN DATA: mengubah daftar ini hanya mengubah PILIHAN di formulir. Produk
// yang sudah terlanjur berkategori lama tetap membawa nilai lamanya, dan filter
// di halaman jelajah membaca kategori DISTINCT dari tabel `products` — jadi
// kategori lama masih akan muncul di sana sampai produknya dikategorikan ulang.
pub const PRODUCT_CATEGORIES: &[&str] = &[
    "Fashion Pria",
    "Fashion Wanita",
    "Elektronik",
    "Handphone & Tablet",
    "Komputer & Laptop",
    "Kesehatan & Kecantikan",
    "Ibu & Bayi",
    "Rumah Tangga",
    "Makanan & Minuman",
    "Olahraga & Outdoor",
    "Otomotif",
    "Hobi & Koleksi",
    "Buku & Alat Tulis",
    "Lainnya",
];

// ── Products ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub price: f64,
    pub sale_price: Option<f64>,
    pub display_price: f64,
    pub venue: Option<String>,
    pub city: Option<String>,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    #[serde(default)]
    pub category: Vec<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: String,
    pub total_sold: i32,
    pub total_quota: i32,
    /// Nama toko penyelenggara — ditampilkan di kartu explore & product detail
    /// menggantikan label generik "Toko".
    #[serde(default)]
    pub merchant_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductVariant {
    pub id: String,
    pub event_id: String,
    pub name: String,
    pub description: Option<String>,
    pub price: f64,
    pub sale_price: Option<f64>,
    #[serde(rename = "effective_price")]
    pub display_price: f64,
    pub quota: i32,
    #[serde(rename = "available")]
    pub remaining: i32,
    pub max_per_order: Option<i32>,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductWithVariants {
    pub id: String,
    pub merchant_id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    /// Titik fokus cover (`object-position`), mis. "50% 50%". Kosong = data
    /// lama; sisi render menjatuhkannya ke tengah.
    #[serde(default)]
    pub cover_focus: String,
    pub venue: Option<String>,
    pub city: Option<String>,
    #[serde(default)]
    pub latitude: Option<f64>,
    #[serde(default)]
    pub longitude: Option<f64>,
    #[serde(default)]
    pub category: Vec<String>,
    pub event_date: DateTime<Utc>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: String,
    pub price: f64,
    pub sale_price: Option<f64>,
    pub display_price: f64,
    pub total_sold: i32,
    pub total_quota: i32,
    /// Nama toko penyelenggara (label organizer + bottom sheet info merchant).
    #[serde(default)]
    pub merchant_name: Option<String>,
    /// Ringkasan profil penyelenggara — ikut payload detail product (1 query,
    /// JOIN + agregat di server) sehingga bottom sheet TIDAK fetch kedua.
    #[serde(default)]
    pub merchant: Option<ProductMerchantInfo>,
    #[serde(default)]
    pub product_variants: Vec<ProductVariant>,
    /// Foto detail product (denah/seat/harga/lainnya), terurut sesuai tampilan.
    #[serde(default)]
    pub detail_images: Vec<WebDetailImage>,
}

/// Satu foto detail product untuk sisi web (seed galeri di halaman edit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDetailImage {
    pub url: String,
    #[serde(default)]
    pub image_type: String,
    #[serde(default)]
    pub caption: String,
    /// Titik fokus `object-position` (mis. "50% 50%"). `serde(default)` +
    /// fallback di sisi render menjaga data lama tetap tampil seperti dulu.
    #[serde(default)]
    pub focus: String,
}

/// Ringkasan merchant yang di-embed di detail product (isi bottom sheet
/// penyelenggara). Nama toko ada di `ProductWithVariants::merchant_name`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductMerchantInfo {
    pub logo_url: Option<String>,
    pub header_url: Option<String>,
    pub description: Option<String>,
    pub verified: bool,
    pub followers: i64,
    pub products_count: i64,
    pub rating_avg: f64,
    pub rating_count: i64,
}

/// Payload varian dari form create/edit product. Dikirim ke server fn sebagai
/// JSON string karena `crate::models` (tipe request server) tidak ter-compile
/// di WASM. `id` Some = update varian lama, None = varian baru.
/// `is_active: Some(false)` = varian lama "dihapus" dari form (dinonaktifkan,
/// bukan DELETE — tiket terjual bisa masih mereferensikannya).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantForm {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub price: f64,
    pub quota: i32,
    #[serde(default)]
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaginatedProducts {
    pub data: Vec<Product>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

// ── Merchant publik (profil + rating & reviews) ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantPublicProfile {
    pub merchant_id: String,
    pub store_name: String,
    pub description: Option<String>,
    pub logo_url: Option<String>,
    /// Header/cover kustom merchant (kosong → hero fallback ke cover product terbaru).
    #[serde(default)]
    pub header_url: Option<String>,
    pub verified: bool,
    pub followers: i64,
    pub products_count: i64,
    pub rating_avg: f64,
    pub rating_count: i64,
    /// Apakah viewer yang sedang login mem-follow merchant ini.
    #[serde(default)]
    pub is_following: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantReviewItem {
    #[serde(default)]
    pub user_id: String,
    pub user_name: String,
    pub rating: i32,
    pub comment: String,
    pub created_at: DateTime<Utc>,
}

/// Profil publik user biasa (halaman /u/{id}).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPublicProfile {
    pub user_id: String,
    pub name: String,
    pub following: i64,
    pub reviews: i64,
    pub stories: i64,
}

/// Satu ulasan yang ditulis user (ke merchant) — /u/{id} tab Reviews.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserReviewItem {
    pub merchant_id: String,
    pub store_name: String,
    pub rating: i32,
    pub comment: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserReviewsData {
    pub total: i64,
    pub items: Vec<UserReviewItem>,
}

/// Payload lengkap halaman /m/{id}: profil + products page-1 + ulasan + story
/// dalam SATU server fn (`get_merchant_public_page`) — 1 round-trip HTTP dari
/// klien alih-alih 4, dan server men-join semua query secara paralel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantPublicPageData {
    pub profile: MerchantPublicProfile,
    pub products: PaginatedProducts,
    pub reviews: MerchantReviewsData,
    pub stories: Vec<crate::web::state::stories::StoryGroup>,
}

/// Payload halaman reviews: ringkasan + daftar ulasan sekaligus (1 round-trip).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantReviewsData {
    pub store_name: String,
    pub avg: f64,
    pub total: i64,
    /// dist[0] = bintang 1 … dist[4] = bintang 5.
    pub dist: [i64; 5],
    pub items: Vec<MerchantReviewItem>,
}

/// Hasil pencarian merchant (autocomplete di /explore).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantSearchItem {
    pub merchant_id: String,
    pub store_name: String,
    pub logo_url: Option<String>,
    pub verified: bool,
}

/// Satu follower merchant (halaman /m/{id}/followers).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowerItem {
    pub user_id: String,
    pub name: String,
    /// role='merchant' ⟺ punya /m/{id} (dijamin trigger migrasi 016). Menentukan
    /// tujuan tautan: "merchant" → /m/{id}, selain itu → /u/{id}.
    pub role: String,
    pub created_at: DateTime<Utc>,
}

/// Satu TOKO yang diikuti pengguna (halaman /following).
///
/// Berbeda dari [`FollowerItem`] yang menggambarkan ORANG: di sini yang penting
/// nama toko dan logonya, karena itulah yang dikenali pembeli dari halaman
/// produk. `users.name` (nama pemilik akun) sengaja tidak dipakai.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowingItem {
    pub merchant_id: String,
    pub store_name: String,
    pub logo_url: String,
    pub verified: bool,
    pub followed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowingData {
    pub total: i64,
    pub items: Vec<FollowingItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantFollowersData {
    pub total: i64,
    pub items: Vec<FollowerItem>,
}

// ── Banners ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Banner {
    pub id: i64,
    pub image_url: String,
    #[serde(rename = "click_url")]
    pub link_url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
}

// ── Users / Auth ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserResponse {
    pub id: String,
    pub email: Option<String>,
    pub name: String,
    pub phone: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    #[serde(alias = "token")]
    pub access_token: String,
    pub user: UserResponse,
}

// ── Tickets ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketResponse {
    pub id: String,
    pub ticket_code: String,
    pub status: String,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub order_id: String,
    pub order_code: String,
    pub event_id: String,
    pub event_name: String,
    pub event_slug: String,
    pub event_date: DateTime<Utc>,
    pub event_venue: Option<String>,
    pub event_city: Option<String>,
    pub variant_id: String,
    pub variant_name: String,
    pub unit_price: f64,
    pub cover_url: Option<String>,
}

// ── Orders ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderListItem {
    pub id: String,
    pub order_code: String,
    pub status: String,
    pub total_amount: f64,
    pub event_name: Option<String>,
    pub event_date: Option<DateTime<Utc>>,
    pub venue: Option<String>,
    pub cover_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expired_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItem {
    #[serde(default)]
    pub event_name: String,
    #[serde(default)]
    pub variant_name: String,
    pub quantity: i32,
    pub subtotal: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDetail {
    pub id: String,
    #[serde(default)]
    pub order_code: String,
    #[serde(default)]
    pub status: String,
    pub total_amount: f64,
    #[serde(default)]
    pub payment_method: Option<String>,
    #[serde(default)]
    pub paid_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub expired_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub items: Vec<OrderItem>,

    // ── Rincian pembayaran ──────────────────────────────────────────────────
    // Halaman detail order menampilkan nomor Virtual Account dan cara membayar
    // untuk order yang masih menunggu; sebelum keranjang pindah ke database,
    // data ini memang tidak ada di mana pun.
    #[serde(default)]
    pub subtotal_amount: f64,
    #[serde(default)]
    pub discount_amount: f64,
    #[serde(default)]
    pub promo_code: Option<String>,
    #[serde(default)]
    pub payment_code: Option<String>,
    #[serde(default)]
    pub payment_name: Option<String>,
    #[serde(default)]
    pub payment_charge: f64,
    #[serde(default)]
    pub payment_reference: Option<String>,
    #[serde(default)]
    pub payment_instruction: Option<String>,
    #[serde(default)]
    pub payment_expired_at: Option<DateTime<Utc>>,
}

// ── Notifications ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationItem {
    pub id: String,
    #[serde(default)]
    pub kind: String,
    pub title: String,
    pub body: String,
    pub is_read: bool,
    #[serde(default)]
    pub target_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

// ── Chat / Pulse ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChatRoom {
    pub id: String,
    pub event_id: String,
    pub name: String,
    #[serde(default)]
    pub member_count: i32,
    #[serde(default)]
    pub last_message: Option<String>,
    #[serde(default)]
    pub unread_count: i32,
    #[serde(default)]
    pub cover_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub room_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub content: String,
    pub sent_at: u64,
    #[serde(default, alias = "msg_type")]
    pub message_type: String,
    /// Ada hanya untuk pesan bergambar.
    #[serde(default)]
    pub media_url: Option<String>,
    /// Pesan yang dibalas, bila ada.
    #[serde(default)]
    pub reply_to: Option<KutipanChat>,
}

/// Cerminan `models::group_chat::KutipanPesan` untuk sisi web.
///
/// Disalin, bukan dipakai bersama: `crate::models` dipagari khusus server di
/// `lib.rs`, sedangkan bentuk ini harus ikut terkompilasi ke WASM. Bidangnya
/// wajib bernama sama persis — keduanya diurai dari bingkai JSON yang SATU.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KutipanChat {
    pub id: String,
    pub sender_name: String,
    pub content: String,
    #[serde(default)]
    pub is_image: bool,
}

// ── Merchant Product Form ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MerchantProductForm {
    pub name: String,
    pub description: String,
    pub venue: String,
    pub city: String,
    pub event_date: String,
    pub start_time: String,
    pub end_time: String,
    pub categories: Vec<String>,
    pub cover_url: Option<String>,
}

// ── Cart & Checkout ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CartItem {
    pub event_id: String,
    pub tier_id: String,
    pub event_title: String,
    pub tier_name: String,
    pub venue_name: String,
    #[serde(default)]
    pub event_cover: String,
    pub quantity: i32,
    pub unit_price: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderItemRef {
    pub event_name: String,
    pub variant_name: String,
    pub quantity: i32,
    pub subtotal: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRef {
    pub id: String,
    pub order_code: String,
    pub status: String,
    pub total_amount: i64,
    pub expired_at: Option<String>,
    pub created_at: Option<String>,
    pub items: Vec<OrderItemRef>,

    // ── Rincian pembayaran ──────────────────────────────────────────────────
    // Semua bernilai default pada order lama / jalur yang tak menyebut kanal,
    // sehingga halaman bisa memutuskan sendiri apakah panel instruksi perlu
    // ditampilkan tanpa memanggil endpoint kedua.
    #[serde(default)]
    pub subtotal_amount: i64,
    #[serde(default)]
    pub discount_amount: i64,
    #[serde(default)]
    pub promo_code: Option<String>,
    #[serde(default)]
    pub payment_code: Option<String>,
    /// Nama kanal yang enak dibaca ("BCA Virtual Account").
    #[serde(default)]
    pub payment_name: Option<String>,
    #[serde(default)]
    pub payment_charge: i64,
    /// Nomor Virtual Account / referensi QRIS.
    #[serde(default)]
    pub payment_reference: Option<String>,
    #[serde(default)]
    pub payment_instruction: Option<String>,
    /// Tenggat bayar menurut kanal (RFC 3339).
    #[serde(default)]
    pub payment_expired_at: Option<String>,
}

// ── Keranjang dari server ────────────────────────────────────────────────────

/// Satu baris keranjang seperti yang dikirim server.
///
/// `tier_id` sengaja memakai nama lama untuk id varian: halaman product detail
/// dan keranjang sudah berbicara dalam istilah itu, dan menggantinya hanya akan
/// menyebar perubahan tanpa menambah kejelasan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CartItemView {
    pub id: String,
    pub tier_id: String,
    pub event_id: String,
    #[serde(default)]
    pub event_slug: String,
    pub event_title: String,
    pub tier_name: String,
    #[serde(default)]
    pub venue_name: String,
    #[serde(default)]
    pub event_cover: String,
    #[serde(default)]
    pub event_date: Option<DateTime<Utc>>,

    /// Pemilik product — keranjang mengelompokkan baris per toko.
    ///
    /// `serde(default)`: keranjang TAMU tersimpan di localStorage peramban, dan
    /// salinan yang ditulis versi lama aplikasi tak memuat field ini. Tanpa
    /// bawaan, keranjang lama gagal dibaca dan isinya lenyap saat orang membuka
    /// halaman setelah pembaruan.
    #[serde(default)]
    pub merchant_id: String,
    #[serde(default)]
    pub merchant_name: String,

    pub quantity: i32,
    /// Harga berlaku sekarang.
    pub unit_price: i64,
    /// Harga saat barang dimasukkan ke keranjang.
    #[serde(default)]
    pub unit_price_snapshot: i64,
    pub subtotal: i64,

    #[serde(default)]
    pub available: i32,
    #[serde(default)]
    pub max_per_order: Option<i32>,
    /// Jumlah melebihi sisa stok — tombol bayar dikunci sampai dikurangi.
    #[serde(default)]
    pub exceeds_stock: bool,
    #[serde(default)]
    pub price_changed: bool,
    /// Dicentang untuk ikut dibayar.
    #[serde(default = "crate::web::models::default_true")]
    pub selected: bool,
}

/// Baris keranjang lahir dalam keadaan tercentang — sama dengan `DEFAULT TRUE`
/// pada kolomnya, sehingga keranjang lama berperilaku persis seperti dulu.
pub fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CartView {
    #[serde(default)]
    pub cart_id: String,
    pub items: Vec<CartItemView>,
    pub subtotal: i64,
    #[serde(default)]
    pub discount: i64,
    #[serde(default)]
    pub promo_code: Option<String>,
    #[serde(default)]
    pub promo_message: String,
    #[serde(default)]
    pub payment_code: Option<String>,
    /// Jumlah tiket yang DICENTANG — dipakai halaman checkout.
    pub total_quantity: i32,
    /// Jumlah SELURUH tiket di keranjang, untuk lencana navigasi.
    #[serde(default)]
    pub cart_quantity: i32,
    /// Subtotal − diskon dari baris tercentang. Biaya kanal belum termasuk.
    pub total: i64,
    /// Pesan barang yang dibuang otomatis (stok habis / product tutup).
    #[serde(default)]
    pub notif: String,
}

/// Kanal pembayaran beserta biayanya untuk nominal yang sedang berlaku.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaymentMethodView {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub vendor: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub image_url: String,
    #[serde(default)]
    pub description: String,
    /// Biaya admin untuk nominal ini.
    pub charge: i64,
    /// Nominal + biaya admin.
    pub total: i64,
    #[serde(default)]
    pub is_instant: bool,
    #[serde(default)]
    pub instruction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaymentOptions {
    pub methods: Vec<PaymentMethodView>,
    /// Nominal yang dipakai menghitung biaya tiap kanal (total keranjang).
    pub amount: i64,
    /// Kanal yang sebelumnya dipilih user, bila ada.
    #[serde(default)]
    pub selected: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrderResponse {
    pub order: OrderRef,
    #[serde(default)]
    pub requires_redirect: bool,
    #[serde(default)]
    pub payment_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatePromoResponse {
    pub valid: bool,
    #[serde(default)]
    pub discount_idr: i64,
    #[serde(default)]
    pub message: String,
}

// ── Subscription ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingSubOrder {
    pub order_id: String,
    pub order_code: String,
    pub plan: String,
    pub amount_idr: i64,
}

// ── Scan ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScanValidateResult {
    pub event_title: String,
    pub tier_name: String,
    pub status: String,
    pub ticket_code: String,
}

// ── Admin ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminStats {
    #[serde(default)]
    pub total_users: i64,
    #[serde(default)]
    pub total_products: i64,
    #[serde(default)]
    pub total_orders: i64,
    #[serde(default)]
    pub total_revenue: f64,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Harga tampilan. Nol berbunyi "Gratis".
///
/// Meneruskan, tidak menyalin: badannya dulu merakit pemisah ribuan sendiri —
/// salinan keempat dari logika yang sama — dan mengejanya `Rp 1.000.000`
/// sementara sebagian besar aplikasi mengeja `Rp1.000.000`.
pub fn format_price(price: f64) -> String {
    crate::web::utils::rupiah_atau_gratis(price as i64)
}

/// Tanggal tampilan, zona WIB, nama bulan Indonesia.
pub use crate::web::utils::waktu::tanggal as format_date;

// `format_datetime` DIBUANG: nol pemanggil, dan ia menampilkan jam UTC dengan
// label "WIB" — tujuh jam meleset, dengan keterangan zona yang meyakinkan.
// Penggantinya bila kelak dibutuhkan: `web::utils::waktu::tanggal_jam`.
