-- ============================================================================
-- Migration: 014_merchant_public_perf.sql — Indeks halaman profil merchant
-- ============================================================================
-- Jalur panas /m/{id} & /m/{id}/reviews:
--
--   * public_profile: sub-query `COUNT(*) FROM events WHERE merchant_id=$1
--     AND status='active'`, dan listing event merchant
--     (WHERE merchant_id=$1 AND status='active' ORDER BY event_date ASC).
--     idx_events_merchant (merchant_id saja) memaksa filter status + sort
--     dikerjakan setelah scan → composite (merchant_id, status, event_date)
--     menutup COUNT sekaligus ORDER BY dalam satu index.
--
--   * reviews & merchant_follows: agregat/COUNT/list per merchant_id sudah
--     tertutup PK komposit (merchant_id, …) + idx_reviews_merchant_created.
--
-- Idempoten (IF NOT EXISTS). Untuk tabel besar/live: ganti ke
-- CREATE INDEX CONCURRENTLY dan jalankan di luar transaksi (psql -f, bukan -1).
-- Setelah apply: ANALYZE;
--
--   psql "$DATABASE_URL" -f migration/014_merchant_public_perf.sql
-- ============================================================================

-- Nama tabelnya DICARI, bukan ditulis langsung: berkas ini lahir sebelum
-- `023_products_rename.sql` me-rename `events` menjadi `products`, jadi menulis
-- `events` di sini membuatnya hanya benar di satu titik sejarah — dan mati
-- dengan `relation "events" does not exist` setiap kali ia diputar ulang di
-- database yang sudah melewati 023. Isinya cuma satu index, jadi tak ada
-- keputusan data yang bisa salah karena dijalankan di era lain.
DO $$
DECLARE
    produk TEXT := COALESCE(to_regclass('public.products')::text,
                            to_regclass('public.events')::text);
BEGIN
    IF produk IS NULL THEN
        RAISE NOTICE 'tabel events/products tak ada — index dilewati.';
        RETURN;
    END IF;

    EXECUTE format(
        'CREATE INDEX IF NOT EXISTS idx_events_merchant_status_date ON %I (merchant_id, status, event_date)',
        produk);
END $$;
