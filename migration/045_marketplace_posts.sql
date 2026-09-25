-- ============================================================================
-- Migration: 045_marketplace_posts.sql — Marketplace C2C (jual-beli COD)
-- ============================================================================
-- Fitur baru: SEMUA user terdaftar (bukan cuma merchant) bisa posting jual
-- atau cari barang baru/bekas. Transaksi COD murni — tabel ini TIDAK menyentuh
-- payment/order sama sekali; penjual & pembeli sepakat lewat chat lalu ketemu
-- langsung. Penjual sendiri yang menandai status 'sold' setelah transaksi.
--
-- Tabel sengaja TERPISAH dari `products` (yang tetap murni untuk event/tiket
-- merchant) — menumpangkan C2C di situ akan mencemari kolom event-specific
-- (event_date/venue/sale_price_window) yang tak relevan untuk barang jual-beli.
--
-- like_count/comment_count DIDENORMALISASI ke kolom `posts`, dijaga TRIGGER
-- pada `post_likes`/`post_comments` — pola sama dengan
-- migration/014a_merchant_rating_agg.sql (reviews_rating_buckets), tapi lebih
-- sederhana: cuma +1/-1 per baris, bukan distribusi bucket per-bintang.
--
--   psql "$DATABASE_URL" -f migration/045_marketplace_posts.sql
-- ============================================================================

CREATE TABLE IF NOT EXISTS posts (
    id            BYTEA PRIMARY KEY,
    user_id       BYTEA NOT NULL REFERENCES users(id),
    kind          VARCHAR(10) NOT NULL,   -- 'jual' | 'cari'
    title         VARCHAR(150) NOT NULL,
    description   TEXT NOT NULL,
    price         BIGINT,                 -- NULL = nego / tak relevan utk 'cari'
    condition     VARCHAR(10),            -- 'baru' | 'bekas', NULL bila kind='cari'
    category      VARCHAR(50),
    city          VARCHAR(100),
    images        JSONB NOT NULL DEFAULT '[]',  -- [{"url": "..."}], maks 6 (app-level)
    status        VARCHAR(10) NOT NULL DEFAULT 'active', -- active|sold|archived
    like_count    INT NOT NULL DEFAULT 0,  -- denormalisasi, dijaga trigger di bawah
    comment_count INT NOT NULL DEFAULT 0,  -- denormalisasi, dijaga trigger di bawah
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at    TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_posts_feed ON posts (status, created_at DESC)
    WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_posts_user ON posts (user_id);

CREATE TABLE IF NOT EXISTS post_likes (
    post_id    BYTEA NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    user_id    BYTEA NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (post_id, user_id)
);

CREATE TABLE IF NOT EXISTS post_comments (
    id         BYTEA PRIMARY KEY,
    post_id    BYTEA NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    user_id    BYTEA NOT NULL REFERENCES users(id),
    body       VARCHAR(1000) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_post_comments_post
    ON post_comments (post_id, created_at) WHERE deleted_at IS NULL;

-- ── Trigger: like_count ──────────────────────────────────────────────────────
-- post_likes tak punya soft-delete (unlike = DELETE baris sungguhan), jadi
-- cukup INSERT/DELETE.
CREATE OR REPLACE FUNCTION trg_post_likes_count() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE posts SET like_count = like_count + 1 WHERE id = NEW.post_id;
        RETURN NEW;
    ELSE -- DELETE
        UPDATE posts SET like_count = like_count - 1 WHERE id = OLD.post_id;
        RETURN OLD;
    END IF;
END;
$$;

DROP TRIGGER IF EXISTS post_likes_count ON post_likes;
CREATE TRIGGER post_likes_count
    AFTER INSERT OR DELETE ON post_likes
    FOR EACH ROW EXECUTE FUNCTION trg_post_likes_count();

-- ── Trigger: comment_count ───────────────────────────────────────────────────
-- Dua jalur naik/turun: INSERT baris baru menaikkan; comment yang di-SOFT-
-- DELETE (deleted_at NULL → NOT NULL) menurunkan. Baris DELETE sungguhan tak
-- dipakai aplikasi (repo memakai UPDATE deleted_at), tapi ditangani juga di
-- sini untuk kelengkapan/konsistensi seandainya ada penghapusan manual.
CREATE OR REPLACE FUNCTION trg_post_comments_count_insert() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE posts SET comment_count = comment_count + 1 WHERE id = NEW.post_id;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS post_comments_count_insert ON post_comments;
CREATE TRIGGER post_comments_count_insert
    AFTER INSERT ON post_comments
    FOR EACH ROW EXECUTE FUNCTION trg_post_comments_count_insert();

CREATE OR REPLACE FUNCTION trg_post_comments_count_soft_delete() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE posts SET comment_count = comment_count - 1 WHERE id = NEW.post_id;
    RETURN NEW;
END;
$$;

-- Guard di TRIGGER (WHEN), bukan di dalam fungsi: menjamin transisi
-- NULL→NOT NULL persis sekali, dan re-UPDATE pada baris yang sudah terhapus
-- (mis. UPDATE lain menyentuh baris yang sama) tidak menghitung dua kali.
DROP TRIGGER IF EXISTS post_comments_count_soft_delete ON post_comments;
CREATE TRIGGER post_comments_count_soft_delete
    AFTER UPDATE OF deleted_at ON post_comments
    FOR EACH ROW
    WHEN (OLD.deleted_at IS NULL AND NEW.deleted_at IS NOT NULL)
    EXECUTE FUNCTION trg_post_comments_count_soft_delete();

CREATE OR REPLACE FUNCTION trg_post_comments_count_delete() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.deleted_at IS NULL THEN
        UPDATE posts SET comment_count = comment_count - 1 WHERE id = OLD.post_id;
    END IF;
    RETURN OLD;
END;
$$;

DROP TRIGGER IF EXISTS post_comments_count_delete ON post_comments;
CREATE TRIGGER post_comments_count_delete
    AFTER DELETE ON post_comments
    FOR EACH ROW EXECUTE FUNCTION trg_post_comments_count_delete();

