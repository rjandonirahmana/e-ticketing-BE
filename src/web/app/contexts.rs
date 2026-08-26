//! web/app/contexts.rs — Tipe context global aplikasi (di-provide sekali di App).
//!
//! Dipisah dari router & providers agar tipe-tipe ini bisa di-import murni
//! tanpa menarik dependency view/router. Halaman lain meng-import lewat
//! re-export `crate::web::app::{AuthResource, CartContext, ...}`.

use leptos::prelude::*;

use crate::web::models::{
    CartItem, CartItemView, CartView, OrderRef, PendingSubOrder, UserResponse,
};

/// Resource auth global — di-provide di `provide_all_app_contexts()`.
pub type AuthResource = Resource<Result<Option<UserResponse>, ServerFnError>>;

#[derive(Clone, Debug, Default)]
pub struct SuccessSnapshot {
    pub order_code: String,
    pub event_name: String,
    pub total_amount: i64,
}

/// Ringkasan harga keranjang. Untuk pengguna yang sudah masuk, seluruh angkanya
/// datang dari server; untuk tamu, dihitung lokal dari isi `localStorage`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CartSummary {
    pub subtotal: i64,
    pub discount: i64,
    pub total: i64,
    pub total_quantity: i32,
    /// Seluruh isi keranjang, dicentang maupun tidak — untuk lencana navigasi.
    pub cart_quantity: i32,
    pub promo_code: Option<String>,
    pub promo_message: String,
    pub payment_code: Option<String>,
    /// Pesan dari server tentang barang yang dibuang otomatis (stok habis).
    pub notif: String,
}

/// Keranjang belanja.
///
/// ── Dua mode, satu antarmuka ────────────────────────────────────────────────
/// **Sudah masuk** → keranjang hidup di database. Setiap perubahan dikirim ke
/// server, dan server mengembalikan SELURUH isi keranjang beserta ringkasan
/// harganya; sinyal di sini menjadi cerminan jawaban itu. Karena itu keranjang
/// mengikuti pemiliknya lintas perangkat, dan harga yang tampil selalu harga
/// yang akan ditagihkan.
///
/// **Tamu** → keranjang hidup di `localStorage`, seperti sebelumnya. Ini
/// disengaja: memaksa masuk sebelum boleh menaruh tiket di keranjang membuang
/// pembeli yang belum siap mendaftar. Begitu ia masuk, `bootstrap()` menuangkan
/// keranjang tamu itu ke keranjang miliknya.
///
/// Perubahan lokal diterapkan LEBIH DULU (optimistis), lalu dikoreksi oleh
/// jawaban server. Tombol +/− karena itu terasa seketika, tetapi angka yang
/// akhirnya menetap adalah angka server.
#[derive(Clone, Copy)]
pub struct CartContext {
    pub items: RwSignal<Vec<CartItemView>>,
    pub summary: RwSignal<CartSummary>,
    /// Sedang menunggu jawaban server.
    pub loading: RwSignal<bool>,
    /// Keranjang milik user yang sudah masuk (bukan keranjang tamu).
    pub authed: RwSignal<bool>,
    /// Sudah pernah dimuat sekali — pembeda "keranjang kosong" dari "belum tahu".
    pub ready: RwSignal<bool>,
    pub error: RwSignal<String>,
}

impl CartContext {
    pub fn new() -> Self {
        Self {
            items: RwSignal::new(Vec::new()),
            summary: RwSignal::new(CartSummary::default()),
            loading: RwSignal::new(false),
            authed: RwSignal::new(false),
            ready: RwSignal::new(false),
            error: RwSignal::new(String::new()),
        }
    }

    // ── Baca ────────────────────────────────────────────────────────────────

    pub fn get_qty(&self, tier_id: &str) -> i32 {
        self.items.with(|v| {
            v.iter()
                .find(|i| i.tier_id == tier_id)
                .map(|i| i.quantity)
                .unwrap_or(0)
        })
    }

    /// Untuk lencana navigasi: seluruh isi keranjang, dicentang maupun tidak.
    pub fn count(&self) -> i32 {
        self.summary.with(|s| s.cart_quantity)
    }

    pub fn is_empty(&self) -> bool {
        self.items.with(|v| v.is_empty())
    }

    // ── Tulis ───────────────────────────────────────────────────────────────

