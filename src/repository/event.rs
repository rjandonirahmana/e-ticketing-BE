use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first, exec_one, exec_rows, get_conn};
use crate::models::event_variants::{EventVariant, EventVariantJson, cmp_by_effective_price};
use crate::models::events::{CreateEventRequest, CreateVariantInline, Event, UpdateEventRequest};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

// ── Slug generator ────────────────────────────────────────────────────────────

/// Buat slug dari merchant_name + event_name + 6‑digit hex random.
/// Rentang 16^6 = 16 juta → praktis bebas tabrakan.
/// Caller wajib retry jika constraint unique tetap gagal.
fn generate_slug(merchant_name: &str, event_name: &str) -> String {
    let slugify = |s: &str| -> String {
        s.chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    };

    let m = slugify(merchant_name);
    let e = slugify(event_name);
    let suffix = rand::random::<u32>() & 0xFF_FFFF; // 24 bit → 6 hex chars
    let max_body = 155 - 7; // 6 hex + 1 dash
    let body = format!("{}-{}", m, e);
    let body = if body.len() > max_body {
        &body[..max_body]
    } else {
        &body
    };
    let body = body.trim_end_matches('-');
    format!("{}-{:06x}", body, suffix)
}

/// True jika anyhow error berasal dari PostgreSQL unique constraint (23505).
fn is_unique_violation(e: &anyhow::Error) -> bool {
    e.to_string().contains("23505")
}

// ── LATERAL subquery: agregasi + harga termurah dari variant aktif ─────────────
//
// Kolom yang diekspos oleh alias "vs":
//   total_sold, total_quota,
//   min_price        — base price variant termurah (by effective_price)
//   min_sale_price   — sale_price variant tsb (NULL jika tidak ada sale aktif)
//   min_sale_start   — sale_price_start_date variant tsb
//   min_sale_end     — sale_price_end_date variant tsb
//   display_price    — min_sale_price jika aktif, else min_price
static VARIANT_STATS_LATERAL: &str = r#"
    LEFT JOIN LATERAL (
        SELECT
            agg.total_sold,
            agg.total_quota,
            best.price            AS min_price,
            best.sale_price       AS min_sale_price,
            best.sale_start       AS min_sale_start,
            best.sale_end         AS min_sale_end,
            CASE
                WHEN best.sale_price IS NOT NULL
                    AND NOW() BETWEEN
                        COALESCE(best.sale_start, '-infinity'::timestamptz)
                    AND COALESCE(best.sale_end,   'infinity'::timestamptz)
                THEN best.sale_price
                ELSE best.price
            END AS display_price
        FROM (
            SELECT
                COALESCE(SUM(sold)::INT,  0) AS total_sold,
                COALESCE(SUM(quota)::INT, 0) AS total_quota
            FROM event_variants
            WHERE event_id = e.id AND is_active = true
        ) agg
        LEFT JOIN LATERAL (
            SELECT
                price::FLOAT8             AS price,
                sale_price::FLOAT8        AS sale_price,
                sale_price_start_date     AS sale_start,
                sale_price_end_date       AS sale_end
            FROM event_variants
            WHERE event_id = e.id AND is_active = true
            ORDER BY (
                CASE
                    WHEN sale_price IS NOT NULL
                        AND NOW() BETWEEN
                            COALESCE(sale_price_start_date, '-infinity'::timestamptz)
                        AND COALESCE(sale_price_end_date,   'infinity'::timestamptz)
                    THEN sale_price::FLOAT8
                    ELSE price::FLOAT8
                END
            ) ASC
            LIMIT 1
        ) best ON true
    ) vs ON true
"#;

// ── Kolom SELECT ──────────────────────────────────────────────────────────────

static EVENT_COLS: &str = r#"
    e.id,
    e.merchant_id,
    e.name,
    e.slug,
    e.description,
    e.cover_url,
    COALESCE(vs.min_price, 0.0)     AS price,
    vs.min_sale_price                AS sale_price,
    vs.min_sale_start                AS sale_price_start_date,
    vs.min_sale_end                  AS sale_price_end_date,
    COALESCE(vs.display_price, 0.0)  AS display_price,
    e.venue,
    e.city,
    e.event_date,
    e.start_time,
    e.end_time,
    e.status,
    e.created_at,
    e.updated_at,
    e.category,
    COALESCE(vs.total_sold,  0)      AS total_sold,
    COALESCE(vs.total_quota, 0)      AS total_quota
