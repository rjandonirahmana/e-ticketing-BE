//! payment/ — integrasi gateway pembayaran dan webhook-nya.
//!
//! ── SATU ATURAN YANG MENENTUKAN SELURUH BENTUK MODUL INI ──────────────────
//! **Hanya webhook yang terverifikasi yang boleh melunaskan order.**
//!
//! Sebelum modul ini ada, yang melunaskan order adalah permintaan dari browser
//! pembeli sendiri (`confirm_order_payment`) tanpa bukti apa pun. Siapa pun
//! yang login bisa membuat order lalu menyatakannya lunas, dan tiketnya
//! terbit. Memindahkan kewenangan itu ke gateway bukan penyempurnaan — ia
//! satu-satunya hal yang membedakan toko dari mesin tiket gratis.
//!
//! Konsekuensinya pada rancangan: jalur klien (`buat_transaksi`) TIDAK PERNAH
//! mengubah status order. Ia hanya meminta gateway membuat tagihan dan
//! mengembalikan cara membayarnya. Status order berubah di satu tempat saja,
//! yaitu pemroses webhook di `webhook.rs`.
//!
//! ── KENAPA `tafsir_webhook` TIDAK ASYNC ───────────────────────────────────
//! Verifikasi tanda tangan adalah perhitungan murni atas byte yang sudah ada
//! di tangan: tak ada I/O, tak ada jaringan, tak ada basis data. Membuatnya
//! sinkron dan bebas efek samping berarti ia bisa diuji dengan vektor yang
//! sudah diketahui jawabannya — dan bagian inilah yang paling pantas diuji,
//! karena salahnya tak pernah terlihat sebagai galat. Verifikasi yang terlalu
//! longgar menerima pembayaran palsu; yang terlalu ketat menolak pembayaran
//! sungguhan dan uangnya sudah telanjur berpindah.
//!
//! ── KOSAKATA ──────────────────────────────────────────────────────────────
//! Tiap gateway punya istilahnya sendiri untuk "lunas": Midtrans bilang
//! `settlement` (atau `capture` dengan `fraud_status=accept`), Faspay
//! mengirim angka `2`, Flip mengirim `SUCCESSFUL`. Penerjemahannya terjadi di
//! sini, satu kali, sehingga tak satu pun bagian lain aplikasi perlu mengenal
//! kosakata gateway mana pun.

pub mod faspay;
pub mod flip;
pub mod midtrans;
pub mod stripe;
pub mod webhook;

use async_trait::async_trait;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Status pembayaran dalam kosakata KITA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusBayar {
    /// Tagihan sudah dibuat, uang belum masuk.
    Menunggu,
    /// Uang sudah masuk dan final. HANYA ini yang menerbitkan tiket.
    Lunas,
    /// Ditolak gateway/penerbit kartu. Pembeli boleh mencoba kanal lain.
    Gagal,
    /// Lewat tenggat tanpa dibayar.
    Kedaluwarsa,
    /// Dikembalikan setelah lunas.
    Dikembalikan,
}

impl StatusBayar {
    /// Istilah yang disimpan di kolom `payment_transactions.status`.
    pub fn sebagai_kolom(self) -> &'static str {
        match self {
            Self::Menunggu => "pending",
            Self::Lunas => "paid",
            Self::Gagal => "failed",
            Self::Kedaluwarsa => "expired",
            Self::Dikembalikan => "refunded",
        }
    }

    /// Status akhir tak boleh berpindah lagi.
    ///
    /// Dipakai pemroses webhook untuk menolak callback yang datang terlambat
    /// dan membawa status lama — jaringan tak menjamin urutan, dan callback
    /// `pending` yang tiba SESUDAH `settlement` akan membatalkan pelunasan
    /// kalau tak ada penjaga ini.
    pub fn final_(self) -> bool {
        matches!(self, Self::Lunas | Self::Dikembalikan)
    }
}

/// Permintaan pembuatan tagihan ke gateway.
#[derive(Debug, Clone)]
pub struct PermintaanBayar {
    /// Kode order kita — dipakai sebagai referensi di sisi gateway supaya
    /// callback bisa ditelusuri balik tanpa tabel perantara.
    pub order_code: String,
    pub amount: Decimal,
    /// Kode kanal di sisi kita (`payment_methods.code`).
    pub payment_code: String,
    pub customer_name: String,
    pub customer_email: Option<String>,
    pub customer_phone: Option<String>,
}

