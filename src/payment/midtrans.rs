//! payment/midtrans.rs — Midtrans Snap + notifikasi HTTP.
//!
//! ── TANDA TANGAN ──────────────────────────────────────────────────────────
//!     signature_key = SHA512(order_id ‖ status_code ‖ gross_amount ‖ ServerKey)
//!
//! Tiga hal yang mudah salah dan semuanya berakhir sebagai "callback selalu
//! ditolak", tanpa petunjuk mana yang keliru:
//!
//!   1. `gross_amount` harus dipakai PERSIS seperti yang dikirim Midtrans —
//!      teks `"100000.00"`, bukan angka 100000. Mem-parsing lalu memformat
//!      ulang menghasilkan untaian berbeda dan hash yang berbeda pula.
//!   2. `status_code` juga teks (`"200"`), bukan bilangan.
//!   3. Yang dipakai ServerKey, BUKAN ClientKey. Keduanya mirip
//!      (`SB-Mid-server-…` vs `SB-Mid-client-…`) dan tertukar adalah kesalahan
//!      paling umum saat pertama memasang.

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sha2::{Digest, Sha512};

use super::{
    banding_aman, GalatWebhook, GatewayBayar, KabarWebhook, PermintaanBayar, StatusBayar,
    TagihanDibuat,
};

pub struct Midtrans {
    server_key: String,
    /// `https://app.sandbox.midtrans.com` atau `https://app.midtrans.com`.
    base_url: String,
    http: reqwest::Client,
}

impl Midtrans {
    pub fn baru(server_key: String, produksi: bool, http: reqwest::Client) -> Self {
        Self {
            server_key,
            base_url: if produksi {
                "https://app.midtrans.com".into()
            } else {
                "https://app.sandbox.midtrans.com".into()
            },
            http,
        }
    }

    /// Terjemahkan kosakata Midtrans ke kosakata kita.
    ///
    /// `capture` SENDIRIAN bukan pelunasan: untuk kartu kredit ia berarti dana
    /// sudah ditahan tetapi mesin antifraud Midtrans masih menimbangnya.
    /// Hanya `fraud_status = accept` yang menjadikannya final; `challenge`
    /// menunggu keputusan manual dan bisa berakhir ditolak. Menerbitkan tiket
    /// pada `challenge` berarti menerbitkannya untuk transaksi yang mungkin
    /// dibatalkan.
    fn status_dari(transaction_status: &str, fraud_status: Option<&str>) -> StatusBayar {
        match transaction_status {
            "settlement" => StatusBayar::Lunas,
            "capture" => match fraud_status {
                Some("accept") | None => StatusBayar::Lunas,
                _ => StatusBayar::Menunggu,
            },
            "pending" => StatusBayar::Menunggu,
            "expire" => StatusBayar::Kedaluwarsa,
            "refund" | "partial_refund" | "chargeback" => StatusBayar::Dikembalikan,
            // `deny`, `cancel`, `failure`, dan apa pun yang belum dikenal.
            // Bawaan yang aman adalah TIDAK melunaskan.
            _ => StatusBayar::Gagal,
        }
    }

    /// Hitung `signature_key` seperti yang Midtrans hitung.
    ///
    /// Dipisah sebagai fungsi murni supaya bisa diuji dengan vektor yang
    /// jawabannya sudah diketahui, tanpa menyentuh jaringan.
    pub fn tanda_tangan(order_id: &str, status_code: &str, gross_amount: &str, server_key: &str) -> String {
        let mut h = Sha512::new();
        h.update(order_id.as_bytes());
        h.update(status_code.as_bytes());
        h.update(gross_amount.as_bytes());
        h.update(server_key.as_bytes());
        hex::encode(h.finalize())
    }
}

