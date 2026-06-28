-- 006_perf_indexes.sql — Indeks untuk hot-path (throughput).
--
-- Diturunkan dari query NYATA di src/repository/* (bukan dari skema migrasi
-- lama yang sudah drift). Semua `CREATE INDEX IF NOT EXISTS` → idempoten, aman
-- dijalankan ulang. Tabel/kolom di sini dipakai oleh query yang sudah berjalan,
-- jadi pasti ada.
--
-- CATATAN PENTING:
--   * PostgreSQL TIDAK meng-index foreign key secara otomatis → JOIN lewat FK
--     tanpa index = sequential scan. Mayoritas indeks di bawah menutup itu.
--   * Untuk tabel besar dengan trafik live, ganti `CREATE INDEX` menjadi
--     `CREATE INDEX CONCURRENTLY` dan jalankan DI LUAR transaksi
--     (psql -f file.sql, JANGAN psql -1) agar tidak mengunci tabel.
--   * Setelah apply, jalankan `ANALYZE;` agar planner memakai indeks baru.

-- ── events: listing publik + detail + filter ──────────────────────────────────
-- Listing publik: WHERE status='active' ORDER BY event_date ASC.
CREATE INDEX IF NOT EXISTS idx_events_status_date    ON events (status, event_date);
-- Event milik merchant tertentu.
CREATE INDEX IF NOT EXISTS idx_events_merchant       ON events (merchant_id);
-- Filter kategori: `category @> $::jsonb` → GIN (jsonb_path_ops cukup utk @>).
CREATE INDEX IF NOT EXISTS idx_events_category_gin   ON events USING gin (category jsonb_path_ops);

-- ── orders: "pesanan saya" ────────────────────────────────────────────────────
-- WHERE customer_id=$1 ORDER BY created_at DESC.
CREATE INDEX IF NOT EXISTS idx_orders_customer_date  ON orders (customer_id, created_at DESC);

-- ── order_items: FK ke order & variant (dipakai di banyak JOIN) ───────────────
CREATE INDEX IF NOT EXISTS idx_order_items_order     ON order_items (order_id);
CREATE INDEX IF NOT EXISTS idx_order_items_variant   ON order_items (ticket_variant_id);

-- ── event_variants: FK ke event (JOIN ev.event_id = e.id) ─────────────────────
CREATE INDEX IF NOT EXISTS idx_event_variants_event  ON event_variants (event_id);

-- ── tickets: FK ke order_item (JOIN t.order_item_id = oi.id) ──────────────────
CREATE INDEX IF NOT EXISTS idx_tickets_order_item    ON tickets (order_item_id);

-- ── group chat ────────────────────────────────────────────────────────────────
-- Riwayat pesan: WHERE room_id=$1 ORDER BY sent_at DESC, id DESC LIMIT.
CREATE INDEX IF NOT EXISTS idx_group_messages_room   ON group_messages (room_id, sent_at DESC, id DESC);
-- Cek keanggotaan (room_id+user_id) — sangat sering dipanggil.
CREATE INDEX IF NOT EXISTS idx_group_members_room_user ON group_members (room_id, user_id);
-- "Room saya": JOIN group_members ON user_id=$1 (butuh user_id sbg leading col).
CREATE INDEX IF NOT EXISTS idx_group_members_user    ON group_members (user_id);
-- Room per event: WHERE r.event_id=$1.
CREATE INDEX IF NOT EXISTS idx_group_rooms_event     ON group_rooms (event_id);

-- ── notifications ─────────────────────────────────────────────────────────────
-- List: WHERE user_id=$1 ORDER BY created_at DESC.
CREATE INDEX IF NOT EXISTS idx_notifications_user_date ON notifications (user_id, created_at DESC);
-- Badge belum-dibaca: WHERE user_id=$1 AND is_read=FALSE → partial index (kecil & cepat).
CREATE INDEX IF NOT EXISTS idx_notifications_unread  ON notifications (user_id) WHERE is_read = FALSE;

ANALYZE;

-- ── OPSIONAL: pencarian teks (ILIKE '%kata%') di explore ──────────────────────
-- ILIKE dengan wildcard depan TIDAK bisa pakai btree. pg_trgm + GIN membuatnya
-- cepat. Butuh extension (umumnya tersedia di managed Postgres). Jalankan blok
-- ini HANYA jika fitur search terasa lambat. Jika tak punya hak buat extension,
-- lewati — query tetap jalan (hanya lebih lambat saat search).
--
-- CREATE EXTENSION IF NOT EXISTS pg_trgm;
-- CREATE INDEX IF NOT EXISTS idx_events_name_trgm  ON events USING gin (name  gin_trgm_ops);
-- CREATE INDEX IF NOT EXISTS idx_events_venue_trgm ON events USING gin (venue gin_trgm_ops);
-- CREATE INDEX IF NOT EXISTS idx_events_city_trgm  ON events USING gin (city  gin_trgm_ops);
-- ANALYZE;
