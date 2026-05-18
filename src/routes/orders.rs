use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::middleware::auth::AuthUser;
use crate::models::orders::{
    CreateOrderRequest, Order, OrderDetailResponse, OrderListItem, PayOrderRequest,
};
use crate::state::AppState;
use crate::utils::error::AppResult;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<CreateOrderRequest>,
) -> AppResult<Json<OrderDetailResponse>> {
    Ok(Json(state.order_svc.create(user.id(), body).await?))
}

/// GET /orders — returns enriched list with event info per order.
pub async fn list_mine(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<OrderListItem>>> {
    Ok(Json(
        state
            .order_svc
            .list_mine(user.id(), q.page.unwrap_or(1), q.per_page.unwrap_or(20))
            .await?,
    ))
}

pub async fn get_one(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<OrderDetailResponse>> {
    Ok(Json(state.order_svc.detail(&id, user.id()).await?))
}

pub async fn pay(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
    Json(body): Json<PayOrderRequest>,
) -> AppResult<Json<OrderDetailResponse>> {
    Ok(Json(
        state
            .order_svc
            .pay(&id, user.id(), &user.0.name, body)
            .await?,
    ))
}

pub async fn cancel(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    state.order_svc.cancel(&id, user.id()).await?;
    Ok(Json(serde_json::json!({ "cancelled": true })))
}