/// Hasil pembuatan tagihan — apa yang harus ditunjukkan ke pembeli.
#[derive(Debug, Clone)]
pub struct TagihanDibuat {
    /// Identitas transaksi di sisi gateway; kunci pencarian saat webhook tiba.
    pub provider_ref: String,
    /// Nomor VA / kode QRIS / apa pun yang disalin pembeli.
    pub pay_reference: Option<String>,
    /// Halaman bayar yang dibuka pembeli.
    pub pay_url: Option<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Jawaban mentah gateway, disimpan apa adanya untuk penelusuran sengketa.
    pub raw: serde_json::Value,
}

/// Kesimpulan atas satu callback.
#[derive(Debug, Clone, PartialEq)]
pub struct KabarWebhook {
    /// Kunci idempotensi. Callback dengan kunci sama diabaikan.
    pub event_key: String,
    /// Referensi transaksi di sisi gateway, untuk mencari barisnya.
    pub provider_ref: String,
    pub status: StatusBayar,
    /// Nominal yang DIAKUI gateway. `None` bila callback tak menyebutnya.
    ///
    /// Diperiksa terhadap nominal transaksi sebelum pelunasan: gateway yang
    /// melaporkan pembayaran lebih kecil daripada tagihan bukan pelunasan.
    pub amount: Option<Decimal>,
}

/// Kenapa sebuah callback tidak diterima.
#[derive(Debug, Clone, PartialEq)]
pub enum GalatWebhook {
    /// Tanda tangan tak cocok. Ini yang paling penting dibedakan dari sisanya:
    /// ia berarti ada yang mencoba memalsukan, bukan sekadar salah bentuk.
    TandaTanganSalah,
    /// Bentuk muatan tak dikenali (bukan JSON, medan wajib hilang).
    MuatanRusak(String),
    /// Peristiwa yang memang tak kita pedulikan (mis. `invoice.created`).
    /// Dijawab 200 supaya gateway berhenti mengulangnya.
    Diabaikan,
}

impl std::fmt::Display for GalatWebhook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TandaTanganSalah => write!(f, "tanda tangan tidak sah"),
            Self::MuatanRusak(s) => write!(f, "muatan rusak: {s}"),
            Self::Diabaikan => write!(f, "peristiwa diabaikan"),
        }
    }
}

/// Satu gateway pembayaran.
#[async_trait]
pub trait GatewayBayar: Send + Sync {
    /// Nama yang masuk ke kolom `provider` dan ke potongan URL webhook.
    fn nama(&self) -> &'static str;

    /// Minta gateway membuat tagihan. TIDAK mengubah status order apa pun.
    async fn buat_tagihan(&self, req: &PermintaanBayar) -> anyhow::Result<TagihanDibuat>;

    /// Verifikasi dan tafsirkan callback.
    ///
    /// `raw` adalah badan permintaan APA ADANYA — bukan hasil parse lalu
    /// serialisasi ulang. Stripe menandatangani byte mentahnya, jadi
    /// membentuk ulang JSON-nya (urutan kunci, spasi) akan menghasilkan
    /// tanda tangan yang tak pernah cocok.
    fn tafsir_webhook(
        &self,
        raw: &[u8],
        headers: &axum::http::HeaderMap,
    ) -> Result<KabarWebhook, GalatWebhook>;
}

// ── Perkakas bersama ──────────────────────────────────────────────────────

/// Pembandingan byte yang waktunya tidak bergantung pada isinya.
///
/// `==` biasa berhenti pada byte pertama yang berbeda. Selisih waktunya kecil,
/// tetapi cukup: penyerang yang bisa mengirim callback berkali-kali dapat
/// menebak tanda tangan yang sah byte demi byte dari selisih itu. Seluruh
/// pembandingan tanda tangan di modul ini WAJIB lewat sini.
pub fn banding_aman(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    if a.len() != b.len() {
        // Panjang bukan rahasia — ia terbaca dari muatannya sendiri.
        return false;
    }
    a.ct_eq(b).into()
}