    /// Masukkan tiket ke keranjang. Menerima `CartItem` (bentuk ringkas yang
    /// dipakai halaman product detail) supaya pemanggil tak perlu tahu tentang
    /// keranjang server sama sekali.
    pub fn add_item(&self, item: CartItem) {
        let qty = item.quantity.max(1);
        let tier = item.tier_id.clone();

        self.items.update(|v| {
            if let Some(existing) = v.iter_mut().find(|i| i.tier_id == tier) {
                existing.quantity += qty;
                existing.subtotal = existing.unit_price * existing.quantity as i64;
            } else {
                v.push(line_from(&item, qty));
            }
        });
        self.after_local_change();

        if self.authed.get_untracked() {
            let this = *self;
            let tier = item.tier_id.clone();
            spawn(async move {
                match crate::web::api::add_to_cart(tier, qty).await {
                    Ok(view) => this.apply(view),
                    Err(e) => this.fail_and_resync(e),
                }
            });
        }
    }

    /// Tetapkan jumlah sebuah baris; `qty <= 0` menghapusnya.
    pub fn update_qty(&self, tier_id: &str, qty: i32) {
        let tier = tier_id.to_string();

        self.items.update(|v| {
            if qty <= 0 {
                v.retain(|i| i.tier_id != tier);
            } else if let Some(it) = v.iter_mut().find(|i| i.tier_id == tier) {
                it.quantity = qty;
                it.subtotal = it.unit_price * qty as i64;
            }
        });
        self.after_local_change();

        if self.authed.get_untracked() {
            let this = *self;
            let tier = tier_id.to_string();
            spawn(async move {
                match crate::web::api::update_cart_quantity(tier, qty.max(0)).await {
                    Ok(view) => this.apply(view),
                    Err(e) => this.fail_and_resync(e),
                }
            });
        }
    }

    /// Centang / lepas centang satu baris keranjang.
    pub fn toggle_selected(&self, tier_id: &str, selected: bool) {
        let tier = tier_id.to_string();
        self.items.update(|v| {
            if let Some(it) = v.iter_mut().find(|i| i.tier_id == tier) {
                it.selected = selected;
            }
        });
        self.after_local_change();

        if self.authed.get_untracked() {
            let this = *self;
            let tier = tier_id.to_string();
            spawn(async move {
                match crate::web::api::select_cart_item(Some(tier), selected).await {
                    Ok(view) => this.apply(view),
                    Err(e) => this.fail_and_resync(e),
                }
            });
        }
    }

    /// Centang / lepas centang seluruh isi keranjang.
    pub fn select_all(&self, selected: bool) {
        self.items.update(|v| {
            for it in v.iter_mut() {
                it.selected = selected;
            }
        });
        self.after_local_change();

        if self.authed.get_untracked() {
            let this = *self;
            spawn(async move {
                match crate::web::api::select_cart_item(None, selected).await {
                    Ok(view) => this.apply(view),
                    Err(e) => this.fail_and_resync(e),
                }
            });
        }
    }

    /// Pasang / lepas kode promo (hanya untuk pengguna yang sudah masuk —
    /// promo divalidasi server, tak ada versi tamunya).
    pub fn set_promo(&self, code: Option<String>) {
        if !self.authed.get_untracked() {
            return;
        }
        let this = *self;
        this.loading.set(true);
        spawn(async move {
            match crate::web::api::apply_cart_promo(code).await {
                Ok(view) => this.apply(view),
                Err(e) => this.fail(e),
            }
            this.loading.set(false);
        });
    }

    /// Simpan kanal pembayaran pilihan user di keranjangnya.
    pub fn set_payment(&self, code: String) {
        // Terapkan dulu supaya pilihan langsung tersorot; server menyusul.
        self.summary.update(|s| s.payment_code = Some(code.clone()));
        if !self.authed.get_untracked() {
            return;
        }
        let this = *self;
        spawn(async move {
            match crate::web::api::select_payment_method(code).await {
                Ok(view) => this.apply(view),
                Err(e) => this.fail(e),
            }
        });
    }

    /// Kosongkan keranjang di layar. Dipanggil setelah checkout berhasil —
    /// server sudah menutup keranjangnya, jadi tak ada permintaan tambahan.
    pub fn reset_after_checkout(&self) {
        self.items.set(Vec::new());
        self.summary.set(CartSummary::default());
        self.error.set(String::new());
        self.persist_local();
    }

    // ── Pemuatan ────────────────────────────────────────────────────────────

    /// Muat keranjang dari server.
    pub fn load(&self) {
        if !self.authed.get_untracked() {
            self.load_local();
            return;
        }
        let this = *self;
        this.loading.set(true);
        spawn(async move {
            match crate::web::api::get_cart().await {
                Ok(view) => this.apply(view),
                Err(e) => this.fail(e),
            }
            this.loading.set(false);
        });
    }

