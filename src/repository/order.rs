use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_first, exec_rows};
use crate::models::orders::{Order, OrderItemResponse};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

// ── Static queries ────────────────────────────────────────────────────────────

static ORDER_COLS: &str = r#"
    id, customer_id, order_code, status,
    total_amount::FLOAT8 AS total_amount,
    payment_method, paid_at, expired_at, created_at, updated_at
"#;

static FIND_ORDER_BY_ID: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM orders WHERE id = $1", ORDER_COLS));

static LIST_ORDERS_BY_CUSTOMER: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM orders WHERE customer_id = $1 \
         ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        ORDER_COLS
    )
});

static FIND_ITEMS_FOR_ORDER: &str = r#"
    SELECT
        oi.id                 AS item_id,
        oi.ticket_variant_id,
        oi.quantity,
        oi.unit_price::FLOAT8 AS unit_price,
        oi.subtotal::FLOAT8   AS subtotal,
        tv.name               AS variant_name,
        tv.event_id,
        e.name                AS event_name
    FROM order_items oi
    JOIN event_variants tv ON oi.ticket_variant_id = tv.id
    JOIN events e           ON tv.event_id = e.id
    WHERE oi.order_id = $1
    ORDER BY oi.created_at
"#;

// ── Lua scripts ───────────────────────────────────────────────────────────────

/// Atomic release: hanya DEL jika value masih milik kita.
pub(crate) const LUA_RELEASE: &str = r#"
if redis.call("get", KEYS[1]) == ARGV[1] then
    return redis.call("del", KEYS[1])
else
    return 0
end
"#;

/// Atomic heartbeat: hanya PEXPIRE jika value masih milik kita.
/// Mencegah extend lock milik request lain setelah lock kita expired.
const LUA_EXTEND: &str = r#"
if redis.call("get", KEYS[1]) == ARGV[1] then
    return redis.call("pexpire", KEYS[1], ARGV[2])
else
    return 0
end
"#;

// ── Code builders ─────────────────────────────────────────────────────────────

fn make_ticket_code(ticket_id: &str) -> String {
    format!("TK{}", &ticket_id[..ticket_id.len().min(12)])
}

// ── LockedVariant ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct LockedVariant {
    pub id_bytes: Vec<u8>,
    pub ulid: String,
    pub price: f64,
    /// Harga efektif: sale_price jika sale aktif, else price.
    pub effective_price: f64,
    pub quota: i32,
    pub sold: i32,
    pub max_per_order: Option<i32>,
    pub is_active: bool,
    pub variant_name: String,
    pub event_id: String,
    pub event_name: String,
}

// ── ItemRow ───────────────────────────────────────────────────────────────────

pub struct ItemRow {
    pub oi_id: String,
    pub oi_bytes: Vec<u8>,
    pub var_bytes: Vec<u8>,
    pub qty: i32,
    pub unit_price: f64,
    pub subtotal: f64,
}

// ── OversellError ─────────────────────────────────────────────────────────────

/// Dikembalikan oleh bump_sold_batch kalau guard DB mendeteksi oversell.
#[derive(Debug)]
pub struct OversellError {
    /// Jumlah variant yang berhasil di-update (bisa 0).
    pub updated: u64,
    /// Jumlah variant yang diharapkan ter-update.
    pub expected: usize,
}

impl std::fmt::Display for OversellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "oversell guard triggered: updated {} of {} variants",
            self.updated, self.expected
        )
    }
}

impl std::error::Error for OversellError {}

// ── OrderTx ───────────────────────────────────────────────────────────────────

pub struct OrderTx;

