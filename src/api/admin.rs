//! api/admin.rs
//!
//! GET /api/admin/products          (private, query: page, status)
//! PUT /api/admin/products/:id      (private, body: { status })

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::state::AppState;
use super::extractor::{app_err, AuthUser};

#[derive(Deserialize, Default)]
pub struct AdminProductsQuery {
    pub page: Option<i64>,
    pub status: Option<String>,
}

async fn list_admin_products(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Query(q): Query<AdminProductsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if claims.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "message": "Akses ditolak" })),
        ));
    }
    use crate::models::products::ProductListQuery;
    let query = ProductListQuery {
        page: q.page,
        per_page: Some(50),
        city: None,
        category: None,
        search: None,
        status: q.status,
    };
    let result = state.product_svc.list(query, None).await.map_err(app_err)?;
    Ok(Json(serde_json::to_value(result).unwrap_or_default()))
}

#[derive(Deserialize)]
pub struct UpdateStatusReq {
    pub status: String,
}

async fn update_product_status(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateStatusReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if claims.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "message": "Akses ditolak" })),
        ));
    }
    let result = state
        .product_svc
        .admin_update_status(&id, &body.status)
        .await
        .map_err(app_err)?;
    // Sama seperti jalur server-function-nya: status berubah = data publik
    // berubah, jadi cache-nya harus ikut dibuang saat itu juga.
    state
        .pub_cache
        .invalidate_product(&result.slug, &result.merchant_id)
        .await;
    Ok(Json(serde_json::json!({ "id": result.id, "status": result.status })))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/admin/products", get(list_admin_products))
        .route("/admin/products/{id}", put(update_product_status))
}
