use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use rust_decimal::Decimal;
use std::sync::LazyLock;
use tokio_postgres::types::ToSql;
use tokio_postgres::Row;

use super::db::{exec_first, exec_rows};
use crate::models::orders::{Order, OrderItemResponse, OrderListItem};
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

// ── Static query strings ──────────────────────────────────────────────────────

static ORDER_COLS: &str = "id, customer_id, order_code, status, total_amount, \
     subtotal_amount, discount_amount, promo_code, payment_method, payment_vendor, \
     payment_code, payment_charge, payment_expired_at, payment_reference, link_pay, \
     paid_at, expired_at, created_at, updated_at";

static FIND_ORDER_BY_ID: LazyLock<String> =
    LazyLock::new(|| format!("SELECT {} FROM orders WHERE id = $1", ORDER_COLS));

static LIST_ORDERS_BY_CUSTOMER: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM orders WHERE customer_id = $1 \
         ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        ORDER_COLS
    )
});

/// Enriched list query — halaman order customer dulu (memakai index
/// idx_orders_customer_date), baru LEFT JOIN LATERAL mengambil SATU item
/// pertama per order (earliest by created_at) + info product-nya.
/// Versi lama memakai CTE `DISTINCT ON` atas SELURUH baris pesanan (semua user,
/// tanpa filter customer) sehingga /orders makin lambat seiring tabel
/// membesar — lateral hanya menyentuh baris milik halaman ini.
static LIST_ORDERS_WITH_EVENT: &str = r#"
    SELECT
        o.id, o.customer_id, o.order_code, o.status, o.total_amount,
        o.payment_method, o.paid_at, o.expired_at, o.created_at, o.updated_at,
        fi.event_name, fi.event_date, fi.venue, fi.cover_url
    FROM (
        SELECT id, customer_id, order_code, status, total_amount,
               payment_method, paid_at, expired_at, created_at, updated_at,
               cart_id
        FROM orders
        WHERE customer_id = $1
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
    ) o
    LEFT JOIN LATERAL (
        SELECT
            e.name        AS event_name,
            e.event_date  AS event_date,
            e.venue       AS venue,
            e.cover_url   AS cover_url
        FROM cart_items ci
        JOIN product_variants ev ON ci.ticket_variant_id = ev.id
        JOIN products e           ON ev.event_id = e.id
        WHERE ci.cart_id = o.cart_id
        ORDER BY ci.created_at
        LIMIT 1
    ) fi ON TRUE
    ORDER BY o.created_at DESC
"#;

/// Baris pesanan sebuah order. Sejak `order_items` dihapus, sumbernya adalah
/// `cart_items` milik keranjang yang sudah ditutup — dihubungkan lewat
/// `orders.cart_id`. `subtotal` tak lagi disimpan sebagai kolom karena ia
/// selalu `unit_price * quantity`, dan satu-satunya gunanya dulu adalah
/// menyimpan hasil perkalian itu.
static QUERY_ITEMS_DETAIL: &str = r#"
    SELECT
        ci.id                       AS item_id,
        ci.ticket_variant_id,
        ci.quantity,
        ci.unit_price               AS unit_price,
        (ci.unit_price * ci.quantity) AS subtotal,
        tv.name                     AS variant_name,
        tv.event_id,
        e.name                      AS event_name
    FROM orders o
    JOIN cart_items ci        ON ci.cart_id = o.cart_id
    JOIN product_variants tv  ON ci.ticket_variant_id = tv.id
    JOIN products e           ON tv.event_id = e.id
    WHERE o.id = $1
    ORDER BY ci.created_at
"#;

// ── Prepared statement SQL strings ───────────────────────────────────────────

static STMT_LOCK_VARIANTS: &str = r#"
    SELECT
        ev.id,
        ev.price,
        CASE
            WHEN ev.sale_price IS NOT NULL
                AND NOW() BETWEEN
                    COALESCE(ev.sale_price_start_date, '-infinity'::timestamptz)
                AND COALESCE(ev.sale_price_end_date,   'infinity'::timestamptz)
            THEN ev.sale_price
            ELSE ev.price
        END             AS effective_price,
        ev.quota,
        ev.sold,
        ev.max_per_order,
        ev.is_active,
        ev.name         AS variant_name,
        e.id            AS event_id_bytes,
        e.name          AS event_name
    FROM product_variants ev
    JOIN products e ON ev.event_id = e.id
    WHERE ev.id = ANY($1)
    FOR UPDATE OF ev
