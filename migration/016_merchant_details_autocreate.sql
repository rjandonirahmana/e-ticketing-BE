-- ============================================================================
-- Migration: 016_merchant_details_autocreate.sql
-- ============================================================================
-- INVARIAN: setiap user berperan 'merchant' DIJAMIN punya baris
-- `merchant_details`. Dengan invarian ini, query profil merchant tak perlu lagi
-- menangani kasus "row belum ada" (EXISTS / LEFT JOIN) → SELECT lebih ringkas &
-- cepat, dan /m/{id} tak pernah balas "Merchant tidak ditemukan" untuk merchant
-- yang sah (dulu: user role=merchant tapi belum membuat toko → NotFound → 500).
--
--   psql "$DATABASE_URL" -f migration/016_merchant_details_autocreate.sql
--
-- CATATAN DEPLOY: jalankan migrasi ini SEBELUM/BERSAMAAN dengan versi kode yang
-- menyederhanakan query (yang kini mengandalkan invarian di atas).
-- ============================================================================

-- ── 1. Fungsi trigger: auto-buat merchant_details saat user jadi merchant ────
-- store_name default = nama user; logo_url '' (kolom NOT NULL tanpa default) —
-- merchant melengkapinya lewat /merchant. ON CONFLICT DO NOTHING → idempoten &
-- aman bila baris sudah dibuat aplikasi lebih dulu. Kolom lain memakai default:
--   description   → NULL (nullable)
--   verified      → FALSE (default)
--   review_1..5   → 0 (NOT NULL DEFAULT 0, migrasi 014)
--   total_review / total_avg_review → GENERATED
CREATE OR REPLACE FUNCTION trg_ensure_merchant_details() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.role = 'merchant' THEN
        INSERT INTO merchant_details (user_id, store_name, logo_url)
        VALUES (NEW.id, NEW.name, '')
        ON CONFLICT (user_id) DO NOTHING;
    END IF;
    RETURN NEW;
END;
$$;

-- AFTER INSERT (user baru langsung merchant) + AFTER UPDATE OF role
-- (customer → merchant). `UPDATE OF role` → trigger TIDAK jalan saat kolom lain
-- di-update (mis. ganti nama), jadi murah.
DROP TRIGGER IF EXISTS users_ensure_merchant_details ON users;
CREATE TRIGGER users_ensure_merchant_details
    AFTER INSERT OR UPDATE OF role ON users
    FOR EACH ROW EXECUTE FUNCTION trg_ensure_merchant_details();

-- ── 2. Backfill: merchant lama yang belum punya baris merchant_details ───────
-- Idempoten (ON CONFLICT DO NOTHING) — aman dijalankan ulang.
INSERT INTO merchant_details (user_id, store_name, logo_url)
SELECT u.id, u.name, ''
FROM   users u
WHERE  u.role = 'merchant'
  AND  NOT EXISTS (SELECT 1 FROM merchant_details md WHERE md.user_id = u.id)
ON CONFLICT (user_id) DO NOTHING;
