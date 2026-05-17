use anyhow::Result;
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first, exec_rows, get_conn};
use crate::models::notification::{CreateNotificationInput, Notification};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

// ── Static queries ────────────────────────────────────────────────────────────

static NOTIF_COLS: &str =
    "id, user_id, kind, title, body, is_read, order_id, ticket_id, created_at, updated_at";

static INSERT_NOTIF: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"INSERT INTO notifications
               (id, user_id, kind, title, body, order_id, ticket_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING {}"#,
        NOTIF_COLS
    )
});

static LIST_FOR_USER: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM notifications WHERE user_id = $1 \
         ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        NOTIF_COLS
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
    async fn mark_read(&self, id: &str, user_id: &str) -> Result<()>;
    async fn mark_all_read(&self, user_id: &str) -> Result<()>;
    async fn unread_count(&self, user_id: &str) -> Result<i64>;
}

// ── Row mapper ────────────────────────────────────────────────────────────────

fn row_to_notif(row: &Row) -> Result<Notification> {
    let id_bytes: Vec<u8> = row.try_get("id")?;
    let user_bytes: Vec<u8> = row.try_get("user_id")?;
    let order_bytes: Option<Vec<u8>> = row.try_get("order_id")?;
    let ticket_bytes: Option<Vec<u8>> = row.try_get("ticket_id")?;

    Ok(Notification {
        id: bin_to_ulid(id_bytes)?,
        user_id: bin_to_ulid(user_bytes)?,
        kind: row.try_get("kind")?,
        title: row.try_get("title")?,
        body: row.try_get("body")?,
        is_read: row.try_get("is_read")?,
        order_id: order_bytes.map(bin_to_ulid).transpose()?,
        ticket_id: ticket_bytes.map(bin_to_ulid).transpose()?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
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
        let order_bytes: Option<Vec<u8>> = input
            .order_id
            .as_deref()
            .map(id_to_vec)
            .transpose()?;
        let ticket_bytes: Option<Vec<u8>> = input
            .ticket_id
            .as_deref()
            .map(id_to_vec)
            .transpose()?;

        let row = exec_first(
            &self.pool,
            &INSERT_NOTIF,
            &[
                &id_bytes.as_slice(),
                &user_bytes.as_slice(),
                &input.kind,
                &input.title,
                &input.body,
                &order_bytes.as_ref().map(|b| b.as_slice()),
                &ticket_bytes.as_ref().map(|b| b.as_slice()),
            ],
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("INSERT notification returned no row"))?;

        row_to_notif(&row)
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
        rows.iter().map(row_to_notif).collect()
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