"#;

/// Kolom & placeholder pembayaran dipakai dua kali (jalur biasa dan jalur
/// idempoten); ditulis sekali di sini supaya keduanya mustahil berselisih.
static PAY_COLS: &str = "cart_id, subtotal_amount, discount_amount, promo_code, \
     payment_vendor, payment_code, payment_method, payment_charge, \
     payment_expired_at, payment_reference, link_pay";

/// `$12` sengaja muncul dua kali: `payment_code` dan `payment_method` selalu
/// bernilai sama — kolom lama dipertahankan agar jalur REST dan data lawas
/// tetap terbaca, tapi tak boleh lagi menyimpang dari yang baru.
static PAY_VALS: &str = "$6, $7, $8, $9, $10, $11, $11, $12, $13, $14, $15";

static STMT_INSERT_ORDER_SIMPLE: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"INSERT INTO orders
           (id, customer_id, order_code, status, total_amount, expired_at, {cols})
           VALUES ($1, $2, $3, 'pending', $4, $5, {vals})
           RETURNING {ret}"#,
        cols = PAY_COLS,
        vals = PAY_VALS,
        ret = ORDER_COLS
    )
});

static STMT_INSERT_ORDER_IDEMPOTENCY: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"WITH ins AS (
               INSERT INTO orders
                   (id, customer_id, order_code, status, total_amount,
                    expired_at, {cols}, idempotency_key)
               VALUES ($1, $2, $3, 'pending', $4, $5, {vals}, $16)
               ON CONFLICT (customer_id, idempotency_key)
               WHERE idempotency_key IS NOT NULL
               DO NOTHING
               RETURNING {0}, TRUE AS is_new
           )
           SELECT {0}, is_new FROM ins
           UNION ALL
           SELECT {0}, FALSE AS is_new
           FROM orders
           WHERE customer_id = $2
             AND idempotency_key = $16
             AND NOT EXISTS (SELECT 1 FROM ins)
           LIMIT 1"#,
        ORDER_COLS,
        cols = PAY_COLS,
        vals = PAY_VALS
    )
});

/// Bekukan baris keranjang menjadi baris pesanan.
///
/// Dipakai dua arah sekaligus:
///   • jalur checkout — barisnya sudah ada, yang berubah hanya `unit_price`,
///     ditimpa dengan harga yang BARU DIKUNCI di dalam transaksi ini;
///   • jalur beli-langsung — barisnya belum ada, jadi INSERT yang berlaku.
///
/// Satu pernyataan untuk keduanya. `ON CONFLICT` di sini bukan kerapian, ia
/// yang membuat dua jalur pembelian tak perlu punya kode penulisan sendiri
/// yang bisa berselisih diam-diam.
static STMT_FREEZE_CART_ITEMS: &str = r#"
    INSERT INTO cart_items (id, cart_id, ticket_variant_id, quantity, unit_price)
    SELECT t.id, $1, t.var_id, t.qty, t.price
    FROM UNNEST($2::bytea[], $3::bytea[], $4::int4[], $5::numeric[])
        AS t(id, var_id, qty, price)
    ON CONFLICT (cart_id, ticket_variant_id) DO UPDATE
        SET quantity   = EXCLUDED.quantity,
            unit_price = EXCLUDED.unit_price,
            updated_at = NOW()
"#;

/// Keranjang sekali-pakai untuk jalur beli-langsung, yang tak pernah punya
/// keranjang terbuka. Ia lahir SUDAH tertutup (`deleted_at` terisi), sehingga
/// tidak menyerempet unique index "satu keranjang aktif per user" dan tak
/// pernah muncul sebagai keranjang milik siapa pun.
static STMT_INSERT_CLOSED_CART: &str = r#"
    INSERT INTO carts (id, user_id, position, deleted_at)
    VALUES ($1, $2, 'direct', NOW())
"#;

/// Pindahkan barang yang TIDAK dipilih ke keranjang lain.
///
/// Sejak baris pesanan tinggal di `cart_items`, checkout menutup keranjangnya.
/// Barang yang sengaja tidak dicentang pembeli karena itu harus dikeluarkan
/// lebih dulu -- kalau tidak, ia ikut terkunci di dalam keranjang yang sudah
/// jadi pesanan, dan dari sisi pembeli barangnya lenyap. Persis kebalikan dari
/// yang ia minta ketika membiarkan kotaknya kosong.
static STMT_MOVE_UNSELECTED: &str = r#"
    UPDATE cart_items SET cart_id = $2, updated_at = NOW()
     WHERE cart_id = $1 AND NOT selected
