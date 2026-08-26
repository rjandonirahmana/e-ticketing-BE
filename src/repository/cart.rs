//! repository/cart.rs — keranjang di database.
//!
//! Dua keputusan yang menentukan bentuk seluruh berkas ini:
//!
//! 1. **Satu keranjang aktif per user**, dijaga unique index parsial
//!    (`uniq_carts_user_active`). Karena itu "ambil keranjang saya" adalah satu
//!    baris tanpa ORDER BY, dan dua tab yang menambah barang bersamaan tak bisa
//!    melahirkan keranjang kembar — yang kedua kalah di index, lalu membaca
//!    keranjang yang sama.
//!
//! 2. **Isi keranjang selalu dibaca bersama keadaan varian TERKINI.** `cart_items`
//!    hanya menyimpan varian, jumlah, dan HARGA saat dimasukkan — tidak ada
//!    salinan nama, cover, atau venue. Semua itu di-JOIN hidup dari `products` dan
//!    `product_variants` di setiap pembacaan, jadi mustahil basi.
//!
//!    Harga adalah satu-satunya pengecualian, dan justru karena ia BOLEH berbeda:
//!    dibanding harga berlaku, salinan itu menjawab "harga berubah sejak Anda
//!    menambahkan" — pertanyaan yang lenyap kalau harganya ikut di-JOIN.

use anyhow::{Context, Result};
use async_trait::async_trait;
use deadpool_postgres::Pool;
use rust_decimal::Decimal;
use std::sync::LazyLock;
use tokio_postgres::Row;

use super::db::{exec_first, exec_rows};
use crate::models::cart::Cart;
use crate::utils::ulid::{bin_to_ulid, id_to_vec, new_ulid, ulid_to_vec};

// ── Potongan SQL yang dipakai berulang ───────────────────────────────────────

static CART_COLS: &str = "id, user_id, promo_code, discount_amount, payment_code, \
     position, created_at, updated_at";

/// Harga berlaku sebuah varian: harga diskon bila sedang dalam rentang promo,
/// selain itu harga normal. Sengaja SAMA PERSIS dengan yang dipakai
/// `repository/order.rs` saat mengunci varian — kalau kedua rumus ini berbeda,
/// angka di keranjang dan angka yang ditagihkan bisa berselisih tanpa ada yang
/// berubah di antaranya.
static EFFECTIVE_PRICE: &str = r#"
    CASE
        WHEN ev.sale_price IS NOT NULL
            AND NOW() BETWEEN
                COALESCE(ev.sale_price_start_date, '-infinity'::timestamptz)
            AND COALESCE(ev.sale_price_end_date,   'infinity'::timestamptz)
        THEN ev.sale_price
        ELSE ev.price
    END
"#;

// ── Keranjang ────────────────────────────────────────────────────────────────

/// Ambil keranjang aktif, buat bila belum ada — dalam SATU perjalanan ke
/// database. Pola CTE `ins`/`UNION ALL` sama dengan jalur idempotensi order:
/// INSERT yang kalah balapan tidak melempar error, ia jatuh ke SELECT.
static GET_OR_CREATE_CART: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"WITH ins AS (
               INSERT INTO carts (id, user_id)
               VALUES ($1, $2)
               ON CONFLICT (user_id) WHERE deleted_at IS NULL DO NOTHING
               RETURNING {0}
           )
           SELECT {0} FROM ins
           UNION ALL
           SELECT {0} FROM carts
           WHERE user_id = $2
             AND deleted_at IS NULL
             AND NOT EXISTS (SELECT 1 FROM ins)
           LIMIT 1"#,
        CART_COLS
    )
});

static FIND_ACTIVE_CART: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM carts WHERE user_id = $1 AND deleted_at IS NULL LIMIT 1",
        CART_COLS
    )
});

static UPDATE_CART_META: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"UPDATE carts
              SET promo_code      = $2,
                  discount_amount = $3,
                  payment_code    = $4,
                  position        = COALESCE($5, position),
                  updated_at      = NOW()
            WHERE id = $1
        RETURNING {}"#,
        CART_COLS
    )
});

