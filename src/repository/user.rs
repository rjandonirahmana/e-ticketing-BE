use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first, exec_one};
use crate::models::users::{RegisterRequest, User, UserRole};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

#[derive(Debug, Clone)]
pub struct UserWithPassword {
    pub user: User,
    pub password_hash: Option<String>,
}

// ── Static query strings ──────────────────────────────────────────────────────

static USER_COLS: &str = r#"
    id,
    email,
    password_hash,
    name,
    phone,
    role,
    created_at,
    updated_at
"#;

static FIND_BY_ID: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM users WHERE id = $1", USER_COLS));

static FIND_BY_PHONE: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM users WHERE phone = $1", USER_COLS));

static FIND_BY_EMAIL: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM users WHERE email = $1", USER_COLS));

static INSERT_USER: LazyLock<String> = LazyLock::new(|| {
    format!(
        "INSERT INTO users (id, email, password_hash, name, phone, role) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {}",
        USER_COLS
    )
});

static UPDATE_PROFILE: &str = r#"
    UPDATE users
       SET name  = COALESCE($2, name),
           phone = COALESCE($3, phone),
           -- Email BOLEH dikosongkan, jadi ia tak bisa memakai pola COALESCE
           -- seperti dua kolom di atas: `NULL` di sana berarti "jangan ubah",
           -- sehingga tak ada cara menyatakan "hapus email saya".
           --
           -- `$4` adalah penanda ubah-atau-tidak, `$5` nilainya. Dengan begitu
           -- "kosongkan" (ubah=true, nilai=NULL) dan "biarkan" (ubah=false)
           -- menjadi dua hal yang berbeda.
           email = CASE WHEN $4 THEN $5 ELSE email END
     WHERE id = $1
"#;

static UPDATE_PASSWORD_HASH: &str = r#"
    UPDATE users
       SET password_hash = $2
     WHERE id = $1
"#;

// ── Trait ─────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn create(
        &self,
        req: &RegisterRequest,
        password_hash: Option<&str>,
        role: UserRole,
    ) -> Result<User>;

    async fn find_by_id(&self, id: &str) -> Result<Option<User>>;

    async fn find_by_email_with_password(&self, email: &str) -> Result<Option<UserWithPassword>>;

    /// `email`: `None` = jangan sentuh kolomnya; `Some(None)` = kosongkan;
    /// `Some(Some(v))` = isi dengan `v`. Dua lapis `Option` itu perlu karena
    /// email boleh dikosongkan — lihat catatan pada `UPDATE_PROFILE`.
    async fn update_profile(
        &self,
        id: &str,
        name: Option<&str>,
        phone: Option<&str>,
        email: Option<Option<&str>>,
    ) -> Result<()>;

    async fn update_password_hash(&self, id: &str, new_hash: &str) -> Result<()>;

    async fn find_by_phone(&self, phone: &str) -> Result<Option<User>>;
    async fn find_by_phone_with_password(&self, email: &str) -> Result<Option<UserWithPassword>>;
}

// ── Postgres impl ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PgUserRepository {
    pool: Pool,
}

impl PgUserRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn row_to_user(row: &Row) -> Result<User> {
        let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
        let role_str: String = row.try_get("role").context("role")?;
        Ok(User {
            id: bin_to_ulid(id_bytes)?,
            email: row.try_get("email").context("email")?,
            name: row.try_get("name").context("name")?,
            phone: row.try_get("phone").context("phone")?,
            role: UserRole::from(role_str.as_str()),
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
        })
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn create(
        &self,
        req: &RegisterRequest,
        password_hash: Option<&str>,
        role: UserRole,
    ) -> Result<User> {
        let id = new_ulid();
        let id_vec = ulid_to_vec(&id)?;
        let role_str = role.to_string();

        // email dari RegisterRequest adalah Option<String>
        let email: Option<&str> = req.email.as_deref();

        let row = exec_one(
            &self.pool,
            &INSERT_USER,
            &[
                &id_vec,
                &email,         // Option<&str> → NULL jika None
                &password_hash, // Option<&str> → NULL jika None
                &req.name,
                &req.phone,
                &role_str,
            ],
        )
        .await?;

        // Gunakan row dari RETURNING — bukan konstruksi manual
        Self::row_to_user(&row)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<User>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_user).transpose()
    }

    async fn find_by_email_with_password(&self, email: &str) -> Result<Option<UserWithPassword>> {
        let row = exec_first(&self.pool, &FIND_BY_EMAIL, &[&email]).await?;
        let Some(row) = row else { return Ok(None) };
        let user = Self::row_to_user(&row)?;
        let password_hash: Option<String> =
            row.try_get("password_hash").context("password_hash")?;
        Ok(Some(UserWithPassword {
            user,
            password_hash,
        }))
    }

    async fn find_by_phone(&self, phone: &str) -> Result<Option<User>> {
        let row = exec_first(&self.pool, &FIND_BY_PHONE, &[&phone]).await?;
        row.as_ref().map(Self::row_to_user).transpose()
    }

    async fn find_by_phone_with_password(&self, phone: &str) -> Result<Option<UserWithPassword>> {
        let row = exec_first(&self.pool, &FIND_BY_PHONE, &[&phone]).await?;
        let Some(row) = row else { return Ok(None) };
        let user = Self::row_to_user(&row)?;
        let password_hash: Option<String> =
            row.try_get("password_hash").context("password_hash")?;
        Ok(Some(UserWithPassword {
            user,
            password_hash,
        }))
    }

    async fn update_profile(
        &self,
        id: &str,
        name: Option<&str>,
        phone: Option<&str>,
        email: Option<Option<&str>>,
    ) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        let ubah_email = email.is_some();
        let nilai_email: Option<&str> = email.flatten();
        exec_drop(
            &self.pool,
            UPDATE_PROFILE,
            &[&id_vec, &name, &phone, &ubah_email, &nilai_email],
        )
        .await?;
        Ok(())
    }

    async fn update_password_hash(&self, id: &str, new_hash: &str) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        exec_drop(&self.pool, UPDATE_PASSWORD_HASH, &[&id_vec, &new_hash]).await?;
        Ok(())
    }
}
