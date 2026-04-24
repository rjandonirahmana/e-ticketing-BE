use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use chrono::{Duration, Utc};
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_first, exec_rows, get_conn};
use crate::models::orders::{Order, OrderItemResponse};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

// ── Static query strings ──────────────────────────────────────────────────────

static ORDER_COLS: &str = r#"
    id,
    customer_id,
    order_code,
    status,
    total_amount::FLOAT8 AS total_amount,
    payment_method,
    paid_at,
    expired_at,
    created_at,
    updated_at
"#;

static FIND_ORDER_BY_ID: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM orders WHERE id = $1", ORDER_COLS));

static LIST_ORDERS_BY_CUSTOMER: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM orders WHERE customer_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        ORDER_COLS
    )
});

static MARK_PAID: &str = r#"
    UPDATE orders
       SET status = 'paid',
           paid_at = NOW(),
           payment_method = $2
     WHERE id = $1
       AND status = 'pending'
"#;

static FIND_ITEMS_FOR_ORDER: &str = r#"
    SELECT
        oi.id          AS item_id,
        oi.ticket_variant_id,
        oi.quantity,
        oi.unit_price::FLOAT8 AS unit_price,
        oi.subtotal::FLOAT8   AS subtotal,
        tv.name        AS variant_name,
        tv.event_id,
        e.name         AS event_name
      FROM order_items oi
      JOIN ticket_variants tv ON oi.ticket_variant_id = tv.id
      JOIN events e           ON tv.event_id = e.id
     WHERE oi.order_id = $1
     ORDER BY oi.created_at
"#;

// Build a short, human-friendly order code: "KN" + 10 hex chars from ULID.
fn make_order_code(order_id: &str) -> String {
    let suffix: String = order_id.chars().take(10).collect();
    format!("KN{}", suffix)
}

// Build ticket code: "TK" + 12 hex from ULID.
fn make_ticket_code(ticket_id: &str) -> String {
    let suffix: String = ticket_id.chars().take(12).collect();
    format!("TK{}", suffix)
}

// ── Trait ─────────────────────────────────────────────────────────────────────

pub struct CreateOrderItemSpec<'a> {
    pub variant_id: &'a str,
    pub quantity: i32,
}

#[async_trait]
pub trait OrderRepository: Send + Sync {
    /// Atomic: lock variants, check stock, insert order + items, increment `sold`.
    /// Tickets are generated lazily on payment in `mark_paid_and_issue_tickets`.
    async fn create_order(
        &self,
        customer_id: &str,
        items: &[CreateOrderItemSpec<'_>],
    ) -> Result<Order>;

    async fn find_by_id(&self, id: &str) -> Result<Option<Order>>;

    async fn list_for_customer(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Order>>;

    async fn list_items(&self, order_id: &str) -> Result<Vec<OrderItemResponse>>;

    /// Atomic: mark order paid + generate one ticket per quantity for each item.
    /// Returns the number of tickets created.
    async fn mark_paid_and_issue_tickets(
        &self,
        order_id: &str,
        payment_method: &str,
    ) -> Result<u64>;

    async fn cancel(&self, order_id: &str) -> Result<()>;
}

// ── Postgres impl ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PgOrderRepository {
    pool: Pool,
}

impl PgOrderRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn row_to_order(row: &Row) -> Result<Order> {
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
}

#[async_trait]
impl OrderRepository for PgOrderRepository {
    async fn create_order(
        &self,
        customer_id: &str,
        items: &[CreateOrderItemSpec<'_>],
    ) -> Result<Order> {
        if items.is_empty() {
            bail!("Order must have at least one item");
        }

        let mut conn = get_conn(&self.pool).await?;
        let tx = conn.transaction().await?;

        // 1. Lock & validate every requested variant in one query.
        //    We FOR UPDATE to serialise concurrent purchases of the same variant.
        let variant_id_bytes: Vec<Vec<u8>> = items
            .iter()
            .map(|i| id_to_vec(i.variant_id))
            .collect::<Result<_>>()?;

        let rows = tx
            .query(
                "SELECT id, price::FLOAT8 AS price, quota, sold, max_per_order, is_active \
                 FROM ticket_variants WHERE id = ANY($1) FOR UPDATE",
                &[&variant_id_bytes],
            )
            .await?;

        // Map by ULID hex for quick lookup.
        use std::collections::HashMap;
        let mut variant_map: HashMap<String, (f64, i32, i32, Option<i32>, bool)> = HashMap::new();
        for row in &rows {
            let id_bytes: Vec<u8> = row.try_get("id")?;
            let id = bin_to_ulid(id_bytes)?;
            variant_map.insert(
                id,
                (
                    row.try_get("price")?,
                    row.try_get("quota")?,
                    row.try_get("sold")?,
                    row.try_get("max_per_order")?,
                    row.try_get("is_active")?,
                ),
            );
        }

        let order_id = new_ulid();
        let order_id_vec = ulid_to_vec(&order_id)?;
        let order_code = make_order_code(&order_id);

        let mut total: f64 = 0.0;

        // 2. Validate stock + collect prices
        let mut item_inserts: Vec<(Vec<u8>, Vec<u8>, i32, f64, f64)> =
            Vec::with_capacity(items.len());
        for item in items {
            let v = variant_map
                .get(item.variant_id)
                .ok_or_else(|| anyhow!("Variant '{}' not found", item.variant_id))?;
            let (price, quota, sold, max_per_order, is_active) = *v;

            if !is_active {
                bail!("Variant '{}' is not active", item.variant_id);
            }
            if let Some(max) = max_per_order {
                if item.quantity > max {
                    bail!(
                        "Variant '{}' allows at most {} per order",
                        item.variant_id,
                        max
                    );
                }
            }
            if sold + item.quantity > quota {
                bail!(
                    "Insufficient stock for variant '{}': {} requested, {} left",
                    item.variant_id,
                    item.quantity,
                    quota - sold
                );
            }

            let subtotal = price * item.quantity as f64;
            total += subtotal;

            let oi_id = new_ulid();
            let oi_vec = ulid_to_vec(&oi_id)?;
            let var_vec = id_to_vec(item.variant_id)?;
            item_inserts.push((oi_vec, var_vec, item.quantity, price, subtotal));
        }

        // 3. Insert the order
        let cust_vec = id_to_vec(customer_id)?;
        let expired_at = Utc::now() + Duration::hours(2);
        let order_row = tx
            .query_one(
                &format!(
                    "INSERT INTO orders (id, customer_id, order_code, status, total_amount, expired_at) \
                     VALUES ($1, $2, $3, 'pending', $4, $5) RETURNING {}",
                    ORDER_COLS
                ),
                &[
                    &order_id_vec,
                    &cust_vec,
                    &order_code,
                    &total,
                    &expired_at,
                ],
            )
            .await?;

        // 4. Insert items + bump sold counter
        for (oi_vec, var_vec, qty, unit_price, subtotal) in &item_inserts {
            tx.execute(
                "INSERT INTO order_items (id, order_id, ticket_variant_id, quantity, unit_price, subtotal) \
                 VALUES ($1, $2, $3, $4, $5, $6)",
                &[oi_vec, &order_id_vec, var_vec, qty, unit_price, subtotal],
            )
            .await?;

            tx.execute(
                "UPDATE ticket_variants SET sold = sold + $2 WHERE id = $1",
                &[var_vec, qty],
            )
            .await?;
        }

        tx.commit().await?;

        Self::row_to_order(&order_row)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Order>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_ORDER_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_order).transpose()
    }

