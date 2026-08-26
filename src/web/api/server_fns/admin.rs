use crate::web::models::*;
use leptos::prelude::*;
#[cfg(feature = "ssr")]
use super::helpers::*;

#[server(GetAdminStats, "/api-fn")]
pub async fn get_admin_stats() -> Result<AdminStats, ServerFnError> {
    use rust_decimal::prelude::ToPrimitive;
    let _claims = require_role("admin").await?;
    let state = app_state().await?;

    // Single round-trip: each scalar subquery is cheap (indexed COUNT / SUM).
    // Revenue counts only paid orders.
    let row = crate::repository::db::exec_one(
        &state.pool,
        r#"
        SELECT
            (SELECT COUNT(*)::BIGINT FROM users)  AS total_users,
            (SELECT COUNT(*)::BIGINT FROM products) AS total_products,
            (SELECT COUNT(*)::BIGINT FROM orders) AS total_orders,
            (SELECT COALESCE(SUM(total_amount), 0)::DECIMAL
                 FROM orders WHERE status = 'paid') AS total_revenue
        "#,
        &[],
    )
    .await
    .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;

    let revenue: rust_decimal::Decimal = row
        .try_get("total_revenue")
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e.to_string()) })?;

    return Ok(AdminStats {
        total_users: row.try_get("total_users").unwrap_or(0),
        total_products: row.try_get("total_products").unwrap_or(0),
        total_orders: row.try_get("total_orders").unwrap_or(0),
        total_revenue: revenue.to_f64().unwrap_or(0.0),
    });
}

/// Buat banner baru (admin). `image_url` = hasil unggah via
/// POST /upload/merchant-image (endpoint itu sudah menerima role admin).
/// Tayang langsung (start = sekarang, tanpa tanggal berakhir).
#[server(CreateBanner, "/api-fn")]
pub async fn create_banner(
    image_url: String,
    click_url: Option<String>,
) -> Result<(), ServerFnError> {
    let _claims = require_role("admin").await?;
    if image_url.trim().is_empty() {
        return Err(ServerFnError::ServerError("URL gambar wajib diisi".into()));
    }
    let state = app_state().await?;
    let req = crate::models::banners::CreateBannerRequest {
        image_url: image_url.clone(),
        click_url: click_url.filter(|s| !s.trim().is_empty()),
        start_date: chrono::Utc::now(),
        end_date: None,
        event_id: None,
    };
    state
        .banner_svc
        .create(image_url, req)
        .await
        .map_err(map_app_error)?;
    // Cache banner publik (get_banners + REST /api/banners) harus segar.
    state.pub_cache.banners.invalidate(&()).await;
    Ok(())
}

/// Update banner (admin): ganti gambar dan/atau link. Field None = tak diubah.
#[server(UpdateBanner, "/api-fn")]
pub async fn update_banner(
    id: i64,
    image_url: Option<String>,
    click_url: Option<String>,
) -> Result<(), ServerFnError> {
    let _claims = require_role("admin").await?;
    let state = app_state().await?;
    let req = crate::models::banners::UpdateBannerRequest {
        image_url: image_url.clone().filter(|s| !s.trim().is_empty()),
        click_url: click_url.filter(|s| !s.trim().is_empty()),
        start_date: None,
        end_date: None,
        event_id: None,
    };
    // Service sudah menerjemahkan id tak dikenal → AppError::NotFound.
    state
        .banner_svc
        .update(id, image_url.filter(|s| !s.trim().is_empty()), req)
        .await
        .map_err(map_app_error)?;
    state.pub_cache.banners.invalidate(&()).await;
    Ok(())
}

/// Hapus banner (admin) — soft delete (deleted_at diisi, riwayat tetap ada).
#[server(DeleteBanner, "/api-fn")]
pub async fn delete_banner(id: i64) -> Result<(), ServerFnError> {
    let _claims = require_role("admin").await?;
    let state = app_state().await?;
    state
        .banner_svc
        .soft_delete(id)
        .await
        .map_err(map_app_error)?;
    state.pub_cache.banners.invalidate(&()).await;
    Ok(())
}

#[server(GetAdminUsers, "/api-fn")]
pub async fn get_admin_users(page: Option<i64>) -> Result<serde_json::Value, ServerFnError> {
    let _claims = require_role("admin").await?;
    let _p = page.unwrap_or(1);
    // TODO: add admin user list service method
    return Ok(serde_json::json!({ "data": [], "total": 0 }));
}

#[server(GetAdminOrders, "/api-fn")]
pub async fn get_admin_orders(page: Option<i64>) -> Result<serde_json::Value, ServerFnError> {
    let _claims = require_role("admin").await?;
    let _p = page.unwrap_or(1);
    // TODO: add admin order list service method
    return Ok(serde_json::json!({ "data": [], "total": 0 }));
}

#[server(GetAdminProducts, "/api-fn")]
pub async fn get_admin_products(
    page: Option<i64>,
    status: Option<String>,
) -> Result<PaginatedProducts, ServerFnError> {
    use crate::models::products::ProductListQuery;
    let _claims = require_role("admin").await?;
    let state = app_state().await?;
    let q = ProductListQuery {
        page,
        per_page: Some(50),
        city: None,
        category: None,
        search: None,
        status,
    };
    let result = state
        .product_svc
        .list(q, None)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_paginated_products_to_web(result));
}

#[server(UpdateProductStatusAdmin, "/api-fn")]
pub async fn update_product_status_admin(
    event_id: String,
    new_status: String,
) -> Result<serde_json::Value, ServerFnError> {
    let _claims = require_role("admin").await?;
    let state = app_state().await?;
    let result = state
        .product_svc
        .admin_update_status(&event_id, &new_status)
        .await
        .map_err(map_app_error)?;
    // Inilah saat sebuah product menjadi (atau berhenti) publik. Tanpa membuang
    // cache di sini, persetujuan admin baru terasa 30–60 detik kemudian —
    // termasuk saat product dibatalkan, yang justru harus hilang seketika.
    state
        .pub_cache
        .invalidate_product(&result.slug, &result.merchant_id)
        .await;
    return Ok(serde_json::json!({ "id": result.id, "status": result.status }));
}