"#;

/// Keranjang terbuka baru untuk menampung barang yang tak jadi dibayar.
static STMT_INSERT_OPEN_CART: &str = r#"
    INSERT INTO carts (id, user_id) VALUES ($1, $2)
"#;

/// Buang keranjang penampung bila ternyata tak ada yang perlu dipindahkan.
static STMT_DELETE_EMPTY_CART: &str = r#"
    DELETE FROM carts c
     WHERE c.id = $1
       AND NOT EXISTS (SELECT 1 FROM cart_items ci WHERE ci.cart_id = c.id)
"#;

/// Tutup keranjang di dalam transaksi yang sama dengan lahirnya order, bukan
/// sesudah commit. Kalau ordernya batal, keranjangnya ikut kembali terbuka.
static STMT_CLOSE_CART: &str = r#"
    UPDATE carts SET deleted_at = NOW(), updated_at = NOW()
     WHERE id = $1 AND deleted_at IS NULL
"#;

static STMT_MINT_TICKETS: &str = r#"
    INSERT INTO tickets (id, cart_item_id, order_id, ticket_code, status)
    SELECT id, item_id, order_id, code, 'active'
    FROM UNNEST($1::bytea[], $2::bytea[], $3::bytea[], $4::text[]) AS t(id, item_id, order_id, code)
"#;

static STMT_BUMP_SOLD: &str = r#"
    WITH agg AS (
        SELECT id, SUM(qty) AS total_qty
        FROM UNNEST($1::bytea[], $2::int4[]) AS t(id, qty)
        GROUP BY id
    )
    UPDATE product_variants ev
       SET sold = ev.sold + agg.total_qty
      FROM agg
     WHERE ev.id = agg.id
       AND (ev.quota - ev.sold) >= agg.total_qty
"#;

static STMT_REFUND_SOLD: &str = r#"
    UPDATE product_variants
       SET sold = GREATEST(0, sold - bump.qty)
      FROM UNNEST($1::bytea[], $2::int4[]) AS bump(id, qty)
     WHERE product_variants.id = bump.id
"#;

/// `payment_code` ikut ditulis, bukan hanya `payment_method`: pembayaran yang
/// masuk lewat jalur mana pun harus meninggalkan kanal yang sama di kedua kolom,
/// kalau tidak laporan per-kanal akan melihat dua dunia yang berbeda.
static STMT_MARK_PAID: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"UPDATE orders
           SET status         = 'paid',
               paid_at        = NOW(),
               updated_at     = NOW(),
               payment_method = $2,
               payment_code   = COALESCE(payment_code, $2)
         WHERE id = $1
           AND status = 'pending'
           AND expired_at > NOW()
         RETURNING {}"#,
        ORDER_COLS
    )
});

static STMT_CANCEL_ORDER: &str =
    "UPDATE orders SET status = 'cancelled' WHERE id = $1 AND status = 'pending'";

static STMT_FETCH_ITEMS_FOR_MINT: &str = r#"
    SELECT ci.id, ci.quantity
      FROM orders o
      JOIN cart_items ci ON ci.cart_id = o.cart_id
     WHERE o.id = $1
"#;

static STMT_FETCH_ITEMS_FOR_REFUND: &str = r#"
    SELECT ci.ticket_variant_id, ci.quantity
      FROM orders o
      JOIN cart_items ci ON ci.cart_id = o.cart_id
     WHERE o.id = $1
"#;

// ── Lua scripts ───────────────────────────────────────────────────────────────

pub(crate) const LUA_RELEASE: &str = r#"
if redis.call("get", KEYS[1]) == ARGV[1] then
    return redis.call("del", KEYS[1])
else
    return 0
end
"#;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_ticket_code(ticket_id: &str) -> String {
    format!("TK{}", &ticket_id[..ticket_id.len().min(12)])
}

