-- ============================================================================
-- Migration: 001b_notifications.sql
-- Tabel `notifications` — juga lahir di luar riwayat migrasi.
-- ============================================================================
--
-- ── KENAPA ADA ──────────────────────────────────────────────────────────────
-- `repository/notification.rs` membaca dan menulis tabel `notifications` sejak
-- lama, dan `006_perf_indexes.sql` membuat dua index di atasnya — tetapi tak
-- ada satu pun berkas yang MEMBUAT tabelnya. Ia dibuat dengan tangan di
-- database berjalan, persis seperti kolom-kolom di `001a`.
--
-- Pada database kosong, 006 karena itu berhenti di
-- `ERROR: relation "notifications" does not exist`, dan seandainya index itu
-- dilewati, halaman notifikasi gagal pada query pertama.
--
-- ── BENTUKNYA: POLYMORPHIC, BUKAN SATU TABEL PER JENIS ──────────────────────
-- Satu baris menunjuk satu benda lain lewat pasangan (`kind`, `target_id`):
--   kind = `order`  → orders.id
--   kind = `ticket` → tickets.id
--   kind = `story`  → stories.id
--
-- Karena tabel tujuannya berbeda-beda, `target_id` SENGAJA TANPA foreign key —
-- tak ada satu tabel pun yang bisa dirujuk. Konsekuensinya dipikul di sisi
-- baca: `FIND_DETAIL` memakai correlated subquery per `kind`, dan target yang
-- sudah terhapus muncul sebagai NULL, bukan sebagai baris notifikasi yang
-- hilang. Itu perilaku yang diinginkan — riwayat notifikasi tetap utuh.
--
-- `kind` tidak diberi CHECK: nilainya ditentukan `models::notification::kind`,
-- dan jenis baru harus bisa lahir tanpa migrasi.
--
-- Idempoten — aman dijalankan berulang.
--   psql "$DATABASE_URL" -f migration/001b_notifications.sql
-- ============================================================================

CREATE TABLE IF NOT EXISTS notifications (
    id          BYTEA        NOT NULL PRIMARY KEY,   -- ULID 16 byte
    user_id     BYTEA        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind        VARCHAR(30)  NOT NULL,
    title       TEXT         NOT NULL,
    body        TEXT         NOT NULL DEFAULT '',
    is_read     BOOLEAN      NOT NULL DEFAULT FALSE,
    -- Tanpa FK: tabel tujuannya ditentukan `kind` (lihat catatan di atas).
    target_id   BYTEA        NULL,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Index daftar dan badge belum-dibaca dibuat oleh `006_perf_indexes.sql`,
-- supaya seluruh index hot-path tetap terkumpul di satu berkas.
