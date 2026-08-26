//! service/cart.rs — aturan keranjang.
//!
//! Perhatikan bahwa `view()` MENULIS ke database, bukan sekadar membaca. Itu
//! disengaja dan mengikuti `GET /cart/view` milik kiddoapi: setiap kali
//! keranjang dibuka, barang yang sudah tak bisa dibeli dibuang dan pemiliknya
//! diberi tahu lewat `notif`. Alternatifnya — membiarkan barang mati mengendap
//! sampai checkout — memindahkan kabar buruk ke detik paling mahal, saat
//! pembeli sudah memilih metode pembayaran.
//!
//! Yang TIDAK dilakukan di sini: memotong jumlah barang secara diam-diam ketika
//! stok tinggal sedikit. Barisnya ditandai `exceeds_stock` dan tombol bayar
//! dikunci di halaman; pembeli yang memutuskan, bukan server.

use std::sync::Arc;

use rust_decimal::Decimal;
use validator::Validate;

use crate::models::cart::{
    Cart, CartItemView, CartView, SaveCartRequest, UpdateCartItemRequest,
};
use crate::repository::cart::{CartItemRow, CartRepository};
use crate::service::payment::PaymentService;
use crate::utils::error::{AppError, AppResult};

pub struct CartService {
    repo: Arc<dyn CartRepository>,
    payment: Arc<PaymentService>,
}

impl CartService {
    pub fn new(repo: Arc<dyn CartRepository>, payment: Arc<PaymentService>) -> Self {
        Self { repo, payment }
    }

    // ── Baca ─────────────────────────────────────────────────────────────────

    /// Isi keranjang lengkap dengan ringkasan harga, sesudah pembersihan barang
    /// mati dan pemeriksaan ulang promo.
    pub async fn view(&self, user_id: &str, is_premium: bool) -> AppResult<CartView> {
        let cart = self.repo.get_or_create(user_id).await?;
        let removed = self.repo.prune_dead_items(&cart.id).await?;
        let rows = self.repo.list_items(&cart.id).await?;
        self.build_view(user_id, cart, rows, removed, is_premium).await
    }

    /// Jumlah tiket di keranjang — untuk lencana di navigasi. Sengaja tidak
    /// memanggil `view()`: lencana muncul di setiap halaman, dan menyeret
    /// pembersihan + validasi promo ke setiap render adalah harga yang tak
    /// sebanding dengan satu angka.
    pub async fn count(&self, user_id: &str) -> AppResult<i64> {
        Ok(self.repo.count_items(user_id).await?)
    }

    // ── Tulis ────────────────────────────────────────────────────────────────

    /// Masukkan tiket ke keranjang (menambah jumlah bila varian sudah ada).
    pub async fn add(
        &self,
        user_id: &str,
        variant_id: &str,
        quantity: i32,
        is_premium: bool,
    ) -> AppResult<CartView> {
        if quantity < 1 {
            return Err(AppError::BadRequest("Jumlah tiket minimal 1".into()));
        }
        let cart = self.repo.get_or_create(user_id).await?;
        let added = self
            .repo
            .upsert_item(&cart.id, variant_id, quantity, false)
            .await?;
        if added.is_none() {
            return Err(AppError::BadRequest(
                "Tiket tidak tersedia atau sudah tidak dijual".into(),
            ));
        }

        let view = self.view(user_id, is_premium).await?;

        // Barang yang baru saja dimasukkan bisa langsung dibuang lagi oleh
        // pembersihan di `view()` -- misalnya stoknya ternyata habis. Kalau itu
        // terjadi, JANGAN kembalikan keranjang yang terlihat normal: dari sisi
        // pembeli, barangnya seolah lenyap tanpa sebab. Kembalikan alasannya.
        if !view.items.iter().any(|i| i.ticket_variant_id == variant_id) {
            let alasan = if view.notif.is_empty() {
                "Tiket tidak tersedia atau sudah tidak dijual".to_string()
            } else {
                view.notif.clone()
            };
            return Err(AppError::BadRequest(alasan));
        }

        Ok(view)
    }

    /// Tetapkan jumlah sebuah baris. `quantity = 0` menghapus barisnya —
    /// perilaku yang sama dengan `CartContext::update_qty` di sisi web, supaya
    /// tombol "−" pada baris terakhir tak perlu memanggil endpoint berbeda.
    pub async fn update_quantity(
        &self,
        user_id: &str,
        req: UpdateCartItemRequest,
        is_premium: bool,
    ) -> AppResult<CartView> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        let cart = self.repo.get_or_create(user_id).await?;

        if req.quantity == 0 {
            self.repo.remove_item(&cart.id, &req.ticket_variant_id).await?;
        } else {
            let n = self
                .repo
                .set_quantity(&cart.id, &req.ticket_variant_id, req.quantity)
                .await?;
            // Baris belum ada (mis. tab lain sudah menghapusnya) → perlakukan
            // sebagai penambahan, bukan diam-diam tak melakukan apa pun.
            if n == 0 {
                self.repo
                    .upsert_item(&cart.id, &req.ticket_variant_id, req.quantity, true)
                    .await?;
            }
        }

