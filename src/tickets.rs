use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
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

pub async fn get_one(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<String>,
) -> AppResult<Json<TicketResponse>> {
    Ok(Json(
        state.ticket_svc.detail_for_customer(&id, user.id()).await?,
    ))
}

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