impl OrderTx {
    /// Lock semua variant dengan SELECT FOR UPDATE OF ev + JOIN events.
    pub async fn lock_variants(
        tx: &tokio_postgres::Transaction<'_>,
        id_bytes_list: &[Vec<u8>],
    ) -> Result<Vec<LockedVariant>> {
        let rows = tx
            .query(
                r#"
                SELECT
                    ev.id,
                    ev.price::FLOAT8                    AS price,
                    CASE
                        WHEN ev.sale_price IS NOT NULL
                            AND NOW() BETWEEN
                                COALESCE(ev.sale_price_start_date, '-infinity'::timestamptz)
                            AND COALESCE(ev.sale_price_end_date,   'infinity'::timestamptz)
                        THEN ev.sale_price::FLOAT8
                        ELSE ev.price::FLOAT8
                    END                                 AS effective_price,
                    ev.quota,
                    ev.sold,
                    ev.max_per_order,
                    ev.is_active,
                    ev.name                             AS variant_name,
                    e.id                                AS event_id_bytes,
                    e.name                              AS event_name
                FROM event_variants ev
                JOIN events e ON ev.event_id = e.id
                WHERE ev.id = ANY($1)
                FOR UPDATE OF ev
                "#,
                &[&id_bytes_list],
            )
            .await
            .context("lock_variants SELECT FOR UPDATE")?;

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

    /// Insert satu baris order — idempotent via CTE atomic.
    ///
    /// # Idempotency
    /// Menggunakan CTE single-query untuk menghindari race condition
    /// antara INSERT dan SELECT pada READ COMMITTED isolation:
    ///
    /// ```sql
    /// WITH ins AS (
    ///     INSERT INTO orders (...) ON CONFLICT DO NOTHING RETURNING *
    /// )
    /// SELECT * FROM ins
    /// UNION ALL
    /// SELECT * FROM orders WHERE customer_id=$1 AND idempotency_key=$2
    ///   AND NOT EXISTS (SELECT 1 FROM ins)
    /// ```
    ///
    /// Ini atomic — tidak ada window antara insert dan fetch.
    ///
    /// # Return
    /// - `(order, true)`  → order baru berhasil di-insert
    /// - `(order, false)` → idempotency conflict, order existing dikembalikan
    pub async fn insert_order(
        tx: &tokio_postgres::Transaction<'_>,
        id_bytes: &[u8],
        customer_bytes: &[u8],
        order_code: &str,
        total: f64,
        expired_at: chrono::DateTime<chrono::Utc>,
        idempotency_key: Option<&str>,
    ) -> Result<(Order, bool)> {
        // Kalau tidak ada idempotency_key → insert biasa, selalu Some
        if idempotency_key.is_none() {
            let row = tx
                .query_one(
                    &format!(
                        "INSERT INTO orders \
                         (id, customer_id, order_code, status, total_amount, expired_at) \
                         VALUES ($1, $2, $3, 'pending', $4, $5) \
                         RETURNING {cols}",
                        cols = ORDER_COLS
                    ),
                    &[&id_bytes, &customer_bytes, &order_code, &total, &expired_at],
                )
                .await
                .context("insert_order (no idempotency_key)")?;
            return Ok((row_to_order(&row)?, true));
        }

        // Ada idempotency_key → CTE atomic: insert OR fetch existing dalam satu query.
        //
        // Kenapa CTE dan bukan dua query terpisah:
        //   Dengan READ COMMITTED (default PG), dua query terpisah bisa race:
        //   - Thread A insert, belum commit
        //   - Thread B fetch → miss → error "conflict tapi tidak ketemu"
        //   CTE dengan UNION ALL ini atomic dalam satu statement — tidak ada gap.
        //
        // UNIQUE INDEX yang dibutuhkan:
        //   CREATE UNIQUE INDEX ON orders(customer_id, idempotency_key)
        //   WHERE idempotency_key IS NOT NULL;
        let row = tx
            .query_one(
                &format!(
                    r#"
                    WITH ins AS (
                        INSERT INTO orders
                            (id, customer_id, order_code, status, total_amount,
                             expired_at, idempotency_key)
                        VALUES ($1, $2, $3, 'pending', $4, $5, $6)
                        ON CONFLICT (customer_id, idempotency_key)
                        WHERE idempotency_key IS NOT NULL
                        DO NOTHING
                        RETURNING {cols}, TRUE AS is_new
                    )
                    SELECT * FROM ins
                    UNION ALL
                    SELECT {cols}, FALSE AS is_new
                    FROM orders
                    WHERE customer_id = $2
                      AND idempotency_key = $6
                      AND NOT EXISTS (SELECT 1 FROM ins)
                    LIMIT 1
                    "#,
                    cols = ORDER_COLS
                ),
                &[
                    &id_bytes,
                    &customer_bytes,
                    &order_code,
                    &total,
                    &expired_at,
                    &idempotency_key,
                ],
            )
            .await
            .context("insert_order (idempotency CTE)")?;

        let is_new: bool = row.try_get("is_new").context("is_new")?;
        Ok((row_to_order(&row)?, is_new))
    }

    /// Batch insert order_items — satu query, bukan O(n) round-trip.
    pub async fn insert_order_items_batch(
        tx: &tokio_postgres::Transaction<'_>,
        order_id_bytes: &[u8],
        items: &[ItemRow],
    ) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        const COLS: usize = 6;
        let mut placeholders = Vec::with_capacity(items.len());
        for i in 0..items.len() {
            let b = i * COLS + 1;
            placeholders.push(format!(
                "(${}, ${}, ${}, ${}, ${}, ${})",
                b,
                b + 1,
                b + 2,
                b + 3,
                b + 4,
                b + 5
            ));
        }

        let sql = format!(
            "INSERT INTO order_items \
             (id, order_id, ticket_variant_id, quantity, unit_price, subtotal) \
             VALUES {}",
            placeholders.join(", ")
        );

        type BoxParam = Box<dyn tokio_postgres::types::ToSql + Sync + Send>;
        let mut params: Vec<BoxParam> = Vec::with_capacity(items.len() * COLS);
        for item in items {
            params.push(Box::new(item.oi_bytes.clone()));
            params.push(Box::new(order_id_bytes.to_vec()));
            params.push(Box::new(item.var_bytes.clone()));
            params.push(Box::new(item.qty));
            params.push(Box::new(item.unit_price));
            params.push(Box::new(item.subtotal));
        }

        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(|p| p.as_ref() as _).collect();
        tx.execute(&sql, &refs)
            .await
            .context("insert_order_items_batch")?;

        Ok(())
    }

