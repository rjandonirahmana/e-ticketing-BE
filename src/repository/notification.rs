use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use serde_json::Value as JsonValue;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first, exec_rows, get_conn};
use crate::models::notification::{CreateNotificationInput, Notification, NotificationTarget};
use crate::utils::ulid::{bin_to_ulid, bin_to_ulid_opt, id_to_vec, new_ulid, ulid_to_vec};

// ── Static queries ────────────────────────────────────────────────────────────
// Skema polymorphic: kolom `target_id` + diskriminator `kind`.

/// Kolom dasar — dipakai INSERT ... RETURNING dan LIST (tanpa join).
static NOTIF_COLS: &str =
    "id, user_id, kind, title, body, is_read, target_id, created_at, updated_at";

static INSERT_NOTIF: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"INSERT INTO notifications
               (id, user_id, kind, title, body, target_id)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING {}"#,
        NOTIF_COLS
    )
});

/// LIST: polos, hanya dari tabel notifications (tanpa join ke target).
static LIST_FOR_USER: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM notifications WHERE user_id = $1 \
         ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        NOTIF_COLS
    )
});

/// DETAIL: 1 notifikasi milik user, ikut menarik data target sesuai `kind`
/// lewat correlated subquery. Hanya di sini join dilakukan.
///
/// Kunci jsonb HARUS sama dengan field di enum `NotificationTarget`
/// dan menyertakan "kind" sebagai tag internal serde.
static FIND_DETAIL: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"
    SELECT
        {cols},

        CASE n.kind
            WHEN 'story'  THEN (SELECT event_title FROM stories WHERE id = n.target_id)
            WHEN 'order'  THEN (SELECT order_code  FROM orders  WHERE id = n.target_id)
            WHEN 'ticket' THEN (SELECT ticket_code FROM tickets WHERE id = n.target_id)
            ELSE NULL
        END AS target_title,

        CASE n.kind
            WHEN 'story' THEN (
                SELECT jsonb_build_object(
                    'kind',        'story',
                    'media_url',   s.media_url,
                    'slug',        s.event_slug,
                    'event_title', s.event_title
                )
                FROM stories s
                WHERE s.id = n.target_id
            )
            WHEN 'order' THEN (
                SELECT jsonb_build_object(
                    'kind',           'order',
                    'order_code',     o.order_code,
                    'status',         o.status,
                    'total_amount',   o.total_amount::text,
                    'payment_method', o.payment_method,
                    'expired_at',     o.expired_at
                )
                FROM orders o
                WHERE o.id = n.target_id
            )
            WHEN 'ticket' THEN (
                SELECT jsonb_build_object(
                    'kind',         'ticket',
                    'ticket_code',  t.ticket_code,
                    'status',       t.status,
                    'used_at',      t.used_at,
                    'variant_name', ev.name,
                    'event_name',   e.name,
                    'event_date',   e.event_date,
                    'venue',        e.venue,
                    'event_slug',   e.slug
                )
                FROM tickets t
                JOIN order_items    oi ON oi.id = t.order_item_id
                JOIN event_variants ev ON ev.id = oi.ticket_variant_id
                JOIN events         e  ON e.id  = ev.event_id
                WHERE t.id = n.target_id
            )
            ELSE NULL
        END AS target_data
    FROM notifications n
    WHERE n.id = $1 AND n.user_id = $2
    "#,
        // prefix kolom dasar dengan alias `n.`
        cols = NOTIF_COLS
            .split(", ")
            .map(|c| format!("n.{c}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
});

static MARK_READ: &str =
    "UPDATE notifications SET is_read = TRUE, updated_at = NOW() WHERE id = $1 AND user_id = $2";

static MARK_ALL_READ: &str =
    "UPDATE notifications SET is_read = TRUE, updated_at = NOW() WHERE user_id = $1";

static UNREAD_COUNT: &str =
    "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND is_read = FALSE";

// ── Repository trait ──────────────────────────────────────────────────────────

