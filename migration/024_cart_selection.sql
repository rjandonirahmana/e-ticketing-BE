-- ============================================================================
-- Migration: 024_cart_selection.sql
-- Pilih-pilih barang di keranjang sebelum bayar.
-- ============================================================================
--
-- ATURAN PENULISAN (sama dengan 022 dan 023)
--   1. TIDAK ADA titik-koma di dalam komentar.
--   2. TIDAK ADA apostrof di dalam komentar. Pakai backtick.
--   3. TIDAK ADA blok dollar-quote.
--   4. TIDAK ADA foreign key di dalam CREATE TABLE.
--
-- ── APA YANG DITAMBAHKAN ────────────────────────────────────────────────────
-- Satu kolom: `cart_items.selected`. Default TRUE supaya keranjang yang sudah
-- ada tetap berperilaku persis seperti sebelumnya -- semua ikut dibayar.
--
-- Saat checkout, hanya baris `selected` yang menjadi pesanan. Baris yang tidak
-- dipilih TIDAK boleh ikut terkunci di dalam keranjang yang ditutup, jadi
-- transaksi order memindahkannya ke keranjang baru yang masih terbuka sebelum
-- keranjang lama ditutup. Tanpa langkah itu, barang yang sengaja ditunda justru
-- lenyap dari keranjang -- persis kebalikan dari yang diminta pembeli.
--
--   psql "$DATABASE_URL" -f migration/024_cart_selection.sql
-- ============================================================================

ALTER TABLE cart_items ADD COLUMN IF NOT EXISTS selected BOOLEAN NOT NULL DEFAULT TRUE;

-- Ringkasan harga hanya menjumlah baris terpilih, dan itu dilakukan pada setiap
-- pembukaan halaman keranjang.
CREATE INDEX IF NOT EXISTS idx_cart_items_selected
    ON cart_items (cart_id)
    WHERE selected;
