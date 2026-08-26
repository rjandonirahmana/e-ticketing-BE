//! service/payment.rs — kanal pembayaran & aturan promo.
//!
//! Semua angka yang menyangkut uang lahir di sini, bukan di browser: daftar
//! kanal beserta biayanya dibaca dari database, dan potongan promo dihitung
//! ulang setiap kali keranjang dibaca maupun saat order dibuat. Halaman
//! checkout hanya menampilkan apa yang sudah diputuskan server.

use std::sync::Arc;

use rust_decimal::Decimal;

use crate::models::payment::{PaymentMethod, Promo, PromoCheck};
use crate::repository::payment::PaymentRepository;
use crate::utils::error::{AppError, AppResult};

pub struct PaymentService {
    repo: Arc<dyn PaymentRepository>,
}

impl PaymentService {
    pub fn new(repo: Arc<dyn PaymentRepository>) -> Self {
        Self { repo }
    }

    // ── Kanal ────────────────────────────────────────────────────────────────

    /// Kanal yang benar-benar bisa dipakai untuk nominal ini.
    ///
    /// Dua penyaringan yang sengaja dilakukan di server:
    ///   • rentang nominal (`min_amount`/`max_amount`) — kanal e-wallet punya
    ///     plafon, dan menawarkannya untuk order di atas plafon hanya membuat
    ///     pembeli gagal di langkah terakhir;
    ///   • `allow_promo` — mengikuti kiddoapi yang menyembunyikan kanal
    ///     ber-promo begitu keranjang memakai kode promo, supaya dua potongan
    ///     tak bertumpuk tanpa disengaja.
    pub async fn list_for(&self, amount: Decimal, has_promo: bool) -> AppResult<Vec<PaymentMethod>> {
        let all = self.repo.list_methods().await?;
        Ok(all
            .into_iter()
            .filter(|m| m.accepts(amount))
            .filter(|m| !has_promo || m.allow_promo)
            .collect())
    }

    pub async fn list_all(&self) -> AppResult<Vec<PaymentMethod>> {
        Ok(self.repo.list_methods().await?)
    }

    pub async fn find(&self, code: &str) -> AppResult<PaymentMethod> {
        self.repo
            .find_method(code)
            .await?
            .ok_or_else(|| AppError::BadRequest(format!("Metode pembayaran '{code}' tidak tersedia")))
    }

    // ── Promo ────────────────────────────────────────────────────────────────

    /// Periksa kode promo terhadap satu keranjang.
    ///
    /// Selalu mengembalikan `PromoCheck` (bukan error) untuk kode yang ditolak:
    /// promo yang tak berlaku adalah jawaban yang sah bagi halaman checkout,
    /// bukan kegagalan sistem. Error hanya untuk kegagalan database.
    #[allow(clippy::too_many_arguments)]
    pub async fn validate_promo(
        &self,
        user_id: &str,
        code: &str,
        subtotal: Decimal,
        total_qty: i32,
        is_premium: bool,
        payment_code: Option<&str>,
    ) -> AppResult<PromoCheck> {
        let code = code.trim();
        if code.is_empty() {
            return Ok(PromoCheck::invalid(code, "Kode promo kosong"));
        }

        let promo = match self.repo.find_promo(code).await? {
            Some(p) => p,
            None => return Ok(PromoCheck::invalid(code, "Kode promo tidak ditemukan")),
        };

        if let Some(reason) = Self::eligibility_error(&promo, subtotal, total_qty, is_premium, payment_code) {
            return Ok(PromoCheck::invalid(code, reason));
        }

        if promo.quota_total > 0 && promo.quota_used >= promo.quota_total {
            return Ok(PromoCheck::invalid(code, "Kuota promo sudah habis"));
        }

        if promo.per_user_limit > 0 {
            let used = self
                .repo
                .count_user_redemptions(promo.id, user_id)
                .await?;
            if used >= promo.per_user_limit as i64 {
                return Ok(PromoCheck::invalid(
                    code,
                    "Anda sudah memakai promo ini sebanyak batas yang diizinkan",
                ));
            }
        }

        let discount = promo.discount_for(subtotal);
        if discount <= Decimal::ZERO {
            return Ok(PromoCheck::invalid(code, "Promo tidak memberi potongan untuk keranjang ini"));
        }

        Ok(PromoCheck {
            valid: true,
            code: promo.code.clone(),
            discount,
            message: if promo.name.is_empty() {
                "Promo berhasil dipakai".into()
            } else {
                promo.name.clone()
            },
            promo_id: Some(promo.id),
        })
    }

    /// Syarat kelayakan yang murni perhitungan — dipisah supaya bisa diuji
    /// tanpa database dan supaya urutan pemeriksaannya terbaca sekaligus.
    fn eligibility_error(
        promo: &Promo,
        subtotal: Decimal,
        total_qty: i32,
        is_premium: bool,
        payment_code: Option<&str>,
    ) -> Option<String> {
        let now = chrono::Utc::now();
        if now < promo.starts_at {
            return Some("Promo belum berlaku".into());
        }
        if let Some(end) = promo.ends_at {
            if now > end {
                return Some("Promo sudah kedaluwarsa".into());
            }
        }
        if promo.premium_only && !is_premium {
            return Some("Promo khusus pengguna premium".into());
        }
        if subtotal < promo.min_cart_amount {
            return Some(format!(
                "Minimum belanja {} untuk memakai promo ini",
                fmt_idr(promo.min_cart_amount)
            ));
        }
        if promo.max_cart_amount > Decimal::ZERO && subtotal > promo.max_cart_amount {
            return Some(format!(
                "Promo hanya untuk belanja sampai {}",
                fmt_idr(promo.max_cart_amount)
            ));
        }
        if promo.min_qty > 0 && total_qty < promo.min_qty {
            return Some(format!("Minimum {} tiket untuk memakai promo ini", promo.min_qty));
        }
        if promo.max_qty > 0 && total_qty > promo.max_qty {
            return Some(format!("Promo hanya untuk maksimal {} tiket", promo.max_qty));
        }
        if let (Some(allowed), Some(chosen)) = (promo.payment_codes.as_ref(), payment_code) {
            if !allowed.is_empty() && !allowed.iter().any(|c| c == chosen) {
                return Some("Promo tidak berlaku untuk metode pembayaran ini".into());
            }
        }
        None
    }

