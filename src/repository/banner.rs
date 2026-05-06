//! repository/banner.rs
//!
//! Generic trait `BannerRepository` + concrete `PgBannerRepository`.
//! Keuntungan generics: BannerService<R> di-monomorphize saat kompilasi →
//! zero virtual-dispatch overhead, inlining penuh, binary lebih kecil.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first, exec_one, exec_rows};
use crate::{
    models::banners::{Banner, CreateBannerRequest, UpdateBannerRequest},
    utils::ulid::{bin_to_ulid, id_to_vec},
};

// ── Column list ───────────────────────────────────────────────────────────────

const BANNER_COLS: &str =
    "id, image_url, click_url, start_date, end_date, deleted_at, event_id, created_at, updated_at";

// ── Row mapper ────────────────────────────────────────────────────────────────

fn row_to_banner(row: &Row) -> Result<Banner> {
    // id adalah bigserial (i64) — bukan bytea/ULID
    let id: i64 = row.try_get("id")?;
    // event_id adalah bytea ULID (FK ke events) — bisa NULL
    let event_bytes: Option<Vec<u8>> = row.try_get("event_id")?;
    Ok(Banner {
        id,
        image_url: row.try_get("image_url")?,
        click_url: row.try_get("click_url")?,
        start_date: row.try_get("start_date")?,
        end_date: row.try_get("end_date")?,
        deleted_at: row.try_get("deleted_at")?,
        event_id: event_bytes.map(bin_to_ulid).transpose()?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

// ── Trait ─────────────────────────────────────────────────────────────────────

/// Abstraksi akses data untuk tabel `banners`.
/// Implementasi: `PgBannerRepository` (production), mock (unit test).
#[async_trait]
pub trait BannerRepository: Send + Sync {
    // ── Public API ────────────────────────────────────────────────────────────

    /// Daftar banner aktif pada waktu `now`:
    /// `deleted_at IS NULL` AND `start_date <= now`
    ///  AND (`end_date IS NULL` OR `end_date >= now`).
    async fn list_active(&self, now: DateTime<Utc>, event_id: Option<&str>) -> Result<Vec<Banner>>;

    /// Ambil satu banner berdasarkan id bigserial — termasuk yang sudah di-soft-delete.
    async fn find_by_id(&self, id: i64) -> Result<Option<Banner>>;

    // ── Admin API ─────────────────────────────────────────────────────────────

    /// Buat banner baru. `image_url` sudah final (hasil upload atau dari body).
    async fn create(&self, image_url: &str, req: &CreateBannerRequest) -> Result<Banner>;

    /// Update banner yang belum di-soft-delete.
    /// `image_url` adalah override URL (hasil upload baru) — None = pakai req.image_url
    /// atau tetap value di DB lewat COALESCE.
    /// Untuk `event_id`: Some("ulid") = ganti, None = biarkan.
    async fn update(
        &self,
        id: i64,
        image_url: Option<&str>,
        req: &UpdateBannerRequest,
    ) -> Result<Option<Banner>>;

    /// Soft-delete: set `deleted_at = now()`.
    /// Mengembalikan `true` jika banner ditemukan dan di-delete, `false` jika tidak ada.
    async fn soft_delete(&self, id: i64) -> Result<bool>;
}

// ── Postgres implementation ───────────────────────────────────────────────────

pub struct PgBannerRepository {
    pool: Pool,
}

impl PgBannerRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BannerRepository for PgBannerRepository {
    // ── list_active ───────────────────────────────────────────────────────────

    async fn list_active(&self, now: DateTime<Utc>, event_id: Option<&str>) -> Result<Vec<Banner>> {
        // Bangun query dinamis — event_id filter opsional
        let mut sql = format!(
            "SELECT {BANNER_COLS} FROM public.banners
             WHERE deleted_at IS NULL
               AND start_date <= $1
               AND (end_date IS NULL OR end_date >= $1)"
        );

        // Konversi ULID string → bytea hanya jika diperlukan
        let eid_bytes: Option<Vec<u8>> = event_id.map(id_to_vec).transpose()?;

        let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&now];
        if let Some(ref bytes) = eid_bytes {
            sql.push_str(" AND event_id = $2");
            params.push(bytes);
        }
        sql.push_str(" ORDER BY start_date DESC, created_at DESC");

        let rows = exec_rows(&self.pool, &sql, &params).await?;
        rows.iter().map(row_to_banner).collect()
    }

    // ── find_by_id ────────────────────────────────────────────────────────────

    async fn find_by_id(&self, id: i64) -> Result<Option<Banner>> {
        let sql = format!("SELECT {BANNER_COLS} FROM public.banners WHERE id = $1");
        let row = exec_first(&self.pool, &sql, &[&id]).await?;
        row.as_ref().map(row_to_banner).transpose()
    }

    // ── create ────────────────────────────────────────────────────────────────

    async fn create(&self, image_url: &str, req: &CreateBannerRequest) -> Result<Banner> {
        // Konversi event_id ULID → bytea jika disertakan
        let eid_bytes: Option<Vec<u8>> = req.event_id.as_deref().map(id_to_vec).transpose()?;

        let sql = format!(
            "INSERT INTO public.banners (image_url, click_url, start_date, end_date, event_id)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING {BANNER_COLS}"
        );
        let row = exec_one(
            &self.pool,
            &sql,
            &[
                &image_url,      // $1
                &req.click_url,  // $2
                &req.start_date, // $3
                &req.end_date,   // $4
                &eid_bytes,      // $5 — None → NULL
            ],
        )
        .await?;
        row_to_banner(&row)
    }

    // ── update ────────────────────────────────────────────────────────────────

    async fn update(
        &self,
        id: i64,
        image_url: Option<&str>,
        req: &UpdateBannerRequest,
    ) -> Result<Option<Banner>> {
        // Prioritas image_url: upload baru > req.image_url > biarkan DB
        let effective_url: Option<&str> = image_url.or(req.image_url.as_deref());

        // event_id: jika Some(ulid) = update, None = biarkan existing
        let eid_bytes: Option<Vec<u8>> = req.event_id.as_deref().map(id_to_vec).transpose()?;
        let update_event_id = req.event_id.is_some();

        let sql = format!(
            "UPDATE public.banners
             SET image_url  = COALESCE($2, image_url),
                 click_url  = COALESCE($3, click_url),
                 start_date = COALESCE($4, start_date),
                 end_date   = COALESCE($5, end_date),
                 event_id   = CASE WHEN $6 THEN $7 ELSE event_id END,
                 updated_at = now()
             WHERE id = $1 AND deleted_at IS NULL
             RETURNING {BANNER_COLS}"
        );
        let row = exec_first(
            &self.pool,
            &sql,
            &[
                &id,              // $1
                &effective_url,   // $2 — COALESCE: None → tetap lama
                &req.click_url,   // $3
                &req.start_date,  // $4
                &req.end_date,    // $5
                &update_event_id, // $6 — apakah event_id di-update?
                &eid_bytes,       // $7 — value baru (atau NULL jika tidak di-update)
            ],
        )
        .await?;
        row.as_ref().map(row_to_banner).transpose()
    }

    // ── soft_delete ───────────────────────────────────────────────────────────

    async fn soft_delete(&self, id: i64) -> Result<bool> {
        let affected = exec_drop(
            &self.pool,
            "UPDATE public.banners SET deleted_at = now() WHERE id = $1 AND deleted_at IS NULL",
            &[&id],
        )
        .await?;
        Ok(affected > 0)
    }
}
