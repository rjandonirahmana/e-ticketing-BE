-- ============================================================================
-- Migration: 043_seed_fake_users.sql  —  SEED USER PALSU (data UJI)
-- ============================================================================
-- Tujuan: mengisi tabel `users` dengan JUTAAN customer palsu — untuk uji
-- performa listing/pagination user, dan supaya login manual saat development
-- punya banyak akun siap pakai tanpa lewat alur OTP WhatsApp.
--
-- SEMUA akun ini punya password yang SAMA: 123456789
-- (hash bcrypt dihitung sekali di luar SQL, lalu dipakai berulang — menghitung
-- bcrypt per baris untuk jutaan baris akan makan waktu berjam-jam; menyalin
-- satu hash yang sama jutaan kali makan waktu detik).
--
-- ── CARA PAKAI ──────────────────────────────────────────────────────────────
--   psql "$DATABASE_URL" -f migration-manual/043_seed_fake_users.sql
--
--   Ganti angka pada generate_series(...) di bawah (default 2.000.000).
--   Idempoten (ON CONFLICT DO NOTHING) — aman dijalankan ulang.
--
-- ── JANGAN PERNAH DI PRODUKSI ────────────────────────────────────────────────
--   Password yang sama untuk jutaan akun adalah lubang keamanan raksasa kalau
--   sampai berjalan di database yang sungguhan dipakai orang. Hanya untuk DB
--   lokal/staging yang isinya memang boleh dibuang.
--
-- ── ID & ROLLBACK ────────────────────────────────────────────────────────────
--   id = 16 byte, byte PERTAMA selalu 0x10 — penanda "user seed", dipakai
--   rollback di bawah. Sisa byte-nya nomor seri `g` (deterministik, sama
--   seperti pola `041_seed_bulk.sql` untuk products, byte0=0x00 di sana).
-- ============================================================================

INSERT INTO users (id, email, password_hash, name, phone, role)
SELECT
    set_byte(decode(lpad(to_hex(g), 32, '0'), 'hex'), 0, 16),      -- id (0x10 = seed user)
    'fakeuser' || g || '@fake.local',                              -- email (unik)
    '$2y$10$dhn1HzU7l7X3Aze7SZ1J0eiVjbbdQTQoVrq6hXHYovKfnS47KI00K', -- bcrypt("123456789")
    (ARRAY['Andi','Budi','Citra','Dewi','Eka','Fajar','Gita','Hadi',
           'Indah','Joko','Kirana','Lestari','Made','Nadia','Oki','Putri'])[1 + (g % 16)]
      || ' ' ||
    (ARRAY['Saputra','Wijaya','Pratama','Kusuma','Santoso','Permata',
           'Nugroho','Setiawan','Hidayat','Wardhani','Utami','Susanto'])[1 + (g % 12)],
    '08' || lpad(g::text, 10, '0'),                                -- phone (unik, bukan nomor asli)
    'customer'
FROM generate_series(1, 2000000) AS g          -- ⚙️ UBAH JUMLAH USER DI SINI
ON CONFLICT (id) DO NOTHING;

ANALYZE users;

-- ── Ringkasan hasil ──────────────────────────────────────────────────────────
SELECT COUNT(*) AS seed_users FROM users WHERE substring(id from 1 for 1) = '\x10';

-- ── ROLLBACK (hapus seed) — uncomment untuk membersihkan ────────────────────
-- Kalau ada order/cart palsu dari 044_seed_fake_orders.sql yang menunjuk ke
-- user ini, hapus 044 DULU (FK `orders.customer_id` ON DELETE RESTRICT
-- menolak hapus user yang masih punya order).
--   DELETE FROM users WHERE substring(id from 1 for 1) = '\x10';