    /// Batch bump sold — pre-aggregated + DB-level oversell guard.
    ///
    /// # Oversell Protection (Defense in Depth)
    /// Walaupun aplikasi sudah validasi stok sebelum memanggil fungsi ini,
    /// guard di DB tetap diperlukan sebagai last line of defense:
    /// - Redis lock bisa gagal (network partition, GC pause, bug)
    /// - Heartbeat bisa miss window
    ///
    /// Query ini:
    /// 1. Pre-aggregate qty per variant via CTE (handle kasus satu variant
    ///    muncul multiple kali dalam satu batch → cegah double counting)
    /// 2. Guard `(quota - sold) >= total_qty` per variant, atomik di DB
    /// 3. Kembalikan jumlah row yang ter-update
    ///
    /// # Error
    /// Mengembalikan `OversellError` kalau `rows_updated != variants_count`.
    /// Caller HARUS rollback transaksi.
    pub async fn bump_sold_batch(
        tx: &tokio_postgres::Transaction<'_>,
        updates: &[(Vec<u8>, i32)],
    ) -> Result<(), anyhow::Error> {
        if updates.is_empty() {
            return Ok(());
        }

        // Pre-aggregate di sisi Rust sebelum kirim ke DB.
        // Ini menghindari kasus variant duplikat dalam satu batch
        // yang bisa membuat guard per-row lolos walaupun total oversell.
        //
        // Contoh tanpa aggregasi:
        //   quota=10, sold=8
        //   batch: [variant_A +2, variant_A +2]
        //   per-row: 10-8 >= 2 ✅, 10-8 >= 2 ✅  → tapi total +4 → oversell!
        //
        // Dengan aggregasi: total_qty=4, 10-8=2 < 4 → guard menolak ✅
        let mut agg: std::collections::HashMap<Vec<u8>, i32> =
            std::collections::HashMap::with_capacity(updates.len());
        for (id, qty) in updates {
            *agg.entry(id.clone()).or_insert(0) += qty;
        }

        let ids: Vec<Vec<u8>> = agg.keys().cloned().collect();
        let qtys: Vec<i32> = agg.values().cloned().collect();
        let expected = ids.len();

        // CTE: aggregate UNNEST → UPDATE dengan guard quota
        // Guard `(ev.quota - ev.sold) >= agg.total_qty` di-eval atomik
        // pada row yang ter-lock (sudah SELECT FOR UPDATE sebelumnya).
        let updated = tx
            .execute(
                r#"
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
                "#,
                &[&ids, &qtys],
            )
            .await
            .context("bump_sold_batch")?;

        if updated as usize != expected {
            return Err(anyhow::anyhow!(OversellError { updated, expected }));
        }

        Ok(())
    }

    /// Mark order paid — double guard: status pending AND belum expired.
    pub async fn mark_paid(
        tx: &tokio_postgres::Transaction<'_>,
        order_bytes: &[u8],
        payment_method: &str,
    ) -> Result<u64> {
        tx.execute(
            "UPDATE orders \
               SET status = 'paid', paid_at = NOW(), payment_method = $2 \
             WHERE id = $1 \
               AND status = 'pending' \
               AND expired_at > NOW()",
            &[&order_bytes, &payment_method],
        )
        .await
        .context("mark_paid")
    }

    /// Ambil (order_item_id_bytes, qty) untuk minting tiket.
    pub async fn fetch_items_for_order(
        tx: &tokio_postgres::Transaction<'_>,
        order_bytes: &[u8],
    ) -> Result<Vec<(Vec<u8>, i32)>> {
        tx.query(
            "SELECT id, quantity FROM order_items WHERE order_id = $1",
            &[&order_bytes],
        )
        .await
        .context("fetch_items_for_order")?
        .iter()
        .map(|r| Ok((r.try_get::<_, Vec<u8>>("id")?, r.try_get("quantity")?)))
        .collect()
    }

