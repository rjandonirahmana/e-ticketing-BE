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
    http::StatusCode,
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

async fn create_order(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
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

    let req = CreateOrderRequest { idempotency_key: None, items };
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