"#;

/// Kolom tanpa price/aggregasi — untuk find_by_*_with_variants,
/// price diisi dari variants di Rust.
static EVENT_COLS_NO_AGG: &str = r#"
    e.id,
    e.merchant_id,
    e.name,
    e.slug,
    e.description,
    e.cover_url,
    e.venue,
    e.city,
    e.event_date,
    e.start_time,
    e.end_time,
    e.status,
    e.created_at,
    e.updated_at,
    e.category
"#;

// ── Fragment jsonb_agg variant — dipakai di dua query ────────────────────────

static VARIANTS_JSONB_AGG: &str = r#"
    COALESCE(
        (SELECT jsonb_agg(
            jsonb_build_object(
                'id',                    encode(v.id, 'hex'),
                'event_id',              encode(v.event_id, 'hex'),
                'name',                  v.name,
                'description',           v.description,
                'price',                 v.price::FLOAT8,
                'sale_price',            v.sale_price::FLOAT8,
                'sale_price_start_date', v.sale_price_start_date::date,
                'sale_price_end_date',   v.sale_price_end_date::date,
                'quota',                 v.quota,
                'sold',                  v.sold,
                'max_per_order',         v.max_per_order,
                'is_active',             v.is_active,
                'sort_order',            v.sort_order,
                'created_at',            v.created_at,
                'updated_at',            v.updated_at
            )
            ORDER BY v.sort_order ASC, v.created_at ASC
        )
        FROM event_variants v
        WHERE v.event_id = e.id AND v.is_active = true),
        '[]'::jsonb
    ) AS variants_json
"#;

// ── Static queries ────────────────────────────────────────────────────────────

static FIND_EVENT_BY_ID: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {cols} FROM events e {lateral} WHERE e.id = $1",
        cols = EVENT_COLS,
        lateral = VARIANT_STATS_LATERAL,
    )
});

static FIND_EVENT_WITH_VARIANTS_BY_SLUG: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {cols}, {agg} FROM events e WHERE e.slug = $1",
        cols = EVENT_COLS_NO_AGG,
        agg = VARIANTS_JSONB_AGG,
    )
});

/// Versi find_by_id_with_variants — satu query tanpa double round‑trip.
static FIND_EVENT_WITH_VARIANTS_BY_ID: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {cols}, {agg} FROM events e WHERE e.id = $1",
        cols = EVENT_COLS_NO_AGG,
        agg = VARIANTS_JSONB_AGG,
    )
});

static INSERT_EVENT: &str = "INSERT INTO events \
     (id, merchant_id, name, slug, description, cover_url, price, venue, city, \
      event_date, start_time, end_time, category) \
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)";

static UPDATE_EVENT: &str = r#"
    UPDATE events
       SET name        = COALESCE($3,  name),
           description = COALESCE($4,  description),
           cover_url   = COALESCE($5,  cover_url),
           venue       = COALESCE($6,  venue),
           city        = COALESCE($7,  city),
           event_date  = COALESCE($8,  event_date),
           start_time  = COALESCE($9,  start_time),
           end_time    = COALESCE($10, end_time),
           status      = COALESCE($11, status),
           category    = COALESCE($12, category)
     WHERE id = $1 AND merchant_id = $2
"#;

static DELETE_EVENT: &str = "DELETE FROM events WHERE id = $1 AND merchant_id = $2";

// ── Variant queries ───────────────────────────────────────────────────────────

static VARIANT_COLS: &str = r#"
    id,
    event_id,
    name,
    description,
    price::FLOAT8               AS price,
    sale_price::FLOAT8          AS sale_price,
    sale_price_start_date,
    sale_price_end_date,
    quota,
    sold,
    max_per_order,
    is_active,
    sort_order,
    created_at,
    updated_at
"#;

static FIND_VARIANT_BY_ID: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM event_variants WHERE id = $1", VARIANT_COLS));

// Jumlah kolom per baris pada batch insert variant
const VARIANT_INSERT_COLS: usize = 11;

