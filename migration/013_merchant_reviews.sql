-- ============================================================================
-- Migration: 013_reviews.sql — Rating/ulasan & follow merchant
-- ============================================================================
-- Halaman baru: profil merchant publik (/m/{id}) + rating & reviews
-- (/m/{id}/reviews). Satu user = satu ulasan per merchant (PK komposit →
-- submit ulang = update ulasan lama, bukan duplikat).
--
--   psql "$DATABASE_URL" -f migration/013_reviews.sql
-- ============================================================================

CREATE TABLE IF NOT EXISTS reviews (
    merchant_id BYTEA       NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_id     BYTEA       NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rating      SMALLINT    NOT NULL CHECK (rating BETWEEN 1 AND 5),
    url_media   TEXT[]      DEFAULT '{}', -- Menyimpan URL video (bisa null
    order_id    BYTEA       NOT NULL REFERENCES orders(id) ON DELETE SET NULL,
    comment     TEXT        NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (merchant_id, user_id)
);

-- Daftar ulasan per merchant, terbaru dulu (jalur baca utama halaman reviews).
CREATE INDEX IF NOT EXISTS idx_reviews_merchant_created
    ON reviews (merchant_id, created_at DESC);

CREATE TABLE IF NOT EXISTS merchant_follows (
    merchant_id BYTEA       NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    follower_id BYTEA       NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (merchant_id, follower_id)
);
