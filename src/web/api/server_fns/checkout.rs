//! web/api/server_fns/checkout.rs — mengubah keranjang menjadi order.
//!
//! Perhatikan apa yang TIDAK diterima `checkout_cart`: daftar tiket, harga,
//! subtotal, potongan. Semuanya dibaca server dari keranjang milik pemanggil.
//! Versi sebelumnya menerima `items_json` dari browser — artinya siapa pun yang
//! bisa memanggil `/api-fn/` bisa menyebut tiket mana yang dibeli. Sekarang
//! satu-satunya yang boleh dipilih klien adalah kanal pembayaran dan kode promo,
//! dan keduanya divalidasi ulang di server.

#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use crate::web::models::{OrderItemRef, OrderRef};
use leptos::prelude::*;
#[cfg(feature = "ssr")]
use super::helpers::*;

// ── Konversi ─────────────────────────────────────────────────────────────────

#[cfg(feature = "ssr")]
pub(super) fn srv_order_detail_to_ref(
    o: crate::models::orders::OrderDetailResponse,
) -> OrderRef {
    use rust_decimal::prelude::ToPrimitive;

    OrderRef {
        id: o.id,
        order_code: o.order_code,
        status: o.status,
        total_amount: o.total_amount.to_i64().unwrap_or(0),
        expired_at: o.expired_at.map(|d| d.to_rfc3339()),
        created_at: Some(o.created_at.to_rfc3339()),
        items: o
            .items
            .iter()
            .map(|i| OrderItemRef {
                event_name: i.event_name.clone(),
                variant_name: i.variant_name.clone(),
                quantity: i.quantity,
                subtotal: i.subtotal.to_i64().unwrap_or(0),
            })
            .collect(),
        subtotal_amount: o.subtotal_amount.to_i64().unwrap_or(0),
        discount_amount: o.discount_amount.to_i64().unwrap_or(0),
        promo_code: o.promo_code,
        payment_code: o.payment_code.or(o.payment_method),
        payment_name: o.payment_name,
        payment_charge: o.payment_charge.to_i64().unwrap_or(0),
        payment_reference: o.payment_reference,
        payment_instruction: o.payment_instruction,
        payment_expired_at: o.payment_expired_at.map(|d| d.to_rfc3339()),
    }
}

// ── Checkout ─────────────────────────────────────────────────────────────────

/// Buat order dari keranjang yang tersimpan di database.
///
/// `idempotency_key` dibuat klien sekali per percobaan checkout: dobel-klik atau
/// retry jaringan dengan kunci yang sama mengembalikan order yang SUDAH ada,
/// bukan order kedua.
#[server(CheckoutCart, "/api-fn")]
pub async fn checkout_cart(
    payment_code: String,
    promo_code: Option<String>,
    idempotency_key: Option<String>,
) -> Result<OrderRef, ServerFnError> {
    use crate::models::orders::CheckoutRequest;

    let claims = auth_claims().await?;
    let state = app_state().await?;
    let is_premium = state
        .story_svc
        .is_premium(&claims.user_id)
        .await
        .unwrap_or(false);

    let order = state
        .order_svc
        .checkout(
            &claims.user_id,
            &claims.name,
            CheckoutRequest {
                payment_code,
                promo_code,
                idempotency_key,
            },
            is_premium,
        )
        .await
        .map_err(map_app_error)?;

    // Pembelian = sinyal minat terkuat (bobot 5). Dicatat server-side dari order
    // asli (tak bisa dipalsukan klien) dan di latar — checkout tidak menunggu.
    state
        .affinity_svc
        .record_purchase(claims.user_id.clone(), order.id.clone());

    Ok(srv_order_detail_to_ref(order))
}

