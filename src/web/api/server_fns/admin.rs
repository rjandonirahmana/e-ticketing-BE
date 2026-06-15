use crate::web::models::*;
use leptos::prelude::*;
#[cfg(feature = "ssr")]
use super::helpers::*;

#[server(GetAdminStats, "/api-fn")]
pub async fn get_admin_stats() -> Result<AdminStats, ServerFnError> {
    let _claims = auth_claims().await?;
    // TODO: add admin stats service method
    return Ok(AdminStats {
        total_users: 0,
        total_events: 0,
        total_orders: 0,
        total_revenue: 0.0,
    });
}

#[server(GetAdminUsers, "/api-fn")]
pub async fn get_admin_users(page: Option<i64>) -> Result<serde_json::Value, ServerFnError> {
    let _claims = auth_claims().await?;
    let _p = page.unwrap_or(1);
    // TODO: add admin user list service method
    return Ok(serde_json::json!({ "data": [], "total": 0 }));
}

#[server(GetAdminOrders, "/api-fn")]
pub async fn get_admin_orders(page: Option<i64>) -> Result<serde_json::Value, ServerFnError> {
    let _claims = auth_claims().await?;
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
    let _claims = auth_claims().await?;
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
    let _claims = auth_claims().await?;
    let state = app_state().await?;
    let result = state
        .event_svc
        .admin_update_status(&event_id, &new_status)
        .await
        .map_err(map_app_error)?;
    return Ok(serde_json::json!({ "id": result.id, "status": result.status }));
}
