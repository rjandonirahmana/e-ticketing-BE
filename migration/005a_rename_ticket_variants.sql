-- Migration: 005a_rename_ticket_variants.sql — luruskan kembali sejarah nama
-- tabel varian tiket.
--
-- ── MASALAH ─────────────────────────────────────────────────────────────────
-- `001.sql` membuat tabel bernama `ticket_variants`. Mulai `006_perf_indexes.sql`
-- dan SETERUSNYA — termasuk seluruh kode aplikasi — yang dipakai adalah
-- `event_variants`. Tak ada satu pun `ALTER TABLE … RENAME` di antara keduanya
-- (sudah dicek case-insensitive di seluruh folder ini).
--
-- Artinya database yang berjalan sekarang memang punya `event_variants` —
-- seseorang me-rename-nya dengan tangan dan tak pernah mencatatnya. Yang rusak
-- adalah kemampuan MEMBANGUN ULANG dari nol: menjalankan `migration/*.sql`
-- berurutan di database kosong akan gagal di 006, karena tabel yang hendak
-- diberi index belum pernah ada dengan nama itu.
--
-- Itu bukan masalah teoretis. Ia menghantam tepat saat paling genting: menyiapkan
-- server baru, memulihkan dari cadangan, atau membuat database untuk pengujian
-- terintegrasi — ketiganya justru momen ketika sejarah migrasi harus bisa
-- dipercaya sepenuhnya.
--
-- ── KENAPA DINOMORI 005a ────────────────────────────────────────────────────
-- Berkas dijalankan berurutan menurut nama (`for f in migration/*.sql`), jadi
-- rename ini WAJIB berada sebelum 006 yang memakai nama barunya. `005a` duduk
-- persis di antara `005_` dan `006_` secara leksikografis. Menomorinya di
-- belakang (mis. 021) tak akan menolong: replay dari nol tetap gagal di 006.
--
-- Sejarah yang sudah ada TIDAK ditulis ulang. `001.sql` dibiarkan apa adanya
-- supaya database yang sudah pernah menjalankannya tetap cocok dengan catatannya.
--
-- Idempotent — aman pada database yang sudah terlanjur di-rename manual maupun
-- yang masih memakai nama lama.
--   psql "$DATABASE_URL" -f migration/005a_rename_ticket_variants.sql

DO $$
BEGIN
    -- Empat keadaan yang mungkin, dan ketiganya yang bukan "perlu rename"
    -- dibiarkan diam-diam supaya berkas ini aman dijalankan berulang.
    IF to_regclass('public.event_variants') IS NOT NULL THEN
        RAISE NOTICE 'event_variants sudah ada — tak ada yang di-rename.';
        RETURN;
    END IF;

    IF to_regclass('public.ticket_variants') IS NULL THEN
        -- Database baru yang belum menjalankan 001 sama sekali: bukan tugas
        -- berkas ini untuk membuat tabelnya.
        RAISE NOTICE 'ticket_variants tak ada — dilewati.';
        RETURN;
    END IF;

    ALTER TABLE ticket_variants RENAME TO event_variants;
    RAISE NOTICE 'ticket_variants → event_variants.';

    -- Constraint & index bawaan ikut terbawa oleh RENAME, tapi NAMANYA tetap
    -- memakai awalan lama (`ticket_variants_pkey`, dst). Itu tak mengganggu
    -- fungsi apa pun — PostgreSQL tak peduli nama constraint — dan sengaja
    -- TIDAK ikut diganti: migrasi lain merujuknya dengan nama lama, dan
    -- mengganti nama demi kerapian akan memutus rujukan itu tanpa manfaat.
END $$;

-- =============================================================================
-- VERIFIKASI MANUAL:
--   SELECT to_regclass('public.event_variants'), to_regclass('public.ticket_variants');
--   -- Harus: event_variants terisi, ticket_variants NULL.
-- =============================================================================