// UPDATE variant: $1 id, $2 merchant_id (lewat JOIN events),
// $3 name, $4 description, $5 price, $6 sale_price,
// $7 sale_price_start_date, $8 sale_price_end_date,
// $9 quota, $10 max_per_order, $11 is_active, $12 sort_order
static UPDATE_VARIANT: &str = r#"
    UPDATE event_variants v
       SET name                  = COALESCE($3,  v.name),
           description           = COALESCE($4,  v.description),
           price                 = COALESCE($5,  v.price),
           sale_price            = COALESCE($6,  v.sale_price),
           sale_price_start_date = COALESCE($7,  v.sale_price_start_date),
           sale_price_end_date   = COALESCE($8,  v.sale_price_end_date),
           quota                 = COALESCE($9,  v.quota),
           max_per_order         = COALESCE($10, v.max_per_order),
           is_active             = COALESCE($11, v.is_active),
           sort_order            = COALESCE($12, v.sort_order)
      FROM events e
     WHERE v.id = $1
       AND v.event_id = e.id
       AND e.merchant_id = $2
"#;

static DELETE_VARIANT: &str = r#"
    DELETE FROM event_variants v
    USING events e
    WHERE v.id = $1
      AND v.event_id = e.id
      AND e.merchant_id = $2
"#;

// ── Filter ────────────────────────────────────────────────────────────────────

pub struct EventListFilter<'a> {
    pub city: Option<&'a str>,
    pub status: Option<&'a str>,
    pub category: Option<&'a str>,
    pub search: Option<&'a str>,
    pub merchant_id: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

// ── Trait ─────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait EventRepository: Send + Sync {
    async fn list(&self, f: &EventListFilter<'_>) -> Result<Vec<Event>>;
    async fn count(&self, f: &EventListFilter<'_>) -> Result<i64>;
    async fn find_by_id(&self, id: &str) -> Result<Option<Event>>;
    async fn find_by_id_with_variants(
        &self,
        id: &str,
    ) -> Result<Option<(Event, Vec<EventVariant>)>>;
    async fn list_categories(&self) -> Result<Vec<String>>;
    async fn find_by_slug_with_variants(
        &self,
        slug: &str,
    ) -> Result<Option<(Event, Vec<EventVariant>)>>;
    async fn create(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        req: &CreateEventRequest,
        cover_url: Option<&str>,
    ) -> Result<Event>;
    async fn create_variants_bulk(
        &self,
        event_id: &str,
        variants: &[CreateVariantInline],
    ) -> Result<Vec<EventVariant>>;
    async fn update(&self, id: &str, merchant_id: &str, req: &UpdateEventRequest) -> Result<()>;
    async fn delete(&self, id: &str, merchant_id: &str) -> Result<()>;
    async fn find_variant(&self, id: &str) -> Result<Option<EventVariant>>;
    async fn update_variant(
        &self,
        id: &str,
        merchant_id: &str,
        name: Option<&str>,
        description: Option<&str>,
        price: Option<f64>,
        sale_price: Option<f64>,
        sale_price_start_date: Option<chrono::DateTime<chrono::Utc>>,
        sale_price_end_date: Option<chrono::DateTime<chrono::Utc>>,
        quota: Option<i32>,
        max_per_order: Option<i32>,
        is_active: Option<bool>,
        sort_order: Option<i32>,
    ) -> Result<()>;
    async fn delete_variant(&self, id: &str, merchant_id: &str) -> Result<()>;
}

// ── Postgres impl ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PgEventRepository {
    pool: Pool,
}

