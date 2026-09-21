//! payment/flip.rs — Flip for Business (Accept Payment) + callback.
//!
//! ── FLIP TIDAK MENANDATANGANI CALLBACK-NYA ────────────────────────────────
//! Ini perbedaan penting dari tiga gateway lain di modul ini, dan ia harus
//! dinyatakan terang-terangan supaya tak ada yang mengira keamanannya setara.
//!
//! Callback Flip datang sebagai `application/x-www-form-urlencoded` dengan dua
//! medan: `data` (JSON) dan `token`. `token` adalah *validation token* TETAP
//! dari dasbor Flip — bukan tanda tangan atas muatannya. Artinya:
//!
//!   • Ia membuktikan pengirimnya tahu rahasia kita, TAPI tidak membuktikan
//!     muatannya tak disunting di tengah jalan.
//!   • Ia sama untuk setiap callback, jadi sekali bocor ia bocor selamanya —
//!     tak ada stempel waktu yang membuatnya basi seperti pada Stripe.
//!
//! Yang bisa kita lakukan, dan dilakukan di sini: bandingkan token dengan
//! waktu tetap, WAJIBKAN HTTPS di sisi proxy, dan — yang paling menentukan —
//! jangan pernah percaya nominal dari callback tanpa mencocokkannya dengan
//! nominal transaksi yang tersimpan (lihat `webhook.rs`).
//!
//! Flip mengulang callback 5 kali dengan jeda 2 menit bila kita tidak membalas
//! 200 dalam 30 detik. Itu sebabnya idempotensi di `webhook.rs` bukan
//! kemewahan: tanpa itu satu pembayaran bisa menerbitkan lima set tiket.

use anyhow::{anyhow, Context};
use async_trait::async_trait;
use rust_decimal::Decimal;
use serde_json::Value;

use super::{
    banding_aman, GalatWebhook, GatewayBayar, KabarWebhook, PermintaanBayar, StatusBayar,
    TagihanDibuat,
};

pub struct Flip {
    secret_key: String,
    validation_token: String,
    base_url: String,
    redirect_url: String,
    http: reqwest::Client,
}

impl Flip {
    pub fn baru(
        secret_key: String,
        validation_token: String,
        produksi: bool,
        redirect_url: String,
        http: reqwest::Client,
    ) -> Self {
        Self {
            secret_key,
            validation_token,
            base_url: if produksi {
                "https://bigflip.id/api/v2".into()
            } else {
                "https://bigflip.id/big_sandbox_api/v2".into()
            },
            redirect_url,
            http,
        }
    }

    fn status_dari(s: &str) -> StatusBayar {
        match s {
            "SUCCESSFUL" => StatusBayar::Lunas,
            "PENDING" => StatusBayar::Menunggu,
            "CANCELLED" | "EXPIRED" => StatusBayar::Kedaluwarsa,
            // `FAILED` dan apa pun yang belum dikenal.
            _ => StatusBayar::Gagal,
        }
    }
}