fn map_item_row(row: &Row) -> Result<OrderItemResponse> {
    let item_bytes: Vec<u8> = row.try_get("item_id")?;
    let var_bytes: Vec<u8> = row.try_get("ticket_variant_id")?;
    let ev_bytes: Vec<u8> = row.try_get("event_id")?;
    Ok(OrderItemResponse {
        id: bin_to_ulid(item_bytes)?,
        ticket_variant_id: bin_to_ulid(var_bytes)?,
        variant_name: row.try_get("variant_name")?,
        event_id: bin_to_ulid(ev_bytes)?,
        event_name: row.try_get("event_name")?,
        quantity: row.try_get("quantity")?,
        unit_price: row.try_get("unit_price")?,
        subtotal: row.try_get("subtotal")?,
    })
}

/// Map a row from LIST_ORDERS_WITH_EVENT into OrderListItem.
fn map_order_list_item(row: &Row) -> Result<OrderListItem> {
    let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
    let cust_bytes: Vec<u8> = row.try_get("customer_id").context("customer_id")?;
    Ok(OrderListItem {
        id: bin_to_ulid(id_bytes)?,
        customer_id: bin_to_ulid(cust_bytes)?,
        order_code: row.try_get("order_code").context("order_code")?,
        status: row.try_get("status").context("status")?,
        total_amount: row.try_get("total_amount").context("total_amount")?,
        payment_method: row.try_get("payment_method")?,
        paid_at: row.try_get("paid_at")?,
        expired_at: row.try_get("expired_at")?,
        created_at: row.try_get("created_at").context("created_at")?,
        event_name: row.try_get("event_name")?,
        event_date: row.try_get("event_date")?,
        venue: row.try_get("venue")?,
        cover_url: row.try_get("cover_url")?,
    })
}

// ── Structs ───────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct LockedVariant {
    pub id_bytes: Vec<u8>,
    pub ulid: String,
    pub price: Decimal,
    pub effective_price: Decimal,
    pub quota: i32,
    pub sold: i32,
    pub max_per_order: Option<i32>,
    pub is_active: bool,
    pub variant_name: String,
    pub event_id: String,
    pub event_name: String,
}

/// Rincian pembayaran yang menempel pada satu order saat ia dibuat.
///
/// Dikemas sebagai satu struct alih-alih sepuluh argumen supaya urutan
/// parameter SQL ($6..$15) hanya perlu benar di SATU tempat.
pub struct OrderPaymentSpec<'a> {
    pub cart_bytes: Option<&'a [u8]>,
    pub subtotal: Decimal,
    pub discount: Decimal,
    pub promo_code: Option<&'a str>,
    pub vendor: Option<&'a str>,
    /// Kode kanal; ikut mengisi kolom lama `payment_method`.
    pub code: Option<&'a str>,
    pub charge: Decimal,
    pub payment_expired_at: Option<chrono::DateTime<chrono::Utc>>,
    pub reference: Option<&'a str>,
    pub link_pay: Option<&'a str>,
}

impl OrderPaymentSpec<'_> {
    /// Order tanpa kanal pembayaran — jalur lama (REST `POST /api/orders`) yang
    /// hanya menyebut tiket. Seluruh totalnya adalah harga tiket: tanpa
    /// potongan, tanpa biaya kanal.
    pub fn plain(subtotal: Decimal) -> Self {
        Self {
            cart_bytes: None,
            subtotal,
            discount: Decimal::ZERO,
            promo_code: None,
            vendor: None,
            code: None,
            charge: Decimal::ZERO,
            payment_expired_at: None,
            reference: None,
            link_pay: None,
        }
    }
}

pub struct ItemRow {
    /// Id baris keranjang yang akan dibekukan menjadi baris pesanan.
    pub oi_id: String,
    pub oi_bytes: Vec<u8>,
    pub var_bytes: Vec<u8>,
    pub qty: i32,
    pub unit_price: Decimal,
    pub subtotal: Decimal,
}

#[derive(Debug)]
pub struct OversellError {
    pub updated: u64,
    pub expected: usize,
    pub variant_ids: Vec<Vec<u8>>,
}

impl std::fmt::Display for OversellError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "oversell guard: updated {} of {} variants",
            self.updated, self.expected
        )
    }
}

impl std::error::Error for OversellError {}

// ── OrderTx ───────────────────────────────────────────────────────────────────

pub struct OrderTx;