impl PgEventRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    fn row_to_event(row: &Row) -> Result<Event> {
        let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
        let merchant_bytes: Vec<u8> = row.try_get("merchant_id").context("merchant_id")?;
        let category_json: Option<serde_json::Value> = row.try_get("category")?;
        let category = match category_json {
            Some(json) => serde_json::from_value(json).unwrap_or_default(),
            None => Vec::new(),
        };
        Ok(Event {
            id: bin_to_ulid(id_bytes)?,
            merchant_id: bin_to_ulid(merchant_bytes)?,
            name: row.try_get("name").context("name")?,
            slug: row.try_get("slug").unwrap_or_default(),
            category,
            description: row.try_get("description").context("description")?,
            cover_url: row.try_get("cover_url").unwrap_or(None),
            price: row.try_get::<_, f64>("price").unwrap_or(0.0),
            sale_price: row.try_get("sale_price").ok().flatten(),
            sale_price_start_date: row.try_get("sale_price_start_date").ok().flatten(),
            sale_price_end_date: row.try_get("sale_price_end_date").ok().flatten(),
            display_price: row.try_get::<_, f64>("display_price").unwrap_or(0.0),
            venue: row.try_get("venue").context("venue")?,
            city: row.try_get("city").context("city")?,
            event_date: row.try_get("event_date").context("event_date")?,
            start_time: row.try_get("start_time")?,
            end_time: row.try_get("end_time")?,
            status: row.try_get("status").context("status")?,
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
            total_sold: row.try_get("total_sold").unwrap_or(0),
            total_quota: row.try_get("total_quota").unwrap_or(0),
        })
    }

    /// Row → Event tanpa kolom price/aggregasi dari DB.
    /// Semua price fields diisi 0/None dan di‑override dari variants nanti.
    fn row_to_event_no_agg(row: &Row) -> Result<Event> {
        let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
        let merchant_bytes: Vec<u8> = row.try_get("merchant_id").context("merchant_id")?;
        let category_json: Option<serde_json::Value> = row.try_get("category")?;
        let category = match category_json {
            Some(json) => serde_json::from_value(json).unwrap_or_default(),
            None => Vec::new(),
        };
        Ok(Event {
            id: bin_to_ulid(id_bytes)?,
            merchant_id: bin_to_ulid(merchant_bytes)?,
            name: row.try_get("name").context("name")?,
            slug: row.try_get("slug").unwrap_or_default(),
            category,
            description: row.try_get("description").context("description")?,
            cover_url: row.try_get("cover_url").unwrap_or(None),
            // Akan diisi dari cheapest variant di apply_cheapest_variant_price()
            price: 0.0,
            sale_price: None,
            sale_price_start_date: None,
            sale_price_end_date: None,
            display_price: 0.0,
            venue: row.try_get("venue").context("venue")?,
            city: row.try_get("city").context("city")?,
            event_date: row.try_get("event_date").context("event_date")?,
            start_time: row.try_get("start_time")?,
            end_time: row.try_get("end_time")?,
            status: row.try_get("status").context("status")?,
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
            total_sold: 0,
            total_quota: 0,
        })
    }

    fn row_to_variant(row: &Row) -> Result<EventVariant> {
        let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
        let event_bytes: Vec<u8> = row.try_get("event_id").context("event_id")?;
        Ok(EventVariant {
            id: bin_to_ulid(id_bytes)?,
            event_id: bin_to_ulid(event_bytes)?,
            name: row.try_get("name").context("name")?,
            description: row.try_get("description").context("description")?,
            price: row.try_get("price").context("price")?,
            sale_price: row.try_get("sale_price").context("sale_price")?,
            sale_price_start_date: row.try_get("sale_price_start_date")?,
            sale_price_end_date: row.try_get("sale_price_end_date")?,
            quota: row.try_get("quota").context("quota")?,
            sold: row.try_get("sold").context("sold")?,
            max_per_order: row.try_get("max_per_order")?,
            is_active: row.try_get("is_active").context("is_active")?,
            sort_order: row.try_get("sort_order").context("sort_order")?,
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
        })
    }

    /// Isi price fields event dari variant aktif dengan effective_price terendah.
    fn apply_cheapest_variant_price(event: &mut Event, variants: &[EventVariant]) {
        if let Some(cheapest) = variants.iter().min_by(|a, b| cmp_by_effective_price(a, b)) {
            event.price = cheapest.price;
            event.sale_price = cheapest.sale_price;
            event.sale_price_start_date = cheapest.sale_price_start_date;
            event.sale_price_end_date = cheapest.sale_price_end_date;
            event.display_price = cheapest.effective_price();
        }
        event.total_sold = variants.iter().map(|v| v.sold).sum();
        event.total_quota = variants.iter().map(|v| v.quota).sum();
    }

    /// Deserialize variants_json → Vec<EventVariant> dari satu row.
    fn parse_variants_json(row: &Row) -> Result<Vec<EventVariant>> {
        let variants_json: serde_json::Value = row.try_get("variants_json")?;
        serde_json::from_value::<Vec<EventVariantJson>>(variants_json)
            .context("deserialize variants_json")?
            .into_iter()
            .map(EventVariantJson::into_variant)
            .collect::<Result<_>>()
    }
}