    async fn list_for_customer(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Order>> {
        let id_vec = id_to_vec(customer_id)?;
        let rows = exec_rows(
            &self.pool,
            &LIST_ORDERS_BY_CUSTOMER,
            &[&id_vec, &limit, &offset],
        )
        .await?;
        rows.iter().map(Self::row_to_order).collect()
    }

    async fn list_items(&self, order_id: &str) -> Result<Vec<OrderItemResponse>> {
        let id_vec = id_to_vec(order_id)?;
        let rows = exec_rows(&self.pool, FIND_ITEMS_FOR_ORDER, &[&id_vec]).await?;
        rows.iter()
            .map(|row| {
                let item_bytes: Vec<u8> = row.try_get("item_id")?;
                let var_bytes: Vec<u8> = row.try_get("ticket_variant_id")?;
                let event_bytes: Vec<u8> = row.try_get("event_id")?;
                Ok(OrderItemResponse {
                    id: bin_to_ulid(item_bytes)?,
                    ticket_variant_id: bin_to_ulid(var_bytes)?,
                    variant_name: row.try_get("variant_name")?,
                    event_id: bin_to_ulid(event_bytes)?,
                    event_name: row.try_get("event_name")?,
                    quantity: row.try_get("quantity")?,
                    unit_price: row.try_get("unit_price")?,
                    subtotal: row.try_get("subtotal")?,
                })
            })
            .collect()
    }

    async fn mark_paid_and_issue_tickets(
        &self,
        order_id: &str,
        payment_method: &str,
    ) -> Result<u64> {
        let mut conn = get_conn(&self.pool).await?;
        let tx = conn.transaction().await?;

        let order_vec = id_to_vec(order_id)?;

        // 1. Mark paid (only if currently pending) — guards double payment.
        let updated = tx
            .execute(MARK_PAID, &[&order_vec, &payment_method])
            .await?;
        if updated == 0 {
            bail!("Order is not in pending state or does not exist");
        }

        // 2. Fetch items so we know how many tickets to mint per item.
        let items = tx
            .query(
                "SELECT id, quantity FROM order_items WHERE order_id = $1",
                &[&order_vec],
            )
            .await?;

        let mut total_minted: u64 = 0;
        for item in &items {
            let item_id_bytes: Vec<u8> = item.try_get("id")?;
            let qty: i32 = item.try_get("quantity")?;

            for _ in 0..qty {
                let ticket_id = new_ulid();
                let ticket_vec = ulid_to_vec(&ticket_id)?;
                let ticket_code = make_ticket_code(&ticket_id);
                tx.execute(
                    "INSERT INTO tickets (id, order_item_id, ticket_code, status) \
                     VALUES ($1, $2, $3, 'active')",
                    &[&ticket_vec, &item_id_bytes, &ticket_code],
                )
                .await?;
                total_minted += 1;
            }
        }

        tx.commit().await?;
        Ok(total_minted)
    }

    async fn cancel(&self, order_id: &str) -> Result<()> {
        let mut conn = get_conn(&self.pool).await?;
        let tx = conn.transaction().await?;

        let order_vec = id_to_vec(order_id)?;

        // Refund stock for any pending order being cancelled.
        let items = tx
            .query(
                "SELECT ticket_variant_id, quantity FROM order_items WHERE order_id = $1",
                &[&order_vec],
            )
            .await?;

        let n = tx
            .execute(
                "UPDATE orders SET status = 'cancelled' WHERE id = $1 AND status = 'pending'",
                &[&order_vec],
            )
            .await?;
        if n == 0 {
            bail!("Order is not pending or does not exist");
        }

        for item in &items {
            let var_bytes: Vec<u8> = item.try_get("ticket_variant_id")?;
            let qty: i32 = item.try_get("quantity")?;
            tx.execute(
                "UPDATE ticket_variants SET sold = GREATEST(0, sold - $2) WHERE id = $1",
                &[&var_bytes, &qty],
            )
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
