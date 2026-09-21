//! Alur checkout — perhitungan uang dari ujung ke ujung.
//!
//! ── APA YANG BELUM DIJAGA TEST LAIN ───────────────────────────────────────
//! `service/payment.rs` sudah menguji potongan promo dan biaya kanal, tetapi
//! masing-masing SENDIRI-SENDIRI: satu test memanggil `discount_for`, test
//! lain memanggil `charge_for`. Keduanya bisa terus lulus sementara
//! rangkaiannya salah — dan rangkaian itulah yang menentukan berapa rupiah
//! yang benar-benar ditagihkan.
//!
//! Satu urutan di `CheckoutPricing::compute` yang tak pernah terlihat dari
//! test per-bagian:
//!
//!     potongan  = promo.discount_for(subtotal)
//!     setelah   = subtotal - potongan
//!     biaya     = method.charge_for(SETELAH)      ← bukan atas subtotal
//!     total     = setelah + biaya
//!
//! Menghitung biaya kanal atas subtotal, bukan atas nominal setelah potongan,
//! adalah kekeliruan yang tak pernah menampakkan diri sebagai galat: angkanya
//! tetap masuk akal, ordernya tetap lahir, tiketnya tetap terbit. Yang terjadi
//! hanya pembeli membayar lebih sedikit atau lebih banyak daripada yang
//! seharusnya — selamanya, sampai ada yang menghitung ulang dengan tangan.

use chrono::Utc;
use e_ticketing::service::order::checkout::CheckoutPricing;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

mod common;
use common::{kanal, promo};

/// Biaya kanal dihitung atas nominal SETELAH potongan.
///
/// Angkanya dipilih supaya kedua kemungkinan menghasilkan jawaban yang
/// berbeda jauh: 3% dari 100.000 = 3.000, sedangkan 3% dari 60.000 = 1.800.
/// Test yang subtotal dan potongannya kebetulan membuat keduanya sama tak
/// membuktikan apa pun.
#[test]
fn biaya_kanal_dihitung_setelah_potongan() {
    let mut m = kanal("cc", "cc");
    m.charge_percent = dec!(3);

    let p = promo("HEMAT40", "fixed", dec!(40000));
    let pricing = CheckoutPricing {
        cart_bytes: None,
        method: &m,
        promo: Some(&p),
    };

    let h = pricing.compute(dec!(100000));

    assert_eq!(h.discount, dec!(40000));
    assert_eq!(h.charge, dec!(1800), "3% harus diambil dari 60.000, bukan 100.000");
    assert_eq!(h.total, dec!(61800));
}

/// Total adalah jumlah dari bagian-bagiannya, bukan angka yang dihitung
/// terpisah. Diperiksa sebagai satu persamaan supaya perubahan pada salah satu
/// suku tak bisa lolos tanpa mengubah totalnya juga.
#[test]
fn total_sama_dengan_subtotal_dikurangi_potongan_ditambah_biaya() {
    let mut m = kanal("va_bca", "va");
    m.charge = 4000;
    m.charge_percent = dec!(1.5);

    let p = promo("DISKON10", "percent", dec!(10));
    let pricing = CheckoutPricing {
        cart_bytes: None,
        method: &m,
        promo: Some(&p),
    };

    let subtotal = dec!(250000);
    let h = pricing.compute(subtotal);

    assert_eq!(h.discount, dec!(25000));
    // 1,5% dari 225.000 = 3.375, ditambah biaya tetap 4.000.
    assert_eq!(h.charge, dec!(7375));
    assert_eq!(h.total, subtotal - h.discount + h.charge);
    assert_eq!(h.total, dec!(232375));
}

/// Promo yang lebih besar daripada belanjanya tak boleh menghasilkan total
/// negatif — yang tersisa hanya biaya kanal.
///
/// Ini jalur yang benar-benar bisa terjadi: kode potongan tetap Rp100.000
/// dipakai pada keranjang Rp75.000. Tanpa penjagaan, ordernya lahir dengan
/// total minus dan pembeli berhak menagih kembaliannya.
#[test]
fn potongan_melebihi_belanja_tak_membuat_total_negatif() {
    let mut m = kanal("qris", "qris");
    m.charge = 750;

    let p = promo("BESAR", "fixed", dec!(100000));
    let pricing = CheckoutPricing {
        cart_bytes: None,
        method: &m,
        promo: Some(&p),
    };

    let h = pricing.compute(dec!(75000));

    assert_eq!(h.discount, dec!(75000), "potongan dipangkas sebesar belanja");
    assert_eq!(h.total, dec!(750), "yang tersisa hanya biaya kanal");
    assert!(h.total >= Decimal::ZERO);
}

