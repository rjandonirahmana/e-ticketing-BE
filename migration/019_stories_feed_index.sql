-- Migration: 019_stories_feed_index.sql — index feed story-bar aktif
--
-- Konteks: list_groups()/list_groups_public() (repository/story.rs) kini memakai
-- CTE `active_users`:
--
--     SELECT user_id, MAX(created_at)
--     FROM   stories
--     WHERE  expires_at > NOW()
--     GROUP  BY user_id
--     ORDER  BY MAX(created_at) DESC
--     LIMIT  60
--
-- Tujuan index ini: buat agregasi CTE itu INDEX-ONLY. Tanpa `expires_at` di
-- index, Postgres harus fetch heap tiap baris hanya untuk mengecek
-- `expires_at > NOW()`. Dengan (user_id, created_at DESC, expires_at) ketiga
-- kolom yang dibutuhkan CTE ada di index → Index-Only Scan, tanpa heap.
--
-- Index lama `idx_stories_user_created (user_id, created_at DESC)` (migrasi 009)
-- menjadi PREFIX dari index ini → boleh DROP agar tak menduplikasi biaya tulis.
-- Baris DROP di bawah opsional; hapus komentarnya bila ingin merapikan.
--
-- Catatan: seperti migrasi index lain, JIKA tabel sudah besar, jalankan dengan
-- CREATE INDEX CONCURRENTLY di LUAR transaksi (psql -f, BUKAN -1) agar tak
-- mengunci tabel:
--   psql "$DATABASE_URL" -f migration/019_stories_feed_index.sql

CREATE INDEX IF NOT EXISTS idx_stories_feed_active
    ON stories (user_id, created_at DESC, expires_at);

-- DROP INDEX IF EXISTS idx_stories_user_created;  -- (opsional) prefix dari index baru

ANALYZE stories;
