//! api/cart.rs — keranjang, kanal pembayaran, dan checkout (REST, semua private).
//!
//! Peta endpoint sengaja dibuat sepadan dengan kiddoapi supaya frontend yang
//! sudah terbiasa dengan bentuk itu tak perlu belajar dua model:
//!
//!   kiddoapi                          PULSE
//!   ────────────────────────────────  ───────────────────────────────────────
//!   GET    /v1/cart/view              GET    /api/cart/view
//!   GET    /v1/cart/count             GET    /api/cart/count
//!   POST   /v2/cart/create            POST   /api/cart/create
//!   PUT    /v1/cart/quantity          PUT    /api/cart/quantity
//!   (tak ada padanan)                POST   /api/cart/select
//!   DELETE /v1/cart/activity/:id      DELETE /api/cart/item/{variant_id}
//!   GET    /v1/payment/view           GET    /api/payments
//!   POST   /v1/order/create           POST   /api/checkout
//!   POST   /v1/order/pay/:id          POST   /api/orders/{id}/pay
//!
//! Perbedaan yang disengaja: penghapusan baris memakai **id varian**, bukan
//! indeks larik seperti `DELETE /cart/activity/{index}` di kiddoweb. Indeks
//! bergantung pada urutan yang kebetulan sedang dilihat klien — dua tab yang
//! terbuka bersamaan bisa menghapus baris yang salah.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::models::cart::{SaveCartRequest, UpdateCartItemRequest};
use crate::models::orders::{CheckoutRequest, PayOrderRequest};
use crate::state::AppState;

use super::extractor::{app_err, AuthUser};

type ApiResult = Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>;

fn ok<T: serde::Serialize>(v: T) -> ApiResult {
    Ok(Json(serde_json::to_value(v).unwrap_or_default()))
}

// ── Keranjang ────────────────────────────────────────────────────────────────

async fn view_cart(AuthUser(claims): AuthUser, State(state): State<Arc<AppState>>) -> ApiResult {
    let premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let cart = state
        .cart_svc
        .view(&claims.user_id, premium)
        .await
        .map_err(app_err)?;
    ok(cart)
}

async fn count_cart(AuthUser(claims): AuthUser, State(state): State<Arc<AppState>>) -> ApiResult {
    let total = state.cart_svc.count(&claims.user_id).await.map_err(app_err)?;
    Ok(Json(serde_json::json!({ "total_quantity": total })))
}

async fn save_cart(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<SaveCartRequest>,
) -> ApiResult {
    let premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let cart = state
        .cart_svc
        .save(&claims.user_id, body, premium)
        .await
        .map_err(app_err)?;
    ok(cart)
}

#[derive(Deserialize)]
struct AddItemReq {
    ticket_variant_id: String,
    #[serde(default = "one")]
    quantity: i32,
}

fn one() -> i32 {
    1
}

async fn add_item(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AddItemReq>,
) -> ApiResult {
    let premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let cart = state
        .cart_svc
        .add(&claims.user_id, &body.ticket_variant_id, body.quantity, premium)
        .await
        .map_err(app_err)?;
    ok(cart)
}

async fn update_quantity(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpdateCartItemRequest>,
) -> ApiResult {
    let premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let cart = state
        .cart_svc
        .update_quantity(&claims.user_id, body, premium)
        .await
        .map_err(app_err)?;
    ok(cart)
}

