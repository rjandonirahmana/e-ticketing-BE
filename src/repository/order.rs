use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use rust_decimal::Decimal;
use std::sync::LazyLock;
use tokio_postgres::types::ToSql;
use tokio_postgres::Row;

use super::db::{exec_first, exec_rows};
use crate::models::orders::{Order, OrderItemResponse, OrderListItem};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

// ── Static query strings ──────────────────────────────────────────────────────

static ORDER_COLS: &str = "id, customer_id, order_code, status, total_amount, \
     payment_method, paid_at, expired_at, created_at, updated_at";

static FIND_ORDER_BY_ID: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM orders WHERE id = $1", ORDER_COLS));

static LIST_ORDERS_BY_CUSTOMER: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM orders WHERE customer_id = $1 \
         ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        ORDER_COLS
    )
});

/// Enriched list query — joins first event info per order via DISTINCT ON.
/// DISTINCT ON (oi.order_id) picks one item per order (earliest by created_at),
/// then LEFT JOINs events to get name, date, venue, cover_url.
static LIST_ORDERS_WITH_EVENT: &str = r#"
    WITH first_item AS (
        SELECT DISTINCT ON (oi.order_id)
            oi.order_id,
            e.name        AS event_name,
            e.event_date  AS event_date,
            e.venue       AS venue,
            e.cover_url   AS cover_url
        FROM order_items oi
        JOIN event_variants ev ON oi.ticket_variant_id = ev.id
        JOIN events e           ON ev.event_id = e.id
        ORDER BY oi.order_id, oi.created_at
    )
    SELECT
        o.id, o.customer_id, o.order_code, o.status, o.total_amount,
        o.payment_method, o.paid_at, o.expired_at, o.created_at, o.updated_at,
        fi.event_name, fi.event_date, fi.venue, fi.cover_url
    FROM orders o
    LEFT JOIN first_item fi ON fi.order_id = o.id
    WHERE o.customer_id = $1
    ORDER BY o.created_at DESC
    LIMIT $2 OFFSET $3
"#;

/// Query items dengan full join — dipakai oleh list_items (repo) dan
/// fetch_items_detail (OrderTx, untuk response pay() dari dalam TX).
static QUERY_ITEMS_DETAIL: &str = r#"
    SELECT
        oi.id                 AS item_id,
        oi.ticket_variant_id,
        oi.quantity,
        oi.unit_price         AS unit_price,
        oi.subtotal           AS subtotal,
        tv.name               AS variant_name,
        tv.event_id,
        e.name                AS event_name
    FROM order_items oi
    JOIN event_variants tv ON oi.ticket_variant_id = tv.id
    JOIN events e           ON tv.event_id = e.id
    WHERE oi.order_id = $1
    ORDER BY oi.created_at
"#;

// ── Prepared statement SQL strings ───────────────────────────────────────────

static STMT_LOCK_VARIANTS: &str = r#"
    SELECT
        ev.id,
        ev.price,
        CASE
            WHEN ev.sale_price IS NOT NULL
                AND NOW() BETWEEN
                    COALESCE(ev.sale_price_start_date, '-infinity'::timestamptz)
                AND COALESCE(ev.sale_price_end_date,   'infinity'::timestamptz)
            THEN ev.sale_price
            ELSE ev.price
        END             AS effective_price,
        ev.quota,
        ev.sold,
        ev.max_per_order,
        ev.is_active,
        ev.name         AS variant_name,
        e.id            AS event_id_bytes,
        e.name          AS event_name
    FROM event_variants ev
    JOIN events e ON ev.event_id = e.id
    WHERE ev.id = ANY($1)
    FOR UPDATE OF ev
"#;

static STMT_INSERT_ORDER_SIMPLE: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"INSERT INTO orders
           (id, customer_id, order_code, status, total_amount, expired_at)
           VALUES ($1, $2, $3, 'pending', $4, $5)
           RETURNING {}"#,
        ORDER_COLS
    )
});

static STMT_INSERT_ORDER_IDEMPOTENCY: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"WITH ins AS (
               INSERT INTO orders
                   (id, customer_id, order_code, status, total_amount,
                    expired_at, idempotency_key)
               VALUES ($1, $2, $3, 'pending', $4, $5, $6)
               ON CONFLICT (customer_id, idempotency_key)
               WHERE idempotency_key IS NOT NULL
               DO NOTHING
               RETURNING {0}, TRUE AS is_new
           )
           SELECT * FROM ins
           UNION ALL
           SELECT {0}, FALSE AS is_new
           FROM orders
           WHERE customer_id = $2
             AND idempotency_key = $6
             AND NOT EXISTS (SELECT 1 FROM ins)
           LIMIT 1"#,
        ORDER_COLS
    )
});

