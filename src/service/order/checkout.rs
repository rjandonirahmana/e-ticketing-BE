//! service/order/checkout.rs — checkout dari keranjang yang tersimpan di database.
//!
//! Bentuknya mengikuti `POST /order/create` milik kiddoapi: permintaan dari
//! klien HANYA menyebut kanal pembayaran (dan opsional kode promo). Isi
//! keranjang, harga satuan, potongan, dan biaya admin semuanya dibaca/dihitung
//! server. Tak ada satu pun angka rupiah yang datang dari browser.
//!
//! Urutan yang dijalankan, dan alasan urutannya:
//!
//!   1. Baca keranjang lewat `CartService::view` — sekalian membuang barang yang
//!      sudah tak bisa dibeli, jadi kegagalan yang bisa diketahui lebih awal
//!      tidak menunggu sampai transaksi.
//!   2. Tolak bila ada baris yang jumlahnya melebihi sisa stok. Server TIDAK
//!      memotong jumlah diam-diam: pembeli yang berhak memutuskan.
//!   3. Ambil jatah kuota promo SEBELUM order dibuat. Kalau order gagal lahir,
//!      jatahnya dikembalikan. Urutan sebaliknya (order dulu, kuota belakangan)
//!      akan memberi potongan pada promo yang kuotanya sudah habis.
//!   4. Buat order lewat jalur `create_inner` yang sama dengan jalur lain —
//!      artinya penguncian varian, penjaga oversell, retry, dan idempotensi
//!      berlaku persis sama di sini.
//!   5. Kanal yang lunas seketika (`is_instant`, atau order nol rupiah)
//!      langsung dibayar sehingga tiketnya terbit tanpa langkah tambahan.

use rust_decimal::Decimal;

use crate::models::cart::CartView;
use crate::models::orders::{
    CheckoutRequest, CreateOrderItemRequest, CreateOrderRequest, OrderDetailResponse,
    PayOrderRequest,
};
use crate::models::payment::{PaymentMethod, Promo};
use crate::utils::error::{AppError, AppResult};
use crate::utils::ulid::id_to_vec;
use validator::Validate;

use super::OrderService;

// ── Perhitungan harga ────────────────────────────────────────────────────────

/// Bahan-bahan perhitungan harga yang dibawa masuk ke dalam transaksi order.
///
/// Yang disimpan di sini adalah ATURAN-nya (promo mana, kanal mana), bukan
/// hasilnya — hasilnya dihitung dari subtotal yang baru dikunci di dalam
/// transaksi, lihat [`CheckoutPricing::compute`].
pub struct CheckoutPricing<'a> {
    pub cart_bytes: Option<Vec<u8>>,
    pub method: &'a PaymentMethod,
    pub promo: Option<&'a Promo>,
}

pub struct PriceBreakdown {
    pub discount: Decimal,
    pub charge: Decimal,
    pub total: Decimal,
    pub promo_code: Option<String>,
    pub payment_expired_at: Option<chrono::DateTime<chrono::Utc>>,
    pub reference: Option<String>,
}

impl CheckoutPricing<'_> {
    pub fn compute(&self, subtotal: Decimal) -> PriceBreakdown {
        let discount = self
            .promo
            .map(|p| p.discount_for(subtotal))
            .unwrap_or(Decimal::ZERO);

        let after_discount = subtotal - discount;
        let charge = self.method.charge_for(after_discount);

        PriceBreakdown {
            discount,
            charge,
            total: after_discount + charge,
            promo_code: self.promo.map(|p| p.code.clone()),
            payment_expired_at: payment_deadline(self.method),
            // Nomor VA baru bisa dibentuk setelah order punya kode; diisi
            // setelah order dibuat (lihat `checkout`). Di sini sengaja None.
            reference: None,
        }
    }
}

