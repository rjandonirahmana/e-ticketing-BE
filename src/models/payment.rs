//! models/payment.rs — kanal pembayaran & kode promo sebagai DATA, bukan kode.
//!
//! Sebelumnya daftar kanal ditulis sebagai konstanta Rust di halaman checkout,
//! jadi menambah kanal atau mengubah biaya admin berarti build ulang. Bentuk di
//! sini mengikuti tabel `payment` milik kiddoapi (name/vendor/code/charge/promo/
//! status/procedure_of_payment) dengan tambahan yang memang dipakai PULSE.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ── Kanal pembayaran ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentMethod {
    pub code: String,
    pub name: String,
    pub vendor: String,
    pub category: String,
    pub image_url: String,
    pub description: String,

    /// Biaya tetap (rupiah).
    pub charge: i32,
    /// Biaya persentase dari subtotal setelah diskon.
    pub charge_percent: Decimal,

    pub min_amount: i64,
    /// 0 = tanpa batas atas.
    pub max_amount: i64,

    pub allow_promo: bool,
    /// Lunas seketika tanpa gateway (tunai di lokasi / order nol rupiah).
    pub is_instant: bool,
    pub va_prefix: String,
    pub instruction: String,
    pub sort_order: i32,
}

impl PaymentMethod {
    /// Biaya admin untuk nominal tertentu: bagian tetap + bagian persentase,
    /// dibulatkan ke rupiah penuh. Dihitung dari nilai SETELAH diskon supaya
    /// promo tidak diam-diam menaikkan ongkos kanal.
    pub fn charge_for(&self, amount_after_discount: Decimal) -> Decimal {
        use rust_decimal::prelude::RoundingStrategy;

        let flat = Decimal::from(self.charge);
        if self.charge_percent.is_zero() {
            return flat;
        }
        let pct = (amount_after_discount * self.charge_percent / Decimal::from(100))
            .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero);
        flat + pct
    }

    /// Nominal ini berada dalam rentang yang dilayani kanal?
    pub fn accepts(&self, amount: Decimal) -> bool {
        let min = Decimal::from(self.min_amount);
        if amount < min {
            return false;
        }
        if self.max_amount > 0 && amount > Decimal::from(self.max_amount) {
            return false;
        }
        true
    }
}

// ── Promo ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Promo {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub discount_type: String,
    pub amount: Decimal,
    pub max_discount: Decimal,
    pub min_cart_amount: Decimal,
    pub max_cart_amount: Decimal,
    pub min_qty: i32,
    pub max_qty: i32,
    pub quota_total: i32,
    pub quota_used: i32,
    pub per_user_limit: i32,
    pub premium_only: bool,
    pub payment_codes: Option<Vec<String>>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
}

impl Promo {
    /// Potongan untuk subtotal tertentu — TANPA memeriksa syarat kelayakan
    /// (itu tugas `PaymentService::validate_promo`). Hasilnya tak pernah
    /// melebihi subtotal: order tak boleh berakhir negatif.
    pub fn discount_for(&self, subtotal: Decimal) -> Decimal {
        use rust_decimal::prelude::RoundingStrategy;

        let raw = if self.discount_type == "percent" {
            let d = (subtotal * self.amount / Decimal::from(100))
                .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero);
            if self.max_discount > Decimal::ZERO && d > self.max_discount {
                self.max_discount
            } else {
                d
            }
        } else {
            self.amount
        };

        if raw > subtotal {
            subtotal
        } else {
            raw
        }
    }
}

/// Hasil pemeriksaan kode promo terhadap satu keranjang.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoCheck {
    pub valid: bool,
    pub code: String,
    pub discount: Decimal,
    pub message: String,
    /// Hanya terisi bila `valid` — dipakai saat mencatat pemakaian promo.
    #[serde(skip)]
    pub promo_id: Option<i64>,
}

impl PromoCheck {
    pub fn invalid(code: &str, message: impl Into<String>) -> Self {
        Self {
            valid: false,
            code: code.to_string(),
            discount: Decimal::ZERO,
            message: message.into(),
            promo_id: None,
        }
    }
}