static STMT_INSERT_ORDER_ITEMS: &str = r#"
    INSERT INTO order_items
    (id, order_id, ticket_variant_id, quantity, unit_price, subtotal)
    SELECT t.id, $1, t.var_id, t.qty, t.price, t.subtotal
    FROM UNNEST($2::bytea[], $3::bytea[], $4::int4[], $5::numeric[], $6::numeric[])
        AS t(id, var_id, qty, price, subtotal)
"#;

static STMT_MINT_TICKETS: &str = r#"
    INSERT INTO tickets (id, order_item_id, order_id, ticket_code, status)
    SELECT id, item_id, order_id, code, 'active'
    FROM UNNEST($1::bytea[], $2::bytea[], $3::bytea[], $4::text[]) AS t(id, item_id, order_id, code)
"#;

static STMT_BUMP_SOLD: &str = r#"
    WITH agg AS (
        SELECT id, SUM(qty) AS total_qty
        FROM UNNEST($1::bytea[], $2::int4[]) AS t(id, qty)
        GROUP BY id
    )
    UPDATE event_variants ev
       SET sold = ev.sold + agg.total_qty
      FROM agg
     WHERE ev.id = agg.id
       AND (ev.quota - ev.sold) >= agg.total_qty
"#;

static STMT_REFUND_SOLD: &str = r#"
    UPDATE event_variants
       SET sold = GREATEST(0, sold - bump.qty)
      FROM UNNEST($1::bytea[], $2::int4[]) AS bump(id, qty)
     WHERE event_variants.id = bump.id
"#;

static STMT_MARK_PAID: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"UPDATE orders
           SET status = 'paid', paid_at = NOW(), payment_method = $2
         WHERE id = $1
           AND status = 'pending'
           AND expired_at > NOW()
         RETURNING {}"#,
        ORDER_COLS
    )
});

static STMT_CANCEL_ORDER: &str =
    "UPDATE orders SET status = 'cancelled' WHERE id = $1 AND status = 'pending'";

static STMT_FETCH_ITEMS_FOR_MINT: &str = "SELECT id, quantity FROM order_items WHERE order_id = $1";

static STMT_FETCH_ITEMS_FOR_REFUND: &str =
    "SELECT ticket_variant_id, quantity FROM order_items WHERE order_id = $1";

// ── Lua scripts ───────────────────────────────────────────────────────────────

pub(crate) const LUA_RELEASE: &str = r#"
if redis.call("get", KEYS[1]) == ARGV[1] then
    return redis.call("del", KEYS[1])
else
    return 0
end
"#;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_ticket_code(ticket_id: &str) -> String {
    format!("TK{}", &ticket_id[..ticket_id.len().min(12)])
}

fn map_item_row(row: &Row) -> Result<OrderItemResponse> {
    let item_bytes: Vec<u8> = row.try_get("item_id")?;
    let var_bytes: Vec<u8> = row.try_get("ticket_variant_id")?;
    let ev_bytes: Vec<u8> = row.try_get("event_id")?;
    Ok(OrderItemResponse {
        id: bin_to_ulid(item_bytes)?,
        ticket_variant_id: bin_to_ulid(var_bytes)?,
        variant_name: row.try_get("variant_name")?,
        event_id: bin_to_ulid(ev_bytes)?,
        event_name: row.try_get("event_name")?,
        quantity: row.try_get("quantity")?,
        unit_price: row.try_get("unit_price")?,
        subtotal: row.try_get("subtotal")?,
    })
}

/// Map a row from LIST_ORDERS_WITH_EVENT into OrderListItem.
fn map_order_list_item(row: &Row) -> Result<OrderListItem> {
    let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
    let cust_bytes: Vec<u8> = row.try_get("customer_id").context("customer_id")?;
    Ok(OrderListItem {
        id: bin_to_ulid(id_bytes)?,
        customer_id: bin_to_ulid(cust_bytes)?,
        order_code: row.try_get("order_code").context("order_code")?,
        status: row.try_get("status").context("status")?,
        total_amount: row.try_get("total_amount").context("total_amount")?,
        payment_method: row.try_get("payment_method")?,
        paid_at: row.try_get("paid_at")?,
        expired_at: row.try_get("expired_at")?,
        created_at: row.try_get("created_at").context("created_at")?,
        event_name: row.try_get("event_name")?,
        event_date: row.try_get("event_date")?,
        venue: row.try_get("venue")?,
        cover_url: row.try_get("cover_url")?,
    })
}

