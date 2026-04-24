-- ============================================================
-- Migration: 001_initial_schema.sql
-- E-Ticketing Schema
-- ============================================================

-- ============================================================
-- Table: users
-- ============================================================
CREATE TABLE IF NOT EXISTS users (
    id bytea NOT NULL PRIMARY KEY,  -- ULID 16 bytes
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    name VARCHAR(255) NOT NULL,
    phone VARCHAR(50),
    role VARCHAR(20) NOT NULL CHECK (role IN ('customer', 'merchant', 'admin')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- Table: merchant_details
-- ============================================================
CREATE TABLE IF NOT EXISTS merchant_details (
    user_id bytea NOT NULL PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    store_name VARCHAR(255) NOT NULL,
    description TEXT,
    logo_url VARCHAR(255) NOT NULL,
    verified BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- Table: events
-- ============================================================
CREATE TABLE IF NOT EXISTS events (
    id bytea NOT NULL PRIMARY KEY,
    merchant_id bytea NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    price DECIMAL(12,2) NOT NULL CHECK (price >= 0),
    sale_price DECIMAL(12,2) NULL,
    sale_price_start_date TIMESTAMPTZ NULL,
    sale_price_end_date TIMESTAMPTZ NULL,
    venue VARCHAR(255),
    city VARCHAR(100),
    event_date TIMESTAMPTZ NOT NULL,
    start_time TIMESTAMPTZ,
    end_time TIMESTAMPTZ,
    status VARCHAR(20) DEFAULT 'active' CHECK (status IN ('active', 'cancelled', 'completed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- Table: ticket_variants
-- ============================================================
CREATE TABLE IF NOT EXISTS ticket_variants (
    id bytea NOT NULL PRIMARY KEY,
    event_id bytea NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    price DECIMAL(12,2) NOT NULL CHECK (price >= 0),
    sale_price DECIMAL(12,2) NULL,
    sale_price_start_date TIMESTAMPTZ NULL,
    sale_price_end_date TIMESTAMPTZ NULL,
    quota INTEGER NOT NULL CHECK (quota >= 0),
    sold INTEGER NOT NULL DEFAULT 0 CHECK (sold >= 0 AND sold <= quota),
    max_per_order INTEGER DEFAULT 10,
    is_active BOOLEAN DEFAULT TRUE,
    sort_order INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- Table: orders
-- ============================================================
CREATE TABLE IF NOT EXISTS orders (
    id bytea NOT NULL PRIMARY KEY,
    customer_id bytea NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    order_code VARCHAR(50) UNIQUE NOT NULL,
    status VARCHAR(20) DEFAULT 'pending' CHECK (status IN ('pending', 'paid', 'cancelled', 'expired')),
    total_amount DECIMAL(12,2) NOT NULL DEFAULT 0,
    payment_method VARCHAR(50),
    paid_at TIMESTAMPTZ,
    expired_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- Table: order_items
-- ============================================================
CREATE TABLE IF NOT EXISTS order_items (
    id bytea NOT NULL PRIMARY KEY,
    order_id bytea NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    ticket_variant_id bytea NOT NULL REFERENCES ticket_variants(id) ON DELETE RESTRICT,
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    unit_price DECIMAL(12,2) NOT NULL,
    subtotal DECIMAL(12,2) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- Table: tickets
-- ============================================================
CREATE TABLE IF NOT EXISTS tickets (
    id bytea NOT NULL PRIMARY KEY,
    order_item_id bytea NOT NULL REFERENCES order_items(id) ON DELETE CASCADE,
    ticket_code VARCHAR(100) UNIQUE NOT NULL,
    status VARCHAR(20) DEFAULT 'active' CHECK (status IN ('active', 'used', 'refunded', 'expired')),
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- Indexes
-- ============================================================
CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);
CREATE INDEX IF NOT EXISTS idx_events_merchant_id ON events(merchant_id);
CREATE INDEX IF NOT EXISTS idx_events_event_date ON events(event_date);
CREATE INDEX IF NOT EXISTS idx_events_status ON events(status);
CREATE INDEX IF NOT EXISTS idx_ticket_variants_event_id ON ticket_variants(event_id);
CREATE INDEX IF NOT EXISTS idx_orders_customer_id ON orders(customer_id);
CREATE INDEX IF NOT EXISTS idx_orders_status ON orders(status);
CREATE INDEX IF NOT EXISTS idx_order_items_order_id ON order_items(order_id);
CREATE INDEX IF NOT EXISTS idx_order_items_ticket_variant_id ON order_items(ticket_variant_id);
CREATE INDEX IF NOT EXISTS idx_tickets_order_item_id ON tickets(order_item_id);
CREATE INDEX IF NOT EXISTS idx_tickets_ticket_code ON tickets(ticket_code);
CREATE INDEX IF NOT EXISTS idx_tickets_status ON tickets(status);

-- ============================================================
-- Trigger: auto update updated_at
-- ============================================================
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_users_updated_at
    BEFORE UPDATE ON users FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_merchant_details_updated_at
    BEFORE UPDATE ON merchant_details FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_events_updated_at
    BEFORE UPDATE ON events FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_ticket_variants_updated_at
    BEFORE UPDATE ON ticket_variants FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_orders_updated_at
    BEFORE UPDATE ON orders FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_tickets_updated_at
    BEFORE UPDATE ON tickets FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();