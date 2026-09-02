-- ============================================================================
-- Migration: 007_seed_bulk.sql  —  SEED DATA MASSAL (uji performa)
-- ============================================================================
-- Tujuan: mengisi tabel `events` + `event_variants` dengan JUTAAN baris agar
-- bisa mengukur seberapa cepat aplikasi diakses (listing, pagination, filter,
-- COUNT) saat data besar.
--
-- Teknik: `INSERT ... SELECT generate_series(...)` — Postgres membuat semua baris
-- di sisi server dalam satu operasi (jauh lebih cepat daripada jutaan INSERT).
--
-- ── CARA PAKAI ──────────────────────────────────────────────────────────────
--   psql "$DATABASE_URL" -f migration/007_seed_bulk.sql
--
--   Ganti angka pada generate_series(...) di BAGIAN 1 (default 1.000.000).
--   Tiap event dapat 3 varian (Reguler/VIP/VVIP) → total baris ≈ 4 × N_EVENTS.
--   Idempoten: aman dijalankan ulang (ON CONFLICT DO NOTHING) — mis. kalau tadi
--   varian gagal dibuat, jalankan lagi dan varian akan terisi.
--
-- ── HARGA (bukan gratis) ─────────────────────────────────────────────────────
--   Harga diambil dari agregasi `event_variants` (min price varian aktif).
--   Reguler = harga dasar (Rp75rb–Rp1jt), VIP = 1.8×, VVIP = 3×. Sekitar 1/3
--   event punya diskon (sale_price 80%) dengan window aktif → display_price ikut.
--
-- ── PERKIRAAN ───────────────────────────────────────────────────────────────
--   1 juta event  → ~4 jt baris, ~0.5–1 GB, ~1–3 menit (VPS kecil).
--
-- ── HAPUS SEED (rollback) ───────────────────────────────────────────────────
--   Lihat blok DELETE di bagian paling bawah (di-comment). event_variants ikut
--   terhapus otomatis karena ON DELETE CASCADE.
-- ============================================================================

-- ── 0) Merchant pemilik seluruh data seed (id tetap 0x…01) ──────────────────
--    Dipakai sebagai penanda agar mudah dibersihkan nanti.
INSERT INTO users (id, email, password_hash, name, phone, role)
VALUES (
    decode('00000000000000000000000000000001', 'hex'),
    'seed-merchant@pulse.local',
    NULL,
    'Seed Merchant',
    '0800000000',
    'merchant'
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO merchant_details (user_id, store_name, description, logo_url, verified)
VALUES (
    decode('00000000000000000000000000000001', 'hex'),
    'Seed Official Store',
    'Toko seed untuk uji performa data massal.',
    'https://image.ulalaapi.store/ticketing/seulgi.jpg',
    TRUE
)
ON CONFLICT (user_id) DO NOTHING;

-- ── 1) Bulk INSERT events ───────────────────────────────────────────────────
--    id  = 16 byte deterministik dari nomor seri (byte pertama = 0x00).
--    price dasar = tier Rp75rb–Rp1jt (varian Reguler mengikuti angka ini).
INSERT INTO events (
    id, merchant_id, name, slug, description, cover_url, detail_images,
    price, venue, city, latitude, longitude, event_date, status, category
)
SELECT
    decode(lpad(to_hex(g), 32, '0'), 'hex'),                       -- id (16 byte)
    decode('00000000000000000000000000000001', 'hex'),            -- merchant_id
    'Seed Event #' || g,                                          -- name
    'seed-' || g,                                                 -- slug (unik)
    'Event uji performa nomor ' || g || '. Data dummy untuk benchmark.',
    'https://picsum.photos/seed/' || g || '/600/450',            -- cover_url
    '[]'::jsonb,                                                  -- detail_images
    (ARRAY[75000,100000,150000,250000,350000,500000,750000,1000000]
        )[1 + (g % 8)]::numeric(12,2),                            -- price dasar (Reguler)
    (ARRAY['Istora Senayan','JCC','GBK','ICE BSD','Balai Sarbini',
           'Beach City','Trans Studio','Jatim Expo'])[1 + (g % 8)],-- venue
    (ARRAY['Jakarta','Bandung','Surabaya','Medan','Bali',
           'Yogyakarta','Semarang','Makassar'])[1 + (g % 8)],     -- city
    -6.2 + (g % 1000) * 0.001,                                    -- latitude
    106.8 + (g % 1000) * 0.001,                                   -- longitude
    NOW() + ((g % 365) || ' days')::interval
          + ((g % 24)  || ' hours')::interval,                   -- event_date (tersebar)
    'active',                                                     -- status
    to_jsonb(ARRAY[
        (ARRAY['Konser','Festival','Teater','Olahraga','Seminar',
               'Workshop','Pameran','Standup'])[1 + (g % 8)]
    ])                                                            -- category jsonb
FROM generate_series(1, 1000000) AS g          -- ⚙️ UBAH JUMLAH EVENT DI SINI
ON CONFLICT (id) DO NOTHING;

