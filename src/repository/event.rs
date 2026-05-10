use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first, exec_one, exec_rows};
use crate::models::event_variants::{cmp_by_effective_price, EventVariant, EventVariantJson};
use crate::models::events::{CreateEventRequest, CreateVariantInline, Event, UpdateEventRequest};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

// ── Slug generator ────────────────────────────────────────────────────────────

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
    let suffix = rand::random::<u32>() & 0xFF_FFFF;
    let max_body = 155 - 7;
    let body = format!("{}-{}", m, e);
    let body = if body.len() > max_body {
        &body[..max_body]
    } else {
        &body
    };
    let body = body.trim_end_matches('-');
    format!("{}-{:06x}", body, suffix)
}

fn is_unique_violation(e: &anyhow::Error) -> bool {
    e.to_string().contains("23505")
}

// ── LATERAL subquery ──────────────────────────────────────────────────────────

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
    e.detail_images,
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

static EVENT_COLS_NO_AGG: &str = r#"
    e.id,
    e.merchant_id,
    e.name,
    e.slug,
    e.description,
    e.cover_url,
    e.detail_images,
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

static ADMIN_UPDATE_EVENT_STATUS: &str = r#"
    UPDATE events
       SET status = $2
     WHERE id = $1
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

static FIND_EVENT_WITH_VARIANTS_BY_ID: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {cols}, {agg} FROM events e WHERE e.id = $1",
        cols = EVENT_COLS_NO_AGG,
        agg = VARIANTS_JSONB_AGG,
    )
});

// $13 category dan $15 detail_images adalah jsonb — tokio-postgres serialise
// serde_json::Value langsung ke jsonb tanpa perlu ::jsonb cast di SQL.
static INSERT_EVENT: &str = "\
    INSERT INTO events \
        (id, merchant_id, name, slug, description, cover_url, price, venue, city, \
         event_date, start_time, end_time, category, status, detail_images) \
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)";

static UPDATE_EVENT: &str = r#"
    UPDATE events
       SET name          = COALESCE($3,  name),
           description   = COALESCE($4,  description),
           cover_url     = COALESCE($5,  cover_url),
           venue         = COALESCE($6,  venue),
           city          = COALESCE($7,  city),
           event_date    = COALESCE($8,  event_date),
           start_time    = COALESCE($9,  start_time),
           end_time      = COALESCE($10, end_time),
           status        = COALESCE($11, status),
           category      = COALESCE($12, category),
           detail_images = COALESCE($13, detail_images)
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

const VARIANT_INSERT_COLS: usize = 11;

