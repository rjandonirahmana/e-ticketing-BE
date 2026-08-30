//! api/orders.rs — Orders & Tickets REST endpoints (semua private).
//!
//! GET  /api/orders
//! GET  /api/orders/:id
//! GET  /api/orders/:id/tickets
//! POST /api/orders
//! GET  /api/tickets              (query: filter, page, pageSize)
//! GET  /api/tickets/:id
//! POST /api/promos/validate

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::state::AppState;
use super::extractor::{app_err, AuthUser};

// ── Orders ────────────────────────────────────────────────────────────────────

async fn list_orders(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let orders = state.order_svc.list_mine(&claims.user_id, 1, 100).await.map_err(app_err)?;
    Ok(Json(serde_json::to_value(orders).unwrap_or_default()))
}

async fn get_order(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let order = state.order_svc.detail(&id, &claims.user_id).await.map_err(app_err)?;
    // Sama seperti jalur web (`web/api/server_fns/order.rs`) dan jalur cart.
    //
    // Ini bukan sekadar melengkapi nama kanal dan instruksi pembayaran yang
    // sebelumnya selalu null di sini. `enrich_payment` juga memuat tambalan
    // untuk nomor VA yang hilang: nomor itu ditulis SESUDAH transaksi order
    // commit, jadi ada jendela sempit di mana proses mati dan order lahir tanpa
    // cara membayarnya. Tambalannya menghitung ulang nomor yang deterministik
    // dari `order_code` lalu menyimpannya.
    //
    // Karena tambalan itu hanya hidup di sini, pembeli lewat web Leptos pulih
    // sendiri begitu membuka ordernya, sedangkan pembeli lewat klien REST tidak
    // pernah pulih — ia memegang order tanpa nomor pembayaran, selamanya.
    let order = state.order_svc.enrich_payment(order).await;
    Ok(Json(serde_json::to_value(order).unwrap_or_default()))
}

async fn get_order_tickets(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let tickets = state
        .ticket_svc
        .list_for_order(&id, &claims.user_id, 1, 100)
        .await
        .map_err(app_err)?;
    Ok(Json(serde_json::to_value(tickets).unwrap_or_default()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateOrderItem {
    tier_id: String,
    quantity: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateOrderReq {
    items: Vec<CreateOrderItem>,
    payment_method: Option<String>,
    promo_code: Option<String>,
}

/// Batas panjang kunci idempotensi.
///
/// Kolomnya `VARCHAR(64)` (migrasi 022a). Kunci yang lebih panjang akan ditolak
/// database di TENGAH transaksi pembuatan order — jadi lebih baik ditolak di
/// sini, dengan pesan yang menyebut sebabnya.
const IDEMPOTENCY_KEY_MAX: usize = 64;

/// Baca `Idempotency-Key` dari header.
///
/// Header, bukan field body: ini konvensi REST yang sudah umum (Stripe dkk),
/// sehingga klien native bisa memasangnya di lapisan HTTP-nya — biasanya sekali
/// di interceptor, bersama logika retry-nya sendiri — tanpa menyentuh bentuk
/// body yang sudah dipakai klien lama.
fn idempotency_key(headers: &HeaderMap) -> Option<String> {
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= IDEMPOTENCY_KEY_MAX)
        .map(String::from)
}

async fn create_order(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    // `HeaderMap` HARUS sebelum `Json`: `Json` mengonsumsi badan permintaan,
    // jadi ia wajib menjadi ekstraktor terakhir.
    headers: HeaderMap,
    Json(body): Json<CreateOrderReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    use crate::models::orders::{CreateOrderItemRequest, CreateOrderRequest};
    use rust_decimal::prelude::ToPrimitive;

    if body.items.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({ "message": "Items tidak boleh kosong" })),
        ));
    }

    let items: Vec<CreateOrderItemRequest> = body
        .items
        .into_iter()
        .map(|i| CreateOrderItemRequest {
            ticket_variant_id: i.tier_id,
            quantity: i.quantity,
        })
        .collect();

    // Jalur ini sengaja tak menerima kanal pembayaran maupun promo: keduanya
    // butuh keranjang server untuk dihitung dengan benar. Pemanggil yang
    // memerlukannya lewat `POST /api/checkout` (lihat api/cart.rs).
    let _ = (body.payment_method, body.promo_code);

    // ── Idempotensi ─────────────────────────────────────────────────────────
    // Jalur ini dipakai klien native, yang justru paling sering berada di
    // sinyal buruk: permintaan terkirim, jawabannya hilang di jalan, klien
    // mencoba lagi — dan tanpa kunci ini percobaan kedua membuat order KEDUA
    // beserta pengurangan stok kedua. Pembeli membayar dua kali untuk barang
    // yang sama, dan stok berkurang untuk pesanan yang tak pernah ia maksud.
    //
    // Dengan kunci yang sama, `ON CONFLICT (customer_id, idempotency_key)` di
    // `repository/order.rs` mengembalikan order yang SUDAH ada alih-alih
    // membuat yang baru.
    //
    // Tetap `Option`: klien yang tak mengirim header berperilaku persis seperti
    // sebelumnya, jadi tak ada klien lama yang rusak karena perubahan ini.
    let req = CreateOrderRequest {
        idempotency_key: idempotency_key(&headers),
        items,
    };
    let is_premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let order = state.order_svc.create(&claims.user_id, req, is_premium).await.map_err(app_err)?;

    // Behavior: pembelian = sinyal minat terkuat (bobot 5), dicatat background.
    state
        .affinity_svc
        .record_purchase(claims.user_id.clone(), order.id.clone());

    Ok(Json(serde_json::json!({
        "order": {
            "id": order.id,
            "orderCode": order.order_code,
            "status": order.status,
            "totalAmount": order.total_amount.to_i64().unwrap_or(0),
            "expiredAt": order.expired_at.map(|d| d.to_rfc3339()),
            "createdAt": order.created_at.to_rfc3339(),
            "items": order.items.iter().map(|i| serde_json::json!({
                "productName": i.event_name,
                "variantName": i.variant_name,
                "quantity": i.quantity,
                "subtotal": i.subtotal,
            })).collect::<Vec<_>>(),
        },
        "requiresRedirect": false,
        "paymentUrl": "",
    })))
}

