use std::sync::LazyLock;

// ── LIKE escape ───────────────────────────────────────────────────────────────

/// FIX: Escape karakter wildcard PostgreSQL LIKE/ILIKE dari user input.
/// Tanpa ini, input seperti "100%" atau "band_name" secara tidak sengaja
/// menjadi wildcard pattern dan mengembalikan hasil yang tidak tepat.
pub(super) fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', r"\%")
        .replace('_', r"\_")
}

// ── Slug generator ────────────────────────────────────────────────────────────

pub(super) fn generate_slug(merchant_name: &str, event_name: &str) -> String {
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

// P2 FIX: Ganti string matching "23505" dengan downcast ke SqlState.
// Pola lama: e.to_string().contains("23505") — allocate String, fragile jika error message berubah.
// Pola baru: downcast ke tokio_postgres::Error → as_db_error() → cek SqlState enum secara langsung.
pub(super) fn is_unique_violation(e: &anyhow::Error) -> bool {
    e.downcast_ref::<tokio_postgres::Error>()
        .and_then(|e| e.as_db_error())
        .map(|e| e.code() == &tokio_postgres::error::SqlState::UNIQUE_VIOLATION)
        .unwrap_or(false)
}

// ── LATERAL subquery ──────────────────────────────────────────────────────────

pub(super) static VARIANT_STATS_LATERAL: &str = r#"
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

pub(super) static EVENT_COLS: &str = r#"
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
    e.latitude,
    e.longitude,
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

pub(super) static EVENT_COLS_NO_AGG: &str = r#"
    e.id,
    e.merchant_id,
    e.name,
    e.slug,
    e.description,
    e.cover_url,
    e.detail_images,
    e.venue,
    e.city,
    e.latitude,
    e.longitude,
    e.event_date,
    e.start_time,
    e.end_time,
    e.status,
    e.created_at,
    e.updated_at,
    e.category
"#;

pub(super) static VARIANTS_JSONB_AGG: &str = r#"
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

pub(super) static ADMIN_UPDATE_EVENT_STATUS: &str = r#"
    UPDATE events
       SET status = $2
     WHERE id = $1
"#;

// ── Static queries ────────────────────────────────────────────────────────────

pub(super) static FIND_EVENT_BY_ID: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {cols} FROM events e {lateral} WHERE e.id = $1",
        cols = EVENT_COLS,
        lateral = VARIANT_STATS_LATERAL,
    )
});

pub(super) static FIND_EVENT_WITH_VARIANTS_BY_SLUG: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {cols}, {agg} FROM events e WHERE e.slug = $1",
        cols = EVENT_COLS_NO_AGG,
        agg = VARIANTS_JSONB_AGG,
    )
});

pub(super) static FIND_EVENT_WITH_VARIANTS_BY_ID: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {cols}, {agg} FROM events e WHERE e.id = $1",
        cols = EVENT_COLS_NO_AGG,
        agg = VARIANTS_JSONB_AGG,
    )
});

// $13 category dan $15 detail_images adalah jsonb — tokio-postgres serialise
// serde_json::Value langsung ke jsonb tanpa perlu ::jsonb cast di SQL.
// FIX: INSERT_EVENT kini pakai RETURNING semua kolom yang dibutuhkan row_to_event_no_agg.
// Menghilangkan find_by_id post-insert (N+1 query) — data sudah tersedia dari RETURNING.
pub(super) static INSERT_EVENT: &str = "\
    INSERT INTO events \
        (id, merchant_id, name, slug, description, cover_url, price, venue, city, \
         event_date, start_time, end_time, category, status, detail_images, latitude, longitude) \
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17) \
    RETURNING \
        id, merchant_id, name, slug, description, cover_url, detail_images, \
        venue, city, latitude, longitude, event_date, start_time, end_time, status, \
        created_at, updated_at, category";

pub(super) static UPDATE_EVENT: &str = r#"
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
           detail_images = COALESCE($13, detail_images),
           latitude      = COALESCE($14, latitude),
           longitude     = COALESCE($15, longitude)
     WHERE id = $1 AND merchant_id = $2
"#;

pub(super) static DELETE_EVENT: &str = "DELETE FROM events WHERE id = $1 AND merchant_id = $2";

// ── Variant queries ───────────────────────────────────────────────────────────

pub(super) static VARIANT_COLS: &str = r#"
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

pub(super) static FIND_VARIANT_BY_ID: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM event_variants WHERE id = $1", VARIANT_COLS));

pub(super) const VARIANT_INSERT_COLS: usize = 11;

pub(super) static UPDATE_VARIANT: &str = r#"
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

pub(super) static DELETE_VARIANT: &str = r#"
    DELETE FROM event_variants v
    USING events e
    WHERE v.id = $1
      AND v.event_id = e.id
      AND e.merchant_id = $2
"#;
