use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_drop, exec_first, exec_one, exec_rows};
use crate::models::event_variants::EventVariant;
use crate::models::events::{CreateEventRequest, CreateVariantInline, Event, UpdateEventRequest};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

/// Generate slug dari merchant_name + event_name + 3 digit random.
/// Format: `{merchant-slug}-{event-slug}-{NNN}`
/// Contoh: `toko-maju-konser-malam-mingguan-042`
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

    // 3 digit random: 0-999
    let suffix = rand::random::<u16>() % 1000;

    // Potong agar total slug tidak lebih dari 155 karakter (slug field VARCHAR(160))
    let max_body = 155 - 4; // 4 = "-NNN"
    let body = format!("{}-{}", m, e);
    let body = if body.len() > max_body {
        &body[..max_body]
    } else {
        &body
    };
    let body = body.trim_end_matches('-');

    format!("{}-{:03}", body, suffix)
}

static EVENT_COLS: &str = r#"
    id,
    merchant_id,
    name,
    slug,
    description,
    cover_url,
    price::FLOAT8 AS price,
    sale_price::FLOAT8 AS sale_price,
    sale_price_start_date,
    sale_price_end_date,
    venue,
    city,
    event_date,
    start_time,
    end_time,
    status,
    created_at,
    updated_at
"#;

static FIND_EVENT_BY_ID: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM events WHERE id = $1", EVENT_COLS));

static FIND_EVENT_BY_SLUG: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM events WHERE slug = $1", EVENT_COLS));

/// Satu query JOIN — event + semua variantnya sekaligus by slug.
/// Baris event parent duplikat per variant (LEFT JOIN), di-collapse di Rust.
static FIND_EVENT_WITH_VARIANTS_BY_SLUG: LazyLock<String> = LazyLock::new(|| {
    String::from(
        r#"
        SELECT
            e.id                        AS e_id,
            e.merchant_id               AS e_merchant_id,
            e.name                      AS e_name,
            e.slug                      AS e_slug,
            e.description               AS e_description,
            e.cover_url                 AS e_cover_url,
            e.price::FLOAT8             AS e_price,
            e.sale_price::FLOAT8        AS e_sale_price,
            e.sale_price_start_date     AS e_sale_price_start_date,
            e.sale_price_end_date       AS e_sale_price_end_date,
            e.venue                     AS e_venue,
            e.city                      AS e_city,
            e.event_date                AS e_event_date,
            e.start_time                AS e_start_time,
            e.end_time                  AS e_end_time,
            e.status                    AS e_status,
            e.created_at                AS e_created_at,
            e.updated_at                AS e_updated_at,
            v.id                        AS v_id,
            v.event_id                  AS v_event_id,
            v.name                      AS v_name,
            v.description               AS v_description,
            v.price::FLOAT8             AS v_price,
            v.sale_price::FLOAT8        AS v_sale_price,
            v.sale_price_start_date     AS v_sale_price_start_date,
            v.sale_price_end_date       AS v_sale_price_end_date,
            v.quota                     AS v_quota,
            v.sold                      AS v_sold,
            v.max_per_order             AS v_max_per_order,
            v.is_active                 AS v_is_active,
            v.sort_order                AS v_sort_order,
            v.created_at                AS v_created_at,
            v.updated_at                AS v_updated_at
        FROM event_variants v
        JOIN events e ON v.event_id = e.id
        WHERE e.slug = $1
        ORDER BY v.sort_order ASC, v.created_at ASC
    "#,
    )
});

static INSERT_EVENT: LazyLock<String> = LazyLock::new(|| {
    format!(
        "INSERT INTO events (id, merchant_id, name, slug, description, cover_url, price, venue, city, \
         event_date, start_time, end_time) \
         VALUES ($1, $2, $3, $4, $5, $6, 0, $7, $8, $9, $10, $11) RETURNING {}",
        EVENT_COLS
    )
});

static UPDATE_EVENT: &str = r#"
    UPDATE events
       SET name        = COALESCE($2, name),
           description = COALESCE($3, description),
           cover_url   = COALESCE($4, cover_url),
           venue       = COALESCE($5, venue),
           city        = COALESCE($6, city),
           event_date  = COALESCE($7, event_date),
           start_time  = COALESCE($8, start_time),
           end_time    = COALESCE($9, end_time),
           status      = COALESCE($10, status)
     WHERE id = $1
"#;

static DELETE_EVENT: &str = "DELETE FROM events WHERE id = $1";

