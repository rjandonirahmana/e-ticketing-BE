//! repository/story.rs
//!
//! Akses data untuk tabel `stories`, `story_views`, dan `user_subscriptions`.

use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use serde_json::Value as JsonValue;
use tokio_postgres::Row;

use super::db::get_conn;
use crate::models::stories::{Story, StoryGroupResponse, StoryItemResponse, SubscriptionActivation, UserSubscription};
use crate::web::models::PendingSubOrder;
use crate::utils::ulid::{bin_to_ulid, bin_to_ulid_opt, id_to_vec, new_ulid, ulid_to_vec};

// ── Row mappers ───────────────────────────────────────────────────────────────

fn row_to_story(row: &Row) -> Result<Story> {
    let id_bytes: Vec<u8> = row.try_get("id")?;
    let user_bytes: Vec<u8> = row.try_get("user_id")?;
    let event_bytes: Option<Vec<u8>> = row.try_get("event_id")?;

    let overlays_json: Option<serde_json::Value> = row.try_get("overlays")?;
    let overlays: Vec<JsonValue> = match overlays_json {
        Some(serde_json::Value::Array(a)) => a,
        _ => vec![],
    };

    Ok(Story {
        id: bin_to_ulid(id_bytes)?,
        user_id: bin_to_ulid(user_bytes)?,
        media_url: row.try_get("media_url")?,
        media_type: row.try_get("media_type")?,
        filter: row.try_get("filter")?,
        overlays,
        event_id: bin_to_ulid_opt(event_bytes)?,
        event_slug: row.try_get("event_slug")?,
        event_title: row.try_get("event_title")?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
    })
}

fn row_to_subscription(row: &Row) -> Result<UserSubscription> {
    let id_bytes: Vec<u8> = row.try_get("id")?;
    let user_bytes: Vec<u8> = row.try_get("user_id")?;
    Ok(UserSubscription {
        id: bin_to_ulid(id_bytes)?,
        user_id: bin_to_ulid(user_bytes)?,
        plan: row.try_get("plan")?,
        started_at: row.try_get("started_at")?,
        expires_at: row.try_get("expires_at")?,
        is_active: row.try_get("is_active")?,
        created_at: row.try_get("created_at")?,
    })
}

// ── Trait ─────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait StoryRepository: Send + Sync {
    /// Cek apakah user punya active premium subscription.
    async fn is_premium(&self, user_id: &str) -> Result<bool>;

    /// Hitung jumlah story yang user buat hari ini (UTC).
    async fn count_today(&self, user_id: &str) -> Result<i64>;

    /// Insert story baru, return record lengkap.
    async fn create(
        &self,
        user_id: &str,
        media_url: &str,
        media_type: &str,
        filter: Option<&str>,
        overlays: &[JsonValue],
        event_id: Option<&str>,
        event_slug: Option<&str>,
        event_title: Option<&str>,
    ) -> Result<Story>;

    /// Ambil semua story aktif (belum expired), dikelompokkan per user.
    /// Story milik `viewer_id` ditaruh di posisi pertama.
    async fn list_groups(&self, viewer_id: &str) -> Result<Vec<StoryGroupResponse>>;

    /// Tandai story sudah dilihat oleh viewer.
    async fn mark_viewed(&self, story_id: &str, viewer_id: &str) -> Result<()>;

    /// Hapus story — hanya oleh owner.
    async fn delete(&self, story_id: &str, owner_id: &str) -> Result<bool>;

    /// Ambil satu story by id.
    async fn find_by_id(&self, story_id: &str) -> Result<Option<Story>>;

    /// Aktifkan premium subscription untuk user — buat subscription_order + user_subscription.
    async fn activate_premium(
        &self,
        user_id: &str,
        plan: &str,
        days: i64,
    ) -> Result<SubscriptionActivation>;

    /// Buat subscription_order dengan status 'pending' — belum aktifkan premium.
    async fn create_pending_subscription_order(
        &self,
        user_id: &str,
        plan: &str,
    ) -> Result<PendingSubOrder>;

    /// Konfirmasi pembayaran: tandai order sebagai 'paid' lalu aktifkan premium.
    async fn confirm_subscription_order(
        &self,
        order_id: &str,
        user_id: &str,
        plan: &str,
        days: i64,
    ) -> Result<SubscriptionActivation>;
}

// ── PostgreSQL implementation ─────────────────────────────────────────────────

pub struct PgStoryRepository {
    pool: Pool,
}

impl PgStoryRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StoryRepository for PgStoryRepository {
    // ── Premium check ─────────────────────────────────────────────────────────

