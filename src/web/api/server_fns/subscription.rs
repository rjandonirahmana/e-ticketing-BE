use leptos::prelude::*;
#[cfg(feature = "ssr")]
use super::helpers::*;
use crate::web::models::PendingSubOrder;

#[server(GetPremiumStatus, "/api-fn")]
pub async fn get_premium_status() -> Result<bool, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    state
        .story_svc
        .is_premium(&claims.user_id)
        .await
        .map_err(map_app_error)
}

/// Buat pending subscription order — belum aktifkan premium.
/// Return: PendingSubOrder berisi order_id, order_code, plan, amount_idr.
#[server(CreateSubscriptionOrder, "/api-fn")]
pub async fn create_subscription_order(plan: String) -> Result<PendingSubOrder, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    state
        .story_svc
        .create_pending_subscription_order(&claims.user_id, &plan)
        .await
        .map_err(map_app_error)
}

/// Konfirmasi pembayaran subscription — aktifkan premium setelah user pilih metode bayar.
#[server(ConfirmSubscriptionPayment, "/api-fn")]
pub async fn confirm_subscription_payment(
    order_id: String,
    plan: String,
) -> Result<String, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let days: i64 = match plan.as_str() {
        "weekly"   => 7,
        "monthly"  => 30,
        "yearly"   => 365,
        "lifetime" => 0,
        _          => 30,
    };
    let activation = state
        .story_svc
        .confirm_subscription_order(&order_id, &claims.user_id, &plan, days)
        .await
        .map_err(map_app_error)?;
    Ok(activation.order_code)
}