#[async_trait]
impl GatewayBayar for Flip {
    fn nama(&self) -> &'static str {
        "flip"
    }

    async fn buat_tagihan(&self, req: &PermintaanBayar) -> anyhow::Result<TagihanDibuat> {
        let form = [
            ("title", format!("Order {}", req.order_code)),
            ("amount", req.amount.round().to_string()),
            // SINGLE = tagihan sekali bayar dengan nominal tetap. `MULTIPLE`
            // akan membiarkan orang membayar berkali-kali pada tautan yang
            // sama, dan tiap pembayaran memicu callback baru.
            ("type", "SINGLE".into()),
            ("is_address_required", "0".into()),
            ("is_phone_number_required", "0".into()),
            ("redirect_url", self.redirect_url.clone()),
            ("sender_name", req.customer_name.clone()),
        ];

        let resp = self
            .http
            .post(format!("{}/pwf/bill", self.base_url))
            // Flip memakai Basic auth dengan secret key sebagai username dan
            // sandi kosong — pola yang sama dengan Midtrans.
            .basic_auth(&self.secret_key, Some(""))
            .form(&form)
            .send()
            .await
            .context("flip: permintaan bill gagal")?;

        let status = resp.status();
        let raw: Value = resp.json().await.context("flip: jawaban bukan JSON")?;
        if !status.is_success() {
            return Err(anyhow!("flip menolak ({status}): {raw}"));
        }

        // `link_id` numerik — dijadikan teks supaya bentuknya seragam dengan
        // gateway lain dan cocok dengan kolom `provider_ref`.
        let link_id = raw
            .get("link_id")
            .map(|v| match v {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                lain => lain.to_string(),
            })
            .ok_or_else(|| anyhow!("flip: jawaban tanpa link_id: {raw}"))?;

        Ok(TagihanDibuat {
            provider_ref: link_id,
            pay_reference: raw
                .get("bill_payment")
                .and_then(|b| b.get("receiver_bank_account"))
                .and_then(|r| r.get("account_number"))
                .and_then(Value::as_str)
                .map(str::to_string),
            pay_url: raw.get("link_url").and_then(Value::as_str).map(|u| {
                // Flip mengembalikan tautan tanpa skema.
                if u.starts_with("http") {
                    u.to_string()
                } else {
                    format!("https://{u}")
                }
            }),
            expires_at: None,
            raw,
        })
    }

    fn tafsir_webhook(
        &self,
        raw: &[u8],
        _headers: &axum::http::HeaderMap,
    ) -> Result<KabarWebhook, GalatWebhook> {
        // Badan berbentuk form, bukan JSON.
        let mut data_json: Option<String> = None;
        let mut token: Option<String> = None;
        for (k, v) in form_urlencoded::parse(raw) {
            match k.as_ref() {
                "data" => data_json = Some(v.into_owned()),
                "token" => token = Some(v.into_owned()),
                _ => {}
            }
        }

        let token = token.ok_or_else(|| GalatWebhook::MuatanRusak("token absen".into()))?;
        if !banding_aman(token.as_bytes(), self.validation_token.as_bytes()) {
            return Err(GalatWebhook::TandaTanganSalah);
        }

        let data_json =
            data_json.ok_or_else(|| GalatWebhook::MuatanRusak("medan data absen".into()))?;
        let d: Value = serde_json::from_str(&data_json)
            .map_err(|e| GalatWebhook::MuatanRusak(format!("data bukan JSON: {e}")))?;

        let link_id = d
            .get("bill_link_id")
            .map(|v| match v {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                lain => lain.to_string(),
            })
            .ok_or_else(|| GalatWebhook::MuatanRusak("bill_link_id absen".into()))?;

        let status_teks = d.get("status").and_then(Value::as_str).unwrap_or("");
        // `id` adalah identitas PEMBAYARAN (bukan tagihan), jadi ia membedakan
        // dua pembayaran berbeda pada tagihan yang sama.
        let bayar_id = d
            .get("id")
            .map(|v| match v {
                Value::Number(n) => n.to_string(),
                Value::String(s) => s.clone(),
                lain => lain.to_string(),
            })
            .unwrap_or_else(|| link_id.clone());

        Ok(KabarWebhook {
            event_key: format!("{bayar_id}:{status_teks}"),
            provider_ref: link_id,
            status: Self::status_dari(status_teks),
            amount: d
                .get("amount")
                .and_then(|v| match v {
                    Value::Number(n) => n.as_i64().map(Decimal::from),
                    Value::String(s) => s.parse().ok(),
                    _ => None,
                }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flip() -> Flip {
        Flip::baru(
            "rahasia".into(),
            "token-sah".into(),
            false,
            "https://x/ok".into(),
            reqwest::Client::new(),
        )
    }

    fn badan(token: &str, status: &str) -> Vec<u8> {
        let data = format!(
            r#"{{"id":9001,"bill_link_id":777,"status":"{status}","amount":150000}}"#
        );
        form_urlencoded::Serializer::new(String::new())
            .append_pair("data", &data)
            .append_pair("token", token)
            .finish()
            .into_bytes()
    }

    #[test]
    fn token_sah_diterima() {
        let b = badan("token-sah", "SUCCESSFUL");
        let k = flip()
            .tafsir_webhook(&b, &axum::http::HeaderMap::new())
            .expect("harus diterima");
        assert_eq!(k.status, StatusBayar::Lunas);
        assert_eq!(k.provider_ref, "777");
        assert_eq!(k.amount, Some(Decimal::from(150000)));
    }

    #[test]
    fn token_salah_ditolak() {
        let b = badan("token-palsu", "SUCCESSFUL");
        assert_eq!(
            flip().tafsir_webhook(&b, &axum::http::HeaderMap::new()),
            Err(GalatWebhook::TandaTanganSalah)
        );
    }

    /// Dua callback untuk pembayaran yang SAMA menghasilkan kunci yang sama —
    /// itulah yang membuat pengulangan Flip (5 kali) tak menerbitkan lima set
    /// tiket.
    #[test]
    fn callback_berulang_berkunci_sama() {
        let b = badan("token-sah", "SUCCESSFUL");
        let kosong = axum::http::HeaderMap::new();
        let a = flip().tafsir_webhook(&b, &kosong).unwrap();
        let c = flip().tafsir_webhook(&b, &kosong).unwrap();
        assert_eq!(a.event_key, c.event_key);
    }

    #[test]
    fn status_gagal_dan_kedaluwarsa_dipetakan() {
        assert_eq!(Flip::status_dari("FAILED"), StatusBayar::Gagal);
        assert_eq!(Flip::status_dari("EXPIRED"), StatusBayar::Kedaluwarsa);
        assert_eq!(Flip::status_dari("PENDING"), StatusBayar::Menunggu);
    }
}
