use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first, exec_one, exec_rows};
use crate::models::event_variant::TicketVariant;
use crate::models::events::{CreateEventRequest, Event, UpdateEventRequest};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

// ── Static query strings ──────────────────────────────────────────────────────
//
// NUMERIC columns (`price`, `sale_price`) are cast to FLOAT8 so they bind
// cleanly to `f64` without pulling in `rust_decimal`.

static EVENT_COLS: &str = r#"
    id,
    merchant_id,
    name,
    description,
    price::FLOAT8 AS price,
    sale_price::FLOAT8 AS sale_price,
    sale_price_start_date,
    sale_price_end_date,
    venue,
    city,
    event_date,
    start_time,
    end_time,
    status,
    created_at,
    updated_at
"#;

static FIND_EVENT_BY_ID: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM events WHERE id = $1", EVENT_COLS));

static INSERT_EVENT: LazyLock<String> = LazyLock::new(|| {
    format!(
        "INSERT INTO events (id, merchant_id, name, description, price, venue, city, \
         event_date, start_time, end_time) \
         VALUES ($1, $2, $3, $4, 0, $5, $6, $7, $8, $9) RETURNING {}",
        EVENT_COLS
    )
});

static UPDATE_EVENT: &str = r#"
    UPDATE events
       SET name        = COALESCE($2, name),
           description = COALESCE($3, description),
           venue       = COALESCE($4, venue),
           city        = COALESCE($5, city),
           event_date  = COALESCE($6, event_date),
           start_time  = COALESCE($7, start_time),
           end_time    = COALESCE($8, end_time),
           status      = COALESCE($9, status)
     WHERE id = $1
"#;

static DELETE_EVENT: &str = "DELETE FROM events WHERE id = $1";

// Variant queries
static VARIANT_COLS: &str = r#"
    id,
    event_id,
    name,
    description,
    price::FLOAT8 AS price,
    sale_price::FLOAT8 AS sale_price,
    sale_price_start_date,
    sale_price_end_date,
    quota,
    sold,
    max_per_order,
    is_active,
    sort_order,
    created_at,
    updated_at
"#;

static FIND_VARIANTS_BY_EVENT: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM ticket_variants WHERE event_id = $1 ORDER BY sort_order, created_at",
        VARIANT_COLS
    )
});

static FIND_VARIANT_BY_ID: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM ticket_variants WHERE id = $1", VARIANT_COLS));

static INSERT_VARIANT: LazyLock<String> = LazyLock::new(|| {
    format!(
        "INSERT INTO ticket_variants (id, event_id, name, description, price, quota, \
         max_per_order, sort_order) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {}",
        VARIANT_COLS
    )
});

static UPDATE_VARIANT: &str = r#"
    UPDATE ticket_variants
       SET name          = COALESCE($2, name),
           description   = COALESCE($3, description),
           price         = COALESCE($4, price),
           quota         = COALESCE($5, quota),
           max_per_order = COALESCE($6, max_per_order),
           is_active     = COALESCE($7, is_active),
           sort_order    = COALESCE($8, sort_order)
     WHERE id = $1
"#;

static DELETE_VARIANT: &str = "DELETE FROM ticket_variants WHERE id = $1";

// ── Trait ─────────────────────────────────────────────────────────────────────

pub struct EventListFilter<'a> {
    pub city: Option<&'a str>,
    pub status: Option<&'a str>,
    pub merchant_id: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

