-- ═════════════════════════════════════════════════════════════════════════════
-- 005 — Koordinat lokasi event (latitude/longitude) untuk peta OpenStreetMap.
-- ═════════════════════════════════════════════════════════════════════════════
-- Idempotent: aman dijalankan ulang.

-- Nama tabelnya DICARI, bukan ditulis langsung — `023_products_rename.sql`
-- me-rename `events` menjadi `products`, dan memutar ulang berkas ini di
-- database yang sudah melewatinya mati dengan `relation "events" does not
-- exist`. Kolomnya sendiri ikut terbawa rename, jadi menambahkannya ke nama
-- yang berlaku hari ini memberi hasil yang SAMA di era mana pun.
DO $$
DECLARE
    produk TEXT := COALESCE(to_regclass('public.products')::text,
                            to_regclass('public.events')::text);
BEGIN
    IF produk IS NULL THEN
        RAISE EXCEPTION 'tabel events/products tak ada — 001.sql belum jalan';
    END IF;

    EXECUTE format(
        'ALTER TABLE %I ADD COLUMN IF NOT EXISTS latitude  DOUBLE PRECISION', produk);
    EXECUTE format(
        'ALTER TABLE %I ADD COLUMN IF NOT EXISTS longitude DOUBLE PRECISION', produk);
END $$;