    async fn is_premium(&self, user_id: &str) -> Result<bool> {
        let uid = id_to_vec(user_id)?;
        let conn = get_conn(&self.pool).await?;
        let row = conn
            .query_opt(
                r#"
                SELECT 1 FROM user_subscriptions
                WHERE  user_id   = $1
                  AND  is_active = TRUE
                  AND  expires_at > NOW()
                LIMIT 1
                "#,
                &[&uid],
            )
            .await
            .context("is_premium query")?;
        Ok(row.is_some())
    }

    // ── Daily story count ─────────────────────────────────────────────────────

    async fn count_today(&self, user_id: &str) -> Result<i64> {
        let uid = id_to_vec(user_id)?;
        let conn = get_conn(&self.pool).await?;
        let row = conn
            .query_one(
                r#"
                SELECT COUNT(*)::BIGINT
                FROM   stories
                WHERE  user_id    = $1
                  AND  created_at >= DATE_TRUNC('day', NOW() AT TIME ZONE 'UTC')
                "#,
                &[&uid],
            )
            .await
            .context("count_today query")?;
        Ok(row.try_get::<_, i64>(0)?)
    }

    // ── Create story ──────────────────────────────────────────────────────────

    async fn create(
        &self,
        user_id: &str,
        media_url: &str,
        media_type: &str,
        filter: Option<&str>,
        overlays: &[JsonValue],
        event_id: Option<&str>,
        event_slug: Option<&str>,
        event_title: Option<&str>,
    ) -> Result<Story> {
        let id_bytes = {
            let ulid = new_ulid();
            ulid_to_vec(&ulid)?
        };
        let uid = id_to_vec(user_id)?;
        let event_id_bytes: Option<Vec<u8>> = match event_id {
            Some(eid) => Some(id_to_vec(eid)?),
            None => None,
        };

        let overlays_json = serde_json::Value::Array(overlays.to_vec());

        let conn = get_conn(&self.pool).await?;
        let row = conn
            .query_one(
                r#"
                INSERT INTO stories
                    (id, user_id, media_url, media_type, filter, overlays,
                     event_id, event_slug, event_title,
                     expires_at)
                VALUES
                    ($1, $2, $3, $4, $5, $6,
                     $7, $8, $9,
                     NOW() + INTERVAL '24 hours')
                RETURNING
                    id, user_id, media_url, media_type, filter, overlays,
                    event_id, event_slug, event_title, created_at, expires_at
                "#,
                &[
                    &id_bytes,
                    &uid,
                    &media_url,
                    &media_type,
                    &filter,
                    &overlays_json,
                    &event_id_bytes,
                    &event_slug,
                    &event_title,
                ],
            )
            .await
            .context("insert story")?;

        row_to_story(&row)
    }

    // ── List groups ───────────────────────────────────────────────────────────

    async fn list_groups(&self, viewer_id: &str) -> Result<Vec<StoryGroupResponse>> {
        let vid = id_to_vec(viewer_id)?;
        let conn = get_conn(&self.pool).await?;

        // Ambil semua story aktif beserta info view
        let rows = conn
            .query(
                r#"
                SELECT
                    s.id,
                    s.user_id,
                    COALESCE(u.name, 'Unknown')       AS username,
                    COALESCE(u.avatar_url, '')         AS avatar_url,
                    s.media_url,
                    s.media_type,
                    s.filter,
                    s.overlays,
                    s.created_at,
                    s.expires_at,
                    s.event_id,
                    s.event_slug,
                    s.event_title,
                    (sv.viewer_id IS NOT NULL)         AS viewed
                FROM   stories s
                JOIN   users u   ON u.id = s.user_id
                LEFT   JOIN story_views sv
                       ON sv.story_id = s.id AND sv.viewer_id = $1
                WHERE  s.expires_at > NOW()
                ORDER BY
                    -- story milik viewer sendiri muncul pertama
                    (s.user_id = $1) DESC,
                    s.user_id,
                    s.created_at ASC
                "#,
                &[&vid],
            )
            .await
            .context("list_groups query")?;

        // Group by user_id (order sudah benar dari ORDER BY)
        let mut groups: Vec<StoryGroupResponse> = Vec::new();
        for row in &rows {
            let id_bytes: Vec<u8> = row.try_get("id")?;
            let user_bytes: Vec<u8> = row.try_get("user_id")?;
            let event_bytes: Option<Vec<u8>> = row.try_get("event_id")?;
            let user_id_str = bin_to_ulid(user_bytes.clone())?;

            let overlays_json: Option<serde_json::Value> = row.try_get("overlays")?;
            let overlays: Vec<JsonValue> = match overlays_json {
                Some(serde_json::Value::Array(a)) => a,
                _ => vec![],
            };

            let item = StoryItemResponse {
                id: bin_to_ulid(id_bytes)?,
                user_id: user_id_str.clone(),
                username: row.try_get("username")?,
                avatar_url: row.try_get("avatar_url")?,
                media_url: row.try_get("media_url")?,
                media_type: row.try_get("media_type")?,
                filter: row.try_get("filter")?,
                overlays,
                created_at: row.try_get("created_at")?,
                expires_at: row.try_get("expires_at")?,
                viewed: row.try_get("viewed")?,
                event_id: bin_to_ulid_opt(event_bytes)?,
                event_slug: row.try_get("event_slug")?,
                event_title: row.try_get("event_title")?,
            };

            if let Some(g) = groups.iter_mut().find(|g| g.user_id == user_id_str) {
                g.stories.push(item);
            } else {
                groups.push(StoryGroupResponse {
                    user_id: user_id_str,
                    username: item.username.clone(),
                    avatar_url: item.avatar_url.clone(),
                    stories: vec![item],
                });
            }
        }

        Ok(groups)
    }

