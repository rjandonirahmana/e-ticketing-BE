-- ============================================================================
-- Migration: 009_orders_stories_perf.sql
-- ============================================================================
-- 1) /orders lambat: query list order sudah diperbaiki di kode (LATERAL,
--    lihat repository/order.rs) TETAPI kecepatannya bergantung pada index
--    (customer_id, created_at DESC) dari migration 006. Kalau 006 belum pernah
--    dijalankan di DB live, Postgres seq-scan + sort SELURUH tabel orders di
--    tiap kunjungan /orders. Semua index di file ini idempoten (IF NOT EXISTS)
--    — jalankan file ini saja sudah cukup menambal DB yang tertinggal.
--
-- 2) /stories versi baru: 1 kartu per user → query grouping per user butuh
--    index (user_id, created_at DESC).
--
--   psql "$DATABASE_URL" -f migration/009_orders_stories_perf.sql
--
--   Catatan produksi: untuk tabel besar dengan trafik live, ganti menjadi
--   CREATE INDEX CONCURRENTLY dan jalankan di luar transaksi.

-- ── /orders ──────────────────────────────────────────────────────────────────
-- Halaman order per customer, terbaru dulu (re-assert dari 006).
CREATE INDEX IF NOT EXISTS idx_orders_customer_date
    ON orders (customer_id, created_at DESC);

-- LATERAL "ambil item pertama per order" (ORDER BY oi.created_at LIMIT 1):
-- composite ini membuatnya murni index-walk tanpa sort per order.
--
-- DIBUNGKUS penjaga karena `023_products_rename.sql` MEMBUANG `order_items` —
-- perannya diambil `cart_items`, yang punya indexnya sendiri di 022. Tanpa
-- penjaga ini, memutar ulang 009 di database yang sudah melewati 023 mati
-- dengan `relation "order_items" does not exist`, dan aplikasinya tak start.
DO $$
BEGIN
    IF to_regclass('public.order_items') IS NOT NULL THEN
        CREATE INDEX IF NOT EXISTS idx_order_items_order_created
            ON order_items (order_id, created_at);
    END IF;
END $$;

-- ── /stories (arsip per user) ────────────────────────────────────────────────
-- Grouping "story terbaru per user" + fetch semua story milik satu user.
CREATE INDEX IF NOT EXISTS idx_stories_user_created
    ON stories (user_id, created_at DESC);

-- Urutan arsip global terbaru-dulu.
CREATE INDEX IF NOT EXISTS idx_stories_created_desc
    ON stories (created_at DESC);

ANALYZE orders;
ANALYZE stories;

DO $$
BEGIN
    IF to_regclass('public.order_items') IS NOT NULL THEN
        ANALYZE order_items;
    END IF;
END $$;
