-- ============================================================================
-- Migration: 044_seed_fake_orders.sql  —  SEED ORDER PALSU (data UJI)
-- ============================================================================
-- Tujuan: mengisi `carts` + `cart_items` + `orders` + `tickets` dengan order
-- palsu berstatus 'paid', dipasangkan ke user & varian yang SUDAH ada.
--
-- ── PRASYARAT (WAJIB dijalankan dulu, dalam urutan ini) ──────────────────────
--   1. migration-manual/043_seed_fake_users.sql  (sumber customer_id)
--   2. migration-manual/041_seed_bulk.sql         (sumber ticket_variant_id)
--
-- ── CARA PAKAI ──────────────────────────────────────────────────────────────
--   psql "$DATABASE_URL" -f migration-manual/044_seed_fake_orders.sql
--
--   Ganti angka pada generate_series(...) di bawah (default 500.000).
--   Idempoten (ON CONFLICT DO NOTHING) — aman dijalankan ulang.
--
-- ── TEKNIK: EMPAT INSERT DIRANTAI LEWAT RETURNING ────────────────────────────
-- CTE data-modifying yang cuma sama-sama SELECT dari sumber yang sama TIDAK
-- punya urutan eksekusi terjamin (Postgres boleh menjalankannya bersamaan).
-- Di sini urutannya PENTING — cart harus ada sebelum order menunjuknya, order
-- harus ada sebelum tiket menunjuknya — jadi tiap tahap men-JOIN ke RETURNING
-- tahap sebelumnya, bukan cuma ke sumbernya. Itu yang memaksa Postgres
-- menjalankannya cart -> order -> cart_item -> ticket, di dalam SATU statement.
--
-- ── JANGAN PERNAH DI PRODUKSI. Cuma untuk DB lokal/staging. ─────────────────
--
-- ── ID & ROLLBACK ────────────────────────────────────────────────────────────
--   Semua id 16 byte, byte PERTAMA menandai jenisnya (beda tabel, jadi aman
--   dipakai ulang lintas tabel): 0x11 cart, 0x12 cart_item, 0x13 order,
--   0x14 ticket. Sisa byte = nomor seri `g`.
-- ============================================================================

WITH gen AS (
    SELECT g,
        set_byte(decode(lpad(to_hex(g), 32, '0'), 'hex'), 0, 17) AS cart_id,       -- 0x11
        set_byte(decode(lpad(to_hex(g), 32, '0'), 'hex'), 0, 18) AS cart_item_id,  -- 0x12
        set_byte(decode(lpad(to_hex(g), 32, '0'), 'hex'), 0, 19) AS order_id,      -- 0x13
        set_byte(decode(lpad(to_hex(g), 32, '0'), 'hex'), 0, 20) AS ticket_id,     -- 0x14
        -- customer_id: user seed acak dari 043 (byte0=0x10), dipilih lewat sisa bagi
        -- terhadap JUMLAH user seed yang sungguhan ada (bukan angka tetap) —
        -- supaya berkas ini tetap benar walau 043 dijalankan dengan N berbeda.
        set_byte(
            decode(lpad(to_hex(1 + (g % GREATEST(u.n_users, 1))), 32, '0'), 'hex'),
            0, 16
        ) AS customer_id,
        -- variant_id: varian Reguler (byte0=0xFF, lihat 041_seed_bulk.sql) dari
        -- product seed #(1 + g % jumlah_product_seed).
        set_byte(decode(lpad(to_hex(1 + (g % GREATEST(p.n_products, 1))), 32, '0'), 'hex'), 0, 255) AS variant_id,
        (NOW() - ((g % 90) || ' days')::interval) AS paid_at
    FROM generate_series(1, 500000) AS g     -- ⚙️ UBAH JUMLAH ORDER DI SINI
    CROSS JOIN (SELECT COUNT(*) AS n_users FROM users WHERE substring(id from 1 for 1) = '\x10') u
    CROSS JOIN (SELECT COUNT(*) AS n_products FROM products WHERE substring(id from 1 for 1) = '\x00') p
),
gen_priced AS (
    SELECT gen.*, pv.price AS unit_price
    FROM gen
    JOIN product_variants pv ON pv.id = gen.variant_id
),
ins_cart AS (
    INSERT INTO carts (id, user_id, position, created_at, updated_at, deleted_at)
    SELECT cart_id, customer_id, 'checkout', paid_at, paid_at, paid_at
    FROM gen_priced
    ON CONFLICT (id) DO NOTHING
    RETURNING id AS cart_id
),
ins_order AS (
    INSERT INTO orders (
        id, customer_id, order_code, status, total_amount, subtotal_amount,
        discount_amount, payment_method, payment_vendor, paid_at, created_at,
        updated_at, cart_id
    )
    SELECT g.order_id, g.customer_id, 'SEED-' || g.g, 'paid', g.unit_price, g.unit_price,
           0, 'QRIS', 'seed', g.paid_at, g.paid_at, g.paid_at, g.cart_id
    FROM gen_priced g
    JOIN ins_cart c ON c.cart_id = g.cart_id
    ON CONFLICT (id) DO NOTHING
    RETURNING id AS order_id
),
ins_item AS (
    INSERT INTO cart_items (id, cart_id, ticket_variant_id, quantity, unit_price, selected, created_at, updated_at)
    SELECT g.cart_item_id, g.cart_id, g.variant_id, 1, g.unit_price, true, g.paid_at, g.paid_at
    FROM gen_priced g
    JOIN ins_order o ON o.order_id = g.order_id
    ON CONFLICT (id) DO NOTHING
    RETURNING id AS cart_item_id
)
INSERT INTO tickets (id, cart_item_id, order_id, ticket_code, status, created_at, updated_at)
SELECT g.ticket_id, g.cart_item_id, g.order_id, 'SEEDTIX-' || g.g, 'active', g.paid_at, g.paid_at
FROM gen_priced g
JOIN ins_item it ON it.cart_item_id = g.cart_item_id
ON CONFLICT (id) DO NOTHING;

ANALYZE carts;
ANALYZE cart_items;
ANALYZE orders;
ANALYZE tickets;

-- ── Ringkasan hasil ──────────────────────────────────────────────────────────
SELECT
    (SELECT COUNT(*) FROM carts   WHERE substring(id from 1 for 1) = '\x11') AS seed_carts,
    (SELECT COUNT(*) FROM orders  WHERE substring(id from 1 for 1) = '\x13') AS seed_orders,
    (SELECT COUNT(*) FROM tickets WHERE substring(id from 1 for 1) = '\x14') AS seed_tickets;

-- ── ROLLBACK (hapus seed) — uncomment untuk membersihkan ────────────────────
-- Urutan turun (anak sebelum induk), sekalipun sebagian sudah CASCADE:
--   DELETE FROM tickets     WHERE substring(id from 1 for 1) = '\x14';
--   DELETE FROM cart_items  WHERE substring(id from 1 for 1) = '\x12';
--   DELETE FROM orders      WHERE substring(id from 1 for 1) = '\x13';
--   DELETE FROM carts       WHERE substring(id from 1 for 1) = '\x11';
