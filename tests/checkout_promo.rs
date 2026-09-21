//! Alur checkout — gerbang kelayakan promo, lewat `PaymentService` yang asli.
//!
//! Syarat kelayakan tinggal di `PaymentService::eligibility_error`, yang
//! privat dan karena itu hanya bisa dicapai dari luar melalui
//! `validate_promo`. Justru begitu yang diinginkan: `validate_promo` adalah
//! satu-satunya pintu yang dipakai checkout (`service/order/checkout.rs`),
//! keranjang (`service/cart.rs`), dan REST (`api/orders.rs`), dan ia
//! memeriksa tiga hal yang TIDAK ada di `eligibility_error` — kuota promo,
//! batas pemakaian per-user, dan penolakan promo yang potongannya nol.
//!
//! Mengujinya dari pintu itu berarti yang dijaga adalah perilaku yang
//! benar-benar dipakai pemanggilnya, bukan potongan logika di dalamnya.
//!
//! Semua test di sini berjalan tanpa layanan apa pun — lihat alasannya di
//! `tests/common/mod.rs`.

use std::sync::Arc;

use chrono::{Duration, Utc};
use e_ticketing::service::payment::PaymentService;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

mod common;
use common::{promo, RepoPalsu};

/// Bangun service atas satu promo tunggal.
fn service(repo: RepoPalsu) -> PaymentService {
    PaymentService::new(Arc::new(repo))
}

/// Pemeriksaan dengan nilai keranjang yang selalu lolos kecuali yang diuji.
async fn periksa(
    svc: &PaymentService,
    kode: &str,
    subtotal: Decimal,
    qty: i32,
    premium: bool,
    kanal: Option<&str>,
) -> e_ticketing::models::payment::PromoCheck {
    svc.validate_promo("user-uji", kode, subtotal, qty, premium, kanal)
        .await
        .expect("repo palsu tak pernah gagal")
}

/// Jalur normal: promo yang memenuhi semua syarat memberi potongan sebesar
/// yang dihitung `discount_for`, dan membawa `promo_id` untuk pencatatan.
#[tokio::test]
async fn promo_layak_memberi_potongan() {
    let svc = service(RepoPalsu::baru().dengan_promo(promo("HEMAT", "fixed", dec!(15000))));

    let h = periksa(&svc, "HEMAT", dec!(100000), 2, false, Some("qris")).await;

    assert!(h.valid);
    assert_eq!(h.discount, dec!(15000));
    assert_eq!(h.promo_id, Some(1));
}

/// Kode kosong ditolak tanpa menyentuh penyimpanan sama sekali.
#[tokio::test]
async fn kode_kosong_ditolak() {
    let svc = service(RepoPalsu::baru());

    let h = periksa(&svc, "   ", dec!(100000), 1, false, None).await;

    assert!(!h.valid);
}

/// Kode yang tak dikenal ditolak sebagai jawaban biasa, BUKAN sebagai galat.
///
/// Bedanya penting bagi halaman checkout: promo yang tak berlaku adalah hal
/// yang wajar dan harus ditampilkan sebagai pesan, sementara galat berarti
/// ada yang rusak. `validate_promo` sengaja mengembalikan `Ok(PromoCheck)`
/// untuk keduanya-bukan-galat, dan test ini yang menjaganya tetap begitu.
#[tokio::test]
async fn kode_tak_dikenal_bukan_galat() {
    let svc = service(RepoPalsu::baru().dengan_promo(promo("ADA", "fixed", dec!(5000))));

    let hasil = svc
        .validate_promo("user-uji", "TIDAKADA", dec!(100000), 1, false, None)
        .await;

    let h = hasil.expect("kode tak dikenal harus Ok, bukan Err");
    assert!(!h.valid);
    assert_eq!(h.discount, Decimal::ZERO);
}

/// Belanja di bawah minimum ditolak, dan pesannya menyebut minimumnya —
/// pembeli tak bisa memperbaiki apa yang tak ia ketahui.
#[tokio::test]
async fn belanja_di_bawah_minimum_ditolak() {
    let mut p = promo("MIN100", "fixed", dec!(20000));
    p.min_cart_amount = dec!(100000);
    let svc = service(RepoPalsu::baru().dengan_promo(p));

    let h = periksa(&svc, "MIN100", dec!(99999), 1, false, None).await;

    assert!(!h.valid);
    assert!(
        h.message.contains("100"),
        "pesan harus menyebut minimumnya, bukan sekadar menolak: {}",
        h.message
    );
}

/// Promo khusus premium ditolak untuk yang bukan premium, dan diterima untuk
/// yang premium — dua sisi diuji bersama supaya syaratnya tak bisa lulus
/// dengan cara menolak semua orang.
#[tokio::test]
async fn promo_premium_hanya_untuk_premium() {
    let mut p = promo("PREMIUM", "fixed", dec!(25000));
    p.premium_only = true;
    let svc = service(RepoPalsu::baru().dengan_promo(p));

    assert!(!periksa(&svc, "PREMIUM", dec!(100000), 1, false, None).await.valid);
    assert!(periksa(&svc, "PREMIUM", dec!(100000), 1, true, None).await.valid);
}

