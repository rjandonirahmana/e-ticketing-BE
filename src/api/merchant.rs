//! api/merchant.rs
//!
//! GET  /api/merchant/products      (private, query: page)
//! GET  /api/merchant/products/:slug (private)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::state::AppState;
use super::extractor::{app_err, AuthUser};

#[derive(Deserialize, Default)]
pub struct PageQuery {
    pub page: Option<i64>,
}

async fn list_merchant_products(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<PageQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    use crate::models::products::ProductListQuery;
    let query = ProductListQuery {
        page: q.page,
        per_page: Some(20),
        city: None,
        category: None,
        search: None,
        status: None,
    };
    let result = state
        .product_svc
        .list(query, Some(&claims.user_id))
        .await
        .map_err(app_err)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

async fn get_merchant_product(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // Login saja tidak cukup: tanpa cek pemilik, merchant mana pun bisa membaca
    // product merchant lain hanya dengan menyalin slug-nya.
    let product = state
        .product_svc
        .get_for_merchant(&slug, &claims.user_id, claims.role == "admin")
        .await
        .map_err(app_err)?;
    Ok(Json(serde_json::to_value(product).unwrap_or_default()))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/merchant/products", get(list_merchant_products))
        .route("/merchant/products/{slug}", get(get_merchant_product))
}