#[async_trait]
pub trait EventRepository: Send + Sync {
    // Events
    async fn list(&self, f: &EventListFilter<'_>) -> Result<Vec<Event>>;
    async fn count(&self, f: &EventListFilter<'_>) -> Result<i64>;
    async fn find_by_id(&self, id: &str) -> Result<Option<Event>>;
    async fn create(&self, merchant_id: &str, req: &CreateEventRequest) -> Result<Event>;
    async fn update(&self, id: &str, req: &UpdateEventRequest) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<()>;

    // Ticket variants
    async fn list_variants(&self, event_id: &str) -> Result<Vec<TicketVariant>>;
    async fn find_variant(&self, id: &str) -> Result<Option<TicketVariant>>;
    async fn create_variant(
        &self,
        event_id: &str,
        name: &str,
        description: Option<&str>,
        price: f64,
        quota: i32,
        max_per_order: Option<i32>,
        sort_order: i32,
    ) -> Result<TicketVariant>;
    async fn update_variant(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        price: Option<f64>,
        quota: Option<i32>,
        max_per_order: Option<i32>,
        is_active: Option<bool>,
        sort_order: Option<i32>,
    ) -> Result<()>;
    async fn delete_variant(&self, id: &str) -> Result<()>;
}

// ── Postgres impl ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PgEventRepository {
    pool: Pool,
}

impl PgEventRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn row_to_event(row: &Row) -> Result<Event> {
        let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
        let merchant_bytes: Vec<u8> = row.try_get("merchant_id").context("merchant_id")?;
        Ok(Event {
            id: bin_to_ulid(id_bytes)?,
            merchant_id: bin_to_ulid(merchant_bytes)?,
            name: row.try_get("name").context("name")?,
            description: row.try_get("description").context("description")?,
            price: row.try_get("price").context("price")?,
            sale_price: row.try_get("sale_price").context("sale_price")?,
            sale_price_start_date: row.try_get("sale_price_start_date")?,
            sale_price_end_date: row.try_get("sale_price_end_date")?,
            venue: row.try_get("venue").context("venue")?,
            city: row.try_get("city").context("city")?,
            event_date: row.try_get("event_date").context("event_date")?,
            start_time: row.try_get("start_time")?,
            end_time: row.try_get("end_time")?,
            status: row.try_get("status").context("status")?,
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
        })
    }

    fn row_to_variant(row: &Row) -> Result<TicketVariant> {
        let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
        let event_bytes: Vec<u8> = row.try_get("event_id").context("event_id")?;
        Ok(TicketVariant {
            id: bin_to_ulid(id_bytes)?,
            event_id: bin_to_ulid(event_bytes)?,
            name: row.try_get("name").context("name")?,
            description: row.try_get("description").context("description")?,
            price: row.try_get("price").context("price")?,
            sale_price: row.try_get("sale_price").context("sale_price")?,
            sale_price_start_date: row.try_get("sale_price_start_date")?,
            sale_price_end_date: row.try_get("sale_price_end_date")?,
            quota: row.try_get("quota").context("quota")?,
            sold: row.try_get("sold").context("sold")?,
            max_per_order: row.try_get("max_per_order")?,
            is_active: row.try_get("is_active").context("is_active")?,
            sort_order: row.try_get("sort_order").context("sort_order")?,
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
        })
    }
}

#[async_trait]
impl EventRepository for PgEventRepository {
    async fn list(&self, f: &EventListFilter<'_>) -> Result<Vec<Event>> {
        // Build dynamic WHERE — we keep it simple and correct rather than trying to
        // share one LazyLock string across every filter combination.
        let mut sql = format!("SELECT {} FROM events WHERE 1 = 1", EVENT_COLS);
        let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
        let mut idx = 1usize;

        let mid_vec;
        if let Some(mid) = f.merchant_id {
            mid_vec = id_to_vec(mid)?;
            sql.push_str(&format!(" AND merchant_id = ${idx}"));
            params.push(Box::new(mid_vec));
            idx += 1;
        }
        if let Some(city) = f.city {
            sql.push_str(&format!(" AND city = ${idx}"));
            params.push(Box::new(city.to_string()));
            idx += 1;
        }
        if let Some(status) = f.status {
            sql.push_str(&format!(" AND status = ${idx}"));
            params.push(Box::new(status.to_string()));
            idx += 1;
        }
        sql.push_str(&format!(
            " ORDER BY event_date ASC LIMIT ${} OFFSET ${}",
            idx,
            idx + 1
        ));
        params.push(Box::new(f.limit));
        params.push(Box::new(f.offset));

        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(|p| p.as_ref() as _).collect();
        let rows = exec_rows(&self.pool, &sql, &refs).await?;
        rows.iter().map(Self::row_to_event).collect()
    }