    /// Dipanggil begitu status masuk diketahui: tuang keranjang tamu ke
    /// keranjang milik user, lalu pakai jawaban server sebagai kebenaran.
    ///
    /// Penggabungan dilakukan sekali dan kunci `localStorage` dibersihkan
    /// sesudahnya — kalau tidak, keranjang tamu yang sama akan dituang lagi di
    /// setiap kali halaman dibuka dan jumlahnya berlipat sendiri.
    pub fn bootstrap(&self) {
        let this = *self;
        this.authed.set(true);

        let guest: Vec<CartItemView> = read_local();
        let payload: Option<String> = if guest.is_empty() {
            None
        } else {
            serde_json::to_string(
                &guest
                    .iter()
                    .map(|i| serde_json::json!({ "tier_id": i.tier_id, "quantity": i.quantity }))
                    .collect::<Vec<_>>(),
            )
            .ok()
        };

        this.loading.set(true);
        spawn(async move {
            let result = match payload {
                Some(json) => {
                    let r = crate::web::api::sync_guest_cart(json).await;
                    if r.is_ok() {
                        clear_local();
                    }
                    r
                }
                None => crate::web::api::get_cart().await,
            };
            match result {
                Ok(view) => this.apply(view),
                Err(e) => this.fail(e),
            }
            this.loading.set(false);
        });
    }

    /// Mode tamu: baca dari `localStorage` dan hitung ringkasannya sendiri.
    pub fn load_local(&self) {
        let items = read_local();
        self.items.set(items);
        self.recompute_local();
        self.ready.set(true);
    }

    // ── Internal ────────────────────────────────────────────────────────────

    fn apply(&self, view: CartView) {
        self.items.set(view.items);
        self.summary.set(CartSummary {
            subtotal: view.subtotal,
            discount: view.discount,
            total: view.total,
            total_quantity: view.total_quantity,
            cart_quantity: view.cart_quantity,
            promo_code: view.promo_code,
            promo_message: view.promo_message,
            payment_code: view.payment_code,
            notif: view.notif,
        });
        self.error.set(String::new());
        self.ready.set(true);
    }

    fn fail(&self, e: ServerFnError) {
        self.error.set(clean_error(&e.to_string()));
        self.ready.set(true);
    }

    /// Gagal SETELAH perubahan optimistis sudah tampil di layar.
    ///
    /// Selain menampilkan alasannya, keadaan layar ditarik kembali ke keadaan
    /// server. Tanpa ini, barang yang ditolak server tetap terlihat di
    /// keranjang sampai halaman dimuat ulang — lalu lenyap tanpa penjelasan,
    /// yang justru gejala yang ingin kita hapus.
    fn fail_and_resync(&self, e: ServerFnError) {
        self.error.set(clean_error(&e.to_string()));
        let this = *self;
        spawn(async move {
            if let Ok(view) = crate::web::api::get_cart().await {
                let pesan = this.error.get_untracked();
                this.apply(view);
                // `apply` mengosongkan pesan error; pasang lagi supaya alasannya
                // tidak ikut hilang bersama penyegaran.
                this.error.set(pesan);
            }
            this.ready.set(true);
        });
    }

    /// Sesudah perubahan optimistis: hitung ulang ringkasan dan (untuk tamu)
    /// simpan ke `localStorage`.
    fn after_local_change(&self) {
        self.recompute_local();
        if !self.authed.get_untracked() {
            self.persist_local();
        }
    }

    fn recompute_local(&self) {
        // Sama seperti di server: harga hanya menjumlah baris yang dicentang,
        // sedangkan lencana navigasi menghitung seluruh isi keranjang.
        let (subtotal, qty, all_qty) = self.items.with_untracked(|v| {
            (
                v.iter()
                    .filter(|i| i.selected)
                    .map(|i| i.unit_price * i.quantity as i64)
                    .sum::<i64>(),
                v.iter().filter(|i| i.selected).map(|i| i.quantity).sum::<i32>(),
                v.iter().map(|i| i.quantity).sum::<i32>(),
            )
        });
        self.summary.update(|s| {
            s.subtotal = subtotal;
            s.total_quantity = qty;
            s.cart_quantity = all_qty;
            // Diskon hanya sah bila datang dari server; saat isi keranjang
            // berubah lokal, potongan lama tak boleh ikut terbawa.
            s.discount = if self.authed.get_untracked() { s.discount } else { 0 };
            s.total = (subtotal - s.discount).max(0);
        });
    }

