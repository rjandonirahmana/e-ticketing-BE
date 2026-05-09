use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_first, exec_rows, get_conn};
use crate::models::tickets::TicketResponse;
use crate::utils::ulid::{bin_to_ulid, id_to_vec};

// ── Static query strings ──────────────────────────────────────────────────────
//
// Tickets are always shown enriched with their order/event/variant context so
// the wallet view doesn't have to do N round-trips. We materialise the join
// in SQL and shape the row in `row_to_ticket_response`.

static TICKET_DETAIL_COLS: &str = r#"
    t.id            AS ticket_id,
    t.ticket_code,
    t.status,
    t.used_at,
    t.created_at,

    oi.id           AS order_item_id,
    oi.unit_price::FLOAT8 AS unit_price,

    o.id            AS order_id,
    o.order_code,
    o.customer_id,

    tv.id           AS variant_id,
    tv.name         AS variant_name,

    e.id            AS event_id,
    e.name          AS event_name,
    e.event_date,
    e.venue         AS event_venue,
    e.city          AS event_city,
    e.cover_url     AS cover_url,
    e.merchant_id
"#;

static FROM_JOINS: &str = r#"
      FROM tickets t
      JOIN order_items oi    ON t.order_item_id = oi.id
      JOIN orders o          ON oi.order_id = o.id
      JOIN event_variants tv ON oi.ticket_variant_id = tv.id
      JOIN events e           ON tv.event_id = e.id
"#;

static FIND_BY_ID: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} {} WHERE t.id = $1",
        TICKET_DETAIL_COLS, FROM_JOINS
    )
});

#[allow(dead_code)]
static FIND_BY_CODE: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} {} WHERE t.ticket_code = $1",
        TICKET_DETAIL_COLS, FROM_JOINS
    )
});

static LIST_FOR_CUSTOMER: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} {} WHERE o.customer_id = $1 ORDER BY t.created_at DESC LIMIT $2 OFFSET $3",
        TICKET_DETAIL_COLS, FROM_JOINS
    )
});

#[async_trait]
pub trait TicketRepository: Send + Sync {
    async fn list_for_customer(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TicketResponse>>;

    async fn find_by_id(&self, id: &str) -> Result<Option<(TicketResponse, String, String)>>;

    /// Lookup by code + atomically transition `active` → `used`.
    /// Only the merchant that owns the parent event is allowed to do this; the
    /// service layer is responsible for checking ownership using the merchant_id
    /// returned in the success path.
    /// Returns the ticket detail and the merchant_id that owns the event.
    async fn validate_by_code(&self, code: &str) -> Result<(TicketResponse, String)>;
}

#[derive(Clone)]
pub struct PgTicketRepository {
    pool: Pool,
}

impl PgTicketRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn row_to_response(row: &Row) -> Result<(TicketResponse, String, String)> {
        // The detail row also carries `customer_id` and `merchant_id` so the
        // service layer can do auth checks. We return them as a tuple alongside
        // the API-shaped TicketResponse.
        let ticket_bytes: Vec<u8> = row.try_get("ticket_id").context("ticket_id")?;
        let order_bytes: Vec<u8> = row.try_get("order_id").context("order_id")?;
        let variant_bytes: Vec<u8> = row.try_get("variant_id").context("variant_id")?;
        let event_bytes: Vec<u8> = row.try_get("event_id").context("event_id")?;
        let customer_bytes: Vec<u8> = row.try_get("customer_id").context("customer_id")?;
        let merchant_bytes: Vec<u8> = row.try_get("merchant_id").context("merchant_id")?;

        let resp = TicketResponse {
            id: bin_to_ulid(ticket_bytes)?,
            ticket_code: row.try_get("ticket_code")?,
            status: row.try_get("status")?,
            used_at: row.try_get("used_at")?,
            created_at: row.try_get("created_at")?,
            order_id: bin_to_ulid(order_bytes)?,
            order_code: row.try_get("order_code")?,
            event_id: bin_to_ulid(event_bytes)?,
            event_name: row.try_get("event_name")?,
            event_date: row.try_get("event_date")?,
            event_venue: row.try_get("event_venue")?,
            event_city: row.try_get("event_city")?,
            variant_id: bin_to_ulid(variant_bytes)?,
            variant_name: row.try_get("variant_name")?,
            unit_price: row.try_get("unit_price")?,
            cover_url: row.try_get("cover_url").unwrap_or(None),
        };

        Ok((
            resp,
            bin_to_ulid(customer_bytes)?,
            bin_to_ulid(merchant_bytes)?,
        ))
    }
}

#[async_trait]
impl TicketRepository for PgTicketRepository {
    async fn list_for_customer(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TicketResponse>> {
        let id_vec = id_to_vec(customer_id)?;
        let rows = exec_rows(&self.pool, &LIST_FOR_CUSTOMER, &[&id_vec, &limit, &offset]).await?;
        rows.iter()
            .map(|r| Self::row_to_response(r).map(|(t, _, _)| t))
            .collect()
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<(TicketResponse, String, String)>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_response).transpose()
    }

    async fn validate_by_code(&self, code: &str) -> Result<(TicketResponse, String)> {
        let mut conn = get_conn(&self.pool).await?;
        let tx = conn.transaction().await?;

        // Lock the ticket row to avoid a double-validate race.
        let row = tx
            .query_opt(
                "SELECT id, status FROM tickets WHERE ticket_code = $1 FOR UPDATE",
                &[&code],
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("Ticket not found"))?;

        let id_bytes: Vec<u8> = row.try_get("id")?;
        let status: String = row.try_get("status")?;

        match status.as_str() {
            "active" => {}
            "used" => bail!("Ticket already used"),
            "refunded" => bail!("Ticket has been refunded"),
            "expired" => bail!("Ticket has expired"),
            other => bail!("Ticket status '{}' cannot be validated", other),
        }

        tx.execute(
            "UPDATE tickets SET status = 'used', used_at = NOW() WHERE id = $1",
            &[&id_bytes],
        )
        .await?;

        // Re-fetch the enriched detail row in the same tx so the response
        // reflects the new `used_at`.
        let detail_row = tx
            .query_one(
                &format!(
                    "SELECT {} {} WHERE t.id = $1",
                    TICKET_DETAIL_COLS, FROM_JOINS
                ),
                &[&id_bytes],
            )
            .await?;

        tx.commit().await?;

        let (resp, _customer, merchant) = Self::row_to_response(&detail_row)?;
        Ok((resp, merchant))
    }
}
