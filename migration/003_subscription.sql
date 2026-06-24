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

-- user_subscriptions: current/past subscription per user
CREATE TABLE IF NOT EXISTS user_subscriptions (
    id bytea NOT NULL PRIMARY KEY,
    user_id bytea NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    subscription_order_id bytea REFERENCES subscription_orders(id),
    plan VARCHAR(20) NOT NULL CHECK (plan IN ('weekly', 'monthly', 'yearly', 'lifetime')),
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_user_subscriptions_user_id ON user_subscriptions(user_id);
CREATE INDEX IF NOT EXISTS idx_user_subscriptions_is_active ON user_subscriptions(is_active);
CREATE INDEX IF NOT EXISTS idx_subscription_orders_user_id ON subscription_orders(user_id);

CREATE TRIGGER update_subscription_orders_updated_at
    BEFORE UPDATE ON subscription_orders FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
