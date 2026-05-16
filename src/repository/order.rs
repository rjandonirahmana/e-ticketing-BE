use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use rust_decimal::Decimal;
use std::sync::LazyLock;
use tokio_postgres::types::ToSql;
use tokio_postgres::Row;

use super::db::{exec_first, exec_rows};
use crate::models::orders::{Order, OrderItemResponse};
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
//
// Semua SQL ditulis sebagai static agar teks selalu identik setiap call.
// tokio-postgres meng-cache prepared statement per-connection keyed by SQL text;
// jika teks berubah (misal string baru tiap call), cache miss terjadi dan
// Parse + Describe round-trip ke PostgreSQL dikirim ulang.
//
// BUG FIX: Semua string yang sebelumnya menggunakan `\\` (backslash literal
// dalam string) sudah diganti ke raw string r#"..."# atau format! dengan raw string.
// Backslash literal dalam SQL tidak valid di PostgreSQL mode standard_conforming_strings
// (default modern PostgreSQL) dan akan menyebabkan syntax error.

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
    // {0} = ORDER_COLS, direferensikan dua kali dalam satu format! call.
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

/// BUG FIX: order_id adalah scalar $1 (bukan array).
/// Sebelumnya order_id masuk UNNEST array → N clone Vec<u8> per call.
/// Sekarang: 0 clone, 1 reference ke slice.
static STMT_INSERT_ORDER_ITEMS: &str = r#"
    INSERT INTO order_items
    (id, order_id, ticket_variant_id, quantity, unit_price, subtotal)
    SELECT t.id, $1, t.var_id, t.qty, t.price, t.subtotal
    FROM UNNEST($2::bytea[], $3::bytea[], $4::int4[], $5::numeric[], $6::numeric[])
        AS t(id, var_id, qty, price, subtotal)
"#;

static STMT_MINT_TICKETS: &str = r#"
    INSERT INTO tickets (id, order_item_id, ticket_code, status)
    SELECT id, item_id, code, 'active'
    FROM UNNEST($1::bytea[], $2::bytea[], $3::text[]) AS t(id, item_id, code)
"#;

/// Guard atomik oversell di DB.
/// SUM(qty) GROUP BY id di dalam CTE menangani duplikasi variant dalam batch.
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

/// FIX [P2-7]: RETURNING full row → mark_paid mengembalikan Order,
/// mengeliminasi find_by_id post-commit di pay().
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

/// Untuk fetch_items_for_mint: hanya butuh (id, quantity) untuk mint_tickets_batch.
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

