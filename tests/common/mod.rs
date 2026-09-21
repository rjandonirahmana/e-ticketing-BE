//! tests/common — perkakas bersama untuk test integrasi alur checkout.
//!
//! ── KENAPA PALSU, BUKAN POSTGRES ──────────────────────────────────────────
//! Alur checkout punya dua bagian yang sifatnya berbeda, dan keduanya butuh
//! perlakuan berbeda pula:
//!
//!   1. ATURAN UANG — potongan promo, syarat kelayakan, biaya kanal, total.
//!      Seluruhnya keputusan aplikasi. Database hanya menyimpan angkanya; ia
//!      tak pernah ikut memutuskan. Bagian inilah yang diuji di sini, lewat
//!      `PaymentRepository` palsu — artinya test-nya berjalan di mesin mana
//!      pun, tanpa layanan apa pun, dan tiap kali menghasilkan hal yang sama.
//!
//!   2. INVARIAN DATABASE — anti-oversell di bawah konkurensi, penguncian
//!      baris, idempotensi. Ini justru TAK BOLEH dipalsukan: yang dijaga
//!      adalah perilaku Postgres, jadi memalsukannya berarti menguji palsuan
//!      itu sendiri. Bagian itu sudah punya rumahnya di
//!      `src/service/order/oversell_test.rs`, ber-`#[ignore]` dan butuh
//!      `TEST_DATABASE_URL`.
//!
//! Yang dipalsukan di sini hanya LAPISAN PENYIMPANAN. Yang diuji tetap
//! `PaymentService` dan `CheckoutPricing` yang asli — bukan tiruannya.

#![allow(dead_code)]

use std::sync::atomic::{AtomicI32, AtomicI64, Ordering};
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use e_ticketing::models::payment::{PaymentMethod, Promo};
use e_ticketing::repository::payment::PaymentRepository;
use rust_decimal::Decimal;

// ── Pembangun data ────────────────────────────────────────────────────────

/// Kanal pembayaran "kosong" — semua biaya nol, tanpa batas nominal.
///
/// Tiap test menyalakan SATU sifat yang sedang diujinya di atas dasar ini.
/// Membangun dari nilai netral membuat test menyatakan sendiri apa yang
/// relevan: kalau sebuah angka muncul di test, ia pasti ikut menentukan
/// hasilnya — tak ada lagi medan yang kebetulan terisi dan diam-diam ikut
/// berpengaruh.
pub fn kanal(code: &str, category: &str) -> PaymentMethod {
    PaymentMethod {
        code: code.into(),
        name: code.to_uppercase(),
        vendor: "uji".into(),
        category: category.into(),
        image_url: String::new(),
        description: String::new(),
        charge: 0,
        charge_percent: Decimal::ZERO,
        min_amount: 0,
        max_amount: 0,
        allow_promo: true,
        is_instant: false,
        va_prefix: String::new(),
        instruction: String::new(),
        sort_order: 0,
    }
}

/// Promo yang lolos SEMUA syarat kelayakan: sudah berlaku, tak kedaluwarsa,
/// tanpa batas keranjang, tanpa kuota, terbuka untuk semua kanal.
///
/// Dengan begitu tiap test kelayakan cukup mematikan satu syarat, dan
/// kegagalannya tak mungkin datang dari syarat lain yang kebetulan ikut
/// melanggar.
pub fn promo(code: &str, discount_type: &str, amount: Decimal) -> Promo {
    Promo {
        id: 1,
        code: code.into(),
        name: format!("Promo {code}"),
        discount_type: discount_type.into(),
        amount,
        max_discount: Decimal::ZERO,
        min_cart_amount: Decimal::ZERO,
        max_cart_amount: Decimal::ZERO,
        min_qty: 0,
        max_qty: 0,
        quota_total: 0,
        quota_used: 0,
        per_user_limit: 0,
        premium_only: false,
        payment_codes: None,
        starts_at: Utc::now() - Duration::days(1),
        ends_at: None,
    }
}

// ── Penyimpanan palsu ─────────────────────────────────────────────────────