impl OrderTx {
    pub async fn lock_variants(
        tx: &deadpool_postgres::Transaction<'_>,
        id_bytes_list: &[Vec<u8>],
    ) -> Result<Vec<LockedVariant>> {
        let stmt = tx
            .prepare_cached(STMT_LOCK_VARIANTS)
            .await
            .context("lock_variants prepare")?;

        let rows = tx
            .query(&stmt, &[&id_bytes_list])
            .await
            .context("lock_variants execute")?;

        rows.iter()
            .map(|row| {
                let id_b: Vec<u8> = row.try_get("id")?;
                let ev_b: Vec<u8> = row.try_get("event_id_bytes")?;
                Ok(LockedVariant {
                    ulid: bin_to_ulid(id_b.clone())?,
                    id_bytes: id_b,
                    price: row.try_get("price")?,
                    effective_price: row.try_get("effective_price")?,
                    quota: row.try_get("quota")?,
                    sold: row.try_get("sold")?,
                    max_per_order: row.try_get("max_per_order")?,
                    is_active: row.try_get("is_active")?,
                    variant_name: row.try_get("variant_name")?,
                    event_id: bin_to_ulid(ev_b)?,
                    event_name: row.try_get("event_name")?,
                })
            })
            .collect()
    }

    pub async fn insert_order(
        tx: &deadpool_postgres::Transaction<'_>,
        id_bytes: &[u8],
        customer_bytes: &[u8],
        order_code: &str,
        total: Decimal,
        expired_at: chrono::DateTime<chrono::Utc>,
        idempotency_key: Option<&str>,
        pay: &OrderPaymentSpec<'_>,
    ) -> Result<(Order, bool)> {
        // Urutan parameter pembayaran ($6..$15) sama persis untuk kedua
        // pernyataan — lihat PAY_COLS/PAY_VALS.
        let pay_params: [&(dyn ToSql + Sync); 10] = [
            &pay.cart_bytes,
            &pay.subtotal,
            &pay.discount,
            &pay.promo_code,
            &pay.vendor,
            &pay.code,
            &pay.charge,
            &pay.payment_expired_at,
            &pay.reference,
            &pay.link_pay,
        ];

        let base: [&(dyn ToSql + Sync); 5] = [
            &id_bytes,
            &customer_bytes,
            &order_code,
            &total,
            &expired_at,
        ];

        let mut params: Vec<&(dyn ToSql + Sync)> = Vec::with_capacity(16);
        params.extend_from_slice(&base);
        params.extend_from_slice(&pay_params);

        if idempotency_key.is_none() {
            let stmt = tx
                .prepare_cached(&STMT_INSERT_ORDER_SIMPLE)
                .await
                .context("insert_order prepare")?;

            let row = tx
                .query_one(&stmt, &params)
                .await
                .context("insert_order execute")?;
            return Ok((row_to_order(&row)?, true));
        }

        let key = idempotency_key.unwrap();
        params.push(&key);

        let stmt = tx
            .prepare_cached(&STMT_INSERT_ORDER_IDEMPOTENCY)
            .await
            .context("insert_order_idempotency prepare")?;

        let row = tx
            .query_one(&stmt, &params)
            .await
            .context("insert_order_idempotency execute")?;

        let is_new: bool = row.try_get("is_new").context("is_new")?;
        Ok((row_to_order(&row)?, is_new))
    }

