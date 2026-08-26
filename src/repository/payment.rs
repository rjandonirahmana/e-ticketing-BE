//! repository/payment.rs — kanal pembayaran & kode promo.
//!
//! Keduanya duduk di satu berkas karena selalu dipakai berbarengan: promo bisa
//! dibatasi pada kanal tertentu, dan biaya kanal dihitung dari nominal SETELAH
//! promo. Memisahkannya hanya akan membuat halaman checkout memanggil dua
//! service untuk satu keputusan.

use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use rust_decimal::Decimal;
use tokio_postgres::Row;

use super::db::{exec_first, exec_rows};
use crate::models::payment::{PaymentMethod, Promo};
use crate::utils::ulid::id_to_vec;

// ── SQL ──────────────────────────────────────────────────────────────────────

static METHOD_COLS: &str = "code, name, vendor, category, image_url, description, \
     charge, charge_percent, min_amount, max_amount, allow_promo, is_instant, \
     va_prefix, instruction, sort_order";

static LIST_METHODS: &str = "SELECT code, name, vendor, category, image_url, description, \
     charge, charge_percent, min_amount, max_amount, allow_promo, is_instant, \
     va_prefix, instruction, sort_order \
     FROM payment_methods \
     WHERE is_active AND deleted_at IS NULL \
     ORDER BY sort_order, name";

static FIND_METHOD: &str = "SELECT code, name, vendor, category, image_url, description, \
     charge, charge_percent, min_amount, max_amount, allow_promo, is_instant, \
     va_prefix, instruction, sort_order \
     FROM payment_methods \
     WHERE code = $1 AND is_active AND deleted_at IS NULL";

/// Pencocokan kode promo TIDAK peduli huruf besar-kecil — sesuai unique index
/// `uniq_promos_code` yang juga dibangun di atas `UPPER(code)`. Kalau salah satu
/// dari keduanya lupa, satu kuota bisa dipakai dua kali lewat beda kapitalisasi.
static FIND_PROMO: &str = r#"
    SELECT id, code, name, discount_type, amount, max_discount,
           min_cart_amount, max_cart_amount, min_qty, max_qty,
           quota_total, quota_used, per_user_limit, premium_only,
           payment_codes, starts_at, ends_at
      FROM promos
     WHERE UPPER(code) = UPPER($1)
       AND deleted_at IS NULL
       AND is_active
     LIMIT 1
"#;

static COUNT_USER_REDEMPTIONS: &str =
    "SELECT COUNT(*)::BIGINT AS total FROM promo_redemptions WHERE promo_id = $1 AND user_id = $2";

/// Ambil satu jatah kuota. Syarat kuota ada DI DALAM pernyataan yang sama dengan
/// penaikannya, jadi dua checkout serentak tak bisa sama-sama lolos memeriksa
/// lalu sama-sama menaikkan — yang kalah mendapat 0 baris.
static RESERVE_QUOTA: &str = r#"
    UPDATE promos
       SET quota_used = quota_used + 1, updated_at = NOW()
     WHERE id = $1
       AND (quota_total = 0 OR quota_used < quota_total)
    RETURNING id
"#;

/// Kembalikan jatah bila order batal lahir. `GREATEST(…, 0)` menjaga kolom tak
/// pernah negatif seandainya pengembalian terjadi dua kali.
static RELEASE_QUOTA: &str = r#"
    UPDATE promos
       SET quota_used = GREATEST(quota_used - 1, 0), updated_at = NOW()
     WHERE id = $1
"#;

static INSERT_REDEMPTION: &str = r#"
    INSERT INTO promo_redemptions (promo_id, user_id, order_id, discount_amount)
    VALUES ($1, $2, $3, $4)
    ON CONFLICT (order_id) DO NOTHING
"#;

// ── Trait ────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait PaymentRepository: Send + Sync {
    async fn list_methods(&self) -> Result<Vec<PaymentMethod>>;
    async fn find_method(&self, code: &str) -> Result<Option<PaymentMethod>>;

    async fn find_promo(&self, code: &str) -> Result<Option<Promo>>;
    async fn count_user_redemptions(&self, promo_id: i64, user_id: &str) -> Result<i64>;

    /// Ambil satu jatah kuota promo. `false` = kuota sudah habis.
    async fn reserve_promo_quota(&self, promo_id: i64) -> Result<bool>;
    async fn release_promo_quota(&self, promo_id: i64) -> Result<()>;
    async fn record_redemption(
        &self,
        promo_id: i64,
        user_id: &str,
        order_id: &str,
        discount: Decimal,
    ) -> Result<()>;
}

// ── Implementasi Postgres ────────────────────────────────────────────────────

pub struct PgPaymentRepository {
    pool: Pool,
}