async fn remove_item(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(variant_id): Path<String>,
) -> ApiResult {
    let premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let cart = state
        .cart_svc
        .remove(&claims.user_id, &variant_id, premium)
        .await
        .map_err(app_err)?;
    ok(cart)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectReq {
    /// Kosong berarti seluruh isi keranjang.
    #[serde(default)]
    ticket_variant_id: Option<String>,
    selected: bool,
}

async fn select_item(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<SelectReq>,
) -> ApiResult {
    let premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let cart = state
        .cart_svc
        .set_selected(
            &claims.user_id,
            body.ticket_variant_id.as_deref().filter(|s| !s.is_empty()),
            body.selected,
            premium,
        )
        .await
        .map_err(app_err)?;
    ok(cart)
}

async fn clear_cart(AuthUser(claims): AuthUser, State(state): State<Arc<AppState>>) -> ApiResult {
    let premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let cart = state
        .cart_svc
        .clear(&claims.user_id, premium)
        .await
        .map_err(app_err)?;
    ok(cart)
}

#[derive(Deserialize)]
struct PromoReq {
    /// Kosong atau absen = lepaskan promo.
    #[serde(default)]
    promo_code: Option<String>,
}

async fn set_promo(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<PromoReq>,
) -> ApiResult {
    let premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let cart = state
        .cart_svc
        .set_promo(&claims.user_id, body.promo_code.as_deref(), premium)
        .await
        .map_err(app_err)?;
    ok(cart)
}

#[derive(Deserialize)]
struct PaymentSelectReq {
    #[serde(default)]
    payment_code: Option<String>,
}

async fn set_payment(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<PaymentSelectReq>,
) -> ApiResult {
    let premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let cart = state
        .cart_svc
        .set_payment(&claims.user_id, body.payment_code.as_deref(), premium)
        .await
        .map_err(app_err)?;
    ok(cart)
}

// ── Kanal pembayaran ─────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct PaymentsQuery {
    /// Nominal yang hendak dibayar. Bila absen, dihitung dari keranjang
    /// pemanggil — halaman checkout tak perlu mengirim angka apa pun.
    amount: Option<i64>,
}

async fn list_payments(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<PaymentsQuery>,
) -> ApiResult {
    use rust_decimal::prelude::ToPrimitive;

    let premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let cart = state
        .cart_svc
        .view(&claims.user_id, premium)
        .await
        .map_err(app_err)?;

    let amount = q
        .amount
        .map(rust_decimal::Decimal::from)
        .unwrap_or(cart.total);
    let has_promo = cart.promo_code.is_some();

    let methods = state
        .payment_svc
        .list_for(amount, has_promo)
        .await
        .map_err(app_err)?;

    // Biaya per kanal ikut dihitung di sini: halaman checkout menampilkan
    // "Rp4.000" di samping tiap pilihan tanpa perlu menyalin rumusnya.
    let data: Vec<serde_json::Value> = methods
        .iter()
        .map(|m| {
            let charge = m.charge_for(amount);
            serde_json::json!({
                "code": m.code,
                "name": m.name,
                "vendor": m.vendor,
                "category": m.category,
                "imageUrl": m.image_url,
                "description": m.description,
                "charge": charge.to_i64().unwrap_or(0),
                "total": (amount + charge).to_i64().unwrap_or(0),
                "isInstant": m.is_instant,
                "instruction": m.instruction,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "data": data,
        "amount": amount.to_i64().unwrap_or(0),
        "selected": cart.payment_code,
    })))
}

// ── Checkout & pembayaran ────────────────────────────────────────────────────

async fn checkout(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CheckoutRequest>,
) -> ApiResult {
    let premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let order = state
        .order_svc
        .checkout(&claims.user_id, &claims.name, body, premium)
        .await
        .map_err(app_err)?;

    // Pembelian = sinyal minat terkuat; dicatat dari order asli, di latar.
    state
        .affinity_svc
        .record_purchase(claims.user_id.clone(), order.id.clone());

    ok(order)
}

async fn pay_order(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<PayOrderRequest>,
) -> ApiResult {
    let order = state
        .order_svc
        .pay(&id, &claims.user_id, &claims.name, body)
        .await
        .map_err(app_err)?;
    ok(state.order_svc.enrich_payment(order).await)
}

async fn cancel_order(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult {
    state
        .order_svc
        .cancel(&id, &claims.user_id)
        .await
        .map_err(app_err)?;
    Ok(Json(serde_json::json!({ "cancelled": true })))
}

// ── Router ───────────────────────────────────────────────────────────────────

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/cart/view", get(view_cart))
        .route("/cart/count", get(count_cart))
        .route("/cart/create", post(save_cart))
        .route("/cart/add", post(add_item))
        .route("/cart/quantity", put(update_quantity))
        .route("/cart/item/{variant_id}", delete(remove_item))
        .route("/cart/select", post(select_item))
        .route("/cart/clear", delete(clear_cart))
        .route("/cart/promo", post(set_promo))
        .route("/cart/payment", post(set_payment))
        .route("/payments", get(list_payments))
        .route("/checkout", post(checkout))
        .route("/orders/{id}/pay", post(pay_order))
        .route("/orders/{id}/cancel", post(cancel_order))
}
