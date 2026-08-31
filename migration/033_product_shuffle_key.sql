-- ============================================================================
-- Migration: 033_product_shuffle_key.sql — urutan tampil yang bisa diacak
-- ============================================================================
-- MASALAH
-- Urutan bawaan daftar produk adalah `ORDER BY event_date ASC` — sepenuhnya
-- tetap. Dengan satu juta produk, halaman pertama berisi produk yang sama
-- selamanya dan sisanya tak pernah dilihat siapa pun. Merchant yang produknya
-- kebetulan berada di urutan 500.000 tak punya jalan untuk pernah tampil.
--
-- KENAPA BUKAN `ORDER BY random()`
-- Dua alasan, dan keduanya fatal di skala ini:
--   1. Ia memaksa pemindaian + pengurutan SELURUH tabel pada SETIAP permintaan.
--   2. Paginasinya rusak: `random()` dinilai ulang tiap kueri, jadi halaman 2
--      diacak ulang dari awal — barang yang sama muncul dua kali dan sebagian
--      lain tak pernah muncul sama sekali.
--
-- CARANYA
-- Satu kolom acak yang TERSIMPAN, dengan indeks. `ORDER BY shuffle_key` lalu
-- menjadi pembacaan indeks murni: cepat, dan urutannya stabil sehingga paginasi
-- tetap tepat. Urutannya diubah dengan MENULIS ULANG kolomnya (lihat catatan
-- rotasi di bawah), bukan dengan mengacak saat baca.
-- ============================================================================

ALTER TABLE products
    ADD COLUMN IF NOT EXISTS shuffle_key double precision;

-- Baris lama diisi bertahap oleh operator (lihat catatan di bawah), bukan di
-- sini: satu UPDATE atas sejuta baris di dalam migrasi akan mengunci tabel dan
-- membengkakkan WAL pada VPS kecil — persis kondisi yang pernah memicu OOM.
ALTER TABLE products
    ALTER COLUMN shuffle_key SET DEFAULT random();

-- Indeks PARSIAL: hanya produk yang benar-benar bisa tampil. Itu memangkas
-- ukurannya drastis dibanding indeks penuh, dan daftar publik memang tak pernah
-- meminta yang lain.
CREATE INDEX IF NOT EXISTS idx_products_shuffle
    ON products (shuffle_key)
    WHERE deleted_at IS NULL AND status = 'active';

-- ── ROTASI ──────────────────────────────────────────────────────────────────
-- Mengacak ulang SELURUH tabel itu mahal dan tak perlu. Mengacak ulang sebagian
-- kecil secara berkala sudah cukup mengubah susunan yang terlihat, karena yang
-- dilihat orang hanyalah puluhan baris teratas. Jalankan berkala (mis. cron
-- harian); potongannya kecil supaya tak pernah menahan tabel lama-lama:
--
--   UPDATE products SET shuffle_key = random()
--    WHERE ctid IN (
--      SELECT ctid FROM products
--       WHERE deleted_at IS NULL AND status = 'active'
--       ORDER BY random() LIMIT 20000
--    );
--
-- Sama perintahnya untuk mengisi baris lama yang `shuffle_key`-nya masih NULL —
-- ganti syaratnya menjadi `WHERE shuffle_key IS NULL`, ulangi sampai 0 baris.
