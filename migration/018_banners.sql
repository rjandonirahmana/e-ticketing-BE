-- ============================================================================
-- Migration: 018_banners.sql — tabel `banners` (slider explore, dikelola admin)
-- ============================================================================
-- Kode (repository/banner.rs) sudah lama memakai tabel ini, tetapi CREATE-nya
-- tidak pernah tercatat di folder migration (kemungkinan dibuat manual di
-- produksi). Idempoten (IF NOT EXISTS) — aman dijalankan di environment yang
-- tabelnya sudah ada.
--
--   psql "$DATABASE_URL" -f migration/018_banners.sql
-- ============================================================================

CREATE TABLE IF NOT EXISTS banners (
    id         BIGSERIAL PRIMARY KEY,
    image_url  TEXT        NOT NULL,
    click_url  TEXT,
    start_date TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    end_date   TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ,
    event_id   BYTEA REFERENCES events(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Query publik: banner aktif (deleted IS NULL + rentang tanggal).
CREATE INDEX IF NOT EXISTS idx_banners_active
    ON banners (start_date, end_date)
    WHERE deleted_at IS NULL;
