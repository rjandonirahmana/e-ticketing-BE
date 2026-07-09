-- ============================================================================
-- Migration: 015_merchant_header.sql — Header image kustom merchant
-- ============================================================================
-- Merchant bisa mengunggah gambar HEADER/cover sendiri untuk profil publik
-- /m/{id} (dan preview di Merchant Hub). Kosong → hero fallback ke cover event
-- terbaru (perilaku lama).
--
--   psql "$DATABASE_URL" -f migration/015_merchant_header.sql
-- ============================================================================

ALTER TABLE merchant_details ADD COLUMN IF NOT EXISTS header_url TEXT;
