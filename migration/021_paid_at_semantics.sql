-- Migration: 021_paid_at_semantics.sql — `paid_at` tak lagi berbohong tentang
-- pesanan yang belum dibayar.
--
-- ── MASALAH ─────────────────────────────────────────────────────────────────
-- `subscription_orders.paid_at` dideklarasikan `TIMESTAMPTZ DEFAULT NOW()`
-- (migrasi 003 dan 004), sementara `status` boleh bernilai 'pending'.
--
-- Artinya setiap baris yang lahir tanpa menyebut `paid_at` — termasuk pesanan
-- yang BELUM dibayar — langsung tercatat punya waktu pembayaran, yaitu detik
-- ia dibuat. Kolom yang seharusnya menjawab "kapan uangnya masuk?" berubah jadi
-- salinan `created_at` yang menyamar.
--
-- Yang rusak karenanya bukan cuma kerapian:
--   • Laporan pendapatan per periode menghitung pesanan yang tak pernah dibayar.
--   • "Berapa lama orang menunda pembayaran" (paid_at − created_at) selalu nol.
--   • Setiap query `WHERE paid_at IS NOT NULL` sebagai pengganti "sudah dibayar"
--     mengembalikan SEMUA baris.
--
-- Bandingkan dengan `orders.paid_at` di migrasi 001 yang memang NULLable tanpa
-- default — di sana semantiknya sudah benar sejak awal.
--
-- ── YANG DILAKUKAN ──────────────────────────────────────────────────────────
-- 1. Buang DEFAULT-nya, supaya baris baru yang belum dibayar bernilai NULL.
-- 2. Kosongkan `paid_at` pada baris yang status-nya JELAS belum/tidak dibayar.
--
-- Baris ber-status 'paid' TIDAK disentuh: waktunya mungkin memang benar, dan
-- kalaupun ia sebenarnya cuma `created_at` yang menyamar, menghapusnya berarti
-- membuang satu-satunya perkiraan yang tersisa. Data lama yang meragukan lebih
-- baik dibiarkan apa adanya daripada dihapus atas nama kerapian.
--
-- ⚠️ Perhatikan `status` di migrasi 004 ber-DEFAULT 'paid'. Bila ada jalur tulis
-- yang mengandalkan default itu untuk pesanan yang sebetulnya masih pending,
-- baris tersebut tak akan ikut dibersihkan di sini — periksa `service` yang
-- membuat subscription order sebelum menyimpulkan angkanya sudah bersih.
--
-- Idempotent.
--   psql "$DATABASE_URL" -f migration/021_paid_at_semantics.sql

DO $$
BEGIN
    IF to_regclass('public.subscription_orders') IS NULL THEN
        RAISE NOTICE 'subscription_orders tak ada — dilewati.';
        RETURN;
    END IF;

    ALTER TABLE subscription_orders ALTER COLUMN paid_at DROP DEFAULT;

    UPDATE subscription_orders
       SET paid_at = NULL
     WHERE paid_at IS NOT NULL
       AND status IN ('pending', 'cancelled');

    RAISE NOTICE 'paid_at: default dibuang, baris pending/cancelled dikosongkan.';
END $$;

-- =============================================================================
-- VERIFIKASI MANUAL:
--   SELECT status, count(*) FILTER (WHERE paid_at IS NULL) AS tanpa_waktu_bayar,
--          count(*) AS total
--     FROM subscription_orders GROUP BY status ORDER BY status;
--   -- 'pending' & 'cancelled' harus SELURUHNYA tanpa waktu bayar.
-- =============================================================================
