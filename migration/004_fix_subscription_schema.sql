-- ═════════════════════════════════════════════════════════════════════════════
-- 004 — FIX: premium order tidak bisa diselesaikan ("belum bisa order premium")
-- ═════════════════════════════════════════════════════════════════════════════
--
-- AKAR MASALAH:
--   migration 002 membuat tabel `user_subscriptions` LEBIH DULU tanpa kolom
--   `subscription_order_id`. Karena migration 003 memakai
--   `CREATE TABLE IF NOT EXISTS`, definisi 003 yang lebih lengkap DILEWATI —
--   sehingga kolom FK billing tidak pernah ada di skema yang sebenarnya.
--
--   Akibatnya `StoryRepository::confirm_subscription_order` yang melakukan
--   `INSERT INTO user_subscriptions (id, user_id, subscription_order_id, ...)`
--   selalu gagal → tombol "Bayar Sekarang" di checkout premium error → premium
--   tidak pernah aktif.
--
-- Migration ini IDEMPOTENT & SELF-CONTAINED: aman dijalankan pada database lama
-- (yang sudah terlanjur memakai skema 002) maupun baru.

-- 1) Pastikan subscription_orders ada (cermin 003; no-op bila sudah ada).
CREATE TABLE IF NOT EXISTS subscription_orders (
    id bytea NOT NULL PRIMARY KEY,
    user_id bytea NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    order_code VARCHAR(50) UNIQUE NOT NULL,
    plan VARCHAR(20) NOT NULL CHECK (plan IN ('weekly', 'monthly', 'yearly', 'lifetime')),
    amount DECIMAL(12,2) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'paid' CHECK (status IN ('pending', 'paid', 'cancelled')),
    paid_at TIMESTAMPTZ DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 2) Tambahkan kolom FK yang hilang di user_subscriptions (inti perbaikan).
ALTER TABLE user_subscriptions
    ADD COLUMN IF NOT EXISTS subscription_order_id bytea REFERENCES subscription_orders(id);

-- 3) Index pendukung.
CREATE INDEX IF NOT EXISTS idx_user_subscriptions_order_id
    ON user_subscriptions (subscription_order_id);
CREATE INDEX IF NOT EXISTS idx_subscription_orders_user_id
    ON subscription_orders (user_id);
