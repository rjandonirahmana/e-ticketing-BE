use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first, exec_one, exec_rows};
use crate::models::merchant::{
    MerchantDetail, MerchantPublicProfile, MerchantReviewItem, MerchantReviewSummary,
};
use crate::utils::ulid::{bin_to_ulid, id_to_vec};

static MERCHANT_COLS: &str = r#"
    user_id,
    store_name,
    description,
    logo_url,
    verified,
    created_at,
    updated_at
"#;

static FIND_BY_USER_ID: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM merchant_details WHERE user_id = $1",
        MERCHANT_COLS
    )
});

static INSERT_MERCHANT: LazyLock<String> = LazyLock::new(|| {
    format!(
        "INSERT INTO merchant_details (user_id, store_name, description, logo_url) \
         VALUES ($1, $2, $3, $4) RETURNING {}",
        MERCHANT_COLS
    )
});

static UPDATE_MERCHANT: &str = r#"
    UPDATE merchant_details
       SET store_name  = COALESCE($2, store_name),
           description = COALESCE($3, description),
           logo_url    = COALESCE($4, logo_url)
     WHERE user_id = $1
"#;

#[async_trait]
pub trait MerchantRepository: Send + Sync {
    async fn find(&self, user_id: &str) -> Result<Option<MerchantDetail>>;

    async fn create(
        &self,
        user_id: &str,
        store_name: &str,
        description: Option<&str>,
        logo_url: &str,
    ) -> Result<MerchantDetail>;

    async fn update(
        &self,
        user_id: &str,
        store_name: Option<&str>,
        description: Option<&str>,
        logo_url: Option<&str>,
    ) -> Result<()>;

    // ── Profil publik + rating & follow ──────────────────────────────────────

    /// Profil merchant untuk halaman publik: info toko + agregat follower,
    /// jumlah event aktif, rata-rata & jumlah rating (satu query).
    async fn public_profile(&self, merchant_id: &str) -> Result<Option<MerchantPublicProfile>>;

    /// Ringkasan rating: avg, total, distribusi per bintang (FILTER agregat).
    async fn review_summary(&self, merchant_id: &str) -> Result<MerchantReviewSummary>;

    /// Daftar ulasan terbaru (join nama user), paginasi limit/offset.
    async fn list_reviews(
        &self,
        merchant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MerchantReviewItem>>;

    /// Simpan/perbarui ulasan user untuk merchant (satu ulasan per user).
    async fn upsert_review(
        &self,
        merchant_id: &str,
        user_id: &str,
        rating: i16,
        comment: &str,
    ) -> Result<()>;

    /// Follow (true) / unfollow (false). Follow hanya berlaku untuk target yang
    /// memang merchant (guard EXISTS di SQL — bukan sekadar percaya input).
    async fn set_follow(&self, merchant_id: &str, follower_id: &str, follow: bool) -> Result<()>;

    async fn is_following(&self, merchant_id: &str, follower_id: &str) -> Result<bool>;
}

#[derive(Clone)]
pub struct PgMerchantRepository {
    pool: Pool,
}

impl PgMerchantRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn row_to_merchant(row: &Row) -> Result<MerchantDetail> {
        let id_bytes: Vec<u8> = row.try_get("user_id").context("user_id")?;
        Ok(MerchantDetail {
            user_id: bin_to_ulid(id_bytes)?,
            store_name: row.try_get("store_name").context("store_name")?,
            description: row.try_get("description").context("description")?,
            logo_url: row.try_get("logo_url").context("logo_url")?,
            verified: row.try_get("verified").context("verified")?,
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
        })
    }
}

#[async_trait]
impl MerchantRepository for PgMerchantRepository {
    async fn find(&self, user_id: &str) -> Result<Option<MerchantDetail>> {
        let id_vec = id_to_vec(user_id)?;
        let row = exec_first(&self.pool, &FIND_BY_USER_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_merchant).transpose()
    }

    async fn create(
        &self,
        user_id: &str,
        store_name: &str,
        description: Option<&str>,
        logo_url: &str,
    ) -> Result<MerchantDetail> {
        let id_vec = id_to_vec(user_id)?;
        let row = exec_one(
            &self.pool,
            &INSERT_MERCHANT,
            &[&id_vec, &store_name, &description, &logo_url],
        )
        .await?;
        Self::row_to_merchant(&row)
    }

    async fn update(
        &self,
        user_id: &str,
        store_name: Option<&str>,
        description: Option<&str>,
        logo_url: Option<&str>,
    ) -> Result<()> {
        let id_vec = id_to_vec(user_id)?;
        exec_drop(
            &self.pool,
            UPDATE_MERCHANT,
            &[&id_vec, &store_name, &description, &logo_url],
        )
        .await?;
        Ok(())
    }

    // ── Profil publik + rating & follow ──────────────────────────────────────