impl PgPaymentRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

fn row_to_method(row: &Row) -> Result<PaymentMethod> {
    Ok(PaymentMethod {
        code: row.try_get("code").context("payment_methods.code")?,
        name: row.try_get("name").context("payment_methods.name")?,
        vendor: row.try_get("vendor").unwrap_or_default(),
        category: row.try_get("category").unwrap_or_default(),
        image_url: row.try_get("image_url").unwrap_or_default(),
        description: row.try_get("description").unwrap_or_default(),
        charge: row.try_get("charge").unwrap_or(0),
        charge_percent: row.try_get("charge_percent").unwrap_or(Decimal::ZERO),
        min_amount: row.try_get("min_amount").unwrap_or(0),
        max_amount: row.try_get("max_amount").unwrap_or(0),
        allow_promo: row.try_get("allow_promo").unwrap_or(true),
        is_instant: row.try_get("is_instant").unwrap_or(false),
        va_prefix: row.try_get("va_prefix").unwrap_or_default(),
        instruction: row.try_get("instruction").unwrap_or_default(),
        sort_order: row.try_get("sort_order").unwrap_or(0),
    })
}

fn row_to_promo(row: &Row) -> Result<Promo> {
    Ok(Promo {
        id: row.try_get("id").context("promos.id")?,
        code: row.try_get("code").context("promos.code")?,
        name: row.try_get("name").unwrap_or_default(),
        discount_type: row.try_get("discount_type").unwrap_or_else(|_| "fixed".into()),
        amount: row.try_get("amount").unwrap_or(Decimal::ZERO),
        max_discount: row.try_get("max_discount").unwrap_or(Decimal::ZERO),
        min_cart_amount: row.try_get("min_cart_amount").unwrap_or(Decimal::ZERO),
        max_cart_amount: row.try_get("max_cart_amount").unwrap_or(Decimal::ZERO),
        min_qty: row.try_get("min_qty").unwrap_or(0),
        max_qty: row.try_get("max_qty").unwrap_or(0),
        quota_total: row.try_get("quota_total").unwrap_or(0),
        quota_used: row.try_get("quota_used").unwrap_or(0),
        per_user_limit: row.try_get("per_user_limit").unwrap_or(0),
        premium_only: row.try_get("premium_only").unwrap_or(false),
        payment_codes: row.try_get("payment_codes").ok().flatten(),
        starts_at: row.try_get("starts_at").context("promos.starts_at")?,
        ends_at: row.try_get("ends_at").ok().flatten(),
    })
}

#[async_trait]
impl PaymentRepository for PgPaymentRepository {
    async fn list_methods(&self) -> Result<Vec<PaymentMethod>> {
        let _ = METHOD_COLS; // daftar kolom didokumentasikan di satu tempat
        let rows = exec_rows(&self.pool, LIST_METHODS, &[]).await?;
        rows.iter().map(row_to_method).collect()
    }

    async fn find_method(&self, code: &str) -> Result<Option<PaymentMethod>> {
        match exec_first(&self.pool, FIND_METHOD, &[&code]).await? {
            Some(row) => Ok(Some(row_to_method(&row)?)),
            None => Ok(None),
        }
    }

    async fn find_promo(&self, code: &str) -> Result<Option<Promo>> {
        match exec_first(&self.pool, FIND_PROMO, &[&code]).await? {
            Some(row) => Ok(Some(row_to_promo(&row)?)),
            None => Ok(None),
        }
    }

    async fn count_user_redemptions(&self, promo_id: i64, user_id: &str) -> Result<i64> {
        let uid = id_to_vec(user_id)?;
        match exec_first(&self.pool, COUNT_USER_REDEMPTIONS, &[&promo_id, &uid]).await? {
            Some(row) => Ok(row.try_get::<_, i64>("total").unwrap_or(0)),
            None => Ok(0),
        }
    }

    async fn reserve_promo_quota(&self, promo_id: i64) -> Result<bool> {
        let n = super::db::exec_drop(&self.pool, RESERVE_QUOTA, &[&promo_id]).await?;
        Ok(n > 0)
    }

    async fn release_promo_quota(&self, promo_id: i64) -> Result<()> {
        super::db::exec_drop(&self.pool, RELEASE_QUOTA, &[&promo_id]).await?;
        Ok(())
    }

    async fn record_redemption(
        &self,
        promo_id: i64,
        user_id: &str,
        order_id: &str,
        discount: Decimal,
    ) -> Result<()> {
        let uid = id_to_vec(user_id)?;
        let oid = id_to_vec(order_id)?;
        super::db::exec_drop(
            &self.pool,
            INSERT_REDEMPTION,
            &[&promo_id, &uid, &oid, &discount],
        )
        .await?;
        Ok(())
    }
}
