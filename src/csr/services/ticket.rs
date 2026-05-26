use super::backend::{ticket_to_issued, BeTicketResponse};
use super::client::{get_private, ApiError};
use crate::csr::models::tickets::TicketResponse;
use crate::csr::models::*;

/// GET /tickets — list the tickets owned by the current user.
///
/// The backend returns a flat array; pagination params are forwarded when
/// the frontend supplies them. The `filter` field is applied client-side
/// because the backend does not currently filter by ACTIVE/PAST/SHARED.
pub async fn list_my_tickets(
    req: &ListMyTicketsRequest,
) -> Result<ListMyTicketsResponse, ApiError> {
    let mut params: Vec<String> = Vec::new();
    if req.page > 0 {
        params.push(format!("page={}", req.page));
    }
    if req.page_size > 0 {
        params.push(format!("per_page={}", req.page_size));
    }
    let path = if params.is_empty() {
        "/tickets".to_string()
    } else {
        format!("/tickets?{}", params.join("&"))
    };

    let resp: Vec<BeTicketResponse> = get_private(&path).await?;
    let mut tickets: Vec<IssuedTicket> = resp.into_iter().map(ticket_to_issued).collect();

    // Optional client-side filter to honour the existing UI contract.
    let filter = req.filter.to_ascii_uppercase();
    if !filter.is_empty() {
        tickets.retain(|t| t.status == filter);
    }

    Ok(ListMyTicketsResponse { tickets })
}

pub async fn get_ticket(id: &str) -> Result<TicketResponse, ApiError> {
    let path = format!("/tickets/{}", id);
    let resp: TicketResponse = get_private(&path).await?;
    Ok(resp)
}