// ── Tickets ───────────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct TicketsQuery {
    filter: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

async fn list_tickets(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<TicketsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let page = q.page.unwrap_or(1);
    let page_size = q.page_size.unwrap_or(20);
    let _ = q.filter; // filter by status to be implemented in repo

    let tickets = state
        .ticket_svc
        .list_for_customer(&claims.user_id, page, page_size)
        .await
        .map_err(app_err)?;
    Ok(Json(serde_json::json!({ "tickets": serde_json::to_value(tickets).unwrap_or_default() })))
}

async fn get_ticket(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let ticket = state
        .ticket_svc
        .detail_for_customer(&id, &claims.user_id)
        .await
        .map_err(app_err)?;
    Ok(Json(serde_json::to_value(ticket).unwrap_or_default()))
}

// ── Promos ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValidatePromoReq {
    promo_code: String,
    subtotal: f64,
}

/// Periksa kode promo terhadap keranjang pemanggil.
///
/// `subtotal` yang dikirim klien hanya dipakai sebagai cadangan bila keranjang
/// server kosong. Bila keranjangnya ada, angka DARI KERANJANG yang menang —
/// kalau tidak, klien bisa menyebut subtotal besar untuk melewati syarat
/// minimum belanja.
async fn validate_promo(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<ValidatePromoReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    use rust_decimal::prelude::{FromPrimitive, ToPrimitive};

    let premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let cart = state
        .cart_svc
        .view(&claims.user_id, premium)
        .await
        .map_err(app_err)?;

    let (subtotal, qty) = if cart.items.is_empty() {
        (
            rust_decimal::Decimal::from_f64(body.subtotal.max(0.0)).unwrap_or_default(),
            1,
        )
    } else {
        (cart.subtotal, cart.total_quantity)
    };

    let check = state
        .payment_svc
        .validate_promo(
            &claims.user_id,
            &body.promo_code,
            subtotal,
            qty,
            premium,
            cart.payment_code.as_deref(),
        )
        .await
        .map_err(app_err)?;

    Ok(Json(serde_json::json!({
        "valid": check.valid,
        "discountIdr": check.discount.to_i64().unwrap_or(0),
        "message": check.message,
    })))
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/orders", get(list_orders).post(create_order))
        .route("/orders/{id}", get(get_order))
        .route("/orders/{id}/tickets", get(get_order_tickets))
        .route("/tickets", get(list_tickets))
        .route("/tickets/{id}", get(get_ticket))
        .route("/promos/validate", post(validate_promo))
}

// ─── Uji penyaring kunci idempotensi ──────────────────────────────────────────
//
// Penyaring ini berdiri antara klien dan sebuah kolom `VARCHAR(64)`. Kalau ia
// meloloskan yang tak seharusnya, kegagalannya terjadi di TENGAH transaksi
// pembuatan order — tempat paling mahal untuk gagal, karena stok sudah dikunci
// dan pembeli sudah menunggu.
#[cfg(test)]
mod tests_idempotency {
    use super::*;

    fn dengan(nilai: &str) -> Option<String> {
        let mut h = HeaderMap::new();
        h.insert("idempotency-key", nilai.parse().unwrap());
        idempotency_key(&h)
    }

    /// Kunci wajar diterima apa adanya.
    #[test]
    fn kunci_wajar_diterima() {
        assert_eq!(
            dengan("01KQ1T2EKWKNCA8S6NQEAXMWA5"),
            Some("01KQ1T2EKWKNCA8S6NQEAXMWA5".to_string())
        );
    }

    /// Tanpa header = perilaku lama, bukan galat. Klien lama yang tak tahu
    /// header ini harus tetap bisa membuat order.
    #[test]
    fn tanpa_header_jadi_none() {
        assert_eq!(idempotency_key(&HeaderMap::new()), None);
    }

    /// Spasi di sekeliling dipangkas — kalau tidak, " k " dan "k" jadi dua
    /// kunci berbeda, dan retry klien yang menambah spasi tak terdeteksi.
    #[test]
    fn spasi_dipangkas() {
        assert_eq!(dengan("  abc123  "), Some("abc123".to_string()));
    }

    /// Header kosong (atau hanya spasi) diperlakukan sebagai TIDAK ADA.
    /// Menyimpan string kosong akan membuat SEMUA order tanpa kunci berbagi
    /// satu kunci yang sama — order kedua pembeli mana pun akan dikira
    /// duplikat dari yang pertama.
    #[test]
    fn kosong_jadi_none() {
        assert_eq!(dengan(""), None);
        assert_eq!(dengan("     "), None);
    }

    /// Tepat 64 karakter masih diterima — itu batas kolomnya, bukan di bawahnya.
    #[test]
    fn tepat_batas_diterima() {
        let k = "a".repeat(IDEMPOTENCY_KEY_MAX);
        assert_eq!(dengan(&k), Some(k));
    }

    /// Lebih dari 64 DITOLAK di sini, bukan dibiarkan menabrak `VARCHAR(64)`
    /// di tengah transaksi pembuatan order.
    #[test]
    fn melebihi_batas_ditolak() {
        let k = "a".repeat(IDEMPOTENCY_KEY_MAX + 1);
        assert_eq!(dengan(&k), None);
    }
}