    /// Bekukan baris keranjang menjadi baris pesanan, dengan harga yang baru
    /// dikunci di dalam transaksi ini.
    pub async fn freeze_cart_items(
        tx: &deadpool_postgres::Transaction<'_>,
        cart_id_bytes: &[u8],
        items: &[ItemRow],
    ) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }

        let ids: Vec<&[u8]> = items.iter().map(|r| r.oi_bytes.as_slice()).collect();
        let var_ids: Vec<&[u8]> = items.iter().map(|r| r.var_bytes.as_slice()).collect();
        let qtys: Vec<i32> = items.iter().map(|r| r.qty).collect();
        let prices: Vec<Decimal> = items.iter().map(|r| r.unit_price).collect();

        let stmt = tx
            .prepare_cached(STMT_FREEZE_CART_ITEMS)
            .await
            .context("freeze_cart_items prepare")?;

        let params: &[&(dyn ToSql + Sync)] = &[&cart_id_bytes, &ids, &var_ids, &qtys, &prices];

        tx.execute(&stmt, params)
            .await
            .context("freeze_cart_items execute")?;

        Ok(())
    }

    /// Buat keranjang sekali-pakai yang lahir sudah tertutup, untuk jalur
    /// beli-langsung yang tak punya keranjang terbuka.
    pub async fn insert_closed_cart(
        tx: &deadpool_postgres::Transaction<'_>,
        cart_id_bytes: &[u8],
        customer_bytes: &[u8],
    ) -> Result<()> {
        let stmt = tx
            .prepare_cached(STMT_INSERT_CLOSED_CART)
            .await
            .context("insert_closed_cart prepare")?;
        tx.execute(&stmt, &[&cart_id_bytes, &customer_bytes])
            .await
            .context("insert_closed_cart execute")?;
        Ok(())
    }

    /// Selamatkan barang yang tidak dicentang: pindahkan ke keranjang terbuka
    /// yang baru, sebelum keranjang lama ditutup menjadi pesanan.
    ///
    /// Dipanggil SESUDAH `close_cart` — unique index "satu keranjang aktif per
    /// user" tak mengizinkan dua keranjang terbuka berdampingan.
    pub async fn rescue_unselected(
        tx: &deadpool_postgres::Transaction<'_>,
        old_cart_bytes: &[u8],
        new_cart_bytes: &[u8],
        customer_bytes: &[u8],
    ) -> Result<u64> {
        let insert = tx
            .prepare_cached(STMT_INSERT_OPEN_CART)
            .await
            .context("rescue_unselected insert prepare")?;
        tx.execute(&insert, &[&new_cart_bytes, &customer_bytes])
            .await
            .context("rescue_unselected insert execute")?;

        let mv = tx
            .prepare_cached(STMT_MOVE_UNSELECTED)
            .await
            .context("rescue_unselected move prepare")?;
        let moved = tx
            .execute(&mv, &[&old_cart_bytes, &new_cart_bytes])
            .await
            .context("rescue_unselected move execute")?;

        // Tak ada yang dipindahkan berarti seluruh isi keranjang memang dibeli.
        // Keranjang penampungnya dibuang supaya tak meninggalkan baris kosong.
        if moved == 0 {
            let del = tx
                .prepare_cached(STMT_DELETE_EMPTY_CART)
                .await
                .context("rescue_unselected cleanup prepare")?;
            tx.execute(&del, &[&new_cart_bytes])
                .await
                .context("rescue_unselected cleanup execute")?;
        }

        Ok(moved)
    }

    /// Tutup keranjang di dalam transaksi order.
    pub async fn close_cart(
        tx: &deadpool_postgres::Transaction<'_>,
        cart_id_bytes: &[u8],
    ) -> Result<u64> {
        let stmt = tx
            .prepare_cached(STMT_CLOSE_CART)
            .await
            .context("close_cart prepare")?;
        tx.execute(&stmt, &[&cart_id_bytes])
            .await
            .context("close_cart execute")
    }

    pub async fn bump_sold_batch(
        tx: &deadpool_postgres::Transaction<'_>,
        updates: &[(&[u8], i32)],
    ) -> Result<(), anyhow::Error> {
        if updates.is_empty() {
            return Ok(());
        }

        let mut agg: std::collections::HashMap<&[u8], i32> =
            std::collections::HashMap::with_capacity(updates.len());
        for &(id, qty) in updates {
            *agg.entry(id).or_insert(0) += qty;
        }

        let ids: Vec<&[u8]> = agg.keys().copied().collect();
        let qtys: Vec<i32> = agg.values().copied().collect();
        let expected = ids.len();

        let stmt = tx
            .prepare_cached(STMT_BUMP_SOLD)
            .await
            .context("bump_sold_batch prepare")?;

        let params: &[&(dyn ToSql + Sync)] = &[&ids, &qtys];
        let updated = tx
            .execute(&stmt, params)
            .await
            .context("bump_sold_batch execute")?;

        if updated as usize != expected {
            let variant_ids: Vec<Vec<u8>> = ids.iter().map(|b| b.to_vec()).collect();
            return Err(anyhow::anyhow!(OversellError {
                updated,
                expected,
                variant_ids,
            }));
        }
        Ok(())
    }

    pub async fn mark_paid(
        tx: &deadpool_postgres::Transaction<'_>,
        order_bytes: &[u8],
        payment_method: &str,
    ) -> Result<Option<Order>> {
        let stmt = tx
            .prepare_cached(&STMT_MARK_PAID)
            .await
            .context("mark_paid prepare")?;

        let params: &[&(dyn ToSql + Sync)] = &[&order_bytes, &payment_method];
        let row = tx
            .query_opt(&stmt, params)
            .await
            .context("mark_paid execute")?;

        row.as_ref().map(row_to_order).transpose()
    }

    pub async fn fetch_items_for_mint(
        tx: &deadpool_postgres::Transaction<'_>,
        order_bytes: &[u8],
    ) -> Result<Vec<(Vec<u8>, i32)>> {
        let stmt = tx
            .prepare_cached(STMT_FETCH_ITEMS_FOR_MINT)
            .await
            .context("fetch_items_for_mint prepare")?;

        tx.query(&stmt, &[&order_bytes])
            .await
            .context("fetch_items_for_mint execute")?
            .iter()
            .map(|r| Ok((r.try_get::<_, Vec<u8>>("id")?, r.try_get("quantity")?)))
            .collect()
    }

    pub async fn fetch_items_detail(
        tx: &deadpool_postgres::Transaction<'_>,
        order_bytes: &[u8],
    ) -> Result<Vec<OrderItemResponse>> {
        let stmt = tx
            .prepare_cached(QUERY_ITEMS_DETAIL)
            .await
            .context("fetch_items_detail prepare")?;

        tx.query(&stmt, &[&order_bytes])
            .await
            .context("fetch_items_detail execute")?
            .iter()
            .map(map_item_row)
            .collect()
    }

    pub async fn mint_tickets_batch(
        tx: &deadpool_postgres::Transaction<'_>,
        items: &[(Vec<u8>, i32)],
        order_id_bytes: &[u8], // ← NEW: stored in every ticket row
    ) -> Result<u64> {
        let total: i32 = items.iter().map(|(_, q)| q).sum();
        if total == 0 {
            return Ok(0);
        }

        let mut ids: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
        let mut item_ids: Vec<Vec<u8>> = Vec::with_capacity(total as usize);
        let mut ord_ids: Vec<Vec<u8>> = Vec::with_capacity(total as usize); // NEW
        let mut codes: Vec<String> = Vec::with_capacity(total as usize);

        for (item_bytes, qty) in items {
            for _ in 0..*qty {
                let id = new_ulid();
                let id_bytes = ulid_to_vec(&id)?;
                let code = make_ticket_code(&id);
                ids.push(id_bytes);
                item_ids.push(item_bytes.clone());
                ord_ids.push(order_id_bytes.to_vec()); // same order for all tickets
                codes.push(code);
            }
        }

        let count = ids.len() as u64;

        let stmt = tx
            .prepare_cached(STMT_MINT_TICKETS)
            .await
            .context("mint_tickets_batch prepare")?;

        let params: &[&(dyn ToSql + Sync)] = &[&ids, &item_ids, &ord_ids, &codes]; // ← +ord_ids
        tx.execute(&stmt, params)
            .await
            .context("mint_tickets_batch execute")?;

        Ok(count)
    }

    pub async fn cancel_order(
        tx: &deadpool_postgres::Transaction<'_>,
        order_bytes: &[u8],
    ) -> Result<u64> {
        let stmt = tx
            .prepare_cached(STMT_CANCEL_ORDER)
            .await
            .context("cancel_order prepare")?;

        tx.execute(&stmt, &[&order_bytes])
            .await
            .context("cancel_order execute")
    }

    pub async fn fetch_items_for_refund(
        tx: &deadpool_postgres::Transaction<'_>,
        order_bytes: &[u8],
    ) -> Result<Vec<(Vec<u8>, i32)>> {
        let stmt = tx
            .prepare_cached(STMT_FETCH_ITEMS_FOR_REFUND)
            .await
            .context("fetch_items_for_refund prepare")?;

        tx.query(&stmt, &[&order_bytes])
            .await
            .context("fetch_items_for_refund execute")?
            .iter()
            .map(|r| {
                Ok((
                    r.try_get::<_, Vec<u8>>("ticket_variant_id")?,
                    r.try_get("quantity")?,
                ))
            })
            .collect()
    }

    pub async fn refund_sold_batch(
        tx: &deadpool_postgres::Transaction<'_>,
        updates: &[(Vec<u8>, i32)],
    ) -> Result<()> {
        if updates.is_empty() {
            return Ok(());
        }

        let ids: Vec<Vec<u8>> = updates.iter().map(|(id, _)| id.clone()).collect();
        let qtys: Vec<i32> = updates.iter().map(|(_, q)| *q).collect();

        let stmt = tx
            .prepare_cached(STMT_REFUND_SOLD)
            .await
            .context("refund_sold_batch prepare")?;

        let params: &[&(dyn ToSql + Sync)] = &[&ids, &qtys];
        tx.execute(&stmt, params)
            .await
            .context("refund_sold_batch execute")?;
        Ok(())
    }
}

