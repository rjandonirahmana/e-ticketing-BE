use crate::web::models::*;
use leptos::prelude::*;
#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use super::helpers::*;

#[server(GetMyOrders, "/api-fn")]
pub async fn get_my_orders() -> Result<Vec<OrderListItem>, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let orders = state
        .order_svc
        .list_mine(&claims.user_id, 1, 100)
        .await
        .map_err(map_app_error)?;
    return Ok(orders.into_iter().map(srv_order_list_item_to_web).collect());
}

#[server(GetOrderDetail, "/api-fn")]
pub async fn get_order_detail(id: String) -> Result<OrderDetail, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let order = state
        .order_svc
        .detail(&id, &claims.user_id)
        .await
        .map_err(map_app_error)?;
    // Nama & instruksi kanal tinggal di `payment_methods`; dilekatkan di sini
    // supaya halaman detail order tak perlu memetakan kode kanal sendiri.
    let order = state.order_svc.enrich_payment(order).await;
    return Ok(srv_order_detail_to_web(order));
}

#[server(GetOrderTickets, "/api-fn")]
pub async fn get_order_tickets(order_id: String) -> Result<Vec<TicketResponse>, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let tickets = state
        .ticket_svc
        .list_for_order(&order_id, &claims.user_id, 1, 100)
        .await
        .map_err(map_app_error)?;
    return Ok(tickets.into_iter().map(srv_ticket_to_web).collect());
}

#[server(CreateOrder, "/api-fn")]
pub async fn create_order(variant_id: String, quantity: i32) -> Result<String, ServerFnError> {
    use crate::models::orders::{CreateOrderItemRequest, CreateOrderRequest};
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let req = CreateOrderRequest {
        idempotency_key: None,
        items: vec![CreateOrderItemRequest {
            ticket_variant_id: variant_id,
            quantity,
        }],
    };
    let is_premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let order = state
        .order_svc
        .create(&claims.user_id, req, is_premium)
        .await
        .map_err(map_app_error)?;
    return Ok(order.id);
}

#[server(CreateOrderMulti, "/api-fn")]
pub async fn create_order_multi(
    variant_id: String,
    quantity: i32,
    _payment_method: String,
) -> Result<String, ServerFnError> {
    use crate::models::orders::{CreateOrderItemRequest, CreateOrderRequest};
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let req = CreateOrderRequest {
        idempotency_key: None,
        items: vec![CreateOrderItemRequest {
            ticket_variant_id: variant_id,
            quantity,
        }],
    };
    let is_premium = state.story_svc.is_premium(&claims.user_id).await.unwrap_or(false);
    let order = state
        .order_svc
        .create(&claims.user_id, req, is_premium)
        .await
        .map_err(map_app_error)?;
    return Ok(order.id);
}
