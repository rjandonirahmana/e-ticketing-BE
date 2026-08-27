-- ============================================================================
-- Migration: 026_variant_image.sql
-- Gambar per varian produk, dan pensiunnya jenis foto detail.
-- ============================================================================
--
-- ATURAN PENULISAN (sama dengan 022, 023, 024)
--   1. TIDAK ADA titik-koma di dalam komentar.
--   2. TIDAK ADA apostrof di dalam komentar. Pakai backtick.
--   3. TIDAK ADA blok dollar-quote.
--   4. TIDAK ADA foreign key di dalam CREATE TABLE.
--
-- ── KENAPA ──────────────────────────────────────────────────────────────────
-- Aplikasi ini tidak lagi menjual tiket acara, melainkan barang. Dua hal
-- berubah karenanya, dan keduanya cuma soal data.
--
-- 1. VARIAN PUNYA WAJAHNYA SENDIRI. Pada tiket, varian adalah kelas duduk --
--    Reguler dan VIP terlihat sama di foto. Pada barang, varian adalah warna
--    atau model, dan pembeli memilihnya justru DENGAN MELIHAT. Tanpa kolom ini
--    pemilih varian hanya berisi nama dan harga, dan pembeli menebak.
--
-- 2. FOTO DETAIL TIDAK LAGI BERJENIS. Tiap elemen `products.detail_images`
--    dulu membawa `image_type` yang memilah `map` (denah lokasi), `seat` (peta
--    kursi), dan `price` (info harga) -- semuanya konsep acara yang tak punya
--    arti bagi sebuah barang. Yang tersisa hanya foto produk biasa yang
--    digeser-geser di halaman detail.
--
-- ── CATATAN BENTUK DATA ─────────────────────────────────────────────────────
-- Foto detail TIDAK punya tabelnya sendiri. Ia tinggal sebagai larik JSONB di
-- `products.detail_images` (lihat `repository/product/helpers.rs`). Jadi tak ada
-- kolom yang bisa di-ALTER dan tak ada index urutan yang perlu dibuat: urutan
-- tampil adalah urutan elemen di dalam larik itu.
--
--   psql "$DATABASE_URL" -f migration/026_variant_image.sql
-- ============================================================================

-- ── 1. Gambar varian ────────────────────────────────────────────────────────
-- NULL = varian tanpa gambar sendiri. Halaman detail jatuh ke cover produk,
-- jadi varian lama tetap tampil wajar tanpa perlu disentuh satu per satu.
ALTER TABLE product_variants ADD COLUMN IF NOT EXISTS image_url TEXT NULL;

-- ── 2. Seragamkan jenis foto detail yang lama ───────────────────────────────
-- Kode baru berhenti menawarkan pilihan jenis dan selalu menulis `other`. Tanpa
-- langkah ini, satu produk lama bisa memuat campuran `map`, `seat`, dan `other`
-- di dalam larik yang sama -- tak merusak apa pun, tapi menyisakan data yang
-- artinya sudah tak ada lagi dan akan membingungkan siapa pun yang membacanya
-- setahun dari sekarang.
--
-- `elem - 'image_type'` membuang kuncinya lebih dulu, lalu `||` memasangnya
-- kembali dengan satu nilai. Menggabung tanpa membuang akan menyisakan kunci
-- ganda pada sebagian versi PostgreSQL.
--
-- Penjaga di WHERE penting: `jsonb_agg` atas larik kosong menghasilkan NULL,
-- yang akan MENGHAPUS galeri produk yang kebetulan belum berisi foto.
UPDATE products
SET detail_images = (
        SELECT jsonb_agg(elem - 'image_type' || jsonb_build_object('image_type', 'other'))
        FROM jsonb_array_elements(detail_images) AS elem
    )
WHERE detail_images IS NOT NULL
  AND jsonb_typeof(detail_images) = 'array'
  AND jsonb_array_length(detail_images) > 0;
