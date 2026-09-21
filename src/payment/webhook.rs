//! payment/webhook.rs — satu-satunya jalan sebuah order menjadi LUNAS.
//!
//! ── URUTANNYA ADALAH RANCANGANNYA ─────────────────────────────────────────
//!
//!   1. Verifikasi tanda tangan. Sebelum ini tak ada satu pun keputusan yang
//!      boleh diambil dari isi muatannya.
//!   2. Catat peristiwanya dengan `ON CONFLICT DO NOTHING`. Yang KALAH pada
//!      `INSERT` itu adalah duplikat, dan ia berhenti di sini.
//!   3. Cocokkan nominal. Gateway yang melaporkan pembayaran lebih kecil
//!      daripada tagihan bukan pelunasan.
//!   4. Baru lunaskan ordernya.
//!
//! ── KENAPA IDEMPOTENSI LEWAT `INSERT`, BUKAN `SELECT` LALU `INSERT` ───────
//! Gateway mengulang callback-nya: Flip lima kali, Midtrans sampai dapat 200,
//! Stripe dengan jeda menaik berhari-hari. Pengulangan itu kerap tiba
//! BERSAMAAN, dan `SELECT` yang tak menemukan apa-apa lalu `INSERT` punya
//! celah di antara keduanya yang cukup lebar untuk dilewati dua permintaan
//! sekaligus — keduanya akan menerbitkan tiket.
//!
//! `INSERT … ON CONFLICT DO NOTHING` memindahkan keputusannya ke dalam satu
//! pernyataan yang dijamin basis data. Yang mendapat baris, dialah yang
//! berhak memproses. Persis pola yang sama dengan penjaga anti-oversell di
//! `repository/order.rs`.
//!
//! ── KODE BALASAN ──────────────────────────────────────────────────────────
//! 200 untuk apa pun yang sudah kita urus — termasuk duplikat dan peristiwa
//! yang tak relevan — supaya gateway berhenti mengulang. 401 HANYA untuk
//! tanda tangan yang salah: itu bukan pengulangan yang gagal, itu pesan yang
//! bukan dari mereka (atau kunci kita salah pasang), dan mengulangnya tak
//! akan pernah menolong.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use rust_decimal::Decimal;

use super::{GalatWebhook, StatusBayar};
use crate::state::AppState;
use crate::utils::ulid::{new_ulid, ulid_to_vec};

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/api/payment/webhook/{provider}", post(terima))
}

async fn terima(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    // `body::Bytes` — badan MENTAH. Stripe menandatangani byte-nya apa adanya,
    // jadi ekstraktor apa pun yang mem-parse lebih dulu (Json, Form) merusak
    // verifikasinya secara permanen.
    badan: axum::body::Bytes,
) -> Response {
    let Some(gw) = state.gateway_bayar.get(provider.as_str()) else {
        tracing::warn!(provider, "webhook untuk gateway yang tak dikonfigurasi");
        return (StatusCode::NOT_FOUND, "gateway tak dikenal").into_response();
    };

    let kabar = match gw.tafsir_webhook(&badan, &headers) {
        Ok(k) => k,
        Err(GalatWebhook::Diabaikan) => {
            return (StatusCode::OK, "diabaikan").into_response();
        }
        Err(GalatWebhook::MuatanRusak(sebab)) => {
            tracing::warn!(provider, sebab, "webhook: muatan rusak");
            return (StatusCode::BAD_REQUEST, "muatan rusak").into_response();
        }
        Err(GalatWebhook::TandaTanganSalah) => {
            // `warn`, bukan `debug`: rentetan baris ini adalah satu-satunya
            // tanda bahwa ada yang mencoba memalsukan pembayaran — atau bahwa
            // kunci kita salah pasang, yang sama pentingnya untuk terlihat.
            tracing::warn!(provider, "webhook DITOLAK: tanda tangan tidak sah");
            catat_peristiwa(&state, &provider, "tanda-tangan-salah", &badan, false).await;
            return (StatusCode::UNAUTHORIZED, "tanda tangan tidak sah").into_response();
        }
    };

    // Idempotensi: yang kalah di sini adalah pengulangan.
    let baru = catat_peristiwa(&state, &provider, &kabar.event_key, &badan, true).await;
    if !baru {
        tracing::info!(provider, event = %kabar.event_key, "webhook diulang — dilewati");
        return (StatusCode::OK, "sudah diproses").into_response();
    }

    if let Err(e) = proses(&state, &provider, &kabar).await {
        // 500 supaya gateway MENGULANG. Kegagalan di sini bersifat sementara
        // (basis data sedang bermasalah), dan callback yang hilang berarti
        // pembeli membayar tanpa mendapat tiket.
        tracing::error!(provider, event = %kabar.event_key, error = %e, "webhook gagal diproses");
        return (StatusCode::INTERNAL_SERVER_ERROR, "gagal diproses").into_response();
    }

    tandai_selesai(&state, &provider, &kabar.event_key).await;
    (StatusCode::OK, "ok").into_response()
}

