-- ============================================================================
-- Migration: 030_soft_delete.sql
-- Data inti TIDAK PERNAH dihapus permanen — hanya ditandai.
-- ============================================================================
--
-- ATURAN PENULISAN (sama dengan 022, 023, 024, 026, 027)
--   1. TIDAK ADA titik-koma di dalam komentar.
--   2. TIDAK ADA apostrof di dalam komentar. Pakai backtick.
--   3. TIDAK ADA blok dollar-quote.
--
-- ── KENAPA ──────────────────────────────────────────────────────────────────
-- `DELETE FROM products` yang sekarang berjalan bukan sekadar membuang satu
-- baris. Produk adalah pusat dari jaring relasi: `product_variants`,
-- `cart_items`, `orders`, `tickets`, `banners`, `group_rooms`, ulasan, dan
-- riwayat penjualan semuanya menunjuk ke sana.
--
-- Menghapusnya berarti salah satu dari dua hal, dan keduanya buruk:
--   * CASCADE ikut membawa pesanan dan tiket yang SUDAH DIBAYAR orang, atau
--   * RESTRICT membuat produk yang pernah laku tak bisa dihapus selamanya,
--     sehingga merchant terjebak dengan katalog yang tak bisa dirapikan.
--
-- Penandaan menyelesaikan keduanya: produk hilang dari etalase, sementara
-- seluruh riwayat yang menunjuk kepadanya tetap utuh dan tetap bisa dibaca.
-- Pembeli yang membuka pesanan lamanya tetap melihat barang apa yang ia beli.
--
-- ── YANG TETAP BOLEH DIHAPUS PERMANEN ───────────────────────────────────────
-- `cart_items` dan `carts` yang belum menjadi pesanan. Isi keranjang bukan
-- riwayat — ia niat yang belum terjadi, tak ada yang menunjuk kepadanya, dan
-- menyimpannya selamanya hanya menumpuk baris tanpa arti.
--
-- Begitu pula `merchant_follows` (ikuti/berhenti ikuti adalah sakelar) dan
-- `refresh_tokens` yang kedaluwarsa.
--
--   psql "$DATABASE_URL" -f migration/030_soft_delete.sql
-- ============================================================================

-- ── 1. Kolom penanda ────────────────────────────────────────────────────────
-- NULL = hidup. Ini disengaja, bukan boolean `is_deleted`: yang ingin diketahui
-- saat menelusuri masalah bukan hanya APAKAH sesuatu dibuang, melainkan KAPAN.
ALTER TABLE products          ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ NULL;
ALTER TABLE product_variants  ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ NULL;
ALTER TABLE users             ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ NULL;
ALTER TABLE merchant_details  ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ NULL;

-- ── 2. Index PARSIAL, bukan index biasa ─────────────────────────────────────
-- Hampir setiap pembacaan katalog kini berakhiran `AND deleted_at IS NULL`, dan
-- barisnya hampir selalu hidup. Index parsial hanya memuat baris yang hidup:
-- lebih kecil daripada index penuh, dan perencana query bisa memakainya
-- langsung tanpa memeriksa ulang syaratnya.
CREATE INDEX IF NOT EXISTS idx_products_hidup
    ON products (merchant_id, created_at DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_product_variants_hidup
    ON product_variants (event_id)
    WHERE deleted_at IS NULL;

-- ── 3. CATATAN: keunikan `slug` SENGAJA TIDAK DISENTUH ──────────────────────
-- Rencana awal migrasi ini mengganti keunikan `products.slug` menjadi unique
-- PARSIAL (`WHERE deleted_at IS NULL`), supaya produk yang dibuang tak
-- menyandera slug-nya selamanya.
--
-- Itu dibatalkan karena satu hal yang tak bisa dipastikan dari repo ini: kolom
-- `slug` TIDAK PERNAH muncul di berkas `migration/` mana pun. Ia lahir di luar
-- riwayat migrasi, jadi tak ada yang tahu apakah ia sudah unik, dan
-- `CREATE UNIQUE INDEX` akan GAGAL bila ternyata sudah ada slug kembar --
-- menggagalkan seluruh startup karena migrasi berjalan otomatis.
--
-- Periksa dulu di server sebelum menambahkannya:
--
--   SELECT slug, COUNT(*) FROM products GROUP BY slug HAVING COUNT(*) > 1
--
-- Bila kosong dan keunikan itu memang diinginkan, tambahkan lewat migrasi
-- tersendiri. Sampai saat itu, produk yang dibuang memang masih memegang
-- slug-nya -- konsekuensi yang jauh lebih ringan daripada startup yang mati.
