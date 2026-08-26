-- ============================================================================
-- Migration: 025_refresh_tokens.sql
-- Refresh token yang sebenarnya.
-- ============================================================================
--
-- ATURAN PENULISAN (sama dengan 022-024)
--   1. TIDAK ADA titik-koma di dalam komentar.
--   2. TIDAK ADA apostrof di dalam komentar. Pakai backtick.
--   3. TIDAK ADA blok dollar-quote.
--   4. TIDAK ADA foreign key di dalam CREATE TABLE.
--
-- ── MASALAH YANG DISELESAIKAN ───────────────────────────────────────────────
-- Sebelum ini, endpoint login mengembalikan access token yang SAMA di kedua
-- field:
--     access_token  = X
--     refresh_token = X
-- dan endpoint refresh menerima apa pun yang lolos verifikasi JWT. Akibatnya:
--   • access token yang bocor bisa terus ditukar jadi token baru,
--   • tidak ada rotasi, tidak ada pencabutan,
--   • logout hanya meminta klien membuang tokennya sendiri.
--
-- ── BENTUK BARU ─────────────────────────────────────────────────────────────
-- Refresh token kini OPAQUE: 32 byte acak, bukan JWT. Pilihan itu bukan sekadar
-- selera. Karena bentuknya berbeda sama sekali dari access token, memakai
-- access token sebagai refresh token menjadi MUSTAHIL SECARA STRUKTUR -- bukan
-- sekadar dilarang oleh pemeriksaan yang bisa lupa ditulis.
--
-- Yang disimpan di sini adalah SHA-256 dari token itu, bukan tokennya. Bocornya
-- isi tabel ini tidak memberi penyerang satu pun token yang bisa dipakai.
--
-- `family_id` mengikat seluruh rantai rotasi. Bila token yang SUDAH dicabut
-- dipakai lagi, itu tanda tokennya dicuri (pemilik sah dan pencuri sama-sama
-- memegang salinan) -- seluruh keluarga langsung dicabut, dan keduanya harus
-- login ulang. Lebih baik pemilik sah kerepotan sekali daripada pencuri
-- mendapat akses permanen.
--
--   psql "$DATABASE_URL" -f migration/025_refresh_tokens.sql
-- ============================================================================


-- ── BAGIAN 1 -- TABEL ───────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS refresh_tokens (
    id          BYTEA       PRIMARY KEY,
    user_id     BYTEA       NOT NULL,

    -- SHA-256 heksadesimal dari token, bukan tokennya.
    token_hash  TEXT        NOT NULL,

    -- Rantai rotasi. Semua turunan dari satu login berbagi nilai yang sama.
    family_id   BYTEA       NOT NULL,

    expires_at  TIMESTAMPTZ NOT NULL,
    revoked_at  TIMESTAMPTZ,

    -- Token yang menggantikan baris ini saat rotasi. Untuk jejak audit.
    replaced_by BYTEA,

    -- Membantu pemilik akun mengenali perangkat saat melihat daftar sesi.
    user_agent  VARCHAR(255) NOT NULL DEFAULT '',

    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);


-- ── BAGIAN 2 -- INDEX ───────────────────────────────────────────────────────

-- Pencarian saat refresh selalu lewat hash. Unik supaya dua baris tak mungkin
-- mengklaim token yang sama.
CREATE UNIQUE INDEX IF NOT EXISTS uniq_refresh_token_hash
    ON refresh_tokens (token_hash);

-- Daftar sesi aktif milik satu user, dan pencabutan massal saat logout semua
-- perangkat.
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user
    ON refresh_tokens (user_id)
    WHERE revoked_at IS NULL;

-- Pencabutan satu keluarga saat terdeteksi token dipakai ulang.
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_family
    ON refresh_tokens (family_id);

-- Pembersihan token kedaluwarsa.
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expiry
    ON refresh_tokens (expires_at);


-- ── BAGIAN 3 -- FOREIGN KEY ─────────────────────────────────────────────────

ALTER TABLE refresh_tokens DROP CONSTRAINT IF EXISTS fk_refresh_tokens_user;
ALTER TABLE refresh_tokens ADD CONSTRAINT fk_refresh_tokens_user
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;
