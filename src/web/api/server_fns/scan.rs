use crate::web::models::*;
use leptos::prelude::*;
use super::helpers::*;

#[server(ScanTicket, "/api-fn")]
pub async fn scan_ticket(ticket_code: String) -> Result<ScanValidateResult, ServerFnError> {
    use crate::models::tickets::ValidateTicketRequest;
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let req = ValidateTicketRequest { ticket_code: ticket_code.clone() };
    let resp = state
        .ticket_svc
        .validate_as_merchant(&claims.user_id, req)
        .await
        .map_err(map_app_error)?;
    return Ok(ScanValidateResult {
        event_title: resp.event_name,
        tier_name: resp.variant_name,
        status: resp.status,
        ticket_code,
    });
}