// Variant queries
static VARIANT_COLS: &str = r#"
    id,
    event_id,
    name,
    description,
    price::FLOAT8 AS price,
    sale_price::FLOAT8 AS sale_price,
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

static FIND_VARIANTS_BY_EVENT: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM event_variants WHERE event_id = $1 ORDER BY sort_order, created_at",
        VARIANT_COLS
    )
});

static FIND_VARIANT_BY_ID: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM event_variants WHERE id = $1", VARIANT_COLS));

static INSERT_VARIANT: LazyLock<String> = LazyLock::new(|| {
    format!(
        "INSERT INTO event_variants (id, event_id, name, description, price, quota, \
         max_per_order, sort_order) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {}",
        VARIANT_COLS
    )
});

static UPDATE_VARIANT: &str = r#"
    UPDATE event_variants
       SET name          = COALESCE($2, name),
           description   = COALESCE($3, description),
           price         = COALESCE($4, price),
           quota         = COALESCE($5, quota),
           max_per_order = COALESCE($6, max_per_order),
           is_active     = COALESCE($7, is_active),
           sort_order    = COALESCE($8, sort_order)
     WHERE id = $1
"#;

static DELETE_VARIANT: &str = "DELETE FROM event_variants WHERE id = $1";

// ── Trait ─────────────────────────────────────────────────────────────────────

pub struct EventListFilter<'a> {
    pub city: Option<&'a str>,
    pub status: Option<&'a str>,
    pub merchant_id: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