    async fn public_profile(&self, merchant_id: &str) -> Result<Option<MerchantPublicProfile>> {
        let id_vec = id_to_vec(merchant_id)?;
        let row = exec_first(
            &self.pool,
            r#"
            SELECT
                md.user_id, md.store_name, md.description, md.logo_url, md.verified,
                (SELECT COUNT(*)::BIGINT FROM merchant_follows f
                  WHERE f.merchant_id = md.user_id)                          AS followers,
                (SELECT COUNT(*)::BIGINT FROM events e
                  WHERE e.merchant_id = md.user_id AND e.status = 'active')  AS events_count,
                COALESCE((SELECT AVG(r.rating)::FLOAT8 FROM reviews r
                  WHERE r.merchant_id = md.user_id), 0)                      AS rating_avg,
                (SELECT COUNT(*)::BIGINT FROM reviews r
                  WHERE r.merchant_id = md.user_id)                          AS rating_count
            FROM merchant_details md
            WHERE md.user_id = $1
            "#,
            &[&id_vec],
        )
        .await?;
        row.map(|r| {
            let id_bytes: Vec<u8> = r.try_get("user_id")?;
            Ok(MerchantPublicProfile {
                merchant_id: bin_to_ulid(id_bytes)?,
                store_name: r.try_get("store_name")?,
                description: r.try_get("description")?,
                logo_url: r.try_get("logo_url")?,
                verified: r.try_get("verified")?,
                followers: r.try_get("followers")?,
                events_count: r.try_get("events_count")?,
                rating_avg: r.try_get("rating_avg")?,
                rating_count: r.try_get("rating_count")?,
            })
        })
        .transpose()
    }

    async fn review_summary(&self, merchant_id: &str) -> Result<MerchantReviewSummary> {
        let id_vec = id_to_vec(merchant_id)?;
        let row = exec_one(
            &self.pool,
            r#"
            SELECT
                COALESCE(AVG(rating)::FLOAT8, 0)              AS avg,
                COUNT(*)::BIGINT                              AS total,
                COUNT(*) FILTER (WHERE rating = 1)::BIGINT    AS d1,
                COUNT(*) FILTER (WHERE rating = 2)::BIGINT    AS d2,
                COUNT(*) FILTER (WHERE rating = 3)::BIGINT    AS d3,
                COUNT(*) FILTER (WHERE rating = 4)::BIGINT    AS d4,
                COUNT(*) FILTER (WHERE rating = 5)::BIGINT    AS d5
            FROM reviews
            WHERE merchant_id = $1
            "#,
            &[&id_vec],
        )
        .await?;
        Ok(MerchantReviewSummary {
            avg: row.try_get("avg")?,
            total: row.try_get("total")?,
            dist: [
                row.try_get("d1")?,
                row.try_get("d2")?,
                row.try_get("d3")?,
                row.try_get("d4")?,
                row.try_get("d5")?,
            ],
        })
    }

    async fn list_reviews(
        &self,
        merchant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MerchantReviewItem>> {
        let id_vec = id_to_vec(merchant_id)?;
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT COALESCE(u.name, 'Pengguna') AS user_name,
                   r.rating::INT4               AS rating,
                   r.comment,
                   r.created_at
            FROM reviews r
            JOIN users u ON u.id = r.user_id
            WHERE r.merchant_id = $1
            ORDER BY r.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            &[&id_vec, &limit, &offset],
        )
        .await?;
        rows.iter()
            .map(|r| {
                Ok(MerchantReviewItem {
                    user_name: r.try_get("user_name")?,
                    rating: r.try_get("rating")?,
                    comment: r.try_get("comment")?,
                    created_at: r.try_get("created_at")?,
                })
            })
            .collect()
    }

    async fn upsert_review(
        &self,
        merchant_id: &str,
        user_id: &str,
        rating: i16,
        comment: &str,
    ) -> Result<()> {
        let mid = id_to_vec(merchant_id)?;
        let uid = id_to_vec(user_id)?;
        // Guard EXISTS: hanya bisa mengulas user yang memang merchant.
        exec_drop(
            &self.pool,
            r#"
            INSERT INTO reviews (merchant_id, user_id, rating, comment)
            SELECT $1, $2, $3, $4
            WHERE EXISTS (SELECT 1 FROM merchant_details WHERE user_id = $1)
            ON CONFLICT (merchant_id, user_id) DO UPDATE
                SET rating = EXCLUDED.rating,
                    comment = EXCLUDED.comment,
                    updated_at = NOW()
            "#,
            &[&mid, &uid, &rating, &comment],
        )
        .await?;
        Ok(())
    }

    async fn set_follow(&self, merchant_id: &str, follower_id: &str, follow: bool) -> Result<()> {
        let mid = id_to_vec(merchant_id)?;
        let fid = id_to_vec(follower_id)?;
        if follow {
            exec_drop(
                &self.pool,
                r#"
                INSERT INTO merchant_follows (merchant_id, follower_id)
                SELECT $1, $2
                WHERE EXISTS (SELECT 1 FROM merchant_details WHERE user_id = $1)
                ON CONFLICT DO NOTHING
                "#,
                &[&mid, &fid],
            )
            .await?;
        } else {
            exec_drop(
                &self.pool,
                "DELETE FROM merchant_follows WHERE merchant_id = $1 AND follower_id = $2",
                &[&mid, &fid],
            )
            .await?;
        }
        Ok(())
    }

    async fn is_following(&self, merchant_id: &str, follower_id: &str) -> Result<bool> {
        let mid = id_to_vec(merchant_id)?;
        let fid = id_to_vec(follower_id)?;
        let row = exec_first(
            &self.pool,
            "SELECT 1 AS x FROM merchant_follows WHERE merchant_id = $1 AND follower_id = $2",
            &[&mid, &fid],
        )
        .await?;
        Ok(row.is_some())
    }
}
