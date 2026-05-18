use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::middleware::auth::AuthUser;
use crate::models::tickets::{TicketResponse, ValidateTicketRequest};
use crate::state::AppState;
use crate::utils::error::AppResult;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

/// GET /api/tickets — all tickets owned by the logged-in user.
pub async fn list_mine(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<TicketResponse>>> {
    Ok(Json(
        state
            .ticket_svc
            .list_for_customer(user.id(), q.page.unwrap_or(1), q.per_page.unwrap_or(20))
            .await?,
    ))
}

/// GET /api/tickets/:id — single ticket detail.
pub async fn get_one(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<TicketResponse>> {
    Ok(Json(
        state.ticket_svc.detail_for_customer(&id, user.id()).await?,
    ))
}

/// GET /api/orders/:id/tickets — all tickets for a specific order.
/// Route is registered under orders path but handled here to keep ticket
/// logic together.
///
/// Returns the tickets that were minted when the order was paid.
/// If the order is still pending (no tickets minted yet) the list is empty.
/// Ownership is enforced: a user can only see their own order's tickets.
pub async fn list_by_order(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(order_id): Path<String>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<TicketResponse>>> {
    Ok(Json(
        state
            .ticket_svc
            .list_for_order(
                &order_id,
                user.id(),
                q.page.unwrap_or(1),
                q.per_page.unwrap_or(50),
            )
            .await?,
    ))
}

/// POST /api/tickets/validate — merchant scans a ticket code to mark it used.
pub async fn validate(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<ValidateTicketRequest>,
) -> AppResult<Json<TicketResponse>> {
    user.require_role("merchant")?;
    Ok(Json(
        state
            .ticket_svc
            .validate_as_merchant(user.id(), body)
            .await?,
    ))
}