    /// Mint semua tiket dalam satu batch INSERT.
    pub async fn mint_tickets_batch(
        tx: &tokio_postgres::Transaction<'_>,
        items: &[(Vec<u8>, i32)],
    ) -> Result<u64> {
        let total: i32 = items.iter().map(|(_, q)| q).sum();
        if total == 0 {
            return Ok(0);
        }

        let mut ticket_rows: Vec<(Vec<u8>, Vec<u8>, String)> = Vec::with_capacity(total as usize);
        for (item_bytes, qty) in items {
            for _ in 0..*qty {
                let id = new_ulid();
                let id_bytes = ulid_to_vec(&id)?;
                let code = make_ticket_code(&id);
                ticket_rows.push((id_bytes, item_bytes.clone(), code));
            }
        }

        const COLS: usize = 3;
        let mut placeholders = Vec::with_capacity(ticket_rows.len());
        for i in 0..ticket_rows.len() {
            let b = i * COLS + 1;
            placeholders.push(format!("(${}, ${}, ${}, 'active')", b, b + 1, b + 2));
        }

        let sql = format!(
            "INSERT INTO tickets (id, order_item_id, ticket_code, status) VALUES {}",
            placeholders.join(", ")
        );

        type BoxParam = Box<dyn tokio_postgres::types::ToSql + Sync + Send>;
        let mut params: Vec<BoxParam> = Vec::with_capacity(ticket_rows.len() * COLS);
        for (id_b, item_b, code) in &ticket_rows {
            params.push(Box::new(id_b.clone()));
            params.push(Box::new(item_b.clone()));
            params.push(Box::new(code.clone()));
        }

        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(|p| p.as_ref() as _).collect();
        tx.execute(&sql, &refs)
            .await
            .context("mint_tickets_batch")?;

        Ok(ticket_rows.len() as u64)
    }

    /// Cancel order — hanya jika masih pending.
    pub async fn cancel_order(
        tx: &tokio_postgres::Transaction<'_>,
        order_bytes: &[u8],
    ) -> Result<u64> {
        tx.execute(
            "UPDATE orders SET status = 'cancelled' \
              WHERE id = $1 AND status = 'pending'",
            &[&order_bytes],
        )
        .await
        .context("cancel_order")
    }

    /// Ambil (variant_id_bytes, qty) untuk refund stok saat cancel.
    pub async fn fetch_items_for_refund(
        tx: &tokio_postgres::Transaction<'_>,
        order_bytes: &[u8],
    ) -> Result<Vec<(Vec<u8>, i32)>> {
        tx.query(
            "SELECT ticket_variant_id, quantity FROM order_items WHERE order_id = $1",
            &[&order_bytes],
        )
        .await
        .context("fetch_items_for_refund")?
        .iter()
        .map(|r| {
            Ok((
                r.try_get::<_, Vec<u8>>("ticket_variant_id")?,
                r.try_get("quantity")?,
            ))
        })
        .collect()
    }

    /// Refund stok saat cancel — GREATEST(0, ...) mencegah sold negatif.
    pub async fn refund_sold_batch(
        tx: &tokio_postgres::Transaction<'_>,
        updates: &[(Vec<u8>, i32)],
    ) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let ids: Vec<Vec<u8>> = updates.iter().map(|(id, _)| id.clone()).collect();
        let qtys: Vec<i32> = updates.iter().map(|(_, q)| *q).collect();

        tx.execute(
            "UPDATE event_variants \
               SET sold = GREATEST(0, sold - bump.qty) \
              FROM UNNEST($1::bytea[], $2::int4[]) AS bump(id, qty) \
             WHERE event_variants.id = bump.id",
            &[&ids, &qtys],
        )
        .await
        .context("refund_sold_batch")?;

        Ok(())
    }
}

// ── Trait ─────────────────────────────────────────────────────────────────────

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

// ── PgOrderRepository ─────────────────────────────────────────────────────────

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
        let rows = exec_rows(
            &self.pool,
            &LIST_ORDERS_BY_CUSTOMER,
            &[&id_vec, &limit, &offset],
        )
        .await?;
        rows.iter().map(row_to_order).collect()
    }

    async fn list_items(&self, order_id: &str) -> Result<Vec<OrderItemResponse>> {
        let id_vec = id_to_vec(order_id)?;
        let rows = exec_rows(&self.pool, FIND_ITEMS_FOR_ORDER, &[&id_vec]).await?;
        rows.iter()
            .map(|row| {
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
            })
            .collect()
    }
}

// ── Shared helper ─────────────────────────────────────────────────────────────

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