/// Tutup keranjang setelah jadi order. Barisnya TIDAK dihapus: `orders.cart_id`
/// menunjuk ke sini, dan isinya adalah bukti apa yang ada di keranjang saat
/// pesanan lahir.
static CLOSE_CART: &str = "UPDATE carts SET deleted_at = NOW(), updated_at = NOW() \
     WHERE id = $1 AND deleted_at IS NULL";

// ── Baris keranjang ──────────────────────────────────────────────────────────

/// Masukkan/naikkan satu baris keranjang. Snapshot diambil langsung dari
/// database (`INSERT … SELECT`), bukan dikirim klien — nama dan harga yang
/// tersimpan karenanya tak bisa dipalsukan dari browser.
///
/// `$5` menentukan arti `$4` saat baris sudah ada:
///   TRUE  → tetapkan jumlah (dipakai tombol +/− dan halaman keranjang)
///   FALSE → tambahkan (dipakai "masukkan keranjang" dari halaman product)
static UPSERT_ITEM: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"INSERT INTO cart_items
               (id, cart_id, ticket_variant_id, quantity, unit_price)
           SELECT $1, $2, ev.id, $4, {price}
           FROM product_variants ev
           JOIN products e ON ev.event_id = e.id
           WHERE ev.id = $3
             AND ev.is_active
             AND e.status = 'active'
           ON CONFLICT (cart_id, ticket_variant_id) DO UPDATE
               SET quantity = LEAST(
                       CASE WHEN $5::bool
                            THEN EXCLUDED.quantity
                            ELSE cart_items.quantity + EXCLUDED.quantity
                       END, 100),
                   updated_at = NOW()
           RETURNING quantity"#,
        price = EFFECTIVE_PRICE
    )
});
// Catatan: `unit_price` sengaja TIDAK ikut di-update saat konflik. Kolom itu
// berarti "harga yang dilihat pembeli ketika memasukkan barang" — menyegarkannya
// setiap kali tombol + ditekan akan menghapus satu-satunya jejak bahwa harga
// sempat berubah.
//
// Harganya diambil dari database lewat `INSERT … SELECT`, bukan dikirim klien,
// jadi angka yang tersimpan tak bisa dipalsukan dari browser.

static SET_QTY: &str = r#"
    UPDATE cart_items SET quantity = $3, updated_at = NOW()
     WHERE cart_id = $1 AND ticket_variant_id = $2
"#;

static SET_SELECTED: &str = r#"
    UPDATE cart_items SET selected = $3, updated_at = NOW()
     WHERE cart_id = $1 AND ticket_variant_id = $2
"#;

static SET_ALL_SELECTED: &str = r#"
    UPDATE cart_items SET selected = $2, updated_at = NOW()
     WHERE cart_id = $1 AND selected <> $2
"#;

static REMOVE_ITEM: &str =
    "DELETE FROM cart_items WHERE cart_id = $1 AND ticket_variant_id = $2";

static CLEAR_ITEMS: &str = "DELETE FROM cart_items WHERE cart_id = $1";

/// Isi keranjang + keadaan varian sekarang.
static LIST_ITEMS: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"SELECT
               ci.id,
               ci.ticket_variant_id,
               ci.quantity,
               ci.unit_price           AS unit_price_snapshot,
               ci.selected,
               {price}                 AS unit_price,
               (ev.quota - ev.sold)    AS available,
               ev.max_per_order,
               ev.name                 AS variant_name,
               ev.event_id,
               e.name                  AS event_name,
               e.slug                  AS event_slug,
               COALESCE(e.venue, '')   AS venue,
               COALESCE(e.cover_url,'') AS cover_url,
               e.event_date
           FROM cart_items ci
           JOIN product_variants ev ON ci.ticket_variant_id = ev.id
           JOIN products e          ON ev.event_id = e.id
           WHERE ci.cart_id = $1
           ORDER BY ci.created_at"#,
        price = EFFECTIVE_PRICE
    )
});

