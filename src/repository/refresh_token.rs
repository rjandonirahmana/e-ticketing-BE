//! repository/refresh_token.rs — penyimpanan refresh token.
//!
//! Yang tersimpan di sini adalah SHA-256 dari token, bukan tokennya. Bocornya
//! isi tabel ini karena itu tidak memberi penyerang satu pun token yang bisa
//! dipakai — sama alasannya dengan tidak menyimpan kata sandi apa adanya.

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first};
use crate::utils::ulid::{bin_to_ulid, id_to_vec};

/// Satu baris refresh token, apa adanya dari database.
#[derive(Debug, Clone)]
pub struct RefreshTokenRow {
    pub id: String,
    pub user_id: String,
    pub family_id: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl RefreshTokenRow {
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }
}

static FIND_BY_HASH: &str = r#"
    SELECT id, user_id, family_id, expires_at, revoked_at
      FROM refresh_tokens
     WHERE token_hash = $1
"#;

static INSERT_TOKEN: &str = r#"
    INSERT INTO refresh_tokens
        (id, user_id, token_hash, family_id, expires_at, user_agent)
    VALUES ($1, $2, $3, $4, $5, $6)
"#;

/// Cabut satu token, sekaligus mencatat penggantinya untuk jejak rotasi.
///
/// `revoked_at IS NULL` di WHERE membuatnya idempoten DAN menjadi penentu
/// balapan: dua permintaan refresh dengan token yang sama secara bersamaan,
/// hanya satu yang mendapat baris — yang kalah tahu dirinya kalah.
static REVOKE_ONE: &str = r#"
    UPDATE refresh_tokens
       SET revoked_at = NOW(), replaced_by = $2
     WHERE id = $1 AND revoked_at IS NULL
"#;

/// Cabut SELURUH keluarga rotasi. Dipakai saat logout dan saat terdeteksi
/// token yang sudah dicabut dipakai ulang.
static REVOKE_FAMILY: &str = r#"
    UPDATE refresh_tokens
       SET revoked_at = NOW()
     WHERE family_id = $1 AND revoked_at IS NULL
"#;

static REVOKE_ALL_FOR_USER: &str = r#"
    UPDATE refresh_tokens
       SET revoked_at = NOW()
     WHERE user_id = $1 AND revoked_at IS NULL
"#;

/// Buang token yang sudah lewat masa berlakunya. Baris yang dicabut TETAP
/// disimpan sampai kedaluwarsa — itulah yang membuat deteksi pemakaian ulang
/// bekerja, karena token curian yang dicoba lagi masih ketemu barisnya.
static DELETE_EXPIRED: &str = "DELETE FROM refresh_tokens WHERE expires_at < NOW()";

#[async_trait]
pub trait RefreshTokenRepository: Send + Sync {
    async fn insert(
        &self,
        id: &str,
        user_id: &str,
        token_hash: &str,
        family_id: &str,
        expires_at: DateTime<Utc>,
        user_agent: &str,
    ) -> Result<()>;

    async fn find_by_hash(&self, token_hash: &str) -> Result<Option<RefreshTokenRow>>;

    /// `true` bila baris ini yang berhasil dicabut (belum dicabut sebelumnya).
    async fn revoke(&self, id: &str, replaced_by: Option<&str>) -> Result<bool>;

    async fn revoke_family(&self, family_id: &str) -> Result<u64>;
    async fn revoke_all_for_user(&self, user_id: &str) -> Result<u64>;
    async fn delete_expired(&self) -> Result<u64>;
}

pub struct PgRefreshTokenRepository {
    pool: Pool,
}

impl PgRefreshTokenRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

fn row_to_token(row: &Row) -> Result<RefreshTokenRow> {
    Ok(RefreshTokenRow {
        id: bin_to_ulid(row.try_get::<_, Vec<u8>>("id").context("refresh_tokens.id")?)?,
        user_id: bin_to_ulid(
            row.try_get::<_, Vec<u8>>("user_id")
                .context("refresh_tokens.user_id")?,
        )?,
        family_id: bin_to_ulid(
            row.try_get::<_, Vec<u8>>("family_id")
                .context("refresh_tokens.family_id")?,
        )?,
        expires_at: row.try_get("expires_at").context("refresh_tokens.expires_at")?,
        revoked_at: row.try_get("revoked_at").unwrap_or(None),
    })
}

#[async_trait]
impl RefreshTokenRepository for PgRefreshTokenRepository {
    async fn insert(
        &self,
        id: &str,
        user_id: &str,
        token_hash: &str,
        family_id: &str,
        expires_at: DateTime<Utc>,
        user_agent: &str,
    ) -> Result<()> {
        let id_b = id_to_vec(id)?;
        let uid = id_to_vec(user_id)?;
        let fam = id_to_vec(family_id)?;
        exec_drop(
            &self.pool,
            INSERT_TOKEN,
            &[&id_b, &uid, &token_hash, &fam, &expires_at, &user_agent],
        )
        .await?;
        Ok(())
    }

    async fn find_by_hash(&self, token_hash: &str) -> Result<Option<RefreshTokenRow>> {
        match exec_first(&self.pool, FIND_BY_HASH, &[&token_hash]).await? {
            Some(row) => Ok(Some(row_to_token(&row)?)),
            None => Ok(None),
        }
    }

    async fn revoke(&self, id: &str, replaced_by: Option<&str>) -> Result<bool> {
        let id_b = id_to_vec(id)?;
        let rep = replaced_by.map(id_to_vec).transpose()?;
        let n = exec_drop(&self.pool, REVOKE_ONE, &[&id_b, &rep]).await?;
        Ok(n > 0)
    }

    async fn revoke_family(&self, family_id: &str) -> Result<u64> {
        let fam = id_to_vec(family_id)?;
        exec_drop(&self.pool, REVOKE_FAMILY, &[&fam]).await
    }

    async fn revoke_all_for_user(&self, user_id: &str) -> Result<u64> {
        let uid = id_to_vec(user_id)?;
        exec_drop(&self.pool, REVOKE_ALL_FOR_USER, &[&uid]).await
    }

    async fn delete_expired(&self) -> Result<u64> {
        exec_drop(&self.pool, DELETE_EXPIRED, &[]).await
    }
}
