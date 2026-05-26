use super::backend::{
    order_detail_to_ref, BeCreateOrderItem, BeCreateOrderPayload, BeOrderDetail, BePayOrderPayload,
};
use super::client::{post_private, ApiError};
use crate::csr::models::*;

/// The backend has no promo/coupon endpoint yet, so we resolve as
/// "no discount" for any code. This keeps the checkout flow working
/// without faking a successful discount.
pub async fn validate_promo(req: &ValidatePromoRequest) -> Result<ValidatePromoResponse, ApiError> {
    let _ = req;
    Ok(ValidatePromoResponse {
        valid: false,
        discount_idr: 0,
        message: "Promo codes are not supported by the API yet.".into(),
    })
}

/// Generate a unique idempotency key using getrandom (WASM-native).
/// 128 bits of random entropy — globally unique without needing a timestamp.
fn idempotency_key() -> String {
    let mut bytes = [0u8; 16];
    let _ = getrandom::fill(&mut bytes);
    // Pre-alloc exact 32 chars — 1 alloc vs 16× sebelumnya
    let mut s = String::with_capacity(32);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{:02X}", b);
    }
    s
}

/// POST /orders — create an order from the cart items.
pub async fn create_order(req: &CreateOrderRequest) -> Result<CreateOrderResponse, ApiError> {
    let items: Vec<BeCreateOrderItem<'_>> = req
        .items
        .iter()
        .map(|c| BeCreateOrderItem {
            ticket_variant_id: &c.tier_id,
            quantity: c.quantity,
        })
        .collect();
    let payload = BeCreateOrderPayload {
        idempotency_key: idempotency_key(),
        items,
    };

    let resp: BeOrderDetail = post_private("/orders", &payload).await?;
    Ok(CreateOrderResponse {
        order: order_detail_to_ref(resp),
        requires_redirect: false,
        payment_url: String::new(),
    })
}

/// POST /orders/:id/pay — record payment for an existing order.
pub async fn confirm_payment(
    req: &ConfirmPaymentRequest,
) -> Result<ConfirmPaymentResponse, ApiError> {
    let path = format!("/orders/{}/pay", req.order_id);
    let payload = BePayOrderPayload {
        payment_method: &req.payment_token,
    };
    let resp: BeOrderDetail = post_private(&path, &payload).await?;
    let success =
        resp.status.eq_ignore_ascii_case("paid") || resp.status.eq_ignore_ascii_case("completed");
    Ok(ConfirmPaymentResponse { success })
}