#[async_trait]
impl EventRepository for PgEventRepository {
    async fn list(&self, f: &EventListFilter<'_>) -> Result<Vec<Event>> {
        let mut sql = format!(
            "SELECT {cols} FROM events e {lateral} WHERE 1 = 1",
            cols = EVENT_COLS,
            lateral = VARIANT_STATS_LATERAL,
        );
        let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
        let mut idx = 1usize;

        let mid_vec;
        if let Some(mid) = f.merchant_id {
            mid_vec = id_to_vec(mid)?;
            sql.push_str(&format!(" AND e.merchant_id = ${idx}"));
            params.push(Box::new(mid_vec));
            idx += 1;
        }
        if let Some(city) = f.city {
            sql.push_str(&format!(" AND e.city ILIKE ${idx}"));
            params.push(Box::new(format!("%{}%", city)));
            idx += 1;
        }
        if let Some(status) = f.status {
            sql.push_str(&format!(" AND e.status = ${idx}"));
            params.push(Box::new(status.to_string()));
            idx += 1;
        }
        if let Some(cat) = f.category {
            let json_val = serde_json::to_value(vec![cat])?;
            sql.push_str(&format!(" AND e.category @> ${idx}::jsonb"));
            params.push(Box::new(json_val));
            idx += 1;
        }
        if let Some(q) = f.search {
            let pattern = format!("%{}%", q);
            sql.push_str(&format!(
                " AND (e.name ILIKE ${idx} OR e.venue ILIKE ${idx} OR e.city ILIKE ${idx})"
            ));
            params.push(Box::new(pattern));
            idx += 1;
        }
        sql.push_str(&format!(
            " ORDER BY e.event_date ASC LIMIT ${} OFFSET ${}",
            idx,
            idx + 1
        ));
        params.push(Box::new(f.limit));
        params.push(Box::new(f.offset));

        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(|p| p.as_ref() as _).collect();
        let rows = exec_rows(&self.pool, &sql, &refs).await?;
        rows.iter().map(Self::row_to_event).collect()
    }

    async fn count(&self, f: &EventListFilter<'_>) -> Result<i64> {
        let mut sql = String::from("SELECT COUNT(*)::BIGINT AS c FROM events WHERE 1 = 1");
        let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
        let mut idx = 1usize;

        if let Some(mid) = f.merchant_id {
            let mid_vec = id_to_vec(mid)?;
            sql.push_str(&format!(" AND merchant_id = ${idx}"));
            params.push(Box::new(mid_vec));
            idx += 1;
        }
        if let Some(city) = f.city {
            sql.push_str(&format!(" AND city ILIKE ${idx}"));
            params.push(Box::new(format!("%{}%", city)));
            idx += 1;
        }
        if let Some(status) = f.status {
            sql.push_str(&format!(" AND status = ${idx}"));
            params.push(Box::new(status.to_string()));
            idx += 1;
        }
        if let Some(cat) = f.category {
            let json_val = serde_json::to_value(vec![cat])?;
            sql.push_str(&format!(" AND category @> ${idx}::jsonb"));
            params.push(Box::new(json_val));
            idx += 1;
        }
        if let Some(q) = f.search {
            let pattern = format!("%{}%", q);
            sql.push_str(&format!(
                " AND (name ILIKE ${idx} OR venue ILIKE ${idx} OR city ILIKE ${idx})"
            ));
            params.push(Box::new(pattern));
            idx += 1; // konsisten dengan list()
        }
        let _ = idx;

        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(|p| p.as_ref() as _).collect();
        let row = exec_one(&self.pool, &sql, &refs).await?;
        Ok(row.try_get::<_, i64>("c")?)
    }

