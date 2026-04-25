-- ============================================================
-- Migration: 002_fix_login_perf.sql
-- Tujuan:
--   1. Index pada users.phone — login query (WHERE phone = $1) tadinya
--      sequential scan, ini biang kerok login lambat.
--   2. Email dijadikan nullable + unique partial — kode Rust memang sudah
--      mengizinkan email = NULL (akun OTP-only), tapi schema lama menolak.
-- ============================================================

-- 1) Index untuk lookup phone (login & duplicate check)
CREATE INDEX IF NOT EXISTS idx_users_phone ON users(phone);

-- 2) Email boleh NULL, dan unique-nya partial (tidak menghitung NULL).
ALTER TABLE users ALTER COLUMN email DROP NOT NULL;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
          FROM pg_constraint
         WHERE conname = 'users_email_key'
           AND conrelid = 'users'::regclass
    ) THEN
        ALTER TABLE users DROP CONSTRAINT users_email_key;
    END IF;
END$$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email_unique
    ON users(email) WHERE email IS NOT NULL;

-- 3) password_hash juga nullable — akun OTP-only mungkin belum punya hash
ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;