// ── Trait + PgOrderRepository ─────────────────────────────────────────────────

pub struct CreateOrderItemSpec<'a> {
    pub variant_id: &'a str,
    pub quantity: i32,
}

#[async_trait]
pub trait OrderRepository: Send + Sync {
    async fn find_by_id(&self, id: &str) -> Result<Option<Order>>;
    async fn list_for_customer(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Order>>;
    /// Enriched list — includes first product's name, date, venue, cover_url.
    async fn list_for_customer_enriched(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<OrderListItem>>;
    async fn list_items(&self, order_id: &str) -> Result<Vec<OrderItemResponse>>;
}

#[derive(Clone)]
pub struct PgOrderRepository {
    pool: Pool,
}

impl PgOrderRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OrderRepository for PgOrderRepository {
    async fn find_by_id(&self, id: &str) -> Result<Option<Order>> {
        let id_vec = id_to_vec(id)?;
        let row = exec_first(&self.pool, &FIND_ORDER_BY_ID, &[&id_vec]).await?;
        row.as_ref().map(row_to_order).transpose()
    }

    async fn list_for_customer(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Order>> {
        let id_vec = id_to_vec(customer_id)?;
        let params: &[&(dyn ToSql + Sync)] = &[&id_vec, &limit, &offset];
        let rows = exec_rows(&self.pool, &LIST_ORDERS_BY_CUSTOMER, params).await?;
        rows.iter().map(row_to_order).collect()
    }

    async fn list_for_customer_enriched(
        &self,
        customer_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<OrderListItem>> {
        let id_vec = id_to_vec(customer_id)?;
        let params: &[&(dyn ToSql + Sync)] = &[&id_vec, &limit, &offset];
        let rows = exec_rows(&self.pool, LIST_ORDERS_WITH_EVENT, params).await?;
        rows.iter().map(map_order_list_item).collect()
    }

    async fn list_items(&self, order_id: &str) -> Result<Vec<OrderItemResponse>> {
        let id_vec = id_to_vec(order_id)?;
        let rows = exec_rows(&self.pool, QUERY_ITEMS_DETAIL, &[&id_vec]).await?;
        rows.iter().map(map_item_row).collect()
    }
}

// ── Row helpers ───────────────────────────────────────────────────────────────

pub(crate) fn row_to_order(row: &Row) -> Result<Order> {
    let id_bytes: Vec<u8> = row.try_get("id").context("id")?;
    let cust_bytes: Vec<u8> = row.try_get("customer_id").context("customer_id")?;
    Ok(Order {
        id: bin_to_ulid(id_bytes)?,
        customer_id: bin_to_ulid(cust_bytes)?,
        order_code: row.try_get("order_code").context("order_code")?,
        status: row.try_get("status").context("status")?,
        total_amount: row.try_get("total_amount").context("total_amount")?,
        subtotal_amount: row.try_get("subtotal_amount").unwrap_or_default(),
        discount_amount: row.try_get("discount_amount").unwrap_or_default(),
        promo_code: row.try_get("promo_code").unwrap_or_default(),
        payment_method: row.try_get("payment_method").context("payment_method")?,
        payment_vendor: row.try_get("payment_vendor").unwrap_or_default(),
        payment_code: row.try_get("payment_code").unwrap_or_default(),
        payment_charge: row.try_get("payment_charge").unwrap_or_default(),
        payment_expired_at: row.try_get("payment_expired_at").unwrap_or_default(),
        payment_reference: row.try_get("payment_reference").unwrap_or_default(),
        link_pay: row.try_get("link_pay").unwrap_or_default(),
        paid_at: row.try_get("paid_at")?,
        expired_at: row.try_get("expired_at")?,
        created_at: row.try_get("created_at").context("created_at")?,
        updated_at: row.try_get("updated_at").context("updated_at")?,
    })
}