/// Catat peristiwa. `true` bila BARIS BARU benar-benar lahir (artinya
/// pemanggil inilah yang berhak memprosesnya).
async fn catat_peristiwa(
    state: &AppState,
    provider: &str,
    event_key: &str,
    badan: &[u8],
    signature_ok: bool,
) -> bool {
    let Ok(id) = ulid_to_vec(&new_ulid()) else {
        return false;
    };
    // Muatan disimpan sebagai JSON bila memang JSON; kalau bukan (Flip
    // mengirim form), dibungkus supaya kolom `jsonb` tetap bisa menerimanya
    // tanpa kehilangan aslinya.
    let payload: serde_json::Value = serde_json::from_slice(badan).unwrap_or_else(|_| {
        serde_json::json!({ "raw": String::from_utf8_lossy(badan) })
    });

    let Ok(client) = state.pool.get().await else {
        return false;
    };
    let hasil = client
        .execute(
            "INSERT INTO payment_webhook_events (id, provider, event_key, signature_ok, payload) \
             VALUES ($1, $2, $3, $4, $5) ON CONFLICT (provider, event_key) DO NOTHING",
            &[&id, &provider, &event_key, &signature_ok, &payload],
        )
        .await;

    match hasil {
        Ok(n) => n == 1,
        Err(e) => {
            tracing::error!(error = %e, "gagal mencatat peristiwa webhook");
            false
        }
    }
}

async fn tandai_selesai(state: &AppState, provider: &str, event_key: &str) {
    let Ok(client) = state.pool.get().await else { return };
    if let Err(e) = client
        .execute(
            "UPDATE payment_webhook_events SET processed_at = NOW() \
             WHERE provider = $1 AND event_key = $2",
            &[&provider, &event_key],
        )
        .await
    {
        tracing::warn!(error = %e, "gagal menandai peristiwa selesai");
    }
}

async fn proses(
    state: &AppState,
    provider: &str,
    kabar: &super::KabarWebhook,
) -> anyhow::Result<()> {
    let client = state.pool.get().await?;

    let Some(row) = client
        .query_opt(
            "SELECT order_id, amount, status, payment_code FROM payment_transactions \
             WHERE provider = $1 AND provider_ref = $2",
            &[&provider, &kabar.provider_ref],
        )
        .await?
    else {
        // Callback untuk transaksi yang tak pernah kita buat. Tanda tangannya
        // sah, jadi ini bukan pemalsuan — kemungkinan besar sisa dari
        // lingkungan lain yang memakai kunci gateway yang sama (sandbox dan
        // produksi berbagi akun). Dicatat, tidak diproses.
        tracing::warn!(provider, r#ref = %kabar.provider_ref, "webhook: transaksi tak dikenal");
        return Ok(());
    };

    let order_id_bytes: Vec<u8> = row.get(0);
    let tagihan: Decimal = row.get(1);
    let status_kini: String = row.get(2);
    let payment_code: String = row.get(3);

    // Status akhir tak boleh mundur. Jaringan tak menjamin urutan, dan
    // callback `pending` yang tiba SESUDAH `settlement` akan membatalkan
    // pelunasan kalau tak dijaga di sini.
    if status_kini == "paid" && !kabar.status.final_() {
        tracing::info!(provider, "webhook membawa status mundur — diabaikan");
        return Ok(());
    }

    client
        .execute(
            "UPDATE payment_transactions \
                SET status = $3, updated_at = NOW(), \
                    paid_at = CASE WHEN $3 = 'paid' THEN NOW() ELSE paid_at END \
              WHERE provider = $1 AND provider_ref = $2",
            &[&provider, &kabar.provider_ref, &kabar.status.sebagai_kolom()],
        )
        .await?;

    if kabar.status != StatusBayar::Lunas {
        return Ok(());
    }

    // ── Nominal WAJIB dicocokkan ──────────────────────────────────────────
    // Tanda tangan membuktikan pesannya dari gateway; ia tidak membuktikan
    // nominalnya cukup. Pada gateway yang mengizinkan pembayaran sebagian —
    // dan pada Flip, yang callback-nya tak ditandatangani sama sekali —
    // inilah satu-satunya yang menghentikan tiket terbit untuk pembayaran
    // yang kurang.
    if let Some(dibayar) = kabar.amount {
        if dibayar < tagihan {
            tracing::error!(
                provider,
                %dibayar, %tagihan,
                "webhook: nominal KURANG dari tagihan — order TIDAK dilunaskan"
            );
            return Ok(());
        }
    }

    let order_id = crate::utils::ulid::bin_to_ulid_ref(&order_id_bytes)?;
    let kanal = if payment_code.is_empty() {
        provider.to_string()
    } else {
        payment_code
    };

    match state.order_svc.pay_dari_gateway(&order_id, &kanal).await {
        Ok(_) => {
            tracing::info!(provider, order_id, "order DILUNASKAN oleh webhook");
            Ok(())
        }
        Err(crate::utils::error::AppError::BadRequest(pesan)) => {
            // Order sudah lunas/batal/kedaluwarsa. Bukan kegagalan yang perlu
            // diulang gateway — cukup dicatat.
            tracing::warn!(provider, order_id, pesan, "webhook: order tak bisa dilunaskan");
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("{e}")),
    }
}