/// Promo yang dibatasi pada kanal tertentu menolak kanal lain.
///
/// Inilah syarat yang paling mudah bocor di checkout: potongan dihitung saat
/// pembeli belum memilih kanal, lalu kanalnya berganti di langkah berikutnya.
/// `checkout` meneruskan `Some(&method.code)` justru untuk ini.
#[tokio::test]
async fn promo_terbatas_kanal_menolak_kanal_lain() {
    let mut p = promo("QRISAJA", "fixed", dec!(10000));
    p.payment_codes = Some(vec!["qris".into()]);
    let svc = service(RepoPalsu::baru().dengan_promo(p));

    assert!(periksa(&svc, "QRISAJA", dec!(100000), 1, false, Some("qris")).await.valid);
    assert!(!periksa(&svc, "QRISAJA", dec!(100000), 1, false, Some("va_bca")).await.valid);
}

/// Promo yang belum berlaku dan yang sudah lewat sama-sama ditolak.
#[tokio::test]
async fn promo_di_luar_masa_berlaku_ditolak() {
    let mut belum = promo("BESOK", "fixed", dec!(10000));
    belum.starts_at = Utc::now() + Duration::hours(1);
    let svc = service(RepoPalsu::baru().dengan_promo(belum));
    assert!(!periksa(&svc, "BESOK", dec!(100000), 1, false, None).await.valid);

    let mut lewat = promo("KEMARIN", "fixed", dec!(10000));
    lewat.ends_at = Some(Utc::now() - Duration::hours(1));
    let svc = service(RepoPalsu::baru().dengan_promo(lewat));
    assert!(!periksa(&svc, "KEMARIN", dec!(100000), 1, false, None).await.valid);
}

/// Kuota yang sudah terpakai habis menutup promo.
///
/// Diperiksa di `validate_promo`, BUKAN hanya saat `reserve_quota` — kalau
/// hanya di sana, halaman checkout menampilkan potongan yang akan gugur
/// beberapa detik kemudian saat tombol bayar ditekan.
#[tokio::test]
async fn kuota_habis_menutup_promo() {
    let mut p = promo("TERBATAS", "fixed", dec!(10000));
    p.quota_total = 100;
    p.quota_used = 100;
    let svc = service(RepoPalsu::baru().dengan_promo(p));

    assert!(!periksa(&svc, "TERBATAS", dec!(100000), 1, false, None).await.valid);
}

/// Batas pemakaian per-user ditegakkan atas riwayat user itu sendiri.
#[tokio::test]
async fn batas_pemakaian_per_user_ditegakkan() {
    let mut p = promo("SEKALI", "fixed", dec!(10000));
    p.per_user_limit = 1;

    let sudah = RepoPalsu::baru().dengan_promo(p.clone()).dengan_pemakaian_user(1);
    assert!(!periksa(&service(sudah), "SEKALI", dec!(100000), 1, false, None).await.valid);

    let belum = RepoPalsu::baru().dengan_promo(p).dengan_pemakaian_user(0);
    assert!(periksa(&service(belum), "SEKALI", dec!(100000), 1, false, None).await.valid);
}

/// Batas jumlah tiket, dua arah.
#[tokio::test]
async fn batas_jumlah_tiket_ditegakkan() {
    let mut p = promo("DUAAN", "fixed", dec!(10000));
    p.min_qty = 2;
    p.max_qty = 4;
    let svc = service(RepoPalsu::baru().dengan_promo(p));

    assert!(!periksa(&svc, "DUAAN", dec!(100000), 1, false, None).await.valid);
    assert!(periksa(&svc, "DUAAN", dec!(100000), 3, false, None).await.valid);
    assert!(!periksa(&svc, "DUAAN", dec!(100000), 5, false, None).await.valid);
}

/// Promo yang lolos semua syarat tapi potongannya nol tetap ditolak.
///
/// Terjadi pada promo persen dengan nominal kecil: 1% dari Rp40 dibulatkan
/// menjadi nol. Menerimanya berarti checkout memasang label "promo berhasil
/// dipakai" pada order yang harganya tak berubah sepeser pun — dan kuota
/// promonya ikut terpakai untuk itu.
#[tokio::test]
async fn promo_tanpa_potongan_ditolak() {
    let svc = service(RepoPalsu::baru().dengan_promo(promo("NOL", "percent", dec!(1))));

    let h = periksa(&svc, "NOL", dec!(40), 1, false, None).await;

    assert!(!h.valid, "potongan nol bukan promo yang berhasil");
}

/// Kuota yang gagal diambil tak boleh ikut berkurang, dan yang dikembalikan
/// harus benar-benar kembali.
///
/// Ini pasangan yang menentukan apakah kuota promo bocor saat order gagal
/// lahir: `checkout` mengambil jatah SEBELUM order dibuat, lalu memanggil
/// `release_quota` bila pembuatannya gagal.
#[tokio::test]
async fn kuota_diambil_dan_dikembalikan_dengan_benar() {
    let repo = Arc::new(
        RepoPalsu::baru()
            .dengan_promo(promo("KUOTA", "fixed", dec!(10000)))
            .dengan_sisa_kuota(1),
    );
    let svc = PaymentService::new(repo.clone());

    assert!(svc.reserve_quota(1).await.unwrap(), "jatah pertama tersedia");
    assert_eq!(repo.sisa_kuota(), 0);

    assert!(!svc.reserve_quota(1).await.unwrap(), "jatah kedua harus gagal");
    assert_eq!(repo.sisa_kuota(), 0, "pengambilan yang gagal tak boleh mengurangi kuota");

    svc.release_quota(1).await.unwrap();
    assert_eq!(repo.sisa_kuota(), 1, "kuota harus kembali utuh");
}
