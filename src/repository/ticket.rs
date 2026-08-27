use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_first, exec_rows, get_conn};
use crate::models::tickets::TicketResponse;
use crate::utils::ulid::{bin_to_ulid, id_to_vec};

// ── Static query strings ──────────────────────────────────────────────────────

static TICKET_DETAIL_COLS: &str = r#"
    t.id            AS ticket_id,
    t.ticket_code,
    t.status,
    t.used_at,
    t.created_at,

    ci.id           AS order_item_id,
    ci.unit_price::FLOAT8 AS unit_price,

    o.id            AS order_id,
    o.order_code,
    o.customer_id,

    tv.id           AS variant_id,
    tv.name         AS variant_name,

    e.id            AS event_id,
    e.name          AS event_name,
    e.slug          AS event_slug,
    e.event_date,
    e.venue         AS event_venue,
    e.city          AS event_city,
    e.cover_url     AS cover_url,
    e.merchant_id
"#;

static FROM_JOINS: &str = r#"
      FROM tickets t
      JOIN cart_items ci       ON t.cart_item_id = ci.id
      JOIN orders o            ON t.order_id = o.id
      JOIN product_variants tv ON ci.ticket_variant_id = tv.id
      JOIN products e          ON tv.event_id = e.id
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

/// FIX: was `WHERE o.order_id = $1` (orders has no order_id column).
/// Corrected to `WHERE o.id = $1`.
/// Also adds `AND o.customer_id = $2` for ownership enforcement at query level.
static LIST_BY_ORDER_ID: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} {} WHERE o.id = $1 AND o.customer_id = $2 \
         ORDER BY t.created_at ASC LIMIT $3 OFFSET $4",
        TICKET_DETAIL_COLS, FROM_JOINS
    )
});

// ── Trait ─────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait TicketRepository: Send + Sync {
    async fn list_for_customer(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TicketResponse>>;

    /// List tickets that belong to a specific order.
    /// `customer_id` is checked in the WHERE clause so a user cannot read
    /// another user's order tickets.
    async fn list_by_order(
        &self,
        order_id: &str,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TicketResponse>>;

    async fn find_by_id(&self, id: &str) -> Result<Option<(TicketResponse, String, String)>>;

    /// Lookup by code + atomically transition `active` → `used`.
    /// Tukarkan kode pengambilan. `merchant_id` WAJIB — kepemilikan diperiksa
    /// DI DALAM transaksi, sebelum baris ditandai terpakai.
    async fn validate_by_code(
        &self,
        code: &str,
        merchant_id: &str,
    ) -> Result<TicketResponse>;
}

// ── PgTicketRepository ────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PgTicketRepository {
    pool: Pool,
}

impl PgTicketRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn row_to_response(row: &Row) -> Result<(TicketResponse, String, String)> {
        let ticket_bytes: Vec<u8> = row.try_get("ticket_id").context("ticket_id")?;
        let order_bytes: Vec<u8> = row.try_get("order_id").context("order_id")?;
        let variant_bytes: Vec<u8> = row.try_get("variant_id").context("variant_id")?;
        let product_bytes: Vec<u8> = row.try_get("event_id").context("event_id")?;
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
            event_id: bin_to_ulid(product_bytes)?,
            event_name: row.try_get("event_name")?,
            event_slug: row.try_get("event_slug")?,
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

    /// FIX: now uses LIST_BY_ORDER_ID (not LIST_FOR_CUSTOMER) and passes
    /// customer_id for ownership check.
    async fn list_by_order(
        &self,
        order_id: &str,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<TicketResponse>> {
        let order_vec = id_to_vec(order_id)?;
        let customer_vec = id_to_vec(customer_id)?;
        let rows = exec_rows(
            &self.pool,
            &LIST_BY_ORDER_ID,
            &[&order_vec, &customer_vec, &limit, &offset],
        )
        .await?;
        rows.iter()
            .map(|r| Self::row_to_response(r).map(|(t, _, _)| t))
            .collect()
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<(TicketResponse, String, String)>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_response).transpose()
    }