#[async_trait]
pub trait NotificationRepository: Send + Sync {
    async fn create(&self, input: CreateNotificationInput) -> Result<Notification>;
    async fn list_for_user(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>>;
    /// Ambil 1 notifikasi milik user + data target (join sesuai `kind`).
    async fn find_detail(&self, id: &str, user_id: &str) -> Result<Notification>;
    async fn mark_read(&self, id: &str, user_id: &str) -> Result<()>;
    async fn mark_all_read(&self, user_id: &str) -> Result<()>;
    async fn unread_count(&self, user_id: &str) -> Result<i64>;
}

// ── Row mappers ───────────────────────────────────────────────────────────────

/// Mapper dasar — `target_title` & `target` di-pass terpisah (hanya ada di detail).
fn row_to_notif(
    row: &Row,
    target_title: Option<String>,
    target: Option<NotificationTarget>,
) -> Result<Notification> {
    let id_bytes: Vec<u8> = row.try_get("id")?;
    let user_bytes: Vec<u8> = row.try_get("user_id")?;
    let target_bytes: Option<Vec<u8>> = row.try_get("target_id")?;

    Ok(Notification {
        id: bin_to_ulid(id_bytes)?,
        user_id: bin_to_ulid(user_bytes)?,
        kind: row.try_get("kind")?,
        title: row.try_get("title")?,
        body: row.try_get("body")?,
        is_read: row.try_get("is_read")?,
        target_id: bin_to_ulid_opt(target_bytes)?,
        target_title,
        target,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// Mapper untuk baris DETAIL (punya kolom target_title & target_data).
fn row_to_notif_detail(row: &Row) -> Result<Notification> {
    let target_title: Option<String> = row.try_get("target_title")?;
    let target_data: Option<JsonValue> = row.try_get("target_data")?;
    let target = target_data
        .map(serde_json::from_value::<NotificationTarget>)
        .transpose()
        .context("decode notifications.target_data")?;
    row_to_notif(row, target_title, target)
}

// ── Postgres implementation ───────────────────────────────────────────────────

pub struct PgNotificationRepository {
    pool: Pool,
}

impl PgNotificationRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NotificationRepository for PgNotificationRepository {
    async fn create(&self, input: CreateNotificationInput) -> Result<Notification> {
        let id = new_ulid();
        let id_bytes = ulid_to_vec(&id)?;
        let user_bytes = id_to_vec(&input.user_id)?;
        let target_bytes: Option<Vec<u8>> =
            input.target_id.as_deref().map(id_to_vec).transpose()?;

        let row = exec_first(
            &self.pool,
            &INSERT_NOTIF,
            &[
                &id_bytes.as_slice(),
                &user_bytes.as_slice(),
                &input.kind,
                &input.title,
                &input.body,
                &target_bytes.as_ref().map(|b| b.as_slice()),
            ],
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("INSERT notification returned no row"))?;

        row_to_notif(&row, None, None)
    }

    async fn list_for_user(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Notification>> {
        let user_bytes = id_to_vec(user_id)?;
        let rows = exec_rows(
            &self.pool,
            &LIST_FOR_USER,
            &[&user_bytes.as_slice(), &limit, &offset],
        )
        .await?;
        // List polos: tanpa target.
        rows.iter()
            .map(|row| row_to_notif(row, None, None))
            .collect()
    }

    async fn find_detail(&self, id: &str, user_id: &str) -> Result<Notification> {
        let id_bytes = id_to_vec(id)?;
        let user_bytes = id_to_vec(user_id)?;
        let row = exec_first(
            &self.pool,
            &FIND_DETAIL,
            &[&id_bytes.as_slice(), &user_bytes.as_slice()],
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("notification not found"))?;
        row_to_notif_detail(&row)
    }

    async fn mark_read(&self, id: &str, user_id: &str) -> Result<()> {
        let id_bytes = id_to_vec(id)?;
        let user_bytes = id_to_vec(user_id)?;
        exec_drop(
            &self.pool,
            MARK_READ,
            &[&id_bytes.as_slice(), &user_bytes.as_slice()],
        )
        .await?;
        Ok(())
    }

    async fn mark_all_read(&self, user_id: &str) -> Result<()> {
        let user_bytes = id_to_vec(user_id)?;
        exec_drop(&self.pool, MARK_ALL_READ, &[&user_bytes.as_slice()]).await?;
        Ok(())
    }

    async fn unread_count(&self, user_id: &str) -> Result<i64> {
        let user_bytes = id_to_vec(user_id)?;
        let conn = get_conn(&self.pool).await?;
        let row = conn
            .query_one(UNREAD_COUNT, &[&user_bytes.as_slice()])
            .await?;
        Ok(row.try_get::<_, i64>(0)?)
    }
}