/// Buang barang yang sudah tak bisa dibeli, kembalikan namanya untuk diberitahukan
/// ke pembeli — padanan pembersihan senyap di `GET /cart/view` milik kiddoapi,
/// tapi di sini dilakukan satu pernyataan SQL alih-alih satu DELETE per baris.
///
/// Yang dianggap mati: varian dinonaktifkan, product tak lagi aktif, stok habis,
/// atau acaranya sudah lewat.
static PRUNE_DEAD_ITEMS: &str = r#"
    DELETE FROM cart_items ci
     USING product_variants ev
      JOIN products e ON ev.event_id = e.id
     WHERE ci.cart_id = $1
       AND ci.ticket_variant_id = ev.id
       AND (
              NOT ev.is_active
           OR e.status <> 'active'
           OR (ev.quota - ev.sold) <= 0
       )
    RETURNING e.name AS event_name, ev.name AS variant_name
"#;

/// Jumlah tiket (bukan jumlah baris) di keranjang aktif — untuk lencana di nav.
static COUNT_ITEMS: &str = r#"
    SELECT COALESCE(SUM(ci.quantity), 0)::BIGINT AS total
      FROM carts c
      LEFT JOIN cart_items ci ON ci.cart_id = c.id
     WHERE c.user_id = $1 AND c.deleted_at IS NULL
"#;

// ── Bentuk baris hasil baca ──────────────────────────────────────────────────

/// Satu baris keranjang apa adanya dari database — service yang mengubahnya
/// jadi `CartItemView` (menghitung subtotal, menandai stok kurang, dst).
#[derive(Debug, Clone)]
pub struct CartItemRow {
    pub id: String,
    pub ticket_variant_id: String,
    pub event_id: String,
    pub event_slug: String,
    pub quantity: i32,
    /// Harga saat dimasukkan ke keranjang (satu-satunya salinan di `cart_items`).
    pub unit_price_snapshot: Decimal,
    /// Harga berlaku sekarang, dihitung dari `product_variants`.
    pub unit_price: Decimal,
    pub available: i32,
    pub max_per_order: Option<i32>,
    /// Ikut dibayar pada checkout berikutnya.
    pub selected: bool,
    // Semua di bawah ini datang dari JOIN, bukan dari salinan di `cart_items`.
    pub variant_name: String,
    pub event_name: String,
    pub venue: String,
    pub cover_url: String,
    pub event_date: Option<chrono::DateTime<chrono::Utc>>,
}

// ── Trait ────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait CartRepository: Send + Sync {
    async fn get_or_create(&self, user_id: &str) -> Result<Cart>;
    async fn find_active(&self, user_id: &str) -> Result<Option<Cart>>;
    async fn list_items(&self, cart_id: &str) -> Result<Vec<CartItemRow>>;

    /// `replace = false` menambah ke jumlah yang sudah ada.
    /// Mengembalikan `None` bila varian tak ada / tak aktif / product-nya tutup.
    async fn upsert_item(
        &self,
        cart_id: &str,
        variant_id: &str,
        quantity: i32,
        replace: bool,
    ) -> Result<Option<i32>>;

    async fn set_quantity(&self, cart_id: &str, variant_id: &str, quantity: i32) -> Result<u64>;
    async fn remove_item(&self, cart_id: &str, variant_id: &str) -> Result<u64>;

    /// Tandai satu baris ikut / tidak ikut dibayar.
    async fn set_selected(&self, cart_id: &str, variant_id: &str, selected: bool) -> Result<u64>;

    /// Tandai seluruh isi keranjang sekaligus.
    async fn set_all_selected(&self, cart_id: &str, selected: bool) -> Result<u64>;
    async fn clear_items(&self, cart_id: &str) -> Result<u64>;

    /// Buang barang yang tak bisa dibeli lagi; kembalikan "Nama Product — Varian".
    async fn prune_dead_items(&self, cart_id: &str) -> Result<Vec<String>>;

    async fn update_meta(
        &self,
        cart_id: &str,
        promo_code: Option<&str>,
        discount: Decimal,
        payment_code: Option<&str>,
        position: Option<&str>,
    ) -> Result<Cart>;

    async fn count_items(&self, user_id: &str) -> Result<i64>;

    /// Tutup keranjang (soft delete) setelah menjadi order.
    async fn close(&self, cart_id: &str) -> Result<u64>;
}