// ── Structs ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct LockedVariant {
    pub id_bytes: Vec<u8>,
    pub ulid: String,
    pub price: Decimal,
    pub effective_price: Decimal,
    pub quota: i32,
    pub sold: i32,
    pub max_per_order: Option<i32>,
    pub is_active: bool,
    pub variant_name: String,
    pub event_id: String,
    pub event_name: String,
}

pub struct ItemRow {
    pub oi_id: String,
    pub oi_bytes: Vec<u8>,
    pub var_bytes: Vec<u8>,
    pub qty: i32,
    pub unit_price: Decimal,
    pub subtotal: Decimal,
}

#[derive(Debug)]
pub struct OversellError {
    pub updated: u64,
    pub expected: usize,
    pub variant_ids: Vec<Vec<u8>>,
}

impl std::fmt::Display for OversellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "oversell guard: updated {} of {} variants",
            self.updated, self.expected
        )
    }
}

impl std::error::Error for OversellError {}

// ── OrderTx ───────────────────────────────────────────────────────────────────

pub struct OrderTx;

impl OrderTx {
    pub async fn lock_variants(
        tx: &tokio_postgres::Transaction<'_>,
        id_bytes_list: &[Vec<u8>],
    ) -> Result<Vec<LockedVariant>> {
        let stmt = tx
            .prepare(STMT_LOCK_VARIANTS)
            .await
            .context("lock_variants prepare")?;

        let rows = tx
            .query(&stmt, &[&id_bytes_list])
            .await
            .context("lock_variants execute")?;

        rows.iter()
            .map(|row| {
                let id_b: Vec<u8> = row.try_get("id")?;
                let ev_b: Vec<u8> = row.try_get("event_id_bytes")?;
                Ok(LockedVariant {
                    ulid: bin_to_ulid(id_b.clone())?,
                    id_bytes: id_b,
                    price: row.try_get("price")?,
                    effective_price: row.try_get("effective_price")?,
                    quota: row.try_get("quota")?,
                    sold: row.try_get("sold")?,
                    max_per_order: row.try_get("max_per_order")?,
                    is_active: row.try_get("is_active")?,
                    variant_name: row.try_get("variant_name")?,
                    event_id: bin_to_ulid(ev_b)?,
                    event_name: row.try_get("event_name")?,
                })
            })
            .collect()
    }

    pub async fn insert_order(
        tx: &tokio_postgres::Transaction<'_>,
        id_bytes: &[u8],
        customer_bytes: &[u8],
        order_code: &str,
        total: Decimal,
        expired_at: chrono::DateTime<chrono::Utc>,
        idempotency_key: Option<&str>,
    ) -> Result<(Order, bool)> {
        if idempotency_key.is_none() {
            let stmt = tx
                .prepare(&STMT_INSERT_ORDER_SIMPLE)
                .await
                .context("insert_order prepare")?;

            let params: &[&(dyn ToSql + Sync)] =
                &[&id_bytes, &customer_bytes, &order_code, &total, &expired_at];
            let row = tx
                .query_one(&stmt, params)
                .await
                .context("insert_order execute")?;
            return Ok((row_to_order(&row)?, true));
        }

        let key = idempotency_key.unwrap();
        let stmt = tx
            .prepare(&STMT_INSERT_ORDER_IDEMPOTENCY)
            .await
            .context("insert_order_idempotency prepare")?;

        let params: &[&(dyn ToSql + Sync)] = &[
            &id_bytes,
            &customer_bytes,
            &order_code,
            &total,
            &expired_at,
            &key,
        ];
        let row = tx
            .query_one(&stmt, params)
            .await
            .context("insert_order_idempotency execute")?;

        let is_new: bool = row.try_get("is_new").context("is_new")?;
        Ok((row_to_order(&row)?, is_new))
    }

    pub async fn insert_order_items_batch(
        tx: &tokio_postgres::Transaction<'_>,
        order_id_bytes: &[u8],
        items: &[ItemRow],
    ) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        let ids: Vec<&[u8]> = items.iter().map(|r| r.oi_bytes.as_slice()).collect();
        let var_ids: Vec<&[u8]> = items.iter().map(|r| r.var_bytes.as_slice()).collect();
        let qtys: Vec<i32> = items.iter().map(|r| r.qty).collect();
        let prices: Vec<Decimal> = items.iter().map(|r| r.unit_price).collect();
        let subtotals: Vec<Decimal> = items.iter().map(|r| r.subtotal).collect();

        let stmt = tx
            .prepare(STMT_INSERT_ORDER_ITEMS)
            .await
            .context("insert_order_items_batch prepare")?;