    // ── Mark viewed ───────────────────────────────────────────────────────────

    async fn mark_viewed(&self, story_id: &str, viewer_id: &str) -> Result<()> {
        let sid = id_to_vec(story_id)?;
        let vid = id_to_vec(viewer_id)?;
        let conn = get_conn(&self.pool).await?;
        conn.execute(
            r#"
            INSERT INTO story_views (story_id, viewer_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
            &[&sid, &vid],
        )
        .await
        .context("mark_viewed")?;
        Ok(())
    }

    // ── Delete ────────────────────────────────────────────────────────────────

    async fn delete(&self, story_id: &str, owner_id: &str) -> Result<bool> {
        let sid = id_to_vec(story_id)?;
        let oid = id_to_vec(owner_id)?;
        let conn = get_conn(&self.pool).await?;
        let n = conn
            .execute(
                "DELETE FROM stories WHERE id = $1 AND user_id = $2",
                &[&sid, &oid],
            )
            .await
            .context("delete story")?;
        Ok(n > 0)
    }

    // ── Find by id ────────────────────────────────────────────────────────────

    async fn find_by_id(&self, story_id: &str) -> Result<Option<Story>> {
        let sid = id_to_vec(story_id)?;
        let conn = get_conn(&self.pool).await?;
        let row = conn
            .query_opt(
                r#"
                SELECT id, user_id, media_url, media_type, filter, overlays,
                       event_id, event_slug, event_title, created_at, expires_at
                FROM   stories
                WHERE  id = $1
                "#,
                &[&sid],
            )
            .await
            .context("find_by_id story")?;
        row.map(|r| row_to_story(&r)).transpose()
    }

    // ── Activate premium ──────────────────────────────────────────────────────

    async fn activate_premium(&self, user_id: &str, plan: &str, days: i64) -> Result<SubscriptionActivation> {
        let uid = id_to_vec(user_id)?;

        let order_ulid = new_ulid();
        let order_code = format!("SUB-{}", &order_ulid.to_uppercase()[..8]);
        let order_id_bytes = ulid_to_vec(&order_ulid)?;

        let amount: rust_decimal::Decimal = match plan {
            "weekly"   => rust_decimal_macros::dec!(9900.00),
            "yearly"   => rust_decimal_macros::dec!(199000.00),
            "lifetime" => rust_decimal_macros::dec!(499000.00),
            _          => rust_decimal_macros::dec!(29000.00),
        };

        let sub_id_bytes = ulid_to_vec(&new_ulid())?;

        let conn = get_conn(&self.pool).await?;

        // 1. Buat subscription_order (billing record)
        conn.execute(
            r#"
            INSERT INTO subscription_orders
                (id, user_id, order_code, plan, amount, status, paid_at)
            VALUES
                ($1, $2, $3, $4, $5, 'paid', NOW())
            "#,
            &[&order_id_bytes, &uid, &order_code, &plan, &amount],
        )
        .await
        .context("insert subscription_order")?;

        // 2. Nonaktifkan subscription lama
        conn.execute(
            "UPDATE user_subscriptions SET is_active = FALSE WHERE user_id = $1 AND is_active = TRUE",
            &[&uid],
        )
        .await
        .context("deactivate old subscription")?;

        // 3. Buat user_subscription baru, link ke subscription_order
        let row = if plan == "lifetime" {
            conn.query_one(
                r#"
                INSERT INTO user_subscriptions
                    (id, user_id, subscription_order_id, plan, expires_at)
                VALUES
                    ($1, $2, $3, $4, '9999-12-31 23:59:59+00'::timestamptz)
                RETURNING id, user_id, plan, started_at, expires_at, is_active, created_at
                "#,
                &[&sub_id_bytes, &uid, &order_id_bytes, &plan],
            )
            .await
            .context("activate premium lifetime")?
        } else {
            conn.query_one(
                r#"
                INSERT INTO user_subscriptions
                    (id, user_id, subscription_order_id, plan, expires_at)
                VALUES
                    ($1, $2, $3, $4, NOW() + ($5::BIGINT * INTERVAL '1 day'))
                RETURNING id, user_id, plan, started_at, expires_at, is_active, created_at
                "#,
                &[&sub_id_bytes, &uid, &order_id_bytes, &plan, &days],
            )
            .await
            .context("activate premium")?
        };

        let subscription = row_to_subscription(&row)?;
        Ok(SubscriptionActivation { subscription, order_code })
    }

    async fn create_pending_subscription_order(&self, user_id: &str, plan: &str) -> Result<PendingSubOrder> {
        let uid = id_to_vec(user_id)?;
        let order_ulid = new_ulid();
        let order_code = format!("SUB-{}", &order_ulid.to_uppercase()[..8]);
        let order_id_bytes = ulid_to_vec(&order_ulid)?;

        let amount: rust_decimal::Decimal = match plan {
            "weekly"   => rust_decimal_macros::dec!(9900.00),
            "yearly"   => rust_decimal_macros::dec!(199000.00),
            "lifetime" => rust_decimal_macros::dec!(499000.00),
            _          => rust_decimal_macros::dec!(29000.00),
        };
        let amount_idr: i64 = amount.try_into().unwrap_or(29000);

        let conn = get_conn(&self.pool).await?;
        conn.execute(
            r#"INSERT INTO subscription_orders
                   (id, user_id, order_code, plan, amount, status)
               VALUES ($1, $2, $3, $4, $5, 'pending')"#,
            &[&order_id_bytes, &uid, &order_code, &plan, &amount],
        )
        .await
        .context("create pending subscription order")?;

        Ok(PendingSubOrder { order_id: order_ulid, order_code, plan: plan.to_string(), amount_idr })
    }

    async fn confirm_subscription_order(
        &self,
        order_id: &str,
        user_id: &str,
        plan: &str,
        days: i64,
    ) -> Result<SubscriptionActivation> {
        let uid = id_to_vec(user_id)?;
        let order_id_bytes = ulid_to_vec(order_id)?;
        let sub_id_bytes = ulid_to_vec(&new_ulid())?;

        let conn = get_conn(&self.pool).await?;

        // Fetch order_code AND plan from the existing order.
        // SECURITY: the granted plan/duration MUST come from the stored order
        // (set at create_pending_subscription_order time alongside the charged
        // amount), NOT from the caller. Otherwise a user could create a cheap
        // `weekly` order then confirm with plan="lifetime" to escalate the
        // entitlement they actually paid for. The `plan`/`days` arguments are
        // therefore ignored here and re-derived from the persisted row.
        let order_row = conn
            .query_one(
                "SELECT order_code, plan FROM subscription_orders WHERE id = $1 AND user_id = $2 AND status = 'pending'",
                &[&order_id_bytes, &uid],
            )
            .await
            .context("find pending subscription order")?;
        let _ = (plan, days); // caller-supplied values ignored; superseded below
        let order_code: String = order_row.try_get(0)?;
        let plan: String = order_row.try_get(1)?;
        let days: i64 = match plan.as_str() {
            "weekly" => 7,
            "yearly" => 365,
            "lifetime" => 0,
            _ => 30, // monthly / default
        };

        // Mark as paid
        conn.execute(
            "UPDATE subscription_orders SET status = 'paid', paid_at = NOW() WHERE id = $1",
            &[&order_id_bytes],
        )
        .await
        .context("confirm subscription payment")?;

        // Deactivate old subscriptions
        conn.execute(
            "UPDATE user_subscriptions SET is_active = FALSE WHERE user_id = $1 AND is_active = TRUE",
            &[&uid],
        )
        .await
        .context("deactivate old subscription")?;

        // Create new subscription
        let row = if plan == "lifetime" {
            conn.query_one(
                r#"INSERT INTO user_subscriptions
                       (id, user_id, subscription_order_id, plan, expires_at)
                   VALUES ($1, $2, $3, $4, '9999-12-31 23:59:59+00'::timestamptz)
                   RETURNING id, user_id, plan, started_at, expires_at, is_active, created_at"#,
                &[&sub_id_bytes, &uid, &order_id_bytes, &plan],
            )
            .await
            .context("activate premium lifetime")?
        } else {
            conn.query_one(
                r#"INSERT INTO user_subscriptions
                       (id, user_id, subscription_order_id, plan, expires_at)
                   VALUES ($1, $2, $3, $4, NOW() + ($5::BIGINT * INTERVAL '1 day'))
                   RETURNING id, user_id, plan, started_at, expires_at, is_active, created_at"#,
                &[&sub_id_bytes, &uid, &order_id_bytes, &plan, &days],
            )
            .await
            .context("activate premium confirm")?
        };

        let subscription = row_to_subscription(&row)?;
        Ok(SubscriptionActivation { subscription, order_code })
    }
}