/// Batas waktu bayar menurut sifat kanal.
///
/// Sengaja TIDAK sama dengan `orders.expired_at` yang menahan stok (2 jam):
/// stok harus dilepas cepat supaya tak ada tiket yang tersandera, sementara
/// tenggat kanal mengikuti kebiasaan kanalnya. Menyatukan keduanya berarti
/// salah satu dari dua janji itu pasti dilanggar.
fn payment_deadline(m: &PaymentMethod) -> Option<chrono::DateTime<chrono::Utc>> {
    if m.is_instant {
        return None;
    }
    let now = chrono::Utc::now();
    let dur = match m.category.as_str() {
        "va" => chrono::Duration::hours(2),
        "ewallet" | "qris" => chrono::Duration::minutes(30),
        _ => chrono::Duration::hours(1),
    };
    Some(now + dur)
}

/// Nomor Virtual Account: awalan bank + 8 digit turunan kode order.
///
/// Deterministik dari `order_code`, jadi nomor yang sama muncul lagi saat
/// halaman instruksi dibuka ulang — pembeli yang sudah menyalin nomornya tak
/// pernah menemukan nomor berbeda pada kunjungan berikutnya.
fn build_reference(method: &PaymentMethod, order_code: &str) -> Option<String> {
    match method.category.as_str() {
        "va" => {
            let digits: String = order_code
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .map(|c| {
                    if c.is_ascii_digit() {
                        c
                    } else {
                        // Huruf dipetakan ke angka agar panjangnya tetap.
                        char::from_digit((c.to_ascii_uppercase() as u32 - 'A' as u32) % 10, 10)
                            .unwrap_or('0')
                    }
                })
                .collect();
            let tail: String = digits.chars().rev().take(8).collect::<Vec<_>>()
                .into_iter().rev().collect();
            Some(format!("{}{}", method.va_prefix, tail))
        }
        "qris" => Some(order_code.to_string()),
        _ => None,
    }
}

// ── Checkout ─────────────────────────────────────────────────────────────────

impl OrderService {
    /// Ubah keranjang menjadi order.
    pub async fn checkout(
        &self,
        customer_id: &str,
        user_name: &str,
        req: CheckoutRequest,
        is_premium: bool,
    ) -> AppResult<OrderDetailResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        // 1) Keranjang — sudah dibersihkan dari barang yang tak bisa dibeli.
        let cart = self.cart.view(customer_id, is_premium).await?;
        if cart.items.is_empty() {
            return Err(AppError::BadRequest("Keranjang Anda kosong".into()));
        }

        // Hanya barang yang DICENTANG yang dibeli. Sisanya tetap di keranjang —
        // transaksi order memindahkannya ke keranjang baru sebelum keranjang
        // ini ditutup (lihat `OrderTx::rescue_unselected`).
        let chosen: Vec<&crate::models::cart::CartItemView> =
            cart.items.iter().filter(|i| i.selected).collect();
        if chosen.is_empty() {
            return Err(AppError::BadRequest(
                "Pilih dulu barang yang ingin dibayar".into(),
            ));
        }

        // 2) Stok kurang → tolak dengan menyebut barangnya, jangan potong diam-diam.
        let short: Vec<String> = chosen
            .iter()
            .filter(|i| i.exceeds_stock)
            .map(|i| format!("{} ({} tersisa)", i.variant_name, i.available.max(0)))
            .collect();
        if !short.is_empty() {
            return Err(AppError::BadRequest(format!(
                "Stok tidak mencukupi untuk: {}",
                short.join(", ")
            )));
        }

        // 3) Kanal pembayaran.
        let method = self.payment.find(&req.payment_code).await?;