    /// ── KEPEMILIKAN DIPERIKSA SEBELUM MENANDAI, DI TRANSAKSI YANG SAMA ──────
    ///
    /// Versi sebelumnya menandai baris `used`, MELAKUKAN COMMIT, lalu
    /// mengembalikan `merchant_id` supaya service membandingkannya. Urutan itu
    /// membuat pemeriksaannya tak berguna: saat penolakan terjadi, kodenya sudah
    /// hangus dan sudah tersimpan permanen.
    ///
    /// Akibatnya siapa pun yang punya akun merchant bisa membakar kode
    /// pengambilan milik pembeli toko LAIN — cukup memindainya sekali. Ia
    /// menerima 403, tapi pembelinya yang menanggung: kodenya sudah `used` dan
    /// toko yang berhak tak akan pernah bisa menukarkannya. Satu pemindaian
    /// tak sengaja di konter yang salah sudah cukup.
    ///
    /// `FOR UPDATE OF t` — bukan `FOR UPDATE` telanjang: dengan JOIN, yang
    /// terakhir mencoba mengunci baris `products` dan `product_variants` juga.
    /// Mengunci katalog produk pada setiap pengambilan barang akan membuat
    /// kasir saling menunggu tanpa alasan, dan memblokir merchant yang kebetulan
    /// sedang menyunting produknya.
    async fn validate_by_code(
        &self,
        code: &str,
        merchant_id: &str,
    ) -> Result<TicketResponse> {
        let mut conn = get_conn(&self.pool).await?;
        let tx = conn.transaction().await?;

        // prepare_cached: statement cache per-koneksi deadpool — pemindaian kode
        // adalah jalur panas di konter, hindari parse+plan berulang.
        const LOCK_SQL: &str = r#"
            SELECT t.id, t.status, e.merchant_id
            FROM tickets t
            JOIN cart_items ci       ON t.cart_item_id = ci.id
            JOIN product_variants tv ON ci.ticket_variant_id = tv.id
            JOIN products e          ON tv.event_id = e.id
            WHERE t.ticket_code = $1
            FOR UPDATE OF t
        "#;
        let lock_stmt = tx.prepare_cached(LOCK_SQL).await?;
        let row = tx
            .query_opt(&lock_stmt, &[&code])
            .await?
            .ok_or_else(|| anyhow::anyhow!("Ticket not found"))?;

        let id_bytes: Vec<u8> = row.try_get("id")?;
        let status: String = row.try_get("status")?;
        let pemilik_bytes: Vec<u8> = row.try_get("merchant_id")?;
        let pemilik = bin_to_ulid(pemilik_bytes)?;

        // Kepemilikan LEBIH DULU, sebelum status: memberi tahu "sudah terpakai"
        // untuk kode milik toko lain membocorkan bahwa kodenya memang ada.
        if pemilik != merchant_id {
            bail!("Ticket belongs to another merchant");
        }

        match status.as_str() {
            "active" => {}
            "used" => bail!("Ticket already used"),
            "refunded" => bail!("Ticket has been refunded"),
            "expired" => bail!("Ticket has expired"),
            other => bail!("Ticket status '{}' cannot be validated", other),
        }

        // P2 FIX: format!() di sini tidak punya format argument — hanya wrap static str
        // ke String baru setiap panggilan (alokasi ~1KB per scan tiket). Gunakan static &str.
        const VALIDATE_DETAIL_SQL: &str = r#"
            WITH updated AS (
                UPDATE tickets
                SET status = 'used', used_at = NOW()
                WHERE id = $1
                RETURNING id, ticket_code, status, used_at,
                          created_at, cart_item_id, order_id
            )
            SELECT
                u.id            AS ticket_id,
                u.ticket_code,
                u.status,
                u.used_at,
                u.created_at,

                ci.id           AS order_item_id,
                ci.unit_price::FLOAT8 AS unit_price,

                o.id            AS order_id,
                o.order_code,
                o.customer_id,

                tv.id           AS variant_id,
                tv.name         AS variant_name,

                e.id            AS event_id,
                e.name          AS event_name,
                e.slug          AS event_slug,
                e.event_date,
                e.venue         AS event_venue,
                e.city          AS event_city,
                e.cover_url     AS cover_url,
                e.merchant_id
            FROM updated u
            JOIN cart_items ci       ON u.cart_item_id = ci.id
            JOIN orders o            ON u.order_id = o.id
            JOIN product_variants tv ON ci.ticket_variant_id = tv.id
            JOIN products e          ON tv.event_id = e.id
        "#;

        let detail_stmt = tx.prepare_cached(VALIDATE_DETAIL_SQL).await?;
        let detail_row = tx.query_one(&detail_stmt, &[&id_bytes]).await?;

        tx.commit().await?;

        let (resp, _customer, _merchant) = Self::row_to_response(&detail_row)?;
        Ok(resp)
    }
}