/// `PaymentRepository` dalam memori.
///
/// Kuota dan hitungan pemakaian per-user disimpan sebagai nilai atomik supaya
/// test bisa memeriksa EFEK SAMPING, bukan cuma nilai balikan: `reserve` yang
/// gagal tak boleh mengurangi kuota, dan `release` harus benar-benar
/// mengembalikannya. Itu pasangan yang menentukan apakah kuota promo bocor
/// saat order gagal lahir.
pub struct RepoPalsu {
    metode: Mutex<Vec<PaymentMethod>>,
    promo: Mutex<Option<Promo>>,
    /// Sisa kuota. Negatif berarti "tanpa kuota" (selalu berhasil).
    sisa_kuota: AtomicI32,
    /// Berapa kali user sudah memakai promo ini sebelumnya.
    pemakaian_user: AtomicI64,
    /// Berapa kali `record_redemption` dipanggil — dipakai memastikan
    /// pencatatan terjadi tepat sekali.
    pencatatan: AtomicI64,
}

impl RepoPalsu {
    pub fn baru() -> Self {
        Self {
            metode: Mutex::new(Vec::new()),
            promo: Mutex::new(None),
            sisa_kuota: AtomicI32::new(-1),
            pemakaian_user: AtomicI64::new(0),
            pencatatan: AtomicI64::new(0),
        }
    }

    pub fn dengan_kanal(self, m: PaymentMethod) -> Self {
        self.metode.lock().unwrap().push(m);
        self
    }

    pub fn dengan_promo(self, p: Promo) -> Self {
        *self.promo.lock().unwrap() = Some(p);
        self
    }

    /// Batasi kuota yang tersisa (dipakai menguji jalur reserve/release).
    pub fn dengan_sisa_kuota(self, n: i32) -> Self {
        self.sisa_kuota.store(n, Ordering::SeqCst);
        self
    }

    /// User sudah memakai promo ini sekian kali sebelumnya.
    pub fn dengan_pemakaian_user(self, n: i64) -> Self {
        self.pemakaian_user.store(n, Ordering::SeqCst);
        self
    }

    pub fn sisa_kuota(&self) -> i32 {
        self.sisa_kuota.load(Ordering::SeqCst)
    }

    pub fn jumlah_pencatatan(&self) -> i64 {
        self.pencatatan.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl PaymentRepository for RepoPalsu {
    async fn list_methods(&self) -> Result<Vec<PaymentMethod>> {
        Ok(self.metode.lock().unwrap().clone())
    }

    async fn find_method(&self, code: &str) -> Result<Option<PaymentMethod>> {
        Ok(self
            .metode
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.code == code)
            .cloned())
    }

    async fn find_promo(&self, code: &str) -> Result<Option<Promo>> {
        Ok(self
            .promo
            .lock()
            .unwrap()
            .clone()
            .filter(|p| p.code == code))
    }

    async fn count_user_redemptions(&self, _promo_id: i64, _user_id: &str) -> Result<i64> {
        Ok(self.pemakaian_user.load(Ordering::SeqCst))
    }

    async fn reserve_promo_quota(&self, _promo_id: i64) -> Result<bool> {
        // Tanpa kuota → selalu berhasil, dan tak ada yang perlu dihitung.
        if self.sisa_kuota.load(Ordering::SeqCst) < 0 {
            return Ok(true);
        }
        // Kurangi hanya bila masih ada sisa. Pola bandingkan-lalu-tukar dipakai
        // supaya pengurangannya tak pernah menembus nol walau dipanggil dari
        // beberapa task sekaligus — sama semangatnya dengan `UPDATE … WHERE
        // (quota - sold) >= qty` di repository aslinya.
        let mut kini = self.sisa_kuota.load(Ordering::SeqCst);
        loop {
            if kini <= 0 {
                return Ok(false);
            }
            match self.sisa_kuota.compare_exchange(
                kini,
                kini - 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return Ok(true),
                Err(sekarang) => kini = sekarang,
            }
        }
    }

    async fn release_promo_quota(&self, _promo_id: i64) -> Result<()> {
        if self.sisa_kuota.load(Ordering::SeqCst) >= 0 {
            self.sisa_kuota.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }

    async fn record_redemption(
        &self,
        _promo_id: i64,
        _user_id: &str,
        _order_id: &str,
        _discount: Decimal,
    ) -> Result<()> {
        self.pencatatan.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