    fn persist_local(&self) {
        #[cfg(target_arch = "wasm32")]
        self.items.with_untracked(|v| {
            if let Some(storage) = local_storage() {
                if v.is_empty() {
                    let _ = storage.remove_item(CART_KEY);
                } else if let Ok(json) = serde_json::to_string(v) {
                    let _ = storage.set_item(CART_KEY, &json);
                }
            }
        });
    }
}

impl Default for CartContext {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helper ───────────────────────────────────────────────────────────────────

/// Hanya dipakai di sisi browser; di SSR keranjang tamu tak pernah dibaca.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
/// Buang bungkus teknis dari pesan server fn supaya yang sampai ke pembeli
/// adalah kalimat yang ditulis service, bukan jejak internal Leptos.
fn clean_error(raw: &str) -> String {
    let s = raw
        .trim_start_matches("error running server function: ")
        .trim_start_matches("ServerFnError: ")
        .trim();
    match s.rsplit_once("Bad request: ") {
        Some((_, pesan)) => pesan.trim().to_string(),
        None => s.to_string(),
    }
}

pub const CART_KEY: &str = "pulse_cart";

/// Jalankan tugas async di klien. Di server `spawn_local` tidak tersedia (dan
/// tidak ada gunanya): keranjang di SSR selalu kosong dan diisi setelah
/// hydration.
fn spawn<F>(fut: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    #[cfg(target_arch = "wasm32")]
    leptos::task::spawn_local(fut);
    #[cfg(not(target_arch = "wasm32"))]
    drop(fut);
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok()).flatten()
}

/// Baca keranjang tamu.
///
/// Menerima DUA bentuk: bentuk sekarang (`CartItemView`) dan bentuk lama
/// (`CartItem`, tanpa `id`/`subtotal`) yang mungkin masih tersimpan di browser
/// pengguna lama. Tanpa jalur kedua itu, keranjang mereka akan tampak kosong
/// setelah pembaruan ini — hilang tanpa jejak, tepat pada orang yang paling
/// sering kembali.
pub fn read_local() -> Vec<CartItemView> {
    #[cfg(target_arch = "wasm32")]
    {
        let raw = local_storage()
            .and_then(|s| s.get_item(CART_KEY).ok())
            .flatten()
            .unwrap_or_default();
        if raw.is_empty() {
            return Vec::new();
        }
        if let Ok(v) = serde_json::from_str::<Vec<CartItemView>>(&raw) {
            return v;
        }
        if let Ok(old) = serde_json::from_str::<Vec<CartItem>>(&raw) {
            return old.iter().map(|i| line_from(i, i.quantity.max(1))).collect();
        }
        Vec::new()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Vec::new()
    }
}

pub fn clear_local() {
    #[cfg(target_arch = "wasm32")]
    if let Some(s) = local_storage() {
        let _ = s.remove_item(CART_KEY);
    }
}

/// Bentuk baris keranjang dari `CartItem` ringkas.
///
/// `available` di-set `i32::MAX` karena sisa stok memang belum diketahui di
/// sisi klien; menandai baris "melebihi stok" atas dasar tebakan hanya akan
/// mengunci tombol bayar tanpa alasan. Angka sebenarnya datang bersama jawaban
/// server pada permintaan berikutnya.
fn line_from(item: &CartItem, quantity: i32) -> CartItemView {
    CartItemView {
        id: item.tier_id.clone(),
        tier_id: item.tier_id.clone(),
        event_id: item.event_id.clone(),
        event_slug: String::new(),
        event_title: item.event_title.clone(),
        tier_name: item.tier_name.clone(),
        venue_name: item.venue_name.clone(),
        event_cover: item.event_cover.clone(),
        event_date: None,
        quantity,
        unit_price: item.unit_price,
        unit_price_snapshot: item.unit_price,
        subtotal: item.unit_price * quantity as i64,
        available: i32::MAX,
        max_per_order: None,
        exceeds_stock: false,
        price_changed: false,
        selected: true,
    }
}

/// SSR-specific PendingOrderCtx (lebih lengkap dari CSR versi order_created.rs).
/// CSR order_created.rs punya PendingOrderCtx sendiri — keduanya di-provide karena
/// komponen berbeda menggunakan tipe berbeda.
#[derive(Clone, Copy)]
pub struct PendingOrderCtx {
    pub pending_order: RwSignal<Option<OrderRef>>,
    pub success_order: RwSignal<Option<SuccessSnapshot>>,
}

/// Context untuk subscription checkout — diisi subscription page, dibaca checkout page.
#[derive(Clone, Copy)]
pub struct PendingSubCtx {
    pub order: RwSignal<Option<PendingSubOrder>>,
}
