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
            (SELECT COUNT(*)::BIGINT FROM events) AS total_events,
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
        total_events: row.try_get("total_events").unwrap_or(0),
        total_orders: row.try_get("total_orders").unwrap_or(0),
        total_revenue: revenue.to_f64().unwrap_or(0.0),
    });
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

#[server(GetAdminEvents, "/api-fn")]
pub async fn get_admin_events(
    page: Option<i64>,
    status: Option<String>,
) -> Result<PaginatedEvents, ServerFnError> {
    use crate::models::events::EventListQuery;
    let _claims = require_role("admin").await?;
    let state = app_state().await?;
    let q = EventListQuery {
        page,
        per_page: Some(50),
        city: None,
        category: None,
        search: None,
        status,
    };
    let result = state
        .event_svc
        .list(q, None)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_paginated_events_to_web(result));
}

#[server(UpdateEventStatusAdmin, "/api-fn")]
pub async fn update_event_status_admin(
    event_id: String,
    new_status: String,
) -> Result<serde_json::Value, ServerFnError> {
    let _claims = require_role("admin").await?;
    let state = app_state().await?;
    let result = state
        .event_svc
        .admin_update_status(&event_id, &new_status)
        .await
        .map_err(map_app_error)?;
    return Ok(serde_json::json!({ "id": result.id, "status": result.status }));
}