// ── Implementasi Postgres ────────────────────────────────────────────────────

pub struct PgCartRepository {
    pool: Pool,
}

impl PgCartRepository {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

fn row_to_cart(row: &Row) -> Result<Cart> {
    Ok(Cart {
        id: bin_to_ulid(row.try_get::<_, Vec<u8>>("id").context("carts.id")?)?,
        user_id: bin_to_ulid(row.try_get::<_, Vec<u8>>("user_id").context("carts.user_id")?)?,
        promo_code: row.try_get("promo_code").context("carts.promo_code")?,
        discount_amount: row
            .try_get("discount_amount")
            .context("carts.discount_amount")?,
        payment_code: row.try_get("payment_code").context("carts.payment_code")?,
        position: row.try_get("position").context("carts.position")?,
        created_at: row.try_get("created_at").context("carts.created_at")?,
        updated_at: row.try_get("updated_at").context("carts.updated_at")?,
    })
}

fn row_to_item(row: &Row) -> Result<CartItemRow> {
    Ok(CartItemRow {
        id: bin_to_ulid(row.try_get::<_, Vec<u8>>("id").context("cart_items.id")?)?,
        ticket_variant_id: bin_to_ulid(
            row.try_get::<_, Vec<u8>>("ticket_variant_id")
                .context("cart_items.ticket_variant_id")?,
        )?,
        event_id: bin_to_ulid(
            row.try_get::<_, Vec<u8>>("event_id")
                .context("product_variants.event_id")?,
        )?,
        event_slug: row.try_get("event_slug").unwrap_or_default(),
        quantity: row.try_get("quantity").context("cart_items.quantity")?,
        unit_price_snapshot: row
            .try_get("unit_price_snapshot")
            .context("cart_items.unit_price")?,
        unit_price: row.try_get("unit_price").context("effective price")?,
        available: row.try_get("available").unwrap_or(0),
        max_per_order: row.try_get("max_per_order").ok().flatten(),
        selected: row.try_get("selected").unwrap_or(true),
        variant_name: row.try_get("variant_name").unwrap_or_default(),
        event_name: row.try_get("event_name").unwrap_or_default(),
        venue: row.try_get("venue").unwrap_or_default(),
        cover_url: row.try_get("cover_url").unwrap_or_default(),
        event_date: row.try_get("event_date").ok().flatten(),
    })
}

#[async_trait]
impl CartRepository for PgCartRepository {
    async fn get_or_create(&self, user_id: &str) -> Result<Cart> {
        let uid = id_to_vec(user_id)?;
        let new_id = ulid_to_vec(&new_ulid())?;
        let row = exec_first(&self.pool, &GET_OR_CREATE_CART, &[&new_id, &uid])
            .await?
            .context("get_or_create cart: tidak ada baris dikembalikan")?;
        row_to_cart(&row)
    }

    async fn find_active(&self, user_id: &str) -> Result<Option<Cart>> {
        let uid = id_to_vec(user_id)?;
        match exec_first(&self.pool, &FIND_ACTIVE_CART, &[&uid]).await? {
            Some(row) => Ok(Some(row_to_cart(&row)?)),
            None => Ok(None),
        }
    }

    async fn list_items(&self, cart_id: &str) -> Result<Vec<CartItemRow>> {
        let cid = id_to_vec(cart_id)?;
        let rows = exec_rows(&self.pool, &LIST_ITEMS, &[&cid]).await?;
        rows.iter().map(row_to_item).collect()
    }

