use std::sync::Arc;
use validator::Validate;

use crate::models::tickets::{TicketResponse, ValidateTicketRequest};
use crate::repository::ticket::TicketRepository;
use crate::utils::error::{AppError, AppResult};

pub struct TicketService {
    repo: Arc<dyn TicketRepository>,
}

impl TicketService {
    pub fn new(repo: Arc<dyn TicketRepository>) -> Self {
        Self { repo }
    }

    pub async fn list_for_customer(
        &self,
        customer_id: &str,
        page: i64,
        per_page: i64,
    ) -> AppResult<Vec<TicketResponse>> {
        let page = page.max(1);
        let per_page = per_page.clamp(1, 100);
        let offset = page.max(1).saturating_sub(1).saturating_mul(per_page);
        Ok(self
            .repo
            .list_for_customer(customer_id, per_page, offset)
            .await?)
    }

    /// List all tickets that belong to a specific order.
    /// Returns 404 if the order has no tickets yet (e.g. still pending).
    /// Ownership is checked inside the repository query (AND o.customer_id = $2).
    pub async fn list_for_order(
        &self,
        order_id: &str,
        customer_id: &str,
        page: i64,
        per_page: i64,
    ) -> AppResult<Vec<TicketResponse>> {
        let page = page.max(1);
        let per_page = per_page.clamp(1, 100);
        let offset = page.max(1).saturating_sub(1).saturating_mul(per_page);
        let tickets = self
            .repo
            .list_by_order(order_id, customer_id, per_page, offset)
            .await?;
        Ok(tickets)
    }

    pub async fn detail_for_customer(
        &self,
        ticket_id: &str,
        customer_id: &str,
    ) -> AppResult<TicketResponse> {
        let (resp, owner_customer, _merchant) = self
            .repo
            .find_by_id(ticket_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Ticket not found".into()))?;
        if owner_customer != customer_id {
            return Err(AppError::Forbidden("Not your ticket".into()));
        }
        Ok(resp)
    }

    pub async fn validate_as_merchant(
        &self,
        merchant_id: &str,
        req: ValidateTicketRequest,
    ) -> AppResult<TicketResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        // Kepemilikan TIDAK lagi diperiksa di sini. Ia dipindah ke dalam
        // transaksi repository, sebelum baris ditandai terpakai — pemeriksaan
        // di sini terjadi sesudah commit, jadi ia menolak pemanggilnya tapi
        // kodenya sudah terlanjur hangus. Lihat catatan di `validate_by_code`.
        let resp = self
            .repo
            .validate_by_code(&req.ticket_code, merchant_id)
            .await
            .map_err(map_validate_err)?;

        Ok(resp)
    }
}

fn map_validate_err(e: anyhow::Error) -> AppError {
    let msg = e.to_string();
    if msg.contains("belongs to another merchant") {
        AppError::Forbidden("Kode ini milik toko lain.".into())
    } else if msg.contains("Ticket not found") {
        AppError::NotFound(msg)
    } else if msg.contains("already used")
        || msg.contains("refunded")
        || msg.contains("expired")
        || msg.contains("cannot be validated")
    {
        AppError::Conflict(msg)
    } else {
        AppError::Internal(e)
    }
}