/// Error saat bump_sold_batch mendeteksi oversell.
/// variant_ids membawa binary IDs semua variant dalam batch — service
/// men-decode ke ULID untuk log tanpa clone tambahan di service layer.
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

        let stmt = tx
            .prepare(&STMT_INSERT_ORDER_IDEMPOTENCY)
            .await
            .context("insert_order (idempotency) prepare")?;

        let params: &[&(dyn ToSql + Sync)] = &[
            &id_bytes,
            &customer_bytes,
            &order_code,
            &total,
            &expired_at,
            &idempotency_key,
        ];
        let row = tx
            .query_one(&stmt, params)
            .await
            .context("insert_order (idempotency) execute")?;
        let is_new: bool = row.try_get("is_new").context("is_new")?;
        Ok((row_to_order(&row)?, is_new))
    }

    /// order_id adalah scalar $1 — tidak ada duplikasi N kali dalam array.
    pub async fn insert_order_items_batch(
        tx: &tokio_postgres::Transaction<'_>,
        order_id_bytes: &[u8],
        items: &[ItemRow],
    ) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        let mut ids: Vec<Vec<u8>> = Vec::with_capacity(items.len());
        let mut var_ids: Vec<Vec<u8>> = Vec::with_capacity(items.len());
        let mut qtys: Vec<i32> = Vec::with_capacity(items.len());
        let mut prices: Vec<Decimal> = Vec::with_capacity(items.len());
        let mut subtotals: Vec<Decimal> = Vec::with_capacity(items.len());

        for item in items {
            ids.push(item.oi_bytes.clone());
            var_ids.push(item.var_bytes.clone());
            qtys.push(item.qty);
            prices.push(item.unit_price);
            subtotals.push(item.subtotal);
        }

        let stmt = tx
            .prepare(STMT_INSERT_ORDER_ITEMS)
            .await
            .context("insert_order_items_batch prepare")?;

        // $1 = order_id scalar, $2-$6 = array per-item
        let params: &[&(dyn ToSql + Sync)] =
            &[&order_id_bytes, &ids, &var_ids, &qtys, &prices, &subtotals];

        tx.execute(&stmt, params)
            .await
            .context("insert_order_items_batch execute")?;

        Ok(())
    }

    /// FIX [P1-5]: Signature berubah dari &[(Vec<u8>, i32)] ke &[(&[u8], i32)].
    ///
    /// Sebelumnya: service harus clone var_bytes N kali untuk membuat bump Vec.
    /// Sekarang: service cukup as_slice() → borrow, zero allocation.
    ///
    /// Aggregasi menggunakan &[u8] sebagai HashMap key (&[u8] implements Eq + Hash).
    /// Clone ke Vec<u8> hanya terjadi di error path (OversellError::variant_ids),
    /// bukan di hot path.
    pub async fn bump_sold_batch(
        tx: &tokio_postgres::Transaction<'_>,
        updates: &[(&[u8], i32)],
    ) -> Result<(), anyhow::Error> {
        if updates.is_empty() {
            return Ok(());
        }

        // Aggregate menggunakan &[u8] sebagai key — no clone needed.
        let mut agg: std::collections::HashMap<&[u8], i32> =
            std::collections::HashMap::with_capacity(updates.len());
        for &(id, qty) in updates {
            *agg.entry(id).or_insert(0) += qty;
        }

        // &[u8] implements ToSql (sebagai BYTEA) → Vec<&[u8]>: ToSql (sebagai BYTEA[]).
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
            // Clone hanya di error path (rare) — bukan hot path.
            let variant_ids: Vec<Vec<u8>> = ids.iter().map(|b| b.to_vec()).collect();
            return Err(anyhow::anyhow!(OversellError {
                updated,
                expected,
                variant_ids,
            }));
        }
        Ok(())
    }

    /// FIX [P2-7]: Menggunakan RETURNING → mengembalikan Order yang sudah di-UPDATE.
    /// pay() tidak perlu find_by_id post-commit lagi.
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

        // None → UPDATE 0 rows (order sudah paid/cancelled/expired)
        row.as_ref().map(row_to_order).transpose()
    }

    /// Items minimal (id, quantity) untuk mint_tickets_batch.
    /// Terpisah dari fetch_items_detail agar tidak JOIN unnecesarily saat minting.
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

    /// Full OrderItemResponse untuk response pay() — query dalam TX.
    /// FIX [P2-7]: Mengeliminasi list_items post-commit di pay().
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
    ) -> Result<u64> {
        let total: i32 = items.iter().map(|(_, q)| q).sum();
        if total == 0 {
            return Ok(0);
        }

        let mut ids: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
        let mut item_ids: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
        let mut codes: Vec<String> = Vec::with_capacity(total as usize);

        for (item_bytes, qty) in items {
            for _ in 0..*qty {
                let id = new_ulid();
                let id_bytes = ulid_to_vec(&id)?;
                let code = make_ticket_code(&id);
                ids.push(id_bytes);
                item_ids.push(item_bytes.clone());
                codes.push(code);
            }
        }

        let count = ids.len() as u64;

        let stmt = tx
            .prepare(STMT_MINT_TICKETS)
            .await
            .context("mint_tickets_batch prepare")?;

        let params: &[&(dyn ToSql + Sync)] = &[&ids, &item_ids, &codes];
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
