-- ============================================================================
-- Migration: 010_seed_stories.sql  —  SEED USER + STORY MASSAL (uji performa)
-- ============================================================================
-- Tujuan: mengukur apakah halaman story tetap cepat saat ada RATUSAN RIBU baris
-- story tersebar di banyak user. Query penentu:
--   • list_groups (feed aktif)      → ambil SEMUA story `expires_at > NOW()`
--                                      (TANPA LIMIT) → biaya = jumlah story AKTIF
--   • list_user_groups_paged (arsip)→ GROUP BY user_id atas SELURUH tabel, lalu
--                                      paginasi → biaya = jumlah user unik + rows
--
-- Karena itu seed ini menyebar `created_at` selama 30 hari: hanya story <24 jam
-- yang "aktif" (~1/30 dari total) — realistis untuk menguji kedua jalur.
--
-- ── CARA PAKAI ──────────────────────────────────────────────────────────────
--   psql "$DATABASE_URL" -f migration/010_seed_stories.sql
--
--   Ubah dua angka di bawah untuk skala berbeda. Default: 50.000 user, 500.000
--   story (≈10 story/user, ≈16.000 aktif). Idempoten (ON CONFLICT DO NOTHING).
--
-- ── PENANDA (untuk cleanup) ─────────────────────────────────────────────────
--   Seed user  : id diawali byte 0x02  (0x01 sudah dipakai seed merchant di 007)
--   Seed story : id diawali byte 0x03
--   Blok DELETE ada di bagian paling bawah (di-comment).
-- ============================================================================

\set n_users   50000
\set n_stories 500000

-- ── 1) Bulk INSERT users (customer) ─────────────────────────────────────────
--    id 16 byte: byte0 = 0x02 (penanda seed-user), 15 byte sisanya = nomor seri.
--    password_hash & email sudah nullable sejak migrasi 002; email tetap diisi
--    unik agar menyerupai data nyata.
--    phone: WAJIB unik & non-null (constraint dari migrasi 012). Nilai bertanda
--    'seed-' + nomor seri → dijamin unik dan tak bentrok dengan nomor asli.
INSERT INTO users (id, email, password_hash, name, phone, role)
SELECT
    decode('02' || lpad(to_hex(gs), 30, '0'), 'hex'),
    'seeduser' || gs || '@pulse.local',
    NULL,
    'Seed User ' || gs,
    '62-seed-' || gs,
    'customer'
FROM generate_series(1, :n_users) AS gs
ON CONFLICT (id) DO NOTHING;

-- ── 2) Bulk INSERT stories ──────────────────────────────────────────────────
--    id  16 byte: byte0 = 0x03 (penanda seed-story), sisanya nomor seri.
--    user_id : round-robin ke :n_users user seed (1..n_users).
--    created_at : tersebar 0–30 hari lalu → hanya <24 jam yang aktif.
--    expires_at : created_at + 24 jam (samakan dengan default aplikasi).
INSERT INTO stories (
    id, user_id, media_url, media_type, filter, overlays,
    event_id, event_slug, event_title, created_at, expires_at
)
SELECT
    decode('03' || lpad(to_hex(gs), 30, '0'), 'hex'),
    decode('02' || lpad(to_hex(1 + (gs % :n_users)), 30, '0'), 'hex'),
    CASE WHEN gs % 5 = 0
         THEN 'https://image.ulalaapi.store/ticketing/seed-story.mp4'
         ELSE 'https://image.ulalaapi.store/ticketing/seed-story.jpg'
    END,
    CASE WHEN gs % 5 = 0 THEN 'video' ELSE 'image' END,
    NULL,
    '[]'::jsonb,
    NULL, NULL, NULL,
    created_at,
    created_at + INTERVAL '24 hours'
FROM (
    SELECT gs, NOW() - (random() * INTERVAL '30 days') AS created_at
    FROM generate_series(1, :n_stories) AS gs
) src
ON CONFLICT (id) DO NOTHING;

-- ── 3) Refresh statistik planner (WAJIB agar EXPLAIN akurat) ─────────────────
ANALYZE users;
ANALYZE stories;

-- ── 4) Ringkasan hasil ──────────────────────────────────────────────────────
SELECT
    (SELECT COUNT(*) FROM users   WHERE substring(id from 1 for 1) = '\x02') AS seed_users,
    (SELECT COUNT(*) FROM stories WHERE substring(id from 1 for 1) = '\x03') AS seed_stories,
    (SELECT COUNT(*) FROM stories WHERE expires_at > NOW())                  AS stories_aktif;

-- ============================================================================
-- UKUR PERFORMA — jalankan manual setelah seed (ganti $VIEWER bila perlu):
--
-- Feed aktif (list_groups) — TANPA LIMIT, ini yang paling berisiko lambat:
--   EXPLAIN (ANALYZE, BUFFERS)
--   SELECT s.id FROM stories s
--   JOIN users u ON u.id = s.user_id
--   WHERE s.expires_at > NOW()
--   ORDER BY s.user_id, s.created_at ASC;
--
-- Arsip paginasi (list_user_groups_paged) — GROUP BY seluruh tabel:
--   EXPLAIN (ANALYZE, BUFFERS)
--   WITH paged_users AS (
--     SELECT user_id, MAX(created_at) latest_at
--     FROM stories GROUP BY user_id ORDER BY latest_at DESC LIMIT 20 OFFSET 0
--   )
--   SELECT s.id FROM paged_users pu
--   JOIN stories s ON s.user_id = pu.user_id
--   ORDER BY pu.latest_at DESC, s.user_id, s.created_at ASC;
-- ============================================================================

-- ── ROLLBACK (hapus seed) — uncomment untuk membersihkan ────────────────────
-- Menghapus user seed otomatis meng-cascade story-nya (ON DELETE CASCADE),
-- tapi hapus story dulu agar cepat & eksplisit:
--   DELETE FROM stories WHERE substring(id from 1 for 1) = '\x03';
--   DELETE FROM users   WHERE substring(id from 1 for 1) = '\x02';
  