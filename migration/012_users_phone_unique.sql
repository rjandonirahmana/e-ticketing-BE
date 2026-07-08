-- ============================================================================
-- Migration: 012_users_phone_unique.sql  —  users.phone → UNIQUE + NOT NULL
-- ============================================================================
-- Tujuan: nomor telepon jadi identitas unik & wajib ada (login by phone).
--
-- URUTAN PENTING (kalau dibalik, migrasi gagal):
--   1) Backfill baris phone NULL dengan placeholder UNIK — kalau tidak,
--      `SET NOT NULL` gagal untuk baris lama yang phone-nya NULL.
--   2) Ganti index non-unik `idx_users_phone` → UNIQUE constraint.
--   3) `SET NOT NULL`.
--
-- ⚠️  Jika ADA nomor duplikat non-null di data lama, penambahan UNIQUE akan
--     GAGAL. Cek dulu & bereskan duplikatnya:
--       SELECT phone, COUNT(*) FROM users GROUP BY phone HAVING COUNT(*) > 1;
--
--   psql "$DATABASE_URL" -f migration/012_users_phone_unique.sql
-- ============================================================================

-- 1) Backfill NULL → placeholder unik (encode(id,'hex') dijamin unik per user).
UPDATE users
SET phone = '62-nophone-' || encode(id, 'hex')
WHERE phone IS NULL;

-- 2) Index non-unik lama tak diperlukan lagi — UNIQUE constraint membuat index
--    unik yang juga melayani lookup login. Tambah constraint idempoten.
DROP INDEX IF EXISTS idx_users_phone;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'users_phone_key'
    ) THEN
        ALTER TABLE users ADD CONSTRAINT users_phone_key UNIQUE (phone);
    END IF;
END$$;

-- 3) Wajib ada (idempoten — no-op bila sudah NOT NULL).
ALTER TABLE users ALTER COLUMN phone SET NOT NULL;