/// Ambil satu header sebagai `&str`, atau `None` bila absen/bukan ASCII.
pub fn header_str<'a>(h: &'a axum::http::HeaderMap, nama: &str) -> Option<&'a str> {
    h.get(nama).and_then(|v| v.to_str().ok())
}

// ── Perakitan dari env ────────────────────────────────────────────────────

/// Bangun peta gateway yang AKTIF dari environment.
///
/// Gateway yang kuncinya tak lengkap TIDAK dimasukkan. Itu keputusan yang
/// disengaja: peta ini juga yang dipakai endpoint webhook untuk mencari
/// penafsir callback, jadi gateway yang setengah terkonfigurasi akan membalas
/// 404 — jauh lebih baik daripada memverifikasi tanda tangan dengan kunci
/// kosong, yang akan menolak semua callback sah tanpa menyebut sebabnya.
///
/// `*_PRODUCTION=1` memindahkan semua alamat ke produksi sekaligus. Satu
/// saklar, bukan satu per gateway: campuran sandbox dan produksi dalam satu
/// proses adalah keadaan yang tak pernah dimaksud siapa pun dan sangat sulit
/// dikenali dari log.
pub fn dari_env() -> std::collections::HashMap<&'static str, std::sync::Arc<dyn GatewayBayar>> {
    use std::sync::Arc;
    let mut peta: std::collections::HashMap<&'static str, Arc<dyn GatewayBayar>> =
        std::collections::HashMap::new();
    let http = reqwest::Client::new();
    let produksi = std::env::var("PAYMENT_PRODUCTION").ok().as_deref() == Some("1");
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());

    if let Some(server_key) = env("MIDTRANS_SERVER_KEY") {
        peta.insert(
            "midtrans",
            Arc::new(midtrans::Midtrans::baru(server_key, produksi, http.clone())),
        );
    }

    if let (Some(sk), Some(whsec)) = (env("STRIPE_SECRET_KEY"), env("STRIPE_WEBHOOK_SECRET")) {
        peta.insert(
            "stripe",
            Arc::new(stripe::Stripe::baru(
                sk,
                whsec,
                env("STRIPE_SUCCESS_URL").unwrap_or_else(|| "https://ulala.space/orders".into()),
                env("STRIPE_CANCEL_URL").unwrap_or_else(|| "https://ulala.space/cart".into()),
                http.clone(),
            )),
        );
    }

    if let (Some(sk), Some(token)) = (env("FLIP_SECRET_KEY"), env("FLIP_VALIDATION_TOKEN")) {
        peta.insert(
            "flip",
            Arc::new(flip::Flip::baru(
                sk,
                token,
                produksi,
                env("FLIP_REDIRECT_URL").unwrap_or_else(|| "https://ulala.space/orders".into()),
                http.clone(),
            )),
        );
    }

    if let (Some(uid), Some(pw), Some(mid)) = (
        env("FASPAY_USER_ID"),
        env("FASPAY_PASSWORD"),
        env("FASPAY_MERCHANT_ID"),
    ) {
        peta.insert(
            "faspay",
            Arc::new(faspay::Faspay::baru(
                uid,
                pw,
                mid,
                produksi,
                env("FASPAY_RETURN_URL").unwrap_or_else(|| "https://ulala.space/orders".into()),
                http.clone(),
            )),
        );
    }

    if peta.is_empty() {
        tracing::warn!(
            "TAK ADA gateway pembayaran terkonfigurasi — tak satu pun order bisa \
             dilunaskan lewat jalur yang sah. Set MIDTRANS_SERVER_KEY / \
             STRIPE_SECRET_KEY+STRIPE_WEBHOOK_SECRET / FLIP_SECRET_KEY+FLIP_VALIDATION_TOKEN \
             / FASPAY_USER_ID+FASPAY_PASSWORD+FASPAY_MERCHANT_ID."
        );
    } else {
        let nama: Vec<&str> = peta.keys().copied().collect();
        tracing::info!(?nama, produksi, "Gateway pembayaran aktif");
    }
    peta
}