static UPDATE_VARIANT: &str = r#"
    UPDATE event_variants v
       SET name                  = COALESCE($3,                     v.name),
           description           = COALESCE($4,                     v.description),
           price                 = COALESCE(($5::float8)::numeric,  v.price),
           sale_price            = COALESCE(($6::float8)::numeric,  v.sale_price),
           sale_price_start_date = COALESCE($7,                     v.sale_price_start_date),
           sale_price_end_date   = COALESCE($8,                     v.sale_price_end_date),
           quota                 = COALESCE($9,                     v.quota),
           max_per_order         = COALESCE($10,                    v.max_per_order),
           is_active             = COALESCE($11,                    v.is_active),
           sort_order            = COALESCE($12,                    v.sort_order)
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
    async fn admin_list_by_status(&self, f: &EventListFilter<'_>) -> Result<Vec<Event>>;
    async fn admin_count_by_status(&self, f: &EventListFilter<'_>) -> Result<i64>;
    async fn admin_update_status(&self, id: &str, status: &str) -> Result<()>;
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
        let detail_images_json: Option<serde_json::Value> =
            row.try_get("detail_images").ok().flatten();
        let detail_images = match detail_images_json {
            Some(json) => serde_json::from_value(json).unwrap_or_default(),
            None => Vec::new(),
        };
        Ok(Event {
            id: bin_to_ulid(id_bytes)?,
            merchant_id: bin_to_ulid(merchant_bytes)?,
            name: row.try_get("name").context("name")?,
            slug: row.try_get("slug").unwrap_or_default(),
            category,
            detail_images,
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

    fn row_to_event_no_agg(row: &Row) -> Result<Event> {
        let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
        let merchant_bytes: Vec<u8> = row.try_get("merchant_id").context("merchant_id")?;
        let category_json: Option<serde_json::Value> = row.try_get("category")?;
        let category = match category_json {
            Some(json) => serde_json::from_value(json).unwrap_or_default(),
            None => Vec::new(),
        };
        let detail_images_json: Option<serde_json::Value> =
            row.try_get("detail_images").ok().flatten();
        let detail_images = match detail_images_json {
            Some(json) => serde_json::from_value(json).unwrap_or_default(),
            None => Vec::new(),
        };
        Ok(Event {
            id: bin_to_ulid(id_bytes)?,
            merchant_id: bin_to_ulid(merchant_bytes)?,
            name: row.try_get("name").context("name")?,
            slug: row.try_get("slug").unwrap_or_default(),
            category,
            detail_images,
            description: row.try_get("description").context("description")?,
            cover_url: row.try_get("cover_url").unwrap_or(None),
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
            is_active: row.try_get::<_, Option<bool>>("is_active")?.unwrap_or(true),
            sort_order: row.try_get::<_, Option<i32>>("sort_order")?.unwrap_or(0),
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
        })
    }

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
        // OPTIMISASI: semua nilai filter dialokasikan sebagai owned values di scope ini,
        // lalu dikumpulkan sebagai &dyn ToSql references — tidak ada Box<dyn ToSql>.
        // Dynamic SQL dipertahankan (vs static nullable params) agar PostgreSQL bisa
        // cache plan yang berbeda per kombinasi filter aktif — lebih optimal dari
        // satu plan generic yang handle semua kombinasi via IS NULL check.
        let mut sql = format!(
            "SELECT {cols} FROM events e {lateral} WHERE 1 = 1",
            cols = EVENT_COLS,
            lateral = VARIANT_STATS_LATERAL,
        );
        // Max 7 params: merchant_id, city, status, category, search, limit, offset
        let mut refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::with_capacity(7);
        let mut idx = 1usize;

        // Owned values hidup di scope ini — refs ke bawah valid sampai akhir fn
        let mid_vec = f.merchant_id.map(id_to_vec).transpose()?;
        let city_pat = f.city.map(|c| format!("%{}%", c));
        let status_own = f.status.map(|s| s.to_string());
        let cat_json = f
            .category
            .map(|c| serde_json::to_value(vec![c]))
            .transpose()?;
        let search_pat = f.search.map(|q| format!("%{}%", q));

        if let Some(ref v) = mid_vec {
            sql.push_str(&format!(" AND e.merchant_id = ${idx}"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = city_pat {
            sql.push_str(&format!(" AND e.city ILIKE ${idx}"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = status_own {
            sql.push_str(&format!(" AND e.status = ${idx}"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = cat_json {
            sql.push_str(&format!(" AND e.category @> ${idx}::jsonb"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = search_pat {
            sql.push_str(&format!(
                " AND (e.name ILIKE ${idx} OR e.venue ILIKE ${idx} OR e.city ILIKE ${idx})"
            ));
            refs.push(v);
            idx += 1;
        }
        sql.push_str(&format!(
            " ORDER BY e.event_date ASC LIMIT ${} OFFSET ${}",
            idx,
            idx + 1
        ));
        refs.push(&f.limit);
        refs.push(&f.offset);

        let rows = exec_rows(&self.pool, &sql, &refs).await?;
        rows.iter().map(Self::row_to_event).collect()
    }

    async fn count(&self, f: &EventListFilter<'_>) -> Result<i64> {
        let mut sql = String::from("SELECT COUNT(*)::BIGINT AS c FROM events WHERE 1 = 1");
        let mut refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::with_capacity(5);
        let mut idx = 1usize;

        let mid_vec = f.merchant_id.map(id_to_vec).transpose()?;
        let city_pat = f.city.map(|c| format!("%{}%", c));
        let status_own = f.status.map(|s| s.to_string());
        let cat_json = f
            .category
            .map(|c| serde_json::to_value(vec![c]))
            .transpose()?;
        let search_pat = f.search.map(|q| format!("%{}%", q));

        if let Some(ref v) = mid_vec {
            sql.push_str(&format!(" AND merchant_id = ${idx}"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = city_pat {
            sql.push_str(&format!(" AND city ILIKE ${idx}"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = status_own {
            sql.push_str(&format!(" AND status = ${idx}"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = cat_json {
            sql.push_str(&format!(" AND category @> ${idx}::jsonb"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = search_pat {
            sql.push_str(&format!(
                " AND (name ILIKE ${idx} OR venue ILIKE ${idx} OR city ILIKE ${idx})"
            ));
            refs.push(v);
            idx += 1;
        }
        let _ = idx;

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

        // serde_json::Value impl ToSql untuk kolom jsonb — tidak perlu ::jsonb cast di SQL
        let category_json = serde_json::to_value(&req.category)?;
        let detail_images_json = serde_json::to_value(&req.detail_images)?;

        // cover_url: Option<&str> — langsung impl ToSql, tidak perlu .to_string()
        // req.description / venue / city: Option<String> — juga langsung impl ToSql

        let mut last_err = anyhow::anyhow!("slug generation failed after max retries");
        for _ in 0..5u8 {
            let slug = generate_slug(merchant_name, &req.name);
            let result = exec_drop(
                &self.pool,
                INSERT_EVENT,
                &[
                    &id_vec,             // $1  bytea
                    &mid_vec,            // $2  bytea
                    &req.name,           // $3  varchar        — String impl ToSql
                    &slug,               // $4  varchar        — String impl ToSql
                    &req.description,    // $5  text NULL      — Option<String> impl ToSql
                    &cover_url,          // $6  text NULL      — Option<&str> impl ToSql
                    &0f64,               // $7  numeric        — f64 impl ToSql
                    &req.venue,          // $8  varchar NULL   — Option<String> impl ToSql
                    &req.city,           // $9  varchar NULL   — Option<String> impl ToSql
                    &req.event_date,     // $10 timestamptz   — DateTime<Utc> impl ToSql
                    &req.start_time,     // $11 timestamptz   — Option<DateTime<Utc>> impl ToSql
                    &req.end_time,       // $12 timestamptz   — Option<DateTime<Utc>> impl ToSql
                    &category_json,      // $13 jsonb         — Value impl ToSql
                    &"edited",           // $14 varchar       — &str impl ToSql
                    &detail_images_json, // $15 jsonb         — Value impl ToSql
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

    async fn create_variants_bulk(
        &self,
        event_id: &str,
        variants: &[CreateVariantInline],
    ) -> Result<Vec<EventVariant>> {
        if variants.is_empty() {
            return Ok(Vec::new());
        }

        let event_vec = id_to_vec(event_id)?;

        // Box<dyn ToSql> di sini dipertahankan karena jumlah params bersifat dinamis
        // (variants.len() × VARIANT_INSERT_COLS). Tidak ada cara simpan &references ke
        // nilai dalam Vec yang tumbuh tanpa lifetime conflict. Ini acceptable karena
        // create_variants_bulk hanya dipanggil saat merchant publish event — bukan hot path.
        // Alternatif lebih baik jangka panjang: refactor ke UNNEST($1::bytea[], $2::text[], ...)
        // yang eliminasi semua per-row alloc sekaligus, tapi butuh schema change params.
        type BoxParam = Box<dyn tokio_postgres::types::ToSql + Sync + Send>;
        let mut ids: Vec<Vec<u8>> = Vec::with_capacity(variants.len());
        for _ in 0..variants.len() {
            let id = new_ulid();
            ids.push(ulid_to_vec(&id)?);
        }

        let cols = VARIANT_INSERT_COLS;
        let mut value_clauses = Vec::with_capacity(variants.len());
        for i in 0..variants.len() {
            let base = i * cols + 1;
            let placeholders: Vec<String> = (base..base + cols)
                .enumerate()
                .map(|(offset, n)| {
                    if offset == 4 || offset == 5 {
                        format!("(${n}::float8)::numeric")
                    } else {
                        format!("${n}")
                    }
                })
                .collect();
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

        let mut params: Vec<BoxParam> = Vec::with_capacity(variants.len() * cols);
        for (i, v) in variants.iter().enumerate() {
            let sort_order = v.sort_order.unwrap_or(i as i32);
            params.push(Box::new(ids[i].clone()));
            params.push(Box::new(event_vec.clone()));
            params.push(Box::new(v.name.clone()));
            params.push(Box::new(v.description.clone()));
            params.push(Box::new(v.price));
            params.push(Box::new(v.sale_price));
            params.push(Box::new(v.sale_price_start_date));
            params.push(Box::new(v.sale_price_end_date));
            params.push(Box::new(v.quota));
            params.push(Box::new(v.max_per_order));
            params.push(Box::new(sort_order));
        }

        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(|p| p.as_ref() as _).collect();
        let rows = exec_rows(&self.pool, &sql, &refs).await?;
        rows.iter().map(Self::row_to_variant).collect()
    }

    async fn update(&self, id: &str, merchant_id: &str, req: &UpdateEventRequest) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        let merchant_id_vec = id_to_vec(merchant_id)?;

        // None → COALESCE pakai nilai lama di DB (tidak berubah)
        let category_json: Option<serde_json::Value> = if req.category.is_empty() {
            None
        } else {
            Some(serde_json::to_value(&req.category)?)
        };
        let detail_images_json: Option<serde_json::Value> = req
            .detail_images
            .as_ref()
            .map(|di| serde_json::to_value(di))
            .transpose()?;

        exec_drop(
            &self.pool,
            UPDATE_EVENT,
            &[
                &id_vec,             // $1  bytea
                &merchant_id_vec,    // $2  bytea
                &req.name,           // $3  Option<String>      impl ToSql
                &req.description,    // $4  Option<String>      impl ToSql
                &req.cover_url,      // $5  Option<String>      impl ToSql
                &req.venue,          // $6  Option<String>      impl ToSql
                &req.city,           // $7  Option<String>      impl ToSql
                &req.event_date,     // $8  Option<DateTime>    impl ToSql
                &req.start_time,     // $9  Option<DateTime>    impl ToSql
                &req.end_time,       // $10 Option<DateTime>    impl ToSql
                &"edited",           // $11 &str               — merchant update → always "edited"
                &category_json,      // $12 Option<Value>       impl ToSql
                &detail_images_json, // $13 Option<Value>       impl ToSql
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

        // name/description sudah Option<&str> — langsung impl ToSql, tidak perlu .to_string()
        exec_drop(
            &self.pool,
            UPDATE_VARIANT,
            &[
                &id_vec,                // $1
                &merchant_id_vec,       // $2
                &name,                  // $3  Option<&str>  impl ToSql
                &description,           // $4  Option<&str>  impl ToSql
                &price,                 // $5  Option<f64>   impl ToSql
                &sale_price,            // $6  Option<f64>   impl ToSql
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

    async fn admin_list_by_status(&self, f: &EventListFilter<'_>) -> Result<Vec<Event>> {
        let mut sql = format!(
            "SELECT {cols} FROM events e {lateral} WHERE 1 = 1",
            cols = EVENT_COLS,
            lateral = VARIANT_STATS_LATERAL,
        );
        let mut refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::with_capacity(6);
        let mut idx = 1usize;

        let city_pat = f.city.map(|c| format!("%{}%", c));
        let status_own = f.status.map(|s| s.to_string());
        let cat_json = f
            .category
            .map(|c| serde_json::to_value(vec![c]))
            .transpose()?;
        let search_pat = f.search.map(|q| format!("%{}%", q));

        if let Some(ref v) = city_pat {
            sql.push_str(&format!(" AND e.city ILIKE ${idx}"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = status_own {
            sql.push_str(&format!(" AND e.status = ${idx}"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = cat_json {
            sql.push_str(&format!(" AND e.category @> ${idx}::jsonb"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = search_pat {
            sql.push_str(&format!(
                " AND (e.name ILIKE ${idx} OR e.venue ILIKE ${idx} OR e.city ILIKE ${idx})"
            ));
            refs.push(v);
            idx += 1;
        }
        sql.push_str(&format!(
            " ORDER BY e.updated_at DESC LIMIT ${} OFFSET ${}",
            idx,
            idx + 1
        ));
        refs.push(&f.limit);
        refs.push(&f.offset);

        let rows = exec_rows(&self.pool, &sql, &refs).await?;
        rows.iter().map(Self::row_to_event).collect()
    }

    async fn admin_count_by_status(&self, f: &EventListFilter<'_>) -> Result<i64> {
        let mut sql = String::from("SELECT COUNT(*)::BIGINT AS c FROM events WHERE 1 = 1");
        let mut refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = Vec::with_capacity(4);
        let mut idx = 1usize;

        let city_pat = f.city.map(|c| format!("%{}%", c));
        let status_own = f.status.map(|s| s.to_string());
        let cat_json = f
            .category
            .map(|c| serde_json::to_value(vec![c]))
            .transpose()?;
        let search_pat = f.search.map(|q| format!("%{}%", q));

        if let Some(ref v) = city_pat {
            sql.push_str(&format!(" AND city ILIKE ${idx}"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = status_own {
            sql.push_str(&format!(" AND status = ${idx}"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = cat_json {
            sql.push_str(&format!(" AND category @> ${idx}::jsonb"));
            refs.push(v);
            idx += 1;
        }
        if let Some(ref v) = search_pat {
            sql.push_str(&format!(
                " AND (name ILIKE ${idx} OR venue ILIKE ${idx} OR city ILIKE ${idx})"
            ));
            refs.push(v);
            idx += 1;
        }
        let _ = idx;

        let row = exec_one(&self.pool, &sql, &refs).await?;
        Ok(row.try_get::<_, i64>("c")?)
    }

    async fn admin_update_status(&self, id: &str, status: &str) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        exec_drop(&self.pool, ADMIN_UPDATE_EVENT_STATUS, &[&id_vec, &status]).await?;
        Ok(())
    }
}