#[async_trait]
impl GatewayBayar for Midtrans {
    fn nama(&self) -> &'static str {
        "midtrans"
    }

    async fn buat_tagihan(&self, req: &PermintaanBayar) -> anyhow::Result<TagihanDibuat> {
        // Midtrans menolak `gross_amount` pecahan: nominalnya WAJIB bulat.
        // Dibulatkan di sini, sekali, supaya tak ada jalur yang mengirim
        // angka berbeda dari yang tersimpan di transaksi kita.
        let gross = req.amount.round().to_string();

        let mut customer = json!({ "first_name": req.customer_name });
        if let Some(e) = &req.customer_email {
            customer["email"] = json!(e);
        }
        if let Some(p) = &req.customer_phone {
            customer["phone"] = json!(p);
        }

        let body = json!({
            "transaction_details": {
                "order_id": req.order_code,
                "gross_amount": gross.parse::<i64>().unwrap_or(0),
            },
            "customer_details": customer,
        });

        let resp = self
            .http
            .post(format!("{}/snap/v1/transactions", self.base_url))
            // Basic auth: ServerKey sebagai username, sandi KOSONG. Titik dua
            // di ujung wajib ada — tanpa itu Midtrans membalas 401.
            .basic_auth(&self.server_key, Some(""))
            .json(&body)
            .send()
            .await
            .context("midtrans: permintaan snap gagal")?;

        let status = resp.status();
        let raw: Value = resp
            .json()
            .await
            .context("midtrans: jawaban snap bukan JSON")?;
        if !status.is_success() {
            return Err(anyhow!("midtrans menolak ({status}): {raw}"));
        }

        Ok(TagihanDibuat {
            // Referensinya adalah kode order KITA, karena itulah yang dikirim
            // balik Midtrans sebagai `order_id` di setiap notifikasi.
            provider_ref: req.order_code.clone(),
            pay_reference: raw.get("token").and_then(Value::as_str).map(str::to_string),
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
        let order_id = teks("order_id");
        let status_code = teks("status_code");
        let gross_amount = teks("gross_amount");
        let kiriman = teks("signature_key");

        if order_id.is_empty() || kiriman.is_empty() {
            return Err(GalatWebhook::MuatanRusak(
                "order_id atau signature_key kosong".into(),
            ));
        }

        let dihitung = Self::tanda_tangan(order_id, status_code, gross_amount, &self.server_key);
        if !banding_aman(dihitung.as_bytes(), kiriman.as_bytes()) {
            return Err(GalatWebhook::TandaTanganSalah);
        }

        let transaction_status = teks("transaction_status");
        let fraud = v.get("fraud_status").and_then(Value::as_str);
        let status = Self::status_dari(transaction_status, fraud);

        Ok(KabarWebhook {
            // Status ikut masuk kunci: callback ULANG untuk status yang sama
            // diabaikan, sedangkan perpindahan pending → settlement
            // menghasilkan kunci baru dan tetap diproses.
            event_key: format!("{order_id}:{transaction_status}:{status_code}"),
            provider_ref: order_id.to_string(),
            status,
            amount: gross_amount.parse::<Decimal>().ok(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vektor dari contoh dokumentasi Midtrans. Yang dijaga bukan "hash-nya
    /// benar" melainkan URUTAN dan BENTUK masukannya — order_id, status_code,
    /// gross_amount, lalu ServerKey, semuanya sebagai teks apa adanya.
    #[test]
    fn tanda_tangan_mengikuti_urutan_midtrans() {
        let t = Midtrans::tanda_tangan("order-101", "200", "10000.00", "kunci-rahasia");
        let manual = {
            let mut h = Sha512::new();
            h.update(b"order-101200" as &[u8]);
            h.update(b"10000.00");
            h.update(b"kunci-rahasia");
            hex::encode(h.finalize())
        };
        assert_eq!(t, manual);
        assert_eq!(t.len(), 128, "SHA-512 heksadesimal = 128 karakter");
    }

    /// `gross_amount` dipakai APA ADANYA. "10000.00" dan "10000" adalah dua
    /// masukan berbeda — inilah sebab paling umum "callback selalu ditolak".
    #[test]
    fn format_nominal_mengubah_tanda_tangan() {
        let a = Midtrans::tanda_tangan("o1", "200", "10000.00", "k");
        let b = Midtrans::tanda_tangan("o1", "200", "10000", "k");
        assert_ne!(a, b);
    }

    #[test]
    fn capture_challenge_bukan_pelunasan() {
        assert_eq!(
            Midtrans::status_dari("capture", Some("accept")),
            StatusBayar::Lunas
        );
        assert_eq!(
            Midtrans::status_dari("capture", Some("challenge")),
            StatusBayar::Menunggu,
            "challenge masih ditimbang antifraud — tiket tak boleh terbit"
        );
        assert_eq!(Midtrans::status_dari("settlement", None), StatusBayar::Lunas);
        assert_eq!(Midtrans::status_dari("expire", None), StatusBayar::Kedaluwarsa);
        assert_eq!(Midtrans::status_dari("deny", None), StatusBayar::Gagal);
    }

    /// Status yang belum dikenal TIDAK melunaskan. Kosakata gateway bisa
    /// bertambah, dan bawaan yang salah di sini berarti tiket terbit untuk
    /// status yang belum pernah kita baca artinya.
    #[test]
    fn status_asing_tidak_melunaskan() {
        assert_eq!(Midtrans::status_dari("sesuatu_yang_baru", None), StatusBayar::Gagal);
    }
}
