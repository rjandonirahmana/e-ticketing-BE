-- ═════════════════════════════════════════════════════════════════════════════
-- 005 — Koordinat lokasi event (latitude/longitude) untuk peta OpenStreetMap.
-- ═════════════════════════════════════════════════════════════════════════════
-- Idempotent: aman dijalankan ulang.

ALTER TABLE events
    ADD COLUMN IF NOT EXISTS latitude  DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS longitude DOUBLE PRECISION;