        let params: &[&(dyn ToSql + Sync)] =
            &[&order_id_bytes, &ids, &var_ids, &qtys, &prices, &subtotals];

        tx.execute(&stmt, params)
            .await
            .context("insert_order_items_batch execute")?;

        Ok(())
    }

    pub async fn bump_sold_batch(
        tx: &tokio_postgres::Transaction<'_>,
        updates: &[(&[u8], i32)],
    ) -> Result<(), anyhow::Error> {
        if updates.is_empty() {
            return Ok(());
        }

        let mut agg: std::collections::HashMap<&[u8], i32> =
            std::collections::HashMap::with_capacity(updates.len());
        for &(id, qty) in updates {
            *agg.entry(id).or_insert(0) += qty;
        }

        let ids: Vec<&[u8]> = agg.keys().copied().collect();
        let qtys: Vec<i32> = agg.values().copied().collect();
        let expected = ids.len();

        let stmt = tx
            .prepare(STMT_BUMP_SOLD)
            .await
            .context("bump_sold_batch prepare")?;

        let params: &[&(dyn ToSql + Sync)] = &[&ids, &qtys];
        let updated = tx
            .execute(&stmt, params)
            .await
            .context("bump_sold_batch execute")?;

        if updated as usize != expected {
            let variant_ids: Vec<Vec<u8>> = ids.iter().map(|b| b.to_vec()).collect();
            return Err(anyhow::anyhow!(OversellError {
                updated,
                expected,
                variant_ids,
            }));
        }
        Ok(())
    }

    pub async fn mark_paid(
        tx: &tokio_postgres::Transaction<'_>,
        order_bytes: &[u8],
        payment_method: &str,
    ) -> Result<Option<Order>> {
        let stmt = tx
            .prepare(&STMT_MARK_PAID)
            .await
            .context("mark_paid prepare")?;

        let params: &[&(dyn ToSql + Sync)] = &[&order_bytes, &payment_method];
        let row = tx
            .query_opt(&stmt, params)
            .await
            .context("mark_paid execute")?;

        row.as_ref().map(row_to_order).transpose()
    }

    pub async fn fetch_items_for_mint(
        tx: &tokio_postgres::Transaction<'_>,
        order_bytes: &[u8],
    ) -> Result<Vec<(Vec<u8>, i32)>> {
        let stmt = tx
            .prepare(STMT_FETCH_ITEMS_FOR_MINT)
            .await
            .context("fetch_items_for_mint prepare")?;

        tx.query(&stmt, &[&order_bytes])
            .await
            .context("fetch_items_for_mint execute")?
            .iter()
            .map(|r| Ok((r.try_get::<_, Vec<u8>>("id")?, r.try_get("quantity")?)))
            .collect()
    }

    pub async fn fetch_items_detail(
        tx: &tokio_postgres::Transaction<'_>,
        order_bytes: &[u8],
    ) -> Result<Vec<OrderItemResponse>> {
        let stmt = tx
            .prepare(QUERY_ITEMS_DETAIL)
            .await
            .context("fetch_items_detail prepare")?;

        tx.query(&stmt, &[&order_bytes])
            .await
            .context("fetch_items_detail execute")?
            .iter()
            .map(map_item_row)
            .collect()
    }

    pub async fn mint_tickets_batch(
        tx: &tokio_postgres::Transaction<'_>,
        items: &[(Vec<u8>, i32)],
        order_id_bytes: &[u8], // ← NEW: stored in every ticket row
    ) -> Result<u64> {
        let total: i32 = items.iter().map(|(_, q)| q).sum();
        if total == 0 {
            return Ok(0);
        }

        let mut ids: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
        let mut item_ids: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
        let mut ord_ids: Vec<Vec<u8>> = Vec::with_capacity(total as usize); // NEW
        let mut codes: Vec<String> = Vec::with_capacity(total as usize);

        for (item_bytes, qty) in items {
            for _ in 0..*qty {
                let id = new_ulid();
                let id_bytes = ulid_to_vec(&id)?;
                let code = make_ticket_code(&id);
                ids.push(id_bytes);
                item_ids.push(item_bytes.clone());
                ord_ids.push(order_id_bytes.to_vec()); // same order for all tickets
                codes.push(code);
            }
        }

        let count = ids.len() as u64;

        let stmt = tx
            .prepare(STMT_MINT_TICKETS)
            .await
            .context("mint_tickets_batch prepare")?;

        let params: &[&(dyn ToSql + Sync)] = &[&ids, &item_ids, &ord_ids, &codes]; // ← +ord_ids
        tx.execute(&stmt, params)
            .await
            .context("mint_tickets_batch execute")?;

        Ok(count)
    }

    pub async fn cancel_order(
        tx: &tokio_postgres::Transaction<'_>,
        order_bytes: &[u8],
    ) -> Result<u64> {
        let stmt = tx
            .prepare(STMT_CANCEL_ORDER)
            .await
            .context("cancel_order prepare")?;

        tx.execute(&stmt, &[&order_bytes])
            .await
            .context("cancel_order execute")
    }

    pub async fn fetch_items_for_refund(
        tx: &tokio_postgres::Transaction<'_>,
        order_bytes: &[u8],
    ) -> Result<Vec<(Vec<u8>, i32)>> {
        let stmt = tx
            .prepare(STMT_FETCH_ITEMS_FOR_REFUND)
            .await
            .context("fetch_items_for_refund prepare")?;

        tx.query(&stmt, &[&order_bytes])
            .await
            .context("fetch_items_for_refund execute")?
            .iter()
            .map(|r| {
                Ok((
                    r.try_get::<_, Vec<u8>>("ticket_variant_id")?,
                    r.try_get("quantity")?,
                ))
            })
            .collect()
    }

    pub async fn refund_sold_batch(
        tx: &tokio_postgres::Transaction<'_>,
        updates: &[(Vec<u8>, i32)],
    ) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let ids: Vec<Vec<u8>> = updates.iter().map(|(id, _)| id.clone()).collect();
        let qtys: Vec<i32> = updates.iter().map(|(_, q)| *q).collect();

        let stmt = tx
            .prepare(STMT_REFUND_SOLD)
            .await
            .context("refund_sold_batch prepare")?;

        let params: &[&(dyn ToSql + Sync)] = &[&ids, &qtys];
        tx.execute(&stmt, params)
            .await
            .context("refund_sold_batch execute")?;
        Ok(())
    }
}