/// Plafon promo persen mengikat DULU, baru biaya kanal dihitung atas sisanya.
/// Urutan terbalik memberi potongan yang lebih besar daripada plafonnya.
#[test]
fn plafon_promo_persen_mengikat_sebelum_biaya_kanal() {
    let mut m = kanal("ovo", "ewallet");
    m.charge_percent = dec!(2);

    let mut p = promo("MAKS50", "percent", dec!(20));
    p.max_discount = dec!(50000);

    let pricing = CheckoutPricing {
        cart_bytes: None,
        method: &m,
        promo: Some(&p),
    };

    // 20% dari 400.000 = 80.000, tapi plafonnya 50.000.
    let h = pricing.compute(dec!(400000));

    assert_eq!(h.discount, dec!(50000));
    // 2% dari 350.000 = 7.000.
    assert_eq!(h.charge, dec!(7000));
    assert_eq!(h.total, dec!(357000));
}

/// Tanpa promo, tak ada potongan — dan kode promo pada rinciannya kosong,
/// bukan string kosong yang menyamar sebagai kode.
#[test]
fn tanpa_promo_tak_ada_potongan() {
    let mut m = kanal("cash", "cash");
    m.charge = 0;

    let pricing = CheckoutPricing {
        cart_bytes: None,
        method: &m,
        promo: None,
    };

    let h = pricing.compute(dec!(120000));

    assert_eq!(h.discount, Decimal::ZERO);
    assert_eq!(h.total, dec!(120000));
    assert!(h.promo_code.is_none());
}

/// Kode promo yang dipakai ikut terbawa ke rinciannya — itulah yang nanti
/// tersimpan di order dan tampil di riwayat pembelian.
#[test]
fn kode_promo_terbawa_ke_rincian() {
    let m = kanal("qris", "qris");
    let p = promo("MERDEKA", "fixed", dec!(10000));

    let pricing = CheckoutPricing {
        cart_bytes: None,
        method: &m,
        promo: Some(&p),
    };

    let h = pricing.compute(dec!(50000));
    assert_eq!(h.promo_code.as_deref(), Some("MERDEKA"));
}

/// Kanal yang lunas seketika tak punya tenggat bayar. Memberinya tenggat
/// berarti halaman instruksi menghitung mundur untuk pembayaran yang sudah
/// selesai.
#[test]
fn kanal_seketika_tanpa_tenggat_bayar() {
    let mut m = kanal("cash", "cash");
    m.is_instant = true;

    let pricing = CheckoutPricing {
        cart_bytes: None,
        method: &m,
        promo: None,
    };

    assert!(pricing.compute(dec!(50000)).payment_expired_at.is_none());
}

/// Tenggat bayar mengikuti sifat kanalnya: VA dua jam, e-wallet dan QRIS
/// tiga puluh menit, sisanya satu jam.
///
/// Diperiksa sebagai RENTANG, bukan nilai persis: `payment_deadline` memanggil
/// `Utc::now()` sendiri, jadi menuntut kesamaan sampai nanodetik hanya
/// menghasilkan test yang gagal sesekali tanpa ada yang rusak.
#[test]
fn tenggat_bayar_mengikuti_kategori_kanal() {
    let kasus = [("va", 120i64), ("ewallet", 30), ("qris", 30), ("cc", 60)];

    for (kategori, menit) in kasus {
        let m = kanal("x", kategori);
        let pricing = CheckoutPricing {
            cart_bytes: None,
            method: &m,
            promo: None,
        };

        let tenggat = pricing
            .compute(dec!(50000))
            .payment_expired_at
            .unwrap_or_else(|| panic!("kanal '{kategori}' harus punya tenggat bayar"));

        let selisih = (tenggat - Utc::now()).num_seconds();
        let sasaran = menit * 60;
        assert!(
            (sasaran - 5..=sasaran + 5).contains(&selisih),
            "kanal '{kategori}': tenggat {selisih} detik, diharapkan sekitar {sasaran}"
        );
    }
}

/// Nomor VA belum ada saat harga dihitung.
///
/// Ia diturunkan dari kode order, dan kode itu baru lahir di dalam transaksi —
/// jadi `reference` yang terisi di sini berarti nomornya dikarang dari sesuatu
/// yang belum tentu jadi kode ordernya.
#[test]
fn nomor_va_belum_terbit_saat_harga_dihitung() {
    let mut m = kanal("va_bca", "va");
    m.va_prefix = "8808".into();

    let pricing = CheckoutPricing {
        cart_bytes: None,
        method: &m,
        promo: None,
    };

    assert!(pricing.compute(dec!(50000)).reference.is_none());
}
