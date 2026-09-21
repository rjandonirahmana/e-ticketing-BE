//! payment/stripe.rs — Stripe Checkout Session + webhook bertanda tangan.
//!
//! ── TANDA TANGAN ──────────────────────────────────────────────────────────
//! Header `Stripe-Signature` berbentuk daftar berkoma:
//!
//!     t=1492774577,v1=5257a869e7…,v0=…
//!
//! dan yang ditandatangani adalah `"{t}.{badan mentah}"` dengan HMAC-SHA256
//! memakai *webhook signing secret* (`whsec_…`) — BUKAN kunci API rahasia
//! (`sk_…`). Keduanya beredar berdampingan dan tertukar adalah kesalahan
//! pertama yang paling sering terjadi.
//!
//! Dua hal yang wajib dan mudah terlupa:
//!
//!   • **Badan MENTAH.** Mem-parse JSON lalu menyerialisasi ulang mengubah
//!     urutan kunci dan spasi, dan tanda tangannya tak akan pernah cocok lagi.
//!     Itu sebabnya `tafsir_webhook` menerima `&[u8]`, bukan `Value`.
//!   • **Jendela waktu.** Tanpa memeriksa `t`, sebuah callback sah yang pernah
//!     terekam bisa dikirim ulang kapan saja selamanya — tanda tangannya tetap
//!     sah karena isinya tak berubah. Jendela 5 menit adalah anjuran Stripe.

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use rust_decimal::Decimal;
use serde_json::Value;
use sha2::Sha256;

use super::{
    banding_aman, header_str, GalatWebhook, GatewayBayar, KabarWebhook, PermintaanBayar,
    StatusBayar, TagihanDibuat,
};

/// Selisih waktu maksimum antara stempel di tanda tangan dan jam kita.
const TOLERANSI_DETIK: i64 = 300;

pub struct Stripe {
    secret_key: String,
    webhook_secret: String,
    success_url: String,
    cancel_url: String,
    http: reqwest::Client,
}

impl Stripe {
    pub fn baru(
        secret_key: String,
        webhook_secret: String,
        success_url: String,
        cancel_url: String,
        http: reqwest::Client,
    ) -> Self {
        Self {
            secret_key,
            webhook_secret,
            success_url,
            cancel_url,
            http,
        }
    }

    /// HMAC-SHA256 atas `"{stempel}.{badan}"`, heksadesimal huruf kecil.
    pub fn tanda_tangan(stempel: &str, badan: &[u8], rahasia: &str) -> String {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(rahasia.as_bytes())
            .expect("HMAC menerima kunci panjang apa pun");
        mac.update(stempel.as_bytes());
        mac.update(b".");
        mac.update(badan);
        hex::encode(mac.finalize().into_bytes())
    }

    /// Pisahkan header `Stripe-Signature` menjadi (stempel, semua nilai v1).
    ///
    /// Nilai `v1` bisa LEBIH DARI SATU: selama pergantian rahasia webhook,
    /// Stripe menandatangani dengan rahasia lama dan baru sekaligus. Menerima
    /// hanya yang pertama membuat pergantian rahasia menjatuhkan pembayaran
    /// yang sah.
    pub fn pisah_header(h: &str) -> (Option<&str>, Vec<&str>) {
        let mut stempel = None;
        let mut v1 = Vec::new();
        for bagian in h.split(',') {
            let bagian = bagian.trim();
            if let Some(t) = bagian.strip_prefix("t=") {
                stempel = Some(t);
            } else if let Some(v) = bagian.strip_prefix("v1=") {
                v1.push(v);
            }
        }
        (stempel, v1)
    }

    fn status_dari(tipe: &str) -> Option<StatusBayar> {
        match tipe {
            "checkout.session.completed" | "payment_intent.succeeded" => Some(StatusBayar::Lunas),
            "payment_intent.processing" => Some(StatusBayar::Menunggu),
            "payment_intent.payment_failed" => Some(StatusBayar::Gagal),
            "checkout.session.expired" => Some(StatusBayar::Kedaluwarsa),
            "charge.refunded" => Some(StatusBayar::Dikembalikan),
            // Stripe mengirim PULUHAN jenis peristiwa lain. Tak satu pun
            // berarti apa-apa bagi order kita, dan menjawabnya 200 membuat
            // Stripe berhenti mengulangnya.
            _ => None,
        }
    }
}

/// Rupiah → satuan terkecil yang diminta Stripe.
///
/// PERIKSA INI SEBELUM MENYALAKAN PRODUKSI. Stripe menyatakan nominal dalam
/// satuan terkecil mata uangnya, dan daftar "zero-decimal" mereka menentukan
/// apakah IDR dikalikan 100 atau tidak. Salah arah di sini berarti menagih
/// seratus kali lipat atau seperseratus — dua-duanya bencana yang baru
/// ketahuan setelah uang berpindah.
fn ke_satuan_terkecil(amount: Decimal) -> i64 {
    (amount * Decimal::from(100)).round().try_into().unwrap_or(0)
}