// ── Trait + PgOrderRepository ─────────────────────────────────────────────────

pub struct CreateOrderItemSpec<'a> {
    pub variant_id: &'a str,
    pub quantity: i32,
}

#[async_trait]
pub trait OrderRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<Order>>;
    async fn list_for_customer(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Order>>;
    /// Enriched list — includes first event's name, date, venue, cover_url.
    async fn list_for_customer_enriched(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<OrderListItem>>;
    async fn list_items(&self, order_id: &str) -> Result<Vec<OrderItemResponse>>;
}

#[derive(Clone)]
pub struct PgOrderRepository {
    pool: Pool,
}

impl PgOrderRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OrderRepository for PgOrderRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Order>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_ORDER_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(row_to_order).transpose()
    }

    async fn list_for_customer(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Order>> {
        let id_vec = id_to_vec(customer_id)?;
        let params: &[&(dyn ToSql + Sync)] = &[&id_vec, &limit, &offset];
        let rows = exec_rows(&self.pool, &LIST_ORDERS_BY_CUSTOMER, params).await?;
        rows.iter().map(row_to_order).collect()
    }

    async fn list_for_customer_enriched(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<OrderListItem>> {
        let id_vec = id_to_vec(customer_id)?;
        let params: &[&(dyn ToSql + Sync)] = &[&id_vec, &limit, &offset];
        let rows = exec_rows(&self.pool, LIST_ORDERS_WITH_EVENT, params).await?;
        rows.iter().map(map_order_list_item).collect()
    }

    async fn list_items(&self, order_id: &str) -> Result<Vec<OrderItemResponse>> {
        let id_vec = id_to_vec(order_id)?;
        let rows = exec_rows(&self.pool, QUERY_ITEMS_DETAIL, &[&id_vec]).await?;
        rows.iter().map(map_item_row).collect()
    }
}

// ── Row helpers ───────────────────────────────────────────────────────────────

pub(crate) fn row_to_order(row: &Row) -> Result<Order> {
    let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
    let cust_bytes: Vec<u8> = row.try_get("customer_id").context("customer_id")?;
    Ok(Order {
        id: bin_to_ulid(id_bytes)?,
        customer_id: bin_to_ulid(cust_bytes)?,
        order_code: row.try_get("order_code").context("order_code")?,
        status: row.try_get("status").context("status")?,
        total_amount: row.try_get("total_amount").context("total_amount")?,
        payment_method: row.try_get("payment_method").context("payment_method")?,
        paid_at: row.try_get("paid_at")?,
        expired_at: row.try_get("expired_at")?,
        created_at: row.try_get("created_at").context("created_at")?,
        updated_at: row.try_get("updated_at").context("updated_at")?,
    })
}