-- ── 2) 3 varian per event (Reguler / VIP / VVIP) di event_variants ──────────
--    Ini SUMBER HARGA & STOK di listing (min price varian aktif). Tanpa varian
--    aktif → harga NULL → tampil "Gratis". Maka bagian ini WAJIB berhasil.
--    id varian = id event dgn byte pertama diset 0xFF/0xFE/0xFD (unik & tak
--    bentrok dgn id event [byte0=0x00] maupun varian asli [ULID]).
INSERT INTO event_variants (
    id, event_id, name, description, price, sale_price,
    sale_price_start_date, sale_price_end_date,
    quota, sold, max_per_order, is_active, sort_order
)
SELECT
    set_byte(e.id, 0, t.bytecode),                               -- id varian unik
    e.id,                                                        -- event_id
    t.tier_name,                                                 -- name (Reguler/VIP/VVIP)
    t.tier_name || ' access',                                    -- description
    p.price,                                                     -- price (numeric)
    CASE WHEN get_byte(e.id, 14) % 3 = 0                         -- ~1/3 event diskon 20%
         THEN round(p.price * 0.80, 2) ELSE NULL END,            -- sale_price
    CASE WHEN get_byte(e.id, 14) % 3 = 0
         THEN NOW() - INTERVAL '1 day'  ELSE NULL END,           -- sale start (aktif)
    CASE WHEN get_byte(e.id, 14) % 3 = 0
         THEN NOW() + INTERVAL '30 days' ELSE NULL END,          -- sale end
    p.quota,                                                     -- quota
    floor(random() * p.quota)::int,                             -- sold (0..quota-1)
    10,                                                          -- max_per_order
    TRUE,                                                        -- is_active (WAJIB true)
    t.sort                                                       -- sort_order
FROM events e
CROSS JOIN (VALUES
    (255, 'Reguler', 1.0::numeric, 200, 0),
    (254, 'VIP',     1.8::numeric,  80, 1),
    (253, 'VVIP',    3.0::numeric,  30, 2)
) AS t(bytecode, tier_name, mult, quota_base, sort)
CROSS JOIN LATERAL (
    SELECT round(e.price * t.mult, 2)                    AS price,
           (t.quota_base + (get_byte(e.id, 15) % 50))    AS quota
) p
WHERE e.merchant_id = decode('00000000000000000000000000000001', 'hex')
ON CONFLICT (id) DO NOTHING;

-- ── 3) Refresh statistik planner (WAJIB agar EXPLAIN realistis) ─────────────
ANALYZE events;
ANALYZE event_variants;

-- ============================================================================
-- CEK CEPAT (jalankan manual di psql):
--   SELECT count(*) FROM events;
--   SELECT count(*) FROM event_variants;
--   -- pastikan harga TIDAK 0/gratis:
--   SELECT min(price), max(price), count(*) FILTER (WHERE sale_price IS NOT NULL)
--     FROM event_variants;
--   -- listing halaman 1 (harusnya cepat, pakai index event_date):
--   EXPLAIN ANALYZE
--     SELECT e.id FROM events e ORDER BY e.event_date ASC LIMIT 20 OFFSET 0;
--   -- pagination halaman jauh (uji OFFSET besar):
--   EXPLAIN ANALYZE
--     SELECT e.id FROM events e ORDER BY e.event_date ASC LIMIT 20 OFFSET 500000;
-- ============================================================================

-- ── HAPUS EVENT YANG TIDAK PUNYA VARIAN (orphan) ────────────────────────────
--    Berguna kalau tadi varian gagal dibuat → event tampil "Gratis". Query ini
--    menghapus event yang TIDAK punya satu pun baris di event_variants.
--    NOT EXISTS = anti-join (efisien; pakai index event_variants(event_id) bila ada).
--
--    (A) Aman — hanya event SEED milik merchant 0x…01:
-- DELETE FROM events e
--  WHERE e.merchant_id = decode('00000000000000000000000000000001', 'hex')
--    AND NOT EXISTS (SELECT 1 FROM event_variants v WHERE v.event_id = e.id);
--
--    (B) Global — SEMUA event tanpa varian (HATI-HATI: termasuk event asli yang
--        mungkin masih draft belum diberi varian):
-- DELETE FROM events e
--  WHERE NOT EXISTS (SELECT 1 FROM event_variants v WHERE v.event_id = e.id);
--
--    Cek dulu berapa yang akan terhapus (ganti DELETE→SELECT count(*)):
-- SELECT count(*) FROM events e
--  WHERE NOT EXISTS (SELECT 1 FROM event_variants v WHERE v.event_id = e.id);

-- ── HAPUS SEED (uncomment untuk rollback) ───────────────────────────────────
-- DELETE FROM events
--  WHERE merchant_id = decode('00000000000000000000000000000001', 'hex');
-- DELETE FROM merchant_details
--  WHERE user_id = decode('00000000000000000000000000000001', 'hex');
-- DELETE FROM users
--  WHERE id = decode('00000000000000000000000000000001', 'hex');
-- VACUUM ANALYZE events;
-- VACUUM ANALYZE event_variants;