        self.view(user_id, is_premium).await
    }

    pub async fn remove(
        &self,
        user_id: &str,
        variant_id: &str,
        is_premium: bool,
    ) -> AppResult<CartView> {
        let cart = self.repo.get_or_create(user_id).await?;
        self.repo.remove_item(&cart.id, variant_id).await?;
        self.view(user_id, is_premium).await
    }

    /// Tandai satu baris (atau seluruh isi keranjang bila `variant_id` kosong)
    /// ikut atau tidak ikut dibayar.
    pub async fn set_selected(
        &self,
        user_id: &str,
        variant_id: Option<&str>,
        selected: bool,
        is_premium: bool,
    ) -> AppResult<CartView> {
        let cart = self.repo.get_or_create(user_id).await?;
        match variant_id {
            Some(v) => self.repo.set_selected(&cart.id, v, selected).await?,
            None => self.repo.set_all_selected(&cart.id, selected).await?,
        };
        self.view(user_id, is_premium).await
    }

    pub async fn clear(&self, user_id: &str, is_premium: bool) -> AppResult<CartView> {
        let cart = self.repo.get_or_create(user_id).await?;
        self.repo.clear_items(&cart.id).await?;
        self.repo
            .update_meta(&cart.id, None, Decimal::ZERO, None, Some("cart"))
            .await?;
        self.view(user_id, is_premium).await
    }

    /// Simpan keranjang sekaligus — padanan `POST /cart/create` kiddoapi.
    ///
    /// `replace = true` menimpa seluruh isi (halaman keranjang menyimpan
    /// keadaannya). `replace = false` menggabungkan — inilah yang dipakai saat
    /// pengunjung yang tadinya belum masuk akhirnya login: keranjang tamu di
    /// `localStorage` dituang ke keranjang miliknya tanpa menghapus apa pun
    /// yang sudah ada di sana dari perangkat lain.
    pub async fn save(
        &self,
        user_id: &str,
        req: SaveCartRequest,
        is_premium: bool,
    ) -> AppResult<CartView> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        let cart = self.repo.get_or_create(user_id).await?;

        if req.replace {
            self.repo.clear_items(&cart.id).await?;
        }

        // Varian yang ditolak database (tak aktif / product tutup) dilewati, tidak
        // menggagalkan seluruh penyimpanan: satu tiket kedaluwarsa di keranjang
        // tamu tak boleh membuat sisa keranjangnya ikut hilang saat login.
        let mut skipped = Vec::new();
        for item in &req.items {
            let saved = self
                .repo
                .upsert_item(&cart.id, &item.ticket_variant_id, item.quantity, req.replace)
                .await?;
            if saved.is_none() {
                skipped.push(item.ticket_variant_id.clone());
            }
        }

        if !skipped.is_empty() {
            tracing::info!(
                user_id,
                count = skipped.len(),
                "save cart: sebagian varian dilewati (tak aktif / product tutup)"
            );
        }

        // Field yang TIDAK disebut permintaan dibiarkan apa adanya. Ini penting
        // untuk penggabungan setelah login: `sync_guest_cart` hanya mengirim
        // daftar tiket, dan tanpa aturan ini kode promo yang dipasang pengguna
        // di perangkat lain akan terhapus hanya karena ia membuka situs di
        // komputer baru.
        let promo_owned = req
            .promo_code
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| cart.promo_code.clone());
        let payment_owned = req
            .payment_code
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| cart.payment_code.clone());

        let promo = promo_owned.as_deref();
        let payment = payment_owned.as_deref();

        self.repo
            .update_meta(
                &cart.id,
                promo,
                // Nilainya dihitung ulang di `build_view`; menyimpan 0 di sini
                // mencegah diskon lama menempel pada keranjang yang isinya
                // sudah berganti.
                Decimal::ZERO,
                payment,
                req.position.as_deref(),
            )
            .await?;

        self.view(user_id, is_premium).await
    }

    /// Pasang / lepas kode promo. `code = None` melepas.
    pub async fn set_promo(
        &self,
        user_id: &str,
        code: Option<&str>,
        is_premium: bool,
    ) -> AppResult<CartView> {
        let cart = self.repo.get_or_create(user_id).await?;
        let code = code.map(str::trim).filter(|s| !s.is_empty());
        self.repo
            .update_meta(&cart.id, code, Decimal::ZERO, cart.payment_code.as_deref(), None)
            .await?;
        self.view(user_id, is_premium).await
    }

    /// Simpan kanal pembayaran pilihan user supaya tidak hilang saat berpindah
    /// halaman atau perangkat.
    pub async fn set_payment(
        &self,
        user_id: &str,
        payment_code: Option<&str>,
        is_premium: bool,
    ) -> AppResult<CartView> {
        let cart = self.repo.get_or_create(user_id).await?;
        let code = payment_code.map(str::trim).filter(|s| !s.is_empty());
        if let Some(c) = code {
            // Divalidasi ke database supaya kode karangan tak pernah tersimpan.
            self.payment.find(c).await?;
        }
        self.repo
            .update_meta(
                &cart.id,
                cart.promo_code.as_deref(),
                cart.discount_amount,
                code,
                Some("payment"),
            )
            .await?;
        self.view(user_id, is_premium).await
    }

    /// Tutup keranjang setelah menjadi order (soft delete). Dipanggil dari
    /// jalur checkout; keranjang baru lahir sendiri saat halaman dibuka lagi.
    pub async fn close(&self, cart_id: &str) -> AppResult<()> {
        self.repo.close(cart_id).await?;
        Ok(())
    }

    /// Keranjang aktif apa adanya — dipakai jalur checkout yang butuh `cart.id`
    /// dan kode promo yang sedang menempel.
    pub async fn active_cart(&self, user_id: &str) -> AppResult<Cart> {
        Ok(self.repo.get_or_create(user_id).await?)
    }

    // ── Perakitan tampilan ───────────────────────────────────────────────────

    async fn build_view(
        &self,
        user_id: &str,
        cart: Cart,
        rows: Vec<CartItemRow>,
        removed: Vec<String>,
        is_premium: bool,
    ) -> AppResult<CartView> {
        let mut items = Vec::with_capacity(rows.len());
        let mut subtotal = Decimal::ZERO;
        let mut total_quantity = 0_i32;
        let mut cart_quantity = 0_i32;

        for r in rows {
            let line = r.unit_price * Decimal::from(r.quantity);
            cart_quantity += r.quantity;

            // Hanya baris terpilih yang masuk hitungan harga. Baris lain tetap
            // ditampilkan, tetapi tidak menaikkan tagihan dan tidak ikut
            // menentukan kelayakan promo.
            if r.selected {
                subtotal += line;
                total_quantity += r.quantity;
            }

            items.push(CartItemView {
                id: r.id,
                ticket_variant_id: r.ticket_variant_id,
                event_id: r.event_id,
                event_slug: r.event_slug,
                quantity: r.quantity,
                unit_price_snapshot: r.unit_price_snapshot,
                unit_price: r.unit_price,
                subtotal: line,
                event_name: r.event_name,
                variant_name: r.variant_name,
                venue: r.venue,
                cover_url: r.cover_url,
                event_date: r.event_date,
                available: r.available,
                max_per_order: r.max_per_order,
                exceeds_stock: r.quantity > r.available,
                price_changed: r.unit_price != r.unit_price_snapshot,
                selected: r.selected,
            });
        }

        // ── Promo: selalu dihitung ulang ────────────────────────────────────
        // Kode yang sah kemarin bisa gugur hari ini karena isi keranjang
        // berubah, kuotanya habis, atau masa berlakunya lewat. Menyimpan angka
        // diskon tanpa memeriksanya ulang berarti menagih dengan potongan yang
        // sudah tak berlaku.
        let mut discount = Decimal::ZERO;
        let mut promo_message = String::new();
        let mut promo_code = cart.promo_code.clone();

        if let Some(code) = cart.promo_code.as_deref() {
            let check = self
                .payment
                .validate_promo(
                    user_id,
                    code,
                    subtotal,
                    total_quantity,
                    is_premium,
                    cart.payment_code.as_deref(),
                )
                .await?;

            if check.valid {
                discount = check.discount;
                promo_message = check.message;
            } else {
                promo_message = check.message;
                promo_code = None;
                // Lepaskan promo yang gugur dari keranjang supaya ringkasan
                // harga di semua perangkat langsung sepakat.
                self.repo
                    .update_meta(
                        &cart.id,
                        None,
                        Decimal::ZERO,
                        cart.payment_code.as_deref(),
                        None,
                    )
                    .await?;
            }
        }

        // Simpan hasil hitungan hanya bila memang berbeda — menghindari satu
        // UPDATE per pembukaan halaman keranjang.
        if promo_code.is_some() && discount != cart.discount_amount {
            self.repo
                .update_meta(
                    &cart.id,
                    promo_code.as_deref(),
                    discount,
                    cart.payment_code.as_deref(),
                    None,
                )
                .await?;
        }

        let notif = if removed.is_empty() {
            String::new()
        } else {
            format!(
                "{} tidak lagi tersedia dan telah dihapus dari keranjang Anda.",
                removed.join(", ")
            )
        };

        Ok(CartView {
            cart_id: cart.id,
            items,
            subtotal,
            discount,
            promo_code,
            promo_message,
            payment_code: cart.payment_code,
            position: cart.position,
            total_quantity,
            cart_quantity,
            total: subtotal - discount,
            notif,
        })
    }
}
