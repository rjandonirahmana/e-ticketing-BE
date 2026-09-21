-- ============================================================================
-- Migration: 001a_events_content_columns.sql
-- Kolom isi produk yang LAHIR DI LUAR riwayat migrasi.
-- ============================================================================
--
-- ── KENAPA ADA ──────────────────────────────────────────────────────────────
-- `slug`, `cover_url`, `detail_images`, dan `category` dipakai di hampir setiap
-- query produk (`repository/product/helpers.rs`: EVENT_COLS, INSERT_EVENT,
-- UPDATE_EVENT), tetapi TIDAK PERNAH dibuat oleh satu pun berkas migrasi.
-- Keempatnya ditambahkan dengan tangan langsung ke database berjalan, dan
-- migrasi 030 sudah mencatat keanehan itu untuk `slug`.
--
-- Akibatnya database KOSONG tak pernah bisa dibangun dari berkas-berkas ini:
--   * `006_perf_indexes.sql` berhenti di `ERROR: column "category" does not exist`
--     saat membuat index GIN kategori,
--   * `026_variant_image.sql` berhenti di `detail_images does not exist`,
--   * dan seandainya keduanya dilewati, aplikasinya tetap gagal pada query
--     produk pertama.
--
-- Berkas ini menuliskan apa yang sudah menjadi kenyataan di produksi, supaya
-- "bangun ulang dari nol" menghasilkan skema yang SAMA dengan yang berjalan.
--
-- ── KENAPA MEMERIKSA DUA NAMA TABEL ─────────────────────────────────────────
-- Berkas ini duduk sebelum `023_products_rename.sql`, jadi pada database baru
-- tabelnya masih bernama `events`. Tetapi pada database LAMA yang sudah
-- melewati 023, tabel itu sudah bernama `products` dan `events` tidak ada lagi
-- — `ALTER TABLE events` di sana akan menggagalkan seluruh deploy. Karena
-- berkas ini BARU, penjalan migrasi akan mencobanya juga di database lama itu
-- (ia belum tercatat di `schema_migrations`), jadi ia harus benar pada
-- keduanya.
--
-- Idempoten: semuanya `IF NOT EXISTS`, aman dijalankan berulang.
--   psql "$DATABASE_URL" -f migration/001a_events_content_columns.sql
-- ============================================================================

DO $$
DECLARE
    tabel TEXT := COALESCE(to_regclass('public.products')::text,
                            to_regclass('public.events')::text);
BEGIN
    IF tabel IS NULL THEN
        RAISE EXCEPTION 'tabel events/products tidak ada — 001.sql belum jalan';
    END IF;

    -- Kunci URL publik `/products/{slug}`. Dibiarkan NULL-able: produk draf
    -- boleh belum punya slug, dan keunikannya SENGAJA tidak dipaksakan di sini
    -- (lihat migration-manual/040_products_slug_unique.sql — database lama bisa
    -- memuat slug kembar, dan memilih pemenangnya adalah keputusan manusia).
    EXECUTE format('ALTER TABLE %I ADD COLUMN IF NOT EXISTS slug TEXT', tabel);

    -- Foto sampul. NULL = belum diunggah; UI jatuh ke placeholder.
    EXECUTE format('ALTER TABLE %I ADD COLUMN IF NOT EXISTS cover_url TEXT', tabel);

    -- Galeri foto detail sebagai larik JSONB — bukan tabel sendiri, karena
    -- urutan tampilnya adalah urutan elemen di dalam larik (lihat 026).
    EXECUTE format(
        'ALTER TABLE %I ADD COLUMN IF NOT EXISTS detail_images JSONB DEFAULT ''[]''::jsonb',
        tabel);

    -- Kategori sebagai larik JSONB. Bentuk larik dipilih karena filter explore
    -- memakai `category @> $1::jsonb` (repository/product/mod.rs), yang butuh
    -- index GIN — dan index itulah yang dibuat 006_perf_indexes.sql.
    EXECUTE format(
        'ALTER TABLE %I ADD COLUMN IF NOT EXISTS category JSONB DEFAULT ''[]''::jsonb',
        tabel);

    -- Detail produk dibuka lewat slug, bukan id (`FIND_EVENT_WITH_VARIANTS_BY_SLUG`).
    -- Index biasa, bukan unik: keunikan diputuskan di berkas manual.
    -- Namanya dipatok `idx_products_slug` pada kedua jalur, supaya database
    -- yang dibangun dari nol dan yang sudah berjalan memberi nama yang SAMA —
    -- kalau tidak, `DROP INDEX` di migrasi mana pun kelak hanya kena separuh.
    EXECUTE format(
        'CREATE INDEX IF NOT EXISTS idx_products_slug ON %I (slug)', tabel);
END $$;
