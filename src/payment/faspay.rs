//! payment/faspay.rs — Faspay Business (Debit) + payment notification.
//!
//! ── TANDA TANGAN ──────────────────────────────────────────────────────────
//!     signature = SHA1( MD5( user_id ‖ password ‖ bill_no ‖ payment_status_code ) )
//!
//! Dua lapis hash warisan dan keduanya sudah lama tak dianggap kuat sendirian.
//! Yang menahannya di sini bukan kekuatan algoritmanya melainkan `password`
//! yang tak pernah meninggalkan server. Karena itu dua hal wajib:
//!
//!   • Pembandingannya tetap berwaktu-tetap. Algoritma lemah bukan alasan
//!     untuk menambah kebocoran lain.
//!   • `password` di sini adalah *merchant password* Faspay, dan pada sebagian
//!     dokumen mereka nilainya yang dipakai sudah berupa MD5 dari sandi asli.
//!     PERIKSA DI DASBOR mana yang berlaku untuk akunmu sebelum produksi —
//!     salah satu dari dua kemungkinan itu akan membuat SETIAP notifikasi
//!     ditolak, dan bentuk kegagalannya tak menunjukkan sebabnya.
//!
//! Untuk pembuatan tagihan, tanda tangannya BERBEDA — tanpa
//! `payment_status_code`, karena saat itu statusnya memang belum ada:
//!     signature = SHA1( MD5( user_id ‖ password ‖ bill_no ) )

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use md5::Md5;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sha1::Sha1;
use sha2::Digest;

use super::{
    banding_aman, GalatWebhook, GatewayBayar, KabarWebhook, PermintaanBayar, StatusBayar,
    TagihanDibuat,
};

pub struct Faspay {
    user_id: String,
    password: String,
    merchant_id: String,
    base_url: String,
    return_url: String,
    http: reqwest::Client,
}

impl Faspay {
    pub fn baru(
        user_id: String,
        password: String,
        merchant_id: String,
        produksi: bool,
        return_url: String,
        http: reqwest::Client,
    ) -> Self {
        Self {
            user_id,
            password,
            merchant_id,
            base_url: if produksi {
                "https://web.faspay.co.id".into()
            } else {
                "https://dev.faspay.co.id".into()
            },
            return_url,
            http,
        }
    }

    /// SHA1(MD5(bagian-bagian yang disambung)).
    pub fn tanda_tangan(bagian: &[&str]) -> String {
        let mut md5 = Md5::new();
        for b in bagian {
            md5.update(b.as_bytes());
        }
        let tengah = hex::encode(md5.finalize());

        let mut sha1 = Sha1::new();
        sha1.update(tengah.as_bytes());
        hex::encode(sha1.finalize())
    }

    /// Kosakata Faspay berupa ANGKA dalam teks.
    ///
    /// Hanya `"2"` yang berarti lunas. `"1"` (sedang diproses) terutama
    /// berbahaya: ia terbaca seperti keberhasilan bagi yang membaca sekilas,
    /// padahal uangnya belum tentu masuk.
    fn status_dari(kode: &str) -> StatusBayar {
        match kode {
            "2" => StatusBayar::Lunas,
            "0" | "1" => StatusBayar::Menunggu,
            "4" => StatusBayar::Dikembalikan,
            "7" => StatusBayar::Kedaluwarsa,
            // "3" gagal, "5" tak ada tagihan, "8" dibatalkan, "9" tak dikenal.
            _ => StatusBayar::Gagal,
        }
    }
}