/// Tandai order pending sebagai lunas — JALUR TIRUAN, dipagari env.
///
/// ── KENAPA INI HARUS DIPAGARI ─────────────────────────────────────────────
/// Fungsi ini menerbitkan tiket dan memotong stok permanen tanpa bukti
/// pembayaran apa pun. Yang memanggilnya adalah browser PEMBELI sendiri, dan
/// satu-satunya yang diperiksa adalah bahwa ordernya miliknya. Artinya siapa
/// pun yang login bisa membuat order lalu menyatakannya lunas — tiketnya
/// terbit, stoknya berkurang, notifikasi "PAID" keluar, dan tak ada satu pun
/// baris log yang tampak salah.
///
/// Ia dipertahankan karena berguna: mengembangkan alur checkout tanpa harus
/// menunggu callback gateway sungguhan jauh lebih cepat. Yang berubah hanyalah
/// ia tak lagi menyala dengan sendirinya.
///
/// **Bawaannya MATI.** Menyalakannya menuntut `PAYMENT_MOCK=1` dituliskan
/// dengan sengaja — dan satu-satunya tempat yang pantas menuliskannya adalah
/// mesin pengembangan. Jalur yang sah ada di `payment::webhook`: gateway yang
/// memberi tahu, tanda tangan yang diperiksa, baru order dilunaskan.
///
/// Kanal yang dipakai diambil dari order itu sendiri — sebelumnya selalu
/// dituliskan "qris" apa pun yang sebenarnya dipilih pembeli, sehingga laporan
/// per-kanal tak pernah bisa dipercaya.
#[server(ConfirmOrderPayment, "/api-fn")]
pub async fn confirm_order_payment(order_id: String) -> Result<OrderRef, ServerFnError> {
    use crate::models::orders::PayOrderRequest;

    let claims = auth_claims().await?;
    let state = app_state().await?;

    if !crate::payment::mock_diizinkan() {
        tracing::warn!(
            user_id = %claims.user_id,
            order_id,
            "confirm_order_payment DITOLAK — PAYMENT_MOCK mati; pelunasan hanya lewat webhook"
        );
        return Err(ServerFnError::ServerError(
            "Pembayaran harus diselesaikan lewat kanal pembayaran.".into(),
        ));
    }

    let current = state
        .order_svc
        .detail(&order_id, &claims.user_id)
        .await
        .map_err(map_app_error)?;

    let method = current
        .payment_code
        .clone()
        .or(current.payment_method.clone())
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| "qris".to_string());

    let paid = state
        .order_svc
        .pay(
            &order_id,
            &claims.user_id,
            &claims.name,
            PayOrderRequest {
                payment_method: method,
            },
        )
        .await
        .map_err(map_app_error)?;

    let paid = state.order_svc.enrich_payment(paid).await;
    Ok(srv_order_detail_to_ref(paid))
}

/// Batalkan order yang belum dibayar; stoknya kembali ke kuota varian.
#[server(CancelOrder, "/api-fn")]
pub async fn cancel_order(order_id: String) -> Result<(), ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    state
        .order_svc
        .cancel(&order_id, &claims.user_id)
        .await
        .map_err(map_app_error)?;
    Ok(())
}

/// Mulai pembayaran sungguhan: minta gateway membuat tagihan, lalu kembalikan
/// cara membayarnya.
///
/// ── INI JALUR YANG SAH; `confirm_order_payment` BUKAN ─────────────────────
/// Fungsi ini TIDAK mengubah status order sama sekali. Ia hanya membuat
/// tagihan di sisi gateway dan mencatat transaksinya, supaya callback nanti
/// punya jalan pulang ke order ini. Yang melunaskan order hanyalah webhook
/// yang tanda tangannya terverifikasi (`payment::webhook`).
///
/// Pemisahan itu yang membuat jalur ini aman dipanggil browser: seberapa pun
/// sering seseorang memanggilnya, tak ada tiket yang terbit. Yang paling buruk
/// terjadi adalah tagihan yang sama dibuat ulang — dan `ON CONFLICT` pada
/// `(provider, provider_ref)` membuat pengulangan itu menghasilkan baris yang
/// sama, bukan baris kedua.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CaraBayar {
    pub provider: String,
    /// Nomor VA / kode yang disalin pembeli.
    pub reference: Option<String>,
    /// Halaman bayar yang dibuka pembeli.
    pub url: Option<String>,
}

#[server(MulaiPembayaran, "/api-fn")]
pub async fn mulai_pembayaran(
    order_id: String,
    provider: String,
) -> Result<CaraBayar, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;

    // Kepemilikan diperiksa lewat `detail`, yang sudah menolak order milik
    // orang lain. Tanpa ini, siapa pun bisa membuat tagihan atas order siapa
    // pun — tak berbahaya secara uang, tapi ia membocorkan kode order dan
    // nominalnya lewat jawaban gateway.
    let order = state
        .order_svc
        .detail(&order_id, &claims.user_id)
        .await
        .map_err(map_app_error)?;

    if order.status != "pending" {
        return Err(ServerFnError::ServerError(
            "Order ini sudah tidak menunggu pembayaran.".into(),
        ));
    }

    let tagihan = crate::payment::buat_tagihan_untuk_order(
        &state,
        &provider,
        &order,
        &claims.name,
        None,
        Some(claims.phone.clone()),
    )
    .await
    .map_err(|e| -> ServerFnError {
        // Sebab aslinya masuk LOG, bukan ke pembeli: jawaban gateway kerap
        // memuat kunci, id internal, dan potongan permintaan kita sendiri.
        tracing::error!(provider, order_id, error = %e, "gagal membuat tagihan gateway");
        ServerFnError::ServerError("Gagal menghubungi kanal pembayaran.".into())
    })?;

    Ok(CaraBayar {
        provider,
        reference: tagihan.pay_reference,
        url: tagihan.pay_url,
    })
}
