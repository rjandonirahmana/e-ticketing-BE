use leptos::prelude::*;
#[cfg(feature = "ssr")]
use super::helpers::*;

#[server(GetPremiumStatus, "/api-fn")]
pub async fn get_premium_status() -> Result<serde_json::Value, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let is_premium = state
        .story_svc
        .is_premium(&claims.user_id)
        .await
        .map_err(map_app_error)?;
    return Ok(serde_json::json!({ "is_premium": is_premium }));
}

#[server(CreateSubscriptionOrder, "/api-fn")]
pub async fn create_subscription_order(plan: String) -> Result<String, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let days: i64 = match plan.as_str() {
        "monthly" => 30,
        "yearly" => 365,
        _ => 30,
    };
    state
        .story_svc
        .activate_premium(&claims.user_id, days)
        .await
        .map_err(map_app_error)?;
    return Ok(claims.user_id);
}