#[async_trait]
pub trait EventRepository: Send + Sync {
    // Events
    async fn list(&self, f: &EventListFilter<'_>) -> Result<Vec<Event>>;
    async fn count(&self, f: &EventListFilter<'_>) -> Result<i64>;
    async fn find_by_id(&self, id: &str) -> Result<Option<Event>>;
    async fn find_by_slug(&self, slug: &str) -> Result<Option<Event>>;
    /// Satu JOIN query — return event + variants, tidak ada round-trip kedua.
    async fn find_by_id_with_variants(
        &self,
        id: &str,
    ) -> Result<Option<(Event, Vec<EventVariant>)>>;
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
    async fn update(&self, id: &str, req: &UpdateEventRequest) -> Result<()>;
    async fn delete(&self, id: &str) -> Result<()>;

    // Ticket variants
    async fn list_variants(&self, event_id: &str) -> Result<Vec<EventVariant>>;
    async fn find_variant(&self, id: &str) -> Result<Option<EventVariant>>;
    async fn create_variant(
        &self,
        event_id: &str,
        name: &str,
        description: Option<&str>,
        price: f64,
        quota: i32,
        max_per_order: Option<i32>,
        sort_order: i32,
    ) -> Result<EventVariant>;
    async fn update_variant(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        price: Option<f64>,
        quota: Option<i32>,
        max_per_order: Option<i32>,
        is_active: Option<bool>,
        sort_order: Option<i32>,
    ) -> Result<()>;
    async fn delete_variant(&self, id: &str) -> Result<()>;
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
        Ok(Event {
            id: bin_to_ulid(id_bytes)?,
            merchant_id: bin_to_ulid(merchant_bytes)?,
            name: row.try_get("name").context("name")?,
            slug: row.try_get("slug").unwrap_or_default(),
            description: row.try_get("description").context("description")?,
            cover_url: row.try_get("cover_url").unwrap_or(None),
            price: row.try_get("price").context("price")?,
            sale_price: row.try_get("sale_price").context("sale_price")?,
            sale_price_start_date: row.try_get("sale_price_start_date")?,
            sale_price_end_date: row.try_get("sale_price_end_date")?,
            venue: row.try_get("venue").context("venue")?,
            city: row.try_get("city").context("city")?,
            event_date: row.try_get("event_date").context("event_date")?,
            start_time: row.try_get("start_time")?,
            end_time: row.try_get("end_time")?,
            status: row.try_get("status").context("status")?,
            created_at: row.try_get("created_at").context("created_at")?,
            updated_at: row.try_get("updated_at").context("updated_at")?,
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
}

#[async_trait]
impl EventRepository for PgEventRepository {
    async fn list(&self, f: &EventListFilter<'_>) -> Result<Vec<Event>> {
        // Build dynamic WHERE — we keep it simple and correct rather than trying to
        // share one LazyLock string across every filter combination.
        let mut sql = format!("SELECT {} FROM events WHERE 1 = 1", EVENT_COLS);
        let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
        let mut idx = 1usize;

        let mid_vec;
        if let Some(mid) = f.merchant_id {
            mid_vec = id_to_vec(mid)?;
            sql.push_str(&format!(" AND merchant_id = ${idx}"));
            params.push(Box::new(mid_vec));
            idx += 1;
        }
        if let Some(city) = f.city {
            sql.push_str(&format!(" AND city = ${idx}"));
            params.push(Box::new(city.to_string()));
            idx += 1;
        }
        if let Some(status) = f.status {
            sql.push_str(&format!(" AND status = ${idx}"));
            params.push(Box::new(status.to_string()));
            idx += 1;
        }
        sql.push_str(&format!(
            " ORDER BY event_date ASC LIMIT ${} OFFSET ${}",
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

        let mid_vec;
        if let Some(mid) = f.merchant_id {
            mid_vec = id_to_vec(mid)?;
            sql.push_str(&format!(" AND merchant_id = ${idx}"));
            params.push(Box::new(mid_vec));
            idx += 1;
        }
        if let Some(city) = f.city {
            sql.push_str(&format!(" AND city = ${idx}"));
            params.push(Box::new(city.to_string()));
            idx += 1;
        }
        if let Some(status) = f.status {
            sql.push_str(&format!(" AND status = ${idx}"));
            params.push(Box::new(status.to_string()));
            let _ = idx;
        }

        let refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
            params.iter().map(|p| p.as_ref() as _).collect();
        let row = exec_one(&self.pool, &sql, &refs).await?;
        Ok(row.try_get::<_, i64>("c")?)
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<Event>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_EVENT_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_event).transpose()
    }

    async fn find_by_slug(&self, slug: &str) -> Result<Option<Event>> {
        let row = exec_first(&self.pool, &FIND_EVENT_BY_SLUG, &[&slug]).await?;
        row.as_ref().map(Self::row_to_event).transpose()
    }

    async fn find_by_id_with_variants(
        &self,
        id: &str,
    ) -> Result<Option<(Event, Vec<EventVariant>)>> {
        // Masih dibutuhkan untuk internal ops (ensure_owner dll)
        let id_vec = id_to_vec(id)?;
        // Reuse logic dengan query lama — lookup by id dulu, lalu slug
        let event = match self.find_by_id(id).await? {
            Some(e) => e,
            None => return Ok(None),
        };
        let variants = self.list_variants(id).await?;
        Ok(Some((event, variants)))
    }

    async fn find_by_slug_with_variants(
        &self,
        slug: &str,
    ) -> Result<Option<(Event, Vec<EventVariant>)>> {
        let rows = exec_rows(&self.pool, &FIND_EVENT_WITH_VARIANTS_BY_SLUG, &[&slug]).await?;

        if rows.is_empty() {
            return Ok(None);
        }

        let first = &rows[0];
        let event = {
            let id_b: Vec<u8> = first.try_get("e_id")?;
            let mid_b: Vec<u8> = first.try_get("e_merchant_id")?;
            Event {
                id: bin_to_ulid(id_b)?,
                merchant_id: bin_to_ulid(mid_b)?,
                name: first.try_get("e_name")?,
                slug: first.try_get("e_slug").unwrap_or_default(),
                description: first.try_get("e_description")?,
                cover_url: first.try_get("e_cover_url").unwrap_or(None),
                price: first.try_get("e_price")?,
                sale_price: first.try_get("e_sale_price")?,
                sale_price_start_date: first.try_get("e_sale_price_start_date")?,
                sale_price_end_date: first.try_get("e_sale_price_end_date")?,
                venue: first.try_get("e_venue")?,
                city: first.try_get("e_city")?,
                event_date: first.try_get("e_event_date")?,
                start_time: first.try_get("e_start_time")?,
                end_time: first.try_get("e_end_time")?,
                status: first.try_get("e_status")?,
                created_at: first.try_get("e_created_at")?,
                updated_at: first.try_get("e_updated_at")?,
            }
        };

        let variants: Vec<EventVariant> = rows
            .iter()
            .filter_map(|row| {
                let id_b: Option<Vec<u8>> = row.try_get("v_id").ok().flatten();
                let id_b = id_b?;
                let event_b: Vec<u8> = row.try_get("v_event_id").ok()?;
                Some(EventVariant {
                    id: bin_to_ulid(id_b).ok()?,
                    event_id: bin_to_ulid(event_b).ok()?,
                    name: row.try_get("v_name").ok()?,
                    description: row.try_get("v_description").ok()?,
                    price: row.try_get("v_price").ok()?,
                    sale_price: row.try_get("v_sale_price").ok()?,
                    sale_price_start_date: row.try_get("v_sale_price_start_date").ok()?,
                    sale_price_end_date: row.try_get("v_sale_price_end_date").ok()?,
                    quota: row.try_get("v_quota").ok()?,
                    sold: row.try_get("v_sold").ok()?,
                    max_per_order: row.try_get("v_max_per_order").ok()?,
                    is_active: row.try_get("v_is_active").ok()?,
                    sort_order: row.try_get("v_sort_order").ok()?,
                    created_at: row.try_get("v_created_at").ok()?,
                    updated_at: row.try_get("v_updated_at").ok()?,
                })
            })
            .collect();

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
        let slug = generate_slug(merchant_name, &req.name);

        let row = exec_one(
            &self.pool,
            &INSERT_EVENT,
            &[
                &id_vec,
                &mid_vec,
                &req.name,
                &slug,
                &req.description,
                &cover_url,
                &req.venue,
                &req.city,
                &req.event_date,
                &req.start_time,
                &req.end_time,
            ],
        )
        .await?;
        Self::row_to_event(&row)
    }

    async fn create_variants_bulk(
        &self,
        event_id: &str,
        variants: &[CreateVariantInline],
    ) -> Result<Vec<EventVariant>> {
        let mut result = Vec::with_capacity(variants.len());
        for (i, v) in variants.iter().enumerate() {
            let id = new_ulid();
            let id_vec = ulid_to_vec(&id)?;
            let event_vec = id_to_vec(event_id)?;
            let sort_order = v.sort_order.unwrap_or(i as i32);
            let row = exec_one(
                &self.pool,
                &INSERT_VARIANT,
                &[
                    &id_vec,
                    &event_vec,
                    &v.name,
                    &v.description,
                    &v.price,
                    &v.quota,
                    &v.max_per_order,
                    &sort_order,
                ],
            )
            .await?;
            result.push(Self::row_to_variant(&row)?);
        }
        Ok(result)
    }

    async fn update(&self, id: &str, req: &UpdateEventRequest) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        exec_drop(
            &self.pool,
            UPDATE_EVENT,
            &[
                &id_vec,
                &req.name,
                &req.description,
                &req.cover_url,
                &req.venue,
                &req.city,
                &req.event_date,
                &req.start_time,
                &req.end_time,
                &req.status,
            ],
        )
        .await?;
        Ok(())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        exec_drop(&self.pool, DELETE_EVENT, &[&id_vec]).await?;
        Ok(())
    }

    async fn list_variants(&self, event_id: &str) -> Result<Vec<EventVariant>> {
        let id_vec = id_to_vec(event_id)?;
        let rows = exec_rows(&self.pool, &FIND_VARIANTS_BY_EVENT, &[&id_vec]).await?;
        rows.iter().map(Self::row_to_variant).collect()
    }

    async fn find_variant(&self, id: &str) -> Result<Option<EventVariant>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_VARIANT_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(Self::row_to_variant).transpose()
    }

    async fn create_variant(
        &self,
        event_id: &str,
        name: &str,
        description: Option<&str>,
        price: f64,
        quota: i32,
        max_per_order: Option<i32>,
        sort_order: i32,
    ) -> Result<EventVariant> {
        let id = new_ulid();
        let id_vec = ulid_to_vec(&id)?;
        let event_vec = id_to_vec(event_id)?;
        let row = exec_one(
            &self.pool,
            &INSERT_VARIANT,
            &[
                &id_vec,
                &event_vec,
                &name,
                &description,
                &price,
                &quota,
                &max_per_order,
                &sort_order,
            ],
        )
        .await?;
        Self::row_to_variant(&row)
    }

    async fn update_variant(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        price: Option<f64>,
        quota: Option<i32>,
        max_per_order: Option<i32>,
        is_active: Option<bool>,
        sort_order: Option<i32>,
    ) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        exec_drop(
            &self.pool,
            UPDATE_VARIANT,
            &[
                &id_vec,
                &name,
                &description,
                &price,
                &quota,
                &max_per_order,
                &is_active,
                &sort_order,
            ],
        )
        .await?;
        Ok(())
    }

    async fn delete_variant(&self, id: &str) -> Result<()> {
        let id_vec = id_to_vec(id)?;
        exec_drop(&self.pool, DELETE_VARIANT, &[&id_vec]).await?;
        Ok(())
    }
}