#[async_trait]
impl GatewayBayar for Stripe {
    fn nama(&self) -> &'static str {
        "stripe"
    }

    async fn buat_tagihan(&self, req: &PermintaanBayar) -> anyhow::Result<TagihanDibuat> {
        let nominal = ke_satuan_terkecil(req.amount).to_string();
        let mut form: Vec<(String, String)> = vec![
            ("mode".into(), "payment".into()),
            ("success_url".into(), self.success_url.clone()),
            ("cancel_url".into(), self.cancel_url.clone()),
            // Kode order kita ikut sebagai referensi klien — inilah yang
            // dikirim balik di webhook dan menjadi jembatan ke order kita.
            ("client_reference_id".into(), req.order_code.clone()),
            ("line_items[0][quantity]".into(), "1".into()),
            ("line_items[0][price_data][currency]".into(), "idr".into()),
            (
                "line_items[0][price_data][unit_amount]".into(),
                nominal,
            ),
            (
                "line_items[0][price_data][product_data][name]".into(),
                format!("Order {}", req.order_code),
            ),
            // Ikut di metadata juga: `client_reference_id` hanya ada pada
            // Checkout Session, sedangkan sebagian peristiwa membawa
            // PaymentIntent yang tak mengenalnya.
            ("metadata[order_code]".into(), req.order_code.clone()),
        ];
        if let Some(e) = &req.customer_email {
            form.push(("customer_email".into(), e.clone()));
        }

        let resp = self
            .http
            .post("https://api.stripe.com/v1/checkout/sessions")
            .bearer_auth(&self.secret_key)
            .form(&form)
            .send()
            .await
            .context("stripe: permintaan checkout session gagal")?;

        let status = resp.status();
        let raw: Value = resp.json().await.context("stripe: jawaban bukan JSON")?;
        if !status.is_success() {
            return Err(anyhow!("stripe menolak ({status}): {raw}"));
        }

        Ok(TagihanDibuat {
            provider_ref: req.order_code.clone(),
            pay_reference: raw.get("id").and_then(Value::as_str).map(str::to_string),
            pay_url: raw.get("url").and_then(Value::as_str).map(str::to_string),
            expires_at: raw
                .get("expires_at")
                .and_then(Value::as_i64)
                .and_then(|d| chrono::DateTime::from_timestamp(d, 0)),
            raw,
        })
    }

    fn tafsir_webhook(
        &self,
        raw: &[u8],
        headers: &axum::http::HeaderMap,
    ) -> Result<KabarWebhook, GalatWebhook> {
        let header = header_str(headers, "stripe-signature")
            .ok_or_else(|| GalatWebhook::MuatanRusak("header Stripe-Signature absen".into()))?;
        let (stempel, daftar_v1) = Self::pisah_header(header);
        let stempel = stempel
            .ok_or_else(|| GalatWebhook::MuatanRusak("stempel t= absen".into()))?;

        // Jendela waktu DULU, sebelum HMAC: menolak yang basi lebih murah
        // daripada menghitung hash atasnya, dan ini jalur yang terbuka ke
        // internet.
        let t: i64 = stempel
            .parse()
            .map_err(|_| GalatWebhook::MuatanRusak("stempel bukan bilangan".into()))?;
        let selisih = (chrono::Utc::now().timestamp() - t).abs();
        if selisih > TOLERANSI_DETIK {
            return Err(GalatWebhook::TandaTanganSalah);
        }

        let dihitung = Self::tanda_tangan(stempel, raw, &self.webhook_secret);
        let cocok = daftar_v1
            .iter()
            .any(|v| banding_aman(dihitung.as_bytes(), v.as_bytes()));
        if !cocok {
            return Err(GalatWebhook::TandaTanganSalah);
        }

        let v: Value = serde_json::from_slice(raw)
            .map_err(|e| GalatWebhook::MuatanRusak(format!("bukan JSON: {e}")))?;

        let tipe = v.get("type").and_then(Value::as_str).unwrap_or("");
        let Some(status) = Self::status_dari(tipe) else {
            return Err(GalatWebhook::Diabaikan);
        };

        let obj = v.pointer("/data/object").unwrap_or(&Value::Null);
        // Urutan pencarian: `client_reference_id` (Checkout Session) dulu,
        // lalu metadata (PaymentIntent dan peristiwa lain).
        let order_code = obj
            .get("client_reference_id")
            .and_then(Value::as_str)
            .or_else(|| obj.pointer("/metadata/order_code").and_then(Value::as_str))
            .ok_or_else(|| {
                GalatWebhook::MuatanRusak("tak ada client_reference_id maupun metadata.order_code".into())
            })?;

        let event_id = v.get("id").and_then(Value::as_str).unwrap_or(tipe);

        Ok(KabarWebhook {
            // Stripe memberi id peristiwa yang benar-benar unik, jadi ia
            // dipakai apa adanya — tak perlu disusun dari bagian lain.
            event_key: event_id.to_string(),
            provider_ref: order_code.to_string(),
            status,
            amount: obj
                .get("amount_total")
                .or_else(|| obj.get("amount_received"))
                .or_else(|| obj.get("amount"))
                .and_then(Value::as_i64)
                .map(|n| Decimal::from(n) / Decimal::from(100)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAHASIA: &str = "whsec_ujicoba";

    fn header_sah(badan: &[u8]) -> String {
        let t = chrono::Utc::now().timestamp().to_string();
        let v1 = Stripe::tanda_tangan(&t, badan, RAHASIA);
        format!("t={t},v1={v1}")
    }

    fn stripe() -> Stripe {
        Stripe::baru(
            "sk_uji".into(),
            RAHASIA.into(),
            "https://x/ok".into(),
            "https://x/batal".into(),
            reqwest::Client::new(),
        )
    }

    fn headers(v: &str) -> axum::http::HeaderMap {
        let mut h = axum::http::HeaderMap::new();
        h.insert("stripe-signature", v.parse().unwrap());
        h
    }

    /// Header dengan BEBERAPA `v1` harus diterima bila salah satunya cocok —
    /// itulah bentuk yang dikirim Stripe selama rahasia webhook diganti.
    #[test]
    fn menerima_salah_satu_dari_banyak_v1() {
        let badan = br#"{"id":"evt_1","type":"checkout.session.completed","data":{"object":{"client_reference_id":"ORD-1"}}}"#;
        let t = chrono::Utc::now().timestamp().to_string();
        let sah = Stripe::tanda_tangan(&t, badan, RAHASIA);
        let h = headers(&format!("t={t},v1=deadbeef,v1={sah}"));

        let kabar = stripe().tafsir_webhook(badan, &h).expect("harus diterima");
        assert_eq!(kabar.status, StatusBayar::Lunas);
        assert_eq!(kabar.provider_ref, "ORD-1");
        assert_eq!(kabar.event_key, "evt_1");
    }

    /// Satu byte berubah di badan → tanda tangan tak cocok lagi. Inilah
    /// seluruh gunanya: muatan yang disunting tak boleh lolos.
    #[test]
    fn badan_disunting_ditolak() {
        let asli = br#"{"id":"evt_1","type":"checkout.session.completed","data":{"object":{"client_reference_id":"ORD-1"}}}"#;
        let h = headers(&header_sah(asli));
        let palsu = br#"{"id":"evt_1","type":"checkout.session.completed","data":{"object":{"client_reference_id":"ORD-9"}}}"#;

        assert_eq!(
            stripe().tafsir_webhook(palsu, &h),
            Err(GalatWebhook::TandaTanganSalah)
        );
    }

    /// Callback sah yang direkam lalu dikirim ulang berhari-hari kemudian
    /// tetap punya tanda tangan yang sah — hanya jendela waktu yang
    /// menghentikannya.
    #[test]
    fn stempel_basi_ditolak() {
        let badan = br#"{"id":"evt_1","type":"checkout.session.completed","data":{"object":{"client_reference_id":"ORD-1"}}}"#;
        let t = (chrono::Utc::now().timestamp() - TOLERANSI_DETIK - 60).to_string();
        let v1 = Stripe::tanda_tangan(&t, badan, RAHASIA);
        let h = headers(&format!("t={t},v1={v1}"));

        assert_eq!(
            stripe().tafsir_webhook(badan, &h),
            Err(GalatWebhook::TandaTanganSalah),
            "tanda tangannya sah, tapi umurnya tidak"
        );
    }

    /// Peristiwa yang tak kita pedulikan dijawab sebagai "diabaikan", bukan
    /// galat — supaya Stripe berhenti mengulangnya.
    #[test]
    fn peristiwa_tak_relevan_diabaikan() {
        let badan = br#"{"id":"evt_2","type":"customer.created","data":{"object":{}}}"#;
        let h = headers(&header_sah(badan));
        assert_eq!(
            stripe().tafsir_webhook(badan, &h),
            Err(GalatWebhook::Diabaikan)
        );
    }

    #[test]
    fn header_tanpa_stempel_ditolak() {
        let badan = b"{}";
        let h = headers("v1=abc");
        assert!(matches!(
            stripe().tafsir_webhook(badan, &h),
            Err(GalatWebhook::MuatanRusak(_))
        ));
    }
}