#[async_trait]
impl GatewayBayar for Faspay {
    fn nama(&self) -> &'static str {
        "faspay"
    }

    async fn buat_tagihan(&self, req: &PermintaanBayar) -> anyhow::Result<TagihanDibuat> {
        let bill_no = req.order_code.clone();
        let signature = Self::tanda_tangan(&[&self.user_id, &self.password, &bill_no]);
        let kedaluwarsa = (chrono::Utc::now() + chrono::Duration::hours(2))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let body = json!({
            "request": "Post Data Transaction",
            "merchant_id": self.merchant_id,
            "bill_no": bill_no,
            "bill_date": chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            "bill_expired": kedaluwarsa,
            "bill_desc": format!("Order {bill_no}"),
            "bill_currency": "IDR",
            // Faspay menolak nominal berpecahan.
            "bill_total": req.amount.round().to_string(),
            "cust_no": req.customer_phone.clone().unwrap_or_default(),
            "cust_name": req.customer_name,
            "msisdn": req.customer_phone.clone().unwrap_or_default(),
            "email": req.customer_email.clone().unwrap_or_default(),
            "terminal": "10",
            "pay_type": "1",
            "return_url": self.return_url,
            "signature": signature,
        });

        let resp = self
            .http
            .post(format!("{}/cvr/300011/10", self.base_url))
            .json(&body)
            .send()
            .await
            .context("faspay: permintaan post data transaction gagal")?;

        let status = resp.status();
        let raw: Value = resp.json().await.context("faspay: jawaban bukan JSON")?;
        if !status.is_success() {
            return Err(anyhow!("faspay menolak ({status}): {raw}"));
        }
        // Faspay membalas 200 bahkan untuk penolakan; yang menentukan adalah
        // `response_code` — "00" sukses.
        let kode = raw
            .get("response_code")
            .and_then(Value::as_str)
            .unwrap_or("");
        if kode != "00" {
            return Err(anyhow!("faspay menolak (response_code {kode}): {raw}"));
        }

        Ok(TagihanDibuat {
            // `bill_no` kita sendiri, karena itulah yang dikirim balik di
            // notifikasi.
            provider_ref: bill_no,
            pay_reference: raw.get("trx_id").and_then(Value::as_str).map(str::to_string),
            pay_url: raw
                .get("redirect_url")
                .and_then(Value::as_str)
                .map(str::to_string),
            expires_at: None,
            raw,
        })
    }

    fn tafsir_webhook(
        &self,
        raw: &[u8],
        _headers: &axum::http::HeaderMap,
    ) -> Result<KabarWebhook, GalatWebhook> {
        let v: Value = serde_json::from_slice(raw)
            .map_err(|e| GalatWebhook::MuatanRusak(format!("bukan JSON: {e}")))?;

        let teks = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("");
        let bill_no = teks("bill_no");
        let kode_status = teks("payment_status_code");
        let kiriman = teks("signature");

        if bill_no.is_empty() || kiriman.is_empty() {
            return Err(GalatWebhook::MuatanRusak(
                "bill_no atau signature kosong".into(),
            ));
        }

        let dihitung =
            Self::tanda_tangan(&[&self.user_id, &self.password, bill_no, kode_status]);
        if !banding_aman(dihitung.as_bytes(), kiriman.as_bytes()) {
            return Err(GalatWebhook::TandaTanganSalah);
        }

        Ok(KabarWebhook {
            event_key: format!("{bill_no}:{kode_status}"),
            provider_ref: bill_no.to_string(),
            status: Self::status_dari(kode_status),
            amount: v
                .get("payment_total")
                .and_then(|x| match x {
                    Value::String(s) => s.parse().ok(),
                    Value::Number(n) => n.as_f64().and_then(Decimal::from_f64_retain),
                    _ => None,
                }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn faspay() -> Faspay {
        Faspay::baru(
            "bot12345".into(),
            "sandi".into(),
            "12345".into(),
            false,
            "https://x/ok".into(),
            reqwest::Client::new(),
        )
    }

    fn badan(bill: &str, kode: &str, tt: &str) -> Vec<u8> {
        format!(
            r#"{{"bill_no":"{bill}","payment_status_code":"{kode}","payment_total":"150000.00","signature":"{tt}"}}"#
        )
        .into_bytes()
    }

    /// Urutan bagian tanda tangan adalah bagian dari kontraknya.
    #[test]
    fn tanda_tangan_dua_lapis_sesuai_urutan() {
        let a = Faspay::tanda_tangan(&["bot12345", "sandi", "INV-1", "2"]);
        let b = Faspay::tanda_tangan(&["sandi", "bot12345", "INV-1", "2"]);
        assert_ne!(a, b, "urutan tak boleh tertukar");
        assert_eq!(a.len(), 40, "SHA-1 heksadesimal = 40 karakter");
    }

    #[test]
    fn notifikasi_sah_diterima() {
        let f = faspay();
        let tt = Faspay::tanda_tangan(&["bot12345", "sandi", "INV-1", "2"]);
        let k = f
            .tafsir_webhook(&badan("INV-1", "2", &tt), &axum::http::HeaderMap::new())
            .expect("harus diterima");
        assert_eq!(k.status, StatusBayar::Lunas);
        assert_eq!(k.provider_ref, "INV-1");
    }

    /// Tanda tangan yang dihitung untuk STATUS LAIN tak boleh lolos —
    /// kalau lolos, siapa pun yang pernah melihat notifikasi "pending" bisa
    /// mengubahnya menjadi "lunas".
    #[test]
    fn tanda_tangan_status_lain_ditolak() {
        let f = faspay();
        let tt_pending = Faspay::tanda_tangan(&["bot12345", "sandi", "INV-1", "1"]);
        assert_eq!(
            f.tafsir_webhook(&badan("INV-1", "2", &tt_pending), &axum::http::HeaderMap::new()),
            Err(GalatWebhook::TandaTanganSalah)
        );
    }

    #[test]
    fn hanya_kode_2_yang_lunas() {
        assert_eq!(Faspay::status_dari("2"), StatusBayar::Lunas);
        assert_eq!(Faspay::status_dari("1"), StatusBayar::Menunggu);
        assert_eq!(Faspay::status_dari("3"), StatusBayar::Gagal);
        assert_eq!(Faspay::status_dari("7"), StatusBayar::Kedaluwarsa);
    }
}