/// Bolehkah jalur pelunasan TIRUAN dipakai?
///
/// Dibaca tiap kali, bukan di-cache: pagar keamanan yang nilainya dibekukan
/// saat start tak bisa dimatikan tanpa menyalakan ulang proses, dan pada saat
/// seseorang sadar ia menyala di produksi, menyalakan ulang adalah hal
/// terakhir yang ingin ia lakukan.
///
/// Hanya `"1"` yang menyalakan. Bukan `"true"`, bukan `"yes"`, bukan nilai
/// apa pun yang kebetulan tak kosong — menerima banyak ejaan berarti satu
/// variabel yang tersisa dari eksperimen lain bisa membuka jalur ini tanpa
/// ada yang bermaksud begitu.
pub fn mock_diizinkan() -> bool {
    std::env::var("PAYMENT_MOCK").ok().as_deref() == Some("1")
}

// ── Pembuatan tagihan: jembatan dari order ke gateway ─────────────────────

/// Buat tagihan di gateway untuk satu order, lalu catat transaksinya.
///
/// ── KENAPA PENCATATANNYA ADA DI SINI, BUKAN DI PEMANGGIL ─────────────────
/// Baris `payment_transactions` bukan catatan tambahan — ia SATU-SATUNYA
/// jembatan dari referensi yang dikirim gateway kembali ke order kita.
/// Webhook mencari `(provider, provider_ref)` di tabel ini; tanpa barisnya,
/// callback yang tanda tangannya sempurna pun berakhir sebagai "transaksi tak
/// dikenal" dan ordernya tak pernah lunas.
///
/// Menyatukan "minta tagihan" dan "catat transaksinya" dalam satu fungsi
/// membuat keduanya mustahil terpisah. Sebelum ini keduanya memang terpisah —
/// tepatnya, yang kedua tidak ada sama sekali — dan akibatnya seluruh jalur
/// pembayaran yang sah tak pernah bisa selesai.
pub async fn buat_tagihan_untuk_order(
    state: &crate::state::AppState,
    provider: &str,
    order: &crate::models::orders::OrderDetailResponse,
    nama_pembeli: &str,
    email: Option<String>,
    telepon: Option<String>,
) -> anyhow::Result<TagihanDibuat> {
    use crate::utils::ulid::{id_to_vec, new_ulid, ulid_to_vec};

    let gw = state
        .gateway_bayar
        .get(provider)
        .ok_or_else(|| anyhow::anyhow!("gateway '{provider}' tidak dikonfigurasi"))?;

    let req = PermintaanBayar {
        order_code: order.order_code.clone(),
        amount: order.total_amount,
        payment_code: order.payment_code.clone().unwrap_or_default(),
        customer_name: nama_pembeli.to_string(),
        customer_email: email,
        customer_phone: telepon,
    };

    let tagihan = gw.buat_tagihan(&req).await?;

    let client = state.pool.get().await?;
    let id = ulid_to_vec(&new_ulid())?;
    let order_bytes = id_to_vec(&order.id)?;

    // `ON CONFLICT … DO UPDATE` bukan kemalasan: pembeli yang menekan "bayar"
    // dua kali pada kanal yang sama harus mendapat tagihan yang SAMA, bukan
    // baris kedua yang membuat webhook punya dua sasaran untuk satu
    // pembayaran. Kuncinya (provider, provider_ref) sudah UNIQUE di skema.
    client
        .execute(
            "INSERT INTO payment_transactions                 (id, order_id, provider, payment_code, provider_ref, status, amount,                  pay_reference, pay_url, expires_at, raw_response)              VALUES ($1,$2,$3,$4,$5,'pending',$6,$7,$8,$9,$10)              ON CONFLICT (provider, provider_ref) DO UPDATE SET                 pay_reference = EXCLUDED.pay_reference,                 pay_url       = EXCLUDED.pay_url,                 expires_at    = EXCLUDED.expires_at,                 raw_response  = EXCLUDED.raw_response,                 updated_at    = NOW()",
            &[
                &id,
                &order_bytes,
                &provider,
                &req.payment_code,
                &tagihan.provider_ref,
                &order.total_amount,
                &tagihan.pay_reference,
                &tagihan.pay_url,
                &tagihan.expires_at,
                &tagihan.raw,
            ],
        )
        .await?;

    tracing::info!(
        provider,
        order_code = %order.order_code,
        r#ref = %tagihan.provider_ref,
        "tagihan gateway dibuat"
    );
    Ok(tagihan)
}