    /// Ambil promo apa adanya — dipakai jalur checkout untuk MENGHITUNG ULANG
    /// potongan dari subtotal yang baru dikunci di dalam transaksi, bukan dari
    /// subtotal yang tadi ditampilkan di halaman.
    pub async fn promo_model(&self, code: &str) -> AppResult<Option<Promo>> {
        Ok(self.repo.find_promo(code).await?)
    }

    /// Ambil satu jatah kuota sebelum order dibuat. Dipanggil dari jalur
    /// checkout; bila order gagal lahir, `release_quota` mengembalikannya.
    pub async fn reserve_quota(&self, promo_id: i64) -> AppResult<bool> {
        Ok(self.repo.reserve_promo_quota(promo_id).await?)
    }

    pub async fn release_quota(&self, promo_id: i64) -> AppResult<()> {
        self.repo.release_promo_quota(promo_id).await?;
        Ok(())
    }

    pub async fn record_redemption(
        &self,
        promo_id: i64,
        user_id: &str,
        order_id: &str,
        discount: Decimal,
    ) -> AppResult<()> {
        self.repo
            .record_redemption(promo_id, user_id, order_id, discount)
            .await?;
        Ok(())
    }
}

/// Format rupiah untuk PESAN KESALAHAN saja (mis. "Minimum belanja Rp150.000").
/// Tampilan harga di halaman punya pemformatnya sendiri di sisi web.
fn fmt_idr(v: Decimal) -> String {
    let n = v.trunc().to_string();
    let mut out = String::with_capacity(n.len() + 6);
    for (i, ch) in n.chars().enumerate() {
        if i > 0 && (n.len() - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(ch);
    }
    format!("Rp{out}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn promo(discount_type: &str, amount: Decimal, max_discount: Decimal) -> Promo {
        Promo {
            id: 1,
            code: "TEST".into(),
            name: "Uji".into(),
            discount_type: discount_type.into(),
            amount,
            max_discount,
            min_cart_amount: Decimal::ZERO,
            max_cart_amount: Decimal::ZERO,
            min_qty: 0,
            max_qty: 0,
            quota_total: 0,
            quota_used: 0,
            per_user_limit: 0,
            premium_only: false,
            payment_codes: None,
            starts_at: chrono::Utc::now() - chrono::Duration::days(1),
            ends_at: None,
        }
    }

    /// Potongan persen tunduk pada plafonnya. Tanpa plafon yang benar-benar
    /// mengikat, promo 10% pada order besar diam-diam berubah jadi hadiah.
    #[test]
    fn persen_dibatasi_plafon() {
        let p = promo("percent", dec!(10), dec!(50000));
        assert_eq!(p.discount_for(dec!(300000)), dec!(30000));
        assert_eq!(p.discount_for(dec!(900000)), dec!(50000));
    }

    /// Potongan tak pernah melebihi belanjanya — order tak boleh berakhir
    /// dengan total negatif, betapa pun besar kode promonya.
    #[test]
    fn potongan_tak_melebihi_subtotal() {
        let p = promo("fixed", dec!(100000), Decimal::ZERO);
        assert_eq!(p.discount_for(dec!(75000)), dec!(75000));
    }

    /// Biaya kanal dihitung dari nominal SETELAH diskon, dan bagian
    /// persentasenya dibulatkan ke rupiah penuh.
    #[test]
    fn biaya_kanal_tetap_plus_persen() {
        let m = PaymentMethod {
            code: "cc".into(),
            name: "Kartu".into(),
            vendor: "midtrans".into(),
            category: "cc".into(),
            image_url: String::new(),
            description: String::new(),
            charge: 2000,
            charge_percent: dec!(2.9),
            min_amount: 0,
            max_amount: 0,
            allow_promo: true,
            is_instant: false,
            va_prefix: String::new(),
            instruction: String::new(),
            sort_order: 0,
        };
        // 2,9% dari 100.000 = 2.900, ditambah biaya tetap 2.000.
        assert_eq!(m.charge_for(dec!(100000)), dec!(4900));
    }

    /// Kanal dengan plafon menolak nominal di atasnya — inilah yang membuat
    /// daftar kanal di halaman checkout tak menawarkan jalan buntu.
    #[test]
    fn plafon_kanal_ditegakkan() {
        let mut m = PaymentMethod {
            code: "gopay".into(),
            name: "GoPay".into(),
            vendor: "midtrans".into(),
            category: "ewallet".into(),
            image_url: String::new(),
            description: String::new(),
            charge: 0,
            charge_percent: Decimal::ZERO,
            min_amount: 10000,
            max_amount: 2000000,
            allow_promo: true,
            is_instant: false,
            va_prefix: String::new(),
            instruction: String::new(),
            sort_order: 0,
        };
        assert!(m.accepts(dec!(50000)));
        assert!(!m.accepts(dec!(5000)));
        assert!(!m.accepts(dec!(3000000)));
        m.max_amount = 0; // 0 = tanpa batas atas
        assert!(m.accepts(dec!(3000000)));
    }
}
