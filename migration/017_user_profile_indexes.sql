-- ============================================================================
-- Migration: 017_user_profile_indexes.sql — index halaman profil user /u/{id}
-- ============================================================================
-- `merchant_follows` & `reviews` ber-PK komposit (merchant_id, …): lookup dari
-- sisi MERCHANT tertutup PK, tetapi halaman profil user /u/{id} melakukan
-- lookup dari sisi USER (kolom KEDUA PK — tidak bisa memakai prefix index):
--
--   * user_public()       : COUNT(*) merchant_follows WHERE follower_id = $1
--   * user_public()       : COUNT(*) reviews          WHERE user_id     = $1
--   * list_user_reviews() : reviews WHERE user_id = $1 ORDER BY created_at DESC
--
-- Tanpa index ini ketiganya seq-scan — makin lambat seiring tabel tumbuh.
--
--   psql "$DATABASE_URL" -f migration/017_user_profile_indexes.sql
--
-- Catatan produksi: pada tabel yang SUDAH besar, buat versi CONCURRENTLY dan
-- jalankan di luar transaksi (pola sama dengan migrasi index sebelumnya).
-- ============================================================================

-- Following per user (count) — cukup kolom kunci.
CREATE INDEX IF NOT EXISTS idx_merchant_follows_follower
    ON merchant_follows (follower_id);

-- Ulasan yang DITULIS user: count + list terbaru dulu (ORDER BY created_at
-- DESC memakai index langsung, tanpa sort).
CREATE INDEX IF NOT EXISTS idx_reviews_user_created
    ON reviews (user_id, created_at DESC);
