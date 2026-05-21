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



-- ── Group Chat Tables ─────────────────────────────────────────────────────────
-- Jalankan setelah migrations existing (users, events, orders, tickets sudah ada)

-- 1 event = 1 group room
CREATE TABLE IF NOT EXISTS group_rooms (
    id          BYTEA        PRIMARY KEY,           -- ULID as 16-byte binary
    event_id    BYTEA        NOT NULL UNIQUE        -- FK ke events.id, satu event satu room
                             REFERENCES events(id) ON DELETE CASCADE,
    name        TEXT         NOT NULL,
    cover_url   TEXT,
    created_by  BYTEA        NOT NULL REFERENCES users(id),
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_group_rooms_event   ON group_rooms(event_id);
CREATE INDEX IF NOT EXISTS idx_group_rooms_creator ON group_rooms(created_by);

-- Member room
CREATE TABLE IF NOT EXISTS group_members (
    room_id   BYTEA  NOT NULL REFERENCES group_rooms(id) ON DELETE CASCADE,
    user_id   BYTEA  NOT NULL REFERENCES users(id)       ON DELETE CASCADE,
    role      TEXT   NOT NULL DEFAULT 'member'
                     CHECK (role IN ('owner','member')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_group_members_user ON group_members(user_id);

-- Pesan grup
CREATE TABLE IF NOT EXISTS group_messages (
    id          BYTEA        PRIMARY KEY,
    room_id     BYTEA        NOT NULL REFERENCES group_rooms(id) ON DELETE CASCADE,
    sender_id   BYTEA        NOT NULL REFERENCES users(id),
    sender_name TEXT         NOT NULL DEFAULT '',
    msg_type    TEXT         NOT NULL DEFAULT 'text'
                             CHECK (msg_type IN ('text','image','shared_ticket','system')),
    content     TEXT         NOT NULL DEFAULT '',
    media_url   TEXT,
    ticket_card JSONB,                              -- TicketCard JSON null kalau bukan shared_ticket
    is_system   BOOLEAN      NOT NULL DEFAULT FALSE,
    sent_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Index utama: history per room, cursor-based pagination
CREATE INDEX IF NOT EXISTS idx_group_messages_room_sent
    ON group_messages(room_id, sent_at DESC, id DESC);

-- Index count per sender di room (untuk enforce customer 1-msg limit)
CREATE INDEX IF NOT EXISTS idx_group_messages_sender
    ON group_messages(room_id, sender_id) WHERE is_system = FALSE;

-- System user placeholder (sender_id untuk system messages)
-- Pastikan ada di tabel users — atau comment kalau schema users berbeda
-- INSERT INTO users (id, name, phone, role, created_at, updated_at)
-- VALUES (decode('00000000000000000000000000000000', 'hex'), 'System Pulse', '+000000000000', 'admin', NOW(), NOW())
-- ON CONFLICT (id) DO NOTHING;



-- ─────────────────────────────────────────────────────────────────────────────
-- Migration: stories, story_views, user_subscriptions
-- ─────────────────────────────────────────────────────────────────────────────

-- ── 1. Premium subscriptions ──────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS user_subscriptions (
    id            BYTEA        PRIMARY KEY,          -- ULID binary 16 bytes
    user_id       BYTEA        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    plan          VARCHAR(32)  NOT NULL DEFAULT 'premium',
    started_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    expires_at    TIMESTAMPTZ  NOT NULL,
    is_active     BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_user_subscriptions_user_active
    ON user_subscriptions (user_id, is_active, expires_at);

-- ── 2. Stories ────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS stories (
    id          BYTEA        PRIMARY KEY,            -- ULID binary 16 bytes
    user_id     BYTEA        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    media_url   TEXT         NOT NULL,
    media_type  VARCHAR(10)  NOT NULL CHECK (media_type IN ('image', 'video')),
    filter      VARCHAR(64),
    overlays    JSONB        NOT NULL DEFAULT '[]',
    -- optional deep-link ke event
    event_id    BYTEA        REFERENCES events(id) ON DELETE SET NULL,
    event_slug  VARCHAR(255),
    event_title TEXT,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    expires_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW() + INTERVAL '24 hours'
);

CREATE INDEX IF NOT EXISTS idx_stories_user_id   ON stories (user_id);
CREATE INDEX IF NOT EXISTS idx_stories_expires_at ON stories (expires_at);

-- ── 3. Story views (per-user dedup) ──────────────────────────────────────────
CREATE TABLE IF NOT EXISTS story_views (
    story_id    BYTEA        NOT NULL REFERENCES stories(id) ON DELETE CASCADE,
    viewer_id   BYTEA        NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    viewed_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    PRIMARY KEY (story_id, viewer_id)
);

-- ── 4. Helper view: aktif stories saja ───────────────────────────────────────
CREATE OR REPLACE VIEW v_active_stories AS
    SELECT s.*,
           u.name        AS username,
           u.avatar_url  AS avatar_url
    FROM   stories s
    JOIN   users u ON u.id = s.user_id
    WHERE  s.expires_at > NOW();

-- ── 5. Add avatar_url to users if it doesn't exist ───────────────────────────
ALTER TABLE users ADD COLUMN IF NOT EXISTS avatar_url VARCHAR(100) NOT NULL DEFAULT 'https://image.ulalaapi.store/ticketing/seulgi.jpg';
