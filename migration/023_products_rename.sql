-- ============================================================================
-- Migration: 023_products_rename.sql
-- events menjadi products, dan order_items dilebur ke cart_items.
-- ============================================================================
--
-- ATURAN PENULISAN BERKAS INI (sama dengan 022)
--   1. TIDAK ADA titik-koma di dalam komentar.
--   2. TIDAK ADA apostrof di dalam komentar. Pakai backtick.
--   3. TIDAK ADA blok dollar-quote.
--   4. TIDAK ADA foreign key di dalam CREATE TABLE.
--
-- ── APA YANG BERUBAH ────────────────────────────────────────────────────────
--   1. Tabel `events` menjadi `products`, `event_variants` menjadi
--      `product_variants`.
--   2. `order_items` DIHAPUS. Baris pesanan kini tinggal di `cart_items`, dan
--      `orders.cart_id` yang menghubungkannya.
--   3. `tickets` dibuat ulang menunjuk `cart_items`.
--
-- ── KENAPA HANYA NAMA TABEL YANG DI-RENAME, BUKAN NAMA KOLOM ────────────────
-- Kolom `event_id` di `product_variants`, `banners`, `group_rooms`, dan tabel
-- lain SENGAJA dibiarkan. Alasannya bukan malas: `ALTER TABLE IF EXISTS x
-- RENAME TO y` bersifat aman dijalankan ulang, sedangkan RENAME COLUMN tidak
-- punya padanan `IF EXISTS` dan akan menggagalkan seluruh berkas pada
-- percobaan kedua. Mengingat riwayat migrasi di database ini yang sering
-- berhenti separuh jalan, keamanan menjalankan ulang lebih berharga daripada
-- kerapian nama kolom. Rename kolom bisa menyusul sebagai langkah tersendiri.
--
-- ── DATA LAMA ───────────────────────────────────────────────────────────────
-- `order_items` dan `tickets` lama DIBUANG beserta isinya, sesuai keputusan
-- bahwa isi database ini data uji. Order lama tetap ada di tabel `orders`,
-- tetapi tak lagi punya rincian baris maupun tiket.
--
--   psql "$DATABASE_URL" -f migration/023_products_rename.sql
-- ============================================================================


-- ════════════════════════════════════════════════════════════════════════════
-- BAGIAN 1 -- RENAME TABEL
-- ════════════════════════════════════════════════════════════════════════════
-- `IF EXISTS` membuat kedua baris ini aman dijalankan ulang: pada percobaan
-- kedua tabel lamanya sudah tidak ada, dan Postgres hanya memberi NOTICE.

ALTER TABLE IF EXISTS events RENAME TO products;

ALTER TABLE IF EXISTS event_variants RENAME TO product_variants;


-- ════════════════════════════════════════════════════════════════════════════
-- BAGIAN 2 -- BUANG order_items DAN tickets LAMA
-- ════════════════════════════════════════════════════════════════════════════
-- CASCADE dipakai karena `tickets` menunjuk `order_items`. Urutannya tetap
-- ditulis eksplisit supaya jelas apa yang ikut terbuang.

DROP TABLE IF EXISTS tickets CASCADE;

DROP TABLE IF EXISTS order_items CASCADE;


-- ════════════════════════════════════════════════════════════════════════════
-- BAGIAN 3 -- cart_items KINI MEMUAT BARIS PESANAN
-- ════════════════════════════════════════════════════════════════════════════
-- Konsekuensi yang harus ditangani: sebelumnya varian yang dihapus merchant
-- boleh menyeret baris keranjang ikut lenyap (CASCADE). Sekarang baris yang
-- sama juga menjadi rincian pesanan yang sudah dibayar, jadi aturan itu harus
-- dibalik menjadi RESTRICT -- varian yang pernah TERJUAL tidak boleh bisa
-- dihapus.
--
-- Agar merchant tetap bisa menghapus varian yang belum pernah laku, kode
-- penghapus varian lebih dulu membuang baris varian itu dari keranjang yang
-- masih TERBUKA (`carts.deleted_at IS NULL`). Yang tersisa dan menghalangi
-- hanyalah keranjang yang sudah menjadi pesanan -- dan itu memang yang kita
-- ingin halangi.

