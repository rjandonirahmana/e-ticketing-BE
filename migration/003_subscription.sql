-- subscription_orders: billing history per subscription purchase
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

-- user_subscriptions: current/past subscription per user.
-- CATATAN PENTING: tabel ini SUDAH dibuat lebih dulu di migration 002 (tanpa
-- kolom subscription_order_id). `CREATE TABLE IF NOT EXISTS` di sini akan
-- DILEWATI, sehingga kolom FK billing tidak pernah ada → confirm_subscription_order
-- gagal (INSERT-nya memakai subscription_order_id) → premium tidak bisa diaktifkan.
-- Karena itu gunakan ALTER idempotent agar kolomnya benar-benar ditambahkan,
-- baik pada database baru maupun lama.
ALTER TABLE user_subscriptions
    ADD COLUMN IF NOT EXISTS subscription_order_id bytea REFERENCES subscription_orders(id);

CREATE INDEX IF NOT EXISTS idx_user_subscriptions_user_id ON user_subscriptions(user_id);
CREATE INDEX IF NOT EXISTS idx_user_subscriptions_is_active ON user_subscriptions(is_active);
CREATE INDEX IF NOT EXISTS idx_subscription_orders_user_id ON subscription_orders(user_id);

-- Idempotent: aman dijalankan ulang.
DROP TRIGGER IF EXISTS update_subscription_orders_updated_at ON subscription_orders;
CREATE TRIGGER update_subscription_orders_updated_at
    BEFORE UPDATE ON subscription_orders FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