    async fn list_categories(&self) -> Result<Vec<String>> {
        let rows = exec_rows(
            &self.pool,
            r#"
            SELECT DISTINCT jsonb_array_elements_text(category) AS cat
            FROM events
            WHERE status = 'active'
              AND category IS NOT NULL
              AND jsonb_array_length(category) > 0
            ORDER BY cat ASC
            "#,
            &[],
        )
        .await?;
        Ok(rows
            .iter()
            .filter_map(|r| r.try_get::<_, String>("cat").ok())
            .filter(|s| !s.is_empty())
            .collect())
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Event>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_EVENT_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_event).transpose()
    }

    /// Satu query — tidak ada double round‑trip ke DB.
    async fn find_by_id_with_variants(
        &self,
        id: &str,
    ) -> Result<Option<(Event, Vec<EventVariant>)>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_EVENT_WITH_VARIANTS_BY_ID, &[&id_vec]).await?;

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let mut event = Self::row_to_event_no_agg(&row)?;
        let variants = Self::parse_variants_json(&row)?;
        Self::apply_cheapest_variant_price(&mut event, &variants);
        Ok(Some((event, variants)))
    }

    async fn find_by_slug_with_variants(
        &self,
        slug: &str,
    ) -> Result<Option<(Event, Vec<EventVariant>)>> {
        let row = exec_first(&self.pool, &FIND_EVENT_WITH_VARIANTS_BY_SLUG, &[&slug]).await?;

        let row = match row {
            Some(r) => r,
            None => return Ok(None),
        };

        let mut event = Self::row_to_event_no_agg(&row)?;
        let variants = Self::parse_variants_json(&row)?;
        Self::apply_cheapest_variant_price(&mut event, &variants);
        Ok(Some((event, variants)))
    }

    /// Insert event, retry sampai 5x jika slug collision (23505).
    async fn create(
        &self,
        merchant_id: &str,
        merchant_name: &str,
        req: &CreateEventRequest,
        cover_url: Option<&str>,
    ) -> Result<Event> {
        let id = new_ulid();
        let id_vec = ulid_to_vec(&id)?;
        let mid_vec = id_to_vec(merchant_id)?;
        let category_json = serde_json::to_value(&req.category)?;

        let mut last_err = anyhow::anyhow!("slug generation failed after max retries");
        for _ in 0..5u8 {
            let slug = generate_slug(merchant_name, &req.name);
            let result = exec_drop(
                &self.pool,
                INSERT_EVENT,
                &[
                    &id_vec,          // $1  id
                    &mid_vec,         // $2  merchant_id
                    &req.name,        // $3  name
                    &slug,            // $4  slug
                    &req.description, // $5  description
                    &cover_url,       // $6  cover_url
                    &0i64,            // $7  price (placeholder — derived from variants)
                    &req.venue,       // $8  venue
                    &req.city,        // $9  city
                    &req.event_date,  // $10 event_date
                    &req.start_time,  // $11 start_time
                    &req.end_time,    // $12 end_time
                    &category_json,   // $13 category
                ],
            )
            .await;

            match result {
                Ok(_) => {
                    return self
                        .find_by_id(&id)
                        .await?
                        .ok_or_else(|| anyhow::anyhow!("event not found after insert: {}", id));
                }
                Err(e) if is_unique_violation(&e) => {
                    last_err = e;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err)
    }

    /// Batch insert semua variants dalam satu query.
    /// Menggunakan satu prepared statement dengan VALUES (...),(...) agar
    /// tidak ada O(n) round‑trip ke database.
    async fn create_variants_bulk(
        &self,
        event_id: &str,
        variants: &[CreateVariantInline],
    ) -> Result<Vec<EventVariant>> {
        if variants.is_empty() {
            return Ok(Vec::new());
        }

        let event_vec = id_to_vec(event_id)?;

        // Siapkan semua id + param bertipe Box<dyn ToSql + Sync + Send>
        // agar lifetime-nya 'static sehingga bisa di-collect ke slice.
        type BoxParam = Box<dyn tokio_postgres::types::ToSql + Sync + Send>;
        let mut ids: Vec<Vec<u8>> = Vec::with_capacity(variants.len());
        for _ in 0..variants.len() {
            let id = new_ulid();
            ids.push(ulid_to_vec(&id)?);
        }

        // Bangun VALUES clause: ($1,$2,...$11),($12,$13,...$22),...
        let cols = VARIANT_INSERT_COLS;
        let mut value_clauses = Vec::with_capacity(variants.len());
        for i in 0..variants.len() {
            let base = i * cols + 1;
            let placeholders: Vec<String> = (base..base + cols).map(|n| format!("${n}")).collect();
            value_clauses.push(format!("({})", placeholders.join(",")));
        }

        let sql = format!(
            "INSERT INTO event_variants \
             (id, event_id, name, description, price, sale_price, sale_price_start_date, \
              sale_price_end_date, quota, max_per_order, sort_order) \
             VALUES {} \
             RETURNING {}",
            value_clauses.join(","),
            VARIANT_COLS,
        );

        // Kumpulkan semua params dalam urutan yang sama dengan placeholders
        let mut params: Vec<BoxParam> = Vec::with_capacity(variants.len() * cols);
        for (i, v) in variants.iter().enumerate() {
            let sort_order = v.sort_order.unwrap_or(i as i32);
            params.push(Box::new(ids[i].clone())); // id
            params.push(Box::new(event_vec.clone())); // event_id
            params.push(Box::new(v.name.clone())); // name
            params.push(Box::new(v.description.clone())); // description
            params.push(Box::new(v.price)); // price
            params.push(Box::new(v.sale_price)); // sale_price
            params.push(Box::new(v.sale_price_start_date)); // sale_price_start_date
            params.push(Box::new(v.sale_price_end_date)); // sale_price_end_date
            params.push(Box::new(v.quota)); // quota
            params.push(Box::new(v.max_per_order)); // max_per_order
            params.push(Box::new(sort_order)); // sort_order
        }

        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(|p| p.as_ref() as _).collect();
        let rows = exec_rows(&self.pool, &sql, &refs).await?;
        rows.iter().map(Self::row_to_variant).collect()
    }

    async fn update(&self, id: &str, merchant_id: &str, req: &UpdateEventRequest) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        let merchant_id_vec = id_to_vec(merchant_id)?;
        let category_json = if req.category.is_empty() {
            None
        } else {
            Some(serde_json::to_value(&req.category)?)
        };

        exec_drop(
            &self.pool,
            UPDATE_EVENT,
            &[
                &id_vec,          // $1
                &merchant_id_vec, // $2
                &req.name,        // $3
                &req.description, // $4
                &req.cover_url,   // $5
                &req.venue,       // $6
                &req.city,        // $7
                &req.event_date,  // $8
                &req.start_time,  // $9
                &req.end_time,    // $10
                &req.status,      // $11
                &category_json,   // $12
            ],
        )
        .await?;
        Ok(())
    }

    async fn delete(&self, id: &str, merchant_id: &str) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        let mid_vec = id_to_vec(merchant_id)?;
        exec_drop(&self.pool, DELETE_EVENT, &[&id_vec, &mid_vec]).await?;
        Ok(())
    }

    async fn find_variant(&self, id: &str) -> Result<Option<EventVariant>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_VARIANT_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_variant).transpose()
    }

    async fn update_variant(
        &self,
        id: &str,
        merchant_id: &str,
        name: Option<&str>,
        description: Option<&str>,
        price: Option<f64>,
        sale_price: Option<f64>,
        sale_price_start_date: Option<chrono::DateTime<chrono::Utc>>,
        sale_price_end_date: Option<chrono::DateTime<chrono::Utc>>,
        quota: Option<i32>,
        max_per_order: Option<i32>,
        is_active: Option<bool>,
        sort_order: Option<i32>,
    ) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        let merchant_id_vec = id_to_vec(merchant_id)?;
        exec_drop(
            &self.pool,
            UPDATE_VARIANT,
            &[
                &id_vec,                // $1
                &merchant_id_vec,       // $2
                &name,                  // $3
                &description,           // $4
                &price,                 // $5
                &sale_price,            // $6
                &sale_price_start_date, // $7
                &sale_price_end_date,   // $8
                &quota,                 // $9
                &max_per_order,         // $10
                &is_active,             // $11
                &sort_order,            // $12
            ],
        )
        .await?;
        Ok(())
    }

    async fn delete_variant(&self, id: &str, merchant_id: &str) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        let mid_vec = id_to_vec(merchant_id)?;
        exec_drop(&self.pool, DELETE_VARIANT, &[&id_vec, &mid_vec]).await?;
        Ok(())
    }
}