-- ── ARTI `unit_price` BERUBAH SAAT KERANJANG DITUTUP ────────────────────────
-- Selama keranjang masih TERBUKA, kolom itu berarti "harga yang dilihat pembeli
-- ketika memasukkan barang", dan selisihnya terhadap harga berlaku dipakai
-- menampilkan "harga berubah sejak Anda menambahkan".
--
-- Saat checkout, transaksi order menimpanya dengan harga yang BARU DIKUNCI dari
-- `product_variants`, lalu menutup keranjangnya. Sejak detik itu kolom tersebut
-- berarti "harga yang ditagihkan" -- peran yang dulu dipegang
-- `order_items.unit_price`. Tak ada kolom tambahan: yang membedakan kedua arti
-- itu adalah `carts.deleted_at`, dan keranjang yang sudah tertutup tak pernah
-- lagi bisa disunting karena seluruh operasi keranjang hanya menyentuh baris
-- dengan `deleted_at IS NULL`.

ALTER TABLE cart_items DROP CONSTRAINT IF EXISTS fk_cart_items_variant;
ALTER TABLE cart_items ADD CONSTRAINT fk_cart_items_variant
    FOREIGN KEY (ticket_variant_id) REFERENCES product_variants(id) ON DELETE RESTRICT;


-- ════════════════════════════════════════════════════════════════════════════
-- BAGIAN 4 -- TABEL tickets BARU
-- ════════════════════════════════════════════════════════════════════════════
-- Satu baris per tiket. `cart_item_id` menggantikan `order_item_id`.
-- `order_id` tetap disimpan langsung meski bisa ditelusuri lewat
-- cart_items -> carts -> orders: jalur terpanas aplikasi ini adalah pemindaian
-- tiket di gerbang masuk, dan di sana satu join lebih sedikit berarti.

CREATE TABLE IF NOT EXISTS tickets (
    id           BYTEA        PRIMARY KEY,
    cart_item_id BYTEA        NOT NULL,
    order_id     BYTEA        NOT NULL,
    ticket_code  VARCHAR(100) NOT NULL,
    status       VARCHAR(20)  NOT NULL DEFAULT 'active'
                              CHECK (status IN ('active','used','refunded','expired')),
    used_at      TIMESTAMPTZ,
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);


-- ════════════════════════════════════════════════════════════════════════════
-- BAGIAN 5 -- INDEX
-- ════════════════════════════════════════════════════════════════════════════

CREATE UNIQUE INDEX IF NOT EXISTS uniq_tickets_code
    ON tickets (ticket_code);

CREATE INDEX IF NOT EXISTS idx_tickets_order
    ON tickets (order_id);

CREATE INDEX IF NOT EXISTS idx_tickets_cart_item
    ON tickets (cart_item_id);


-- ════════════════════════════════════════════════════════════════════════════
-- BAGIAN 6 -- FOREIGN KEY
-- ════════════════════════════════════════════════════════════════════════════
-- Ditambahkan terpisah dan paling akhir, sesuai aturan 4.

ALTER TABLE tickets DROP CONSTRAINT IF EXISTS fk_tickets_cart_item;
ALTER TABLE tickets ADD CONSTRAINT fk_tickets_cart_item
    FOREIGN KEY (cart_item_id) REFERENCES cart_items(id) ON DELETE CASCADE;

ALTER TABLE tickets DROP CONSTRAINT IF EXISTS fk_tickets_order;
ALTER TABLE tickets ADD CONSTRAINT fk_tickets_order
    FOREIGN KEY (order_id) REFERENCES orders(id) ON DELETE CASCADE;