        // 4) Promo: kode dari permintaan mengalahkan yang menempel di keranjang.
        let wanted = req
            .promo_code
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| cart.promo_code.clone());

        let mut promo_model: Option<Promo> = None;
        if let Some(code) = wanted.as_deref() {
            let check = self
                .payment
                .validate_promo(
                    customer_id,
                    code,
                    cart.subtotal,
                    cart.total_quantity,
                    is_premium,
                    Some(&method.code),
                )
                .await?;

            if check.valid {
                promo_model = self.payment.promo_model(code).await?;
            } else if req.promo_code.is_some() {
                // Kode yang DISEBUT pemanggil dan ternyata tak berlaku adalah
                // kesalahan yang harus terlihat; kode warisan dari keranjang
                // cukup digugurkan diam-diam (`CartService::view` sudah
                // melepasnya dan menaruh alasannya di `promo_message`).
                return Err(AppError::BadRequest(check.message));
            }
        }

        // 5) Batas nominal kanal, diperiksa atas nilai yang akan ditagihkan.
        let estimated_discount = promo_model
            .as_ref()
            .map(|p| p.discount_for(cart.subtotal))
            .unwrap_or(Decimal::ZERO);
        let after_discount = cart.subtotal - estimated_discount;
        if !method.accepts(after_discount) {
            return Err(AppError::BadRequest(format!(
                "{} tidak melayani nominal ini",
                method.name
            )));
        }

        // 6) Ambil jatah kuota promo lebih dulu; kembalikan bila order gagal.
        let reserved_promo = match promo_model.as_ref() {
            Some(p) if p.quota_total > 0 => {
                if !self.payment.reserve_quota(p.id).await? {
                    return Err(AppError::BadRequest("Kuota promo sudah habis".into()));
                }
                Some(p.id)
            }
            Some(p) => Some(p.id),
            None => None,
        };

        // 7) Buat order lewat jalur yang sama dengan pembelian langsung.
        let items: Vec<CreateOrderItemRequest> = chosen
            .iter()
            .map(|i| CreateOrderItemRequest {
                ticket_variant_id: i.ticket_variant_id.clone(),
                quantity: i.quantity,
            })
            .collect();

        let cart_bytes = id_to_vec(&cart.cart_id).ok();
        let pricing = CheckoutPricing {
            cart_bytes,
            method: &method,
            promo: promo_model.as_ref(),
        };

        let create_req = CreateOrderRequest {
            idempotency_key: req.idempotency_key.clone(),
            items,
        };

        let created = match self
            .create_inner(customer_id, create_req, is_premium, Some(&pricing))
            .await
        {
            Ok(o) => o,
            Err(e) => {
                if let (Some(id), Some(p)) = (reserved_promo, promo_model.as_ref()) {
                    if p.quota_total > 0 {
                        if let Err(re) = self.payment.release_quota(id).await {
                            tracing::warn!(error = %re, promo_id = id, "gagal mengembalikan kuota promo");
                        }
                    }
                }
                return Err(e);
            }
        };

        // 8) Catat pemakaian promo (penegak `per_user_limit` untuk order berikutnya).
        if let (Some(id), true) = (reserved_promo, created.discount_amount > Decimal::ZERO) {
            if let Err(e) = self
                .payment
                .record_redemption(id, customer_id, &created.id, created.discount_amount)
                .await
            {
                tracing::warn!(error = %e, order_id = %created.id, "gagal mencatat pemakaian promo");
            }
        }

        // Keranjang sudah ditutup DI DALAM transaksi order (lihat
        // `create_in_tx`), jadi tak ada langkah tambahan di sini. Menutupnya
        // setelah commit seperti versi sebelumnya menyisakan celah: bila proses
        // mati di antara keduanya, keranjang tetap terbuka padahal ordernya
        // sudah lahir, dan pembeli melihat tiket yang sama dua kali.

        // 10) Nomor VA / referensi QRIS baru bisa dibentuk setelah kode order ada.
        let reference = build_reference(&method, &created.order_code);
        if let Some(ref r) = reference {
            if let Err(e) = self.repo_set_reference(&created.id, r).await {
                tracing::warn!(error = %e, order_id = %created.id, "gagal menyimpan nomor pembayaran");
            }
        }

        // 11) Kanal yang lunas seketika (tunai di lokasi) dan order nol rupiah
        //     langsung dibayar, supaya tiketnya terbit tanpa langkah tambahan.
        let instant = method.is_instant || created.total_amount <= Decimal::ZERO;
        let mut result = if instant {
            let order_id = created.id.clone();
            match self
                .pay(
                    &order_id,
                    customer_id,
                    user_name,
                    PayOrderRequest {
                        payment_method: method.code.clone(),
                    },
                )
                .await
            {
                Ok(paid) => paid,
                Err(e) => {
                    // Ordernya SUDAH lahir dan tercatat, jadi mengembalikan
                    // error di sini akan menampilkan kegagalan atas sesuatu
                    // yang sebenarnya berhasil. Yang dikembalikan adalah
                    // keadaan sebenarnya: order menunggu pembayaran.
                    //
                    // Yang TIDAK boleh adalah diam. Versi sebelumnya memakai
                    // `unwrap_or(created)`, sehingga pembayaran yang gagal
                    // tak meninggalkan jejak apa pun — dan order tunai yang
                    // semestinya lunas seketika mengendap sebagai pending
                    // tanpa ada yang tahu. Dicatat `error!` supaya terlihat
                    // di alert dan bisa direkonsiliasi.
                    tracing::error!(
                        error = %e,
                        order_id = %order_id,
                        payment_code = %method.code,
                        "pembayaran instan GAGAL — order tertinggal pending"
                    );
                    created
                }
            }
        } else {
            created
        };

        result.payment_reference = reference;
        result.payment_name = Some(method.name.clone());
        result.payment_instruction = Some(method.instruction.clone());
        Ok(result)
    }

    /// Lengkapi respons order dengan nama & instruksi kanalnya. Data itu tinggal
    /// di `payment_methods`, jadi halaman detail order tak perlu memetakan kode
    /// kanal ke nama yang enak dibaca sendiri (dan tak bisa jadi tidak sinkron).
    pub async fn enrich_payment(&self, mut order: OrderDetailResponse) -> OrderDetailResponse {
        let code = match order.payment_code.clone().or_else(|| order.payment_method.clone()) {
            Some(c) if !c.is_empty() => c,
            _ => return order,
        };
        if let Ok(m) = self.payment.find(&code).await {
            // Nomor VA ditulis SETELAH transaksi order commit, jadi ada jendela
            // sempit di mana proses bisa mati dan meninggalkan order tanpa
            // nomor pembayaran — pembeli menerima pesanan tanpa cara membayarnya.
            //
            // Ketimbang memperlebar transaksi, kekosongan itu ditambal di sini:
            // nomornya deterministik dari `order_code`, jadi menghitungnya ulang
            // saat halaman dibuka menghasilkan nomor yang SAMA dengan yang tadi
            // gagal tersimpan. Sekalian disimpan supaya tak dihitung terus.
            if order.payment_reference.as_deref().unwrap_or("").is_empty() {
                if let Some(r) = build_reference(&m, &order.order_code) {
                    if let Err(e) = self.repo_set_reference(&order.id, &r).await {
                        tracing::warn!(error = %e, order_id = %order.id,
                            "gagal menambal nomor pembayaran");
                    }
                    order.payment_reference = Some(r);
                }
            }

            order.payment_name = Some(m.name);
            order.payment_instruction = Some(m.instruction);
        }
        order
    }

    /// Simpan nomor VA / referensi QRIS pada order.
    async fn repo_set_reference(&self, order_id: &str, reference: &str) -> AppResult<()> {
        let id = id_to_vec(order_id).map_err(AppError::Internal)?;
        crate::repository::db::exec_drop(
            &self.pool,
            "UPDATE orders SET payment_reference = $2, updated_at = NOW() WHERE id = $1",
            &[&id, &reference],
        )
        .await
        .map_err(AppError::Internal)?;
        Ok(())
    }
}

/// Ringkasan keranjang untuk halaman checkout: total tiket, potongan, dan
/// biaya kanal untuk SETIAP kanal yang tersedia — dihitung server sekali jalan
/// sehingga halaman tak perlu menghitung ulang saat pilihan berpindah.
pub struct CheckoutSummary {
    pub cart: CartView,
    pub methods: Vec<(PaymentMethod, Decimal, Decimal)>,
}