    async fn count(&self, f: &EventListFilter<'_>) -> Result<i64> {
        let mut sql = String::from("SELECT COUNT(*)::BIGINT AS c FROM events WHERE 1 = 1");
        let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
        let mut idx = 1usize;

        let mid_vec;
        if let Some(mid) = f.merchant_id {
            mid_vec = id_to_vec(mid)?;
            sql.push_str(&format!(" AND merchant_id = ${idx}"));
            params.push(Box::new(mid_vec));
            idx += 1;
        }
        if let Some(city) = f.city {
            sql.push_str(&format!(" AND city = ${idx}"));
            params.push(Box::new(city.to_string()));
            idx += 1;
        }
        if let Some(status) = f.status {
            sql.push_str(&format!(" AND status = ${idx}"));
            params.push(Box::new(status.to_string()));
            let _ = idx;
        }

        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(|p| p.as_ref() as _).collect();
        let row = exec_one(&self.pool, &sql, &refs).await?;
        Ok(row.try_get::<_, i64>("c")?)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Event>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_EVENT_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_event).transpose()
    }

    async fn create(&self, merchant_id: &str, req: &CreateEventRequest) -> Result<Event> {
        let id = new_ulid();
        let id_vec = ulid_to_vec(&id)?;
        let mid_vec = id_to_vec(merchant_id)?;
        let row = exec_one(
            &self.pool,
            &INSERT_EVENT,
            &[
                &id_vec,
                &mid_vec,
                &req.name,
                &req.description,
                &req.venue,
                &req.city,
                &req.event_date,
                &req.start_time,
                &req.end_time,
            ],
        )
        .await?;
        Self::row_to_event(&row)
    }

    async fn update(&self, id: &str, req: &UpdateEventRequest) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        exec_drop(
            &self.pool,
            UPDATE_EVENT,
            &[
                &id_vec,
                &req.name,
                &req.description,
                &req.venue,
                &req.city,
                &req.event_date,
                &req.start_time,
                &req.end_time,
                &req.status,
            ],
        )
        .await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        exec_drop(&self.pool, DELETE_EVENT, &[&id_vec]).await?;
        Ok(())
    }

    async fn list_variants(&self, event_id: &str) -> Result<Vec<TicketVariant>> {
        let id_vec = id_to_vec(event_id)?;
        let rows = exec_rows(&self.pool, &FIND_VARIANTS_BY_EVENT, &[&id_vec]).await?;
        rows.iter().map(Self::row_to_variant).collect()
    }

    async fn find_variant(&self, id: &str) -> Result<Option<TicketVariant>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_VARIANT_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_variant).transpose()
    }

    async fn create_variant(
        &self,
        event_id: &str,
        name: &str,
        description: Option<&str>,
        price: f64,
        quota: i32,
        max_per_order: Option<i32>,
        sort_order: i32,
    ) -> Result<TicketVariant> {
        let id = new_ulid();
        let id_vec = ulid_to_vec(&id)?;
        let event_vec = id_to_vec(event_id)?;
        let row = exec_one(
            &self.pool,
            &INSERT_VARIANT,
            &[
                &id_vec,
                &event_vec,
                &name,
                &description,
                &price,
                &quota,
                &max_per_order,
                &sort_order,
            ],
        )
        .await?;
        Self::row_to_variant(&row)
    }

    async fn update_variant(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        price: Option<f64>,
        quota: Option<i32>,
        max_per_order: Option<i32>,
        is_active: Option<bool>,
        sort_order: Option<i32>,
    ) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        exec_drop(
            &self.pool,
            UPDATE_VARIANT,
            &[
                &id_vec,
                &name,
                &description,
                &price,
                &quota,
                &max_per_order,
                &is_active,
                &sort_order,
            ],
        )
        .await?;
        Ok(())
    }

    async fn delete_variant(&self, id: &str) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        exec_drop(&self.pool, DELETE_VARIANT, &[&id_vec]).await?;
        Ok(())
    }
}
