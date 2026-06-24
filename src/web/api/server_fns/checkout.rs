use crate::web::models::{CreateOrderResponse, OrderItemRef, OrderRef};
use leptos::prelude::*;
#[cfg(feature = "ssr")]
use super::helpers::*;

#[server(CreateOrderCart, "/api-fn")]
pub async fn create_order_cart(
    items_json: String,
    payment_method: String,
    _promo_code: Option<String>,
) -> Result<crate::web::models::CreateOrderResponse, ServerFnError> {
    use crate::models::orders::{CreateOrderItemRequest, CreateOrderRequest};
    use rust_decimal::prelude::ToPrimitive;

    let claims = auth_claims().await?;
    let state = app_state().await?;

    #[derive(serde::Deserialize)]
    struct CartItemReq {
        tier_id: String,
        quantity: i32,
    }

    let items_raw: Vec<CartItemReq> =
        serde_json::from_str(&items_json).unwrap_or_default();

    let items: Vec<CreateOrderItemRequest> = items_raw
        .into_iter()
        .map(|i| CreateOrderItemRequest {
            ticket_variant_id: i.tier_id,
            quantity: i.quantity,
        })
        .collect();

    if items.is_empty() {
        return Err(ServerFnError::ServerError("Items tidak boleh kosong".into()));
    }

    let req = CreateOrderRequest {
        idempotency_key: None,
        items,
    };

    let is_premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let order = state
        .order_svc
        .create(&claims.user_id, req, is_premium)
        .await
        .map_err(map_app_error)?;

    let order_ref = OrderRef {
        id: order.id.clone(),
        order_code: order.order_code.clone(),
        status: order.status.clone(),
        total_amount: order.total_amount.to_i64().unwrap_or(0),
        expired_at: order.expired_at.map(|d| d.to_rfc3339()),
        created_at: Some(order.created_at.to_rfc3339()),
        items: order
            .items
            .iter()
            .map(|i| OrderItemRef {
                event_name: i.event_name.clone(),
                variant_name: i.variant_name.clone(),
                quantity: i.quantity,
                subtotal: i.subtotal.to_i64().unwrap_or(0),
            })
            .collect(),
    };

    // If payment_method is "qris" or "cash", auto-pay
    if payment_method == "qris" || payment_method == "cash" {
        use crate::models::orders::PayOrderRequest;
        let pay_req = PayOrderRequest {
            payment_method: payment_method.clone(),
        };
        let _ = state
            .order_svc
            .pay(&order.id, &claims.user_id, &claims.name, pay_req)
            .await;
    }

    return Ok(CreateOrderResponse {
        order: order_ref,
        requires_redirect: false,
        payment_url: String::new(),
    });
}

#[server(ValidatePromo, "/api-fn")]
pub async fn validate_promo(
    _promo_code: String,
    _subtotal: i64,
) -> Result<crate::web::models::ValidatePromoResponse, ServerFnError> {
    // Promo/coupon system not yet implemented in service layer
    return Ok(crate::web::models::ValidatePromoResponse {
        valid: false,
        discount_idr: 0,
        message: "Promo tidak valid".into(),
    });
}

#[server(ConfirmOrderPayment, "/api-fn")]
pub async fn confirm_order_payment(
    order_id: String,
) -> Result<crate::web::models::OrderRef, ServerFnError> {
    use crate::models::orders::PayOrderRequest;
    use rust_decimal::prelude::ToPrimitive;

    let claims = auth_claims().await?;
    let state = app_state().await?;
    let pay_req = PayOrderRequest {
        payment_method: "qris".into(),
    };
    let paid = state
        .order_svc
        .pay(&order_id, &claims.user_id, &claims.name, pay_req)
        .await
        .map_err(map_app_error)?;

    return Ok(OrderRef {
        id: paid.id,
        order_code: paid.order_code,
        status: paid.status,
        total_amount: paid.total_amount.to_i64().unwrap_or(0),
        expired_at: paid.expired_at.map(|d| d.to_rfc3339()),
        created_at: Some(paid.created_at.to_rfc3339()),
        items: paid
            .items
            .into_iter()
            .map(|i| OrderItemRef {
                event_name: i.event_name,
                variant_name: i.variant_name,
                quantity: i.quantity,
                subtotal: i.subtotal.to_i64().unwrap_or(0),
            })
            .collect(),
    });
}