    async fn upsert_item(
        &self,
        cart_id: &str,
        variant_id: &str,
        quantity: i32,
        replace: bool,
    ) -> Result<Option<i32>> {
        let cid = id_to_vec(cart_id)?;
        let vid = id_to_vec(variant_id)?;
        let item_id = ulid_to_vec(&new_ulid())?;
        let qty = quantity.clamp(1, 100);

        let row = exec_first(
            &self.pool,
            &UPSERT_ITEM,
            &[&item_id, &cid, &vid, &qty, &replace],
        )
        .await?;

        // Tidak ada baris = WHERE di SELECT tak terpenuhi: varian tak ada,
        // dinonaktifkan, atau product-nya tak aktif. Bukan error database —
        // service yang memutuskan pesan untuk pengguna.
        Ok(row.map(|r| r.get::<_, i32>("quantity")))
    }

    async fn set_quantity(&self, cart_id: &str, variant_id: &str, quantity: i32) -> Result<u64> {
        let cid = id_to_vec(cart_id)?;
        let vid = id_to_vec(variant_id)?;
        let qty = quantity.clamp(1, 100);
        super::db::exec_drop(&self.pool, SET_QTY, &[&cid, &vid, &qty]).await
    }

    async fn remove_item(&self, cart_id: &str, variant_id: &str) -> Result<u64> {
        let cid = id_to_vec(cart_id)?;
        let vid = id_to_vec(variant_id)?;
        super::db::exec_drop(&self.pool, REMOVE_ITEM, &[&cid, &vid]).await
    }

    async fn set_selected(&self, cart_id: &str, variant_id: &str, selected: bool) -> Result<u64> {
        let cid = id_to_vec(cart_id)?;
        let vid = id_to_vec(variant_id)?;
        super::db::exec_drop(&self.pool, SET_SELECTED, &[&cid, &vid, &selected]).await
    }

    async fn set_all_selected(&self, cart_id: &str, selected: bool) -> Result<u64> {
        let cid = id_to_vec(cart_id)?;
        super::db::exec_drop(&self.pool, SET_ALL_SELECTED, &[&cid, &selected]).await
    }

    async fn clear_items(&self, cart_id: &str) -> Result<u64> {
        let cid = id_to_vec(cart_id)?;
        super::db::exec_drop(&self.pool, CLEAR_ITEMS, &[&cid]).await
    }

    async fn prune_dead_items(&self, cart_id: &str) -> Result<Vec<String>> {
        let cid = id_to_vec(cart_id)?;
        let rows = exec_rows(&self.pool, PRUNE_DEAD_ITEMS, &[&cid]).await?;
        Ok(rows
            .iter()
            .map(|r| {
                let product: String = r.try_get("event_name").unwrap_or_default();
                let variant: String = r.try_get("variant_name").unwrap_or_default();
                if variant.is_empty() {
                    product
                } else {
                    format!("{product} — {variant}")
                }
            })
            .collect())
    }

    async fn update_meta(
        &self,
        cart_id: &str,
        promo_code: Option<&str>,
        discount: Decimal,
        payment_code: Option<&str>,
        position: Option<&str>,
    ) -> Result<Cart> {
        let cid = id_to_vec(cart_id)?;
        let row = exec_first(
            &self.pool,
            &UPDATE_CART_META,
            &[&cid, &promo_code, &discount, &payment_code, &position],
        )
        .await?
        .context("update_meta: keranjang tidak ditemukan")?;
        row_to_cart(&row)
    }

    async fn count_items(&self, user_id: &str) -> Result<i64> {
        let uid = id_to_vec(user_id)?;
        match exec_first(&self.pool, COUNT_ITEMS, &[&uid]).await? {
            Some(row) => Ok(row.try_get::<_, i64>("total").unwrap_or(0)),
            None => Ok(0),
        }
    }

    async fn close(&self, cart_id: &str) -> Result<u64> {
        let cid = id_to_vec(cart_id)?;
        super::db::exec_drop(&self.pool, CLOSE_CART, &[&cid]).await
    }
}
