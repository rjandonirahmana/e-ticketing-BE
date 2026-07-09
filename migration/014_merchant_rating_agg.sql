-- ============================================================================
-- Migration: 014_merchant_rating_agg.sql — Rating agregat di merchant_details
-- ============================================================================
-- Denormalisasi rating agar profil publik /m/{id} & ringkasan ulasan TIDAK
-- perlu AVG/COUNT scan tabel `reviews` tiap baca. Tabel `reviews` kini murni
-- untuk MENAMPILKAN daftar ulasan; agregat hidup di merchant_details:
--
--   review_1..review_5  : jumlah ulasan per bintang (distribusi)
--   total_review        : jumlah ulasan (GENERATED = review_1+..+review_5)
--   total_avg_review    : rata-rata bintang (GENERATED, 0 bila belum ada ulasan)
--
-- Kolom agregat dijaga otomatis oleh TRIGGER pada `reviews` (INSERT/UPDATE/
-- DELETE) — termasuk kasus user MENGUBAH rating (upsert) dan cascade-delete
-- saat user dihapus. Jadi `upsert_review` di aplikasi tak perlu berubah dan
-- agregat mustahil melenceng dari tabel `reviews`.
--
--   psql "$DATABASE_URL" -f migration/014_merchant_rating_agg.sql
-- ============================================================================

-- ── 1. Kolom bucket (basis; yang ditulis trigger) ──────────────────────────
ALTER TABLE merchant_details ADD COLUMN IF NOT EXISTS review_1 BIGINT NOT NULL DEFAULT 0;
ALTER TABLE merchant_details ADD COLUMN IF NOT EXISTS review_2 BIGINT NOT NULL DEFAULT 0;
ALTER TABLE merchant_details ADD COLUMN IF NOT EXISTS review_3 BIGINT NOT NULL DEFAULT 0;
ALTER TABLE merchant_details ADD COLUMN IF NOT EXISTS review_4 BIGINT NOT NULL DEFAULT 0;
ALTER TABLE merchant_details ADD COLUMN IF NOT EXISTS review_5 BIGINT NOT NULL DEFAULT 0;

-- ── 2. Kolom turunan (GENERATED — selalu konsisten, tak bisa di-write) ──────
-- Catatan: kolom generated hanya boleh mereferensikan kolom BASIS (bukan
-- generated lain), jadi jumlah bintang ditulis eksplisit di kedua ekspresi.
ALTER TABLE merchant_details ADD COLUMN IF NOT EXISTS total_review BIGINT
    GENERATED ALWAYS AS (review_1 + review_2 + review_3 + review_4 + review_5) STORED;

ALTER TABLE merchant_details ADD COLUMN IF NOT EXISTS total_avg_review DOUBLE PRECISION
    GENERATED ALWAYS AS (
        CASE
            WHEN (review_1 + review_2 + review_3 + review_4 + review_5) = 0 THEN 0::float8
            ELSE (review_1 + 2*review_2 + 3*review_3 + 4*review_4 + 5*review_5)::float8
                 / (review_1 + review_2 + review_3 + review_4 + review_5)
        END
    ) STORED;

-- ── 3. Backfill bucket dari `reviews` yang sudah ada (idempoten: SET absolut) ─
UPDATE merchant_details md SET
    review_1 = COALESCE(c.c1, 0),
    review_2 = COALESCE(c.c2, 0),
    review_3 = COALESCE(c.c3, 0),
    review_4 = COALESCE(c.c4, 0),
    review_5 = COALESCE(c.c5, 0)
FROM (
    SELECT merchant_id,
           COUNT(*) FILTER (WHERE rating = 1) AS c1,
           COUNT(*) FILTER (WHERE rating = 2) AS c2,
           COUNT(*) FILTER (WHERE rating = 3) AS c3,
           COUNT(*) FILTER (WHERE rating = 4) AS c4,
           COUNT(*) FILTER (WHERE rating = 5) AS c5
    FROM reviews
    GROUP BY merchant_id
) c
WHERE md.user_id = c.merchant_id;

-- ── 4. Trigger pemelihara bucket ────────────────────────────────────────────
-- Aritmetika boolean → int (rating=N)::int menghasilkan 1 utk bucket yang cocok,
-- 0 lainnya. UPDATE memakai selisih (baru - lama) sehingga rating yang TIDAK
-- berubah = net nol, dan ganti rating memindah bucket dengan benar. merchant_id
-- stabil pada UPDATE (bagian dari PK reviews + jalur upsert ON CONFLICT).
CREATE OR REPLACE FUNCTION trg_reviews_rating_buckets() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE merchant_details SET
            review_1 = review_1 + (NEW.rating = 1)::int,
            review_2 = review_2 + (NEW.rating = 2)::int,
            review_3 = review_3 + (NEW.rating = 3)::int,
            review_4 = review_4 + (NEW.rating = 4)::int,
            review_5 = review_5 + (NEW.rating = 5)::int
        WHERE user_id = NEW.merchant_id;
        RETURN NEW;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE merchant_details SET
            review_1 = review_1 - (OLD.rating = 1)::int,
            review_2 = review_2 - (OLD.rating = 2)::int,
            review_3 = review_3 - (OLD.rating = 3)::int,
            review_4 = review_4 - (OLD.rating = 4)::int,
            review_5 = review_5 - (OLD.rating = 5)::int
        WHERE user_id = OLD.merchant_id;
        RETURN OLD;
    ELSE  -- UPDATE: pindahkan bucket lama → baru (selisih)
        UPDATE merchant_details SET
            review_1 = review_1 + (NEW.rating = 1)::int - (OLD.rating = 1)::int,
            review_2 = review_2 + (NEW.rating = 2)::int - (OLD.rating = 2)::int,
            review_3 = review_3 + (NEW.rating = 3)::int - (OLD.rating = 3)::int,
            review_4 = review_4 + (NEW.rating = 4)::int - (OLD.rating = 4)::int,
            review_5 = review_5 + (NEW.rating = 5)::int - (OLD.rating = 5)::int
        WHERE user_id = NEW.merchant_id;
        RETURN NEW;
    END IF;
END;
$$;

DROP TRIGGER IF EXISTS reviews_rating_buckets ON reviews;
CREATE TRIGGER reviews_rating_buckets
    AFTER INSERT OR UPDATE OR DELETE ON reviews
    FOR EACH ROW EXECUTE FUNCTION trg_reviews_rating_buckets();
