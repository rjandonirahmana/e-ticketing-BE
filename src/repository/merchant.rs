use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first, exec_one};
use crate::models::merchant::MerchantDetail;
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
}
