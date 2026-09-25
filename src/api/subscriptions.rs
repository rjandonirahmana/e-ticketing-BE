//! api/subscriptions.rs — Subscription REST endpoints (private).
//!
//! POST /api/subscriptions/order   — buat subscription order
//! GET  /api/subscriptions/status  — cek status premium

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::state::AppState;
use super::extractor::{app_err, AuthUser};

#[derive(Deserialize)]
struct CreateSubReq {
    plan: String,
}

/// PENTING: jalur ini dulu langsung memanggil `activate_premium` — status
/// `paid` ditulis dan langganan (termasuk plan "lifetime") dinyalakan TANPA
/// pembayaran apa pun, untuk user login mana pun. Alur web yang sah lewat
/// `create_pending_subscription_order` (pending) → konfirmasi berbayar yang
/// digerbangi `PAYMENT_MOCK`. Digerbangi sama di sini sampai REST punya jalur
/// pending+webhook-nya sendiri.
async fn create_subscription_order(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSubReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !crate::payment::mock_diizinkan() {
        tracing::warn!(
            user_id = %claims.user_id,
            plan = %body.plan,
            "REST create_subscription_order DITOLAK — PAYMENT_MOCK mati"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "message": "Langganan harus diselesaikan lewat kanal pembayaran."
            })),
        ));
    }

    let days: i64 = match body.plan.as_str() {
        "weekly"   => 7,
        "monthly"  => 30,
        "yearly"   => 365,
        "lifetime" => 0,
        _ => {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({ "message": "Plan tidak valid. Pilih: weekly, monthly, yearly, lifetime" })),
            ));
        }
    };

    let activation = state
        .story_svc
        .activate_premium(&claims.user_id, &body.plan, days)
        .await
        .map_err(app_err)?;

    Ok(Json(serde_json::json!({
        "order_code": activation.order_code,
        "plan": activation.subscription.plan,
        "expires_at": activation.subscription.expires_at,
        "is_active": activation.subscription.is_active,
    })))
}

async fn get_premium_status(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let is_premium = state
        .story_svc
        .is_premium(&claims.user_id)
        .await
        .map_err(app_err)?;

    Ok(Json(serde_json::json!({ "is_premium": is_premium })))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/subscriptions/order", post(create_subscription_order))
        .route("/subscriptions/status", get(get_premium_status))
}
