use crate::web::models::*;
use leptos::prelude::*;
use super::helpers::*;

#[server(GetMyTickets, "/api-fn")]
pub async fn get_my_tickets() -> Result<Vec<TicketResponse>, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let tickets = state
        .ticket_svc
        .list_for_customer(&claims.user_id, 1, 100)
        .await
        .map_err(map_app_error)?;
    return Ok(tickets.into_iter().map(srv_ticket_to_web).collect());
}

#[server(GetTicketDetail, "/api-fn")]
pub async fn get_ticket_detail(id: String) -> Result<TicketResponse, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let ticket = state
        .ticket_svc
        .detail_for_customer(&id, &claims.user_id)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_ticket_to_web(ticket));
}
