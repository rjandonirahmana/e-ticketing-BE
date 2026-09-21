-- 006_perf_indexes.sql — Indeks untuk hot-path (throughput).
--
-- Diturunkan dari query NYATA di src/repository/* (bukan dari skema migrasi
-- lama yang sudah drift). Semua `CREATE INDEX IF NOT EXISTS` → idempoten, aman
-- dijalankan ulang.
--
-- ── KENAPA NAMA TABELNYA DICARI, BUKAN DITULIS LANGSUNG ─────────────────────
-- Berkas ini lahir di era ketika tabelnya bernama `events` dan `event_variants`.
-- `023_products_rename.sql` kemudian me-rename keduanya menjadi `products` dan
-- `product_variants`, dan MEMBUANG `order_items` serta bentuk lama `tickets`.
--
-- Akibatnya berkas ini hanya benar di SATU titik sejarah. Dijalankan pada
-- database yang sudah melewati 023 — dan itu terjadi setiap kali penjalan
-- migrasi menemukan 006 belum tercatat di `schema_migrations` — ia mati dengan
--
--     ERROR: relation "events" does not exist
--     ERROR: relation "event_variants" does not exist
--
-- dan aplikasinya menolak start. Galat itu MENYESATKAN: yang salah bukan
-- databasenya, melainkan anggapan berkas ini bahwa dunia berhenti di 022.
--
-- Karena isinya SEMATA-MATA index, tak ada satu pun keputusan data di sini yang
-- bisa salah bila dijalankan di era lain. Jadi tiap tabel dicari dengan nama
-- yang BERLAKU di database ini, dan index yang tabelnya memang sudah tak ada
-- lagi (`order_items`, `tickets.order_item_id`) dilewati — penggantinya dibuat
-- oleh 023. Nama index-nya sengaja TIDAK berubah, supaya database yang sudah
-- punya index itu tidak mendapat duplikat dengan nama lain.
--
-- CATATAN PENTING:
--   * PostgreSQL TIDAK meng-index foreign key secara otomatis → JOIN lewat FK
--     tanpa index = sequential scan. Mayoritas indeks di bawah menutup itu.
--   * Untuk tabel besar dengan trafik live, ganti `CREATE INDEX` menjadi
--     `CREATE INDEX CONCURRENTLY` dan jalankan DI LUAR transaksi
--     (psql -f file.sql, JANGAN psql -1) agar tidak mengunci tabel.
--   * `ANALYZE` di bawah dijalankan agar planner memakai indeks baru.

DO $$
DECLARE
    -- Nama yang BERLAKU hari ini, apa pun eranya. NULL = tabelnya memang tak
    -- ada di database ini, dan index-nya dilewati tanpa menggagalkan apa pun.
    produk TEXT := COALESCE(to_regclass('public.products')::text,
                            to_regclass('public.events')::text);
    varian TEXT := COALESCE(to_regclass('public.product_variants')::text,
                            to_regclass('public.event_variants')::text,
                            to_regclass('public.ticket_variants')::text);
BEGIN
    -- ── produk: listing publik + detail + filter ────────────────────────────
    IF produk IS NOT NULL THEN
        -- Listing publik: WHERE status='active' ORDER BY event_date ASC.
        EXECUTE format(
            'CREATE INDEX IF NOT EXISTS idx_events_status_date ON %I (status, event_date)',
            produk);
        -- Produk milik merchant tertentu.
        EXECUTE format(
            'CREATE INDEX IF NOT EXISTS idx_events_merchant ON %I (merchant_id)',
            produk);
        -- Filter kategori: `category @> $::jsonb` → GIN (jsonb_path_ops cukup utk @>).
        -- Kolom `category` dibuat `001a_events_content_columns.sql`.
        EXECUTE format(
            'CREATE INDEX IF NOT EXISTS idx_events_category_gin ON %I USING gin (category jsonb_path_ops)',
            produk);
    END IF;

    -- ── varian: FK ke produk (JOIN v.event_id = e.id) ───────────────────────
    -- Nama KOLOM `event_id` sengaja tak pernah ikut di-rename (lihat 023).
    IF varian IS NOT NULL THEN
        EXECUTE format(
            'CREATE INDEX IF NOT EXISTS idx_event_variants_event ON %I (event_id)',
            varian);
    END IF;

    -- ── orders: "pesanan saya" ──────────────────────────────────────────────
    -- WHERE customer_id=$1 ORDER BY created_at DESC.
    CREATE INDEX IF NOT EXISTS idx_orders_customer_date ON orders (customer_id, created_at DESC);

    -- ── order_items: FK ke order & variant ──────────────────────────────────
    -- DIBUANG oleh 023 — perannya diambil `cart_items`. Dilewati bila sudah
    -- tak ada, bukan digantikan: `cart_items` punya indexnya sendiri di 022.
    IF to_regclass('public.order_items') IS NOT NULL THEN
        CREATE INDEX IF NOT EXISTS idx_order_items_order   ON order_items (order_id);
        CREATE INDEX IF NOT EXISTS idx_order_items_variant ON order_items (ticket_variant_id);
    END IF;

    -- ── tickets: FK ke order_item ───────────────────────────────────────────
    -- 023 membuang tabel ini dan membuatnya ulang dengan `cart_item_id`
    -- menggantikan `order_item_id`, berikut indexnya sendiri. Jadi yang
    -- diperiksa KOLOMNYA, bukan tabelnya: tabel `tickets` tetap ada, tetapi
    -- kolom yang hendak di-index bisa saja sudah tidak.
    IF EXISTS (SELECT 1 FROM information_schema.columns
                WHERE table_schema = 'public' AND table_name = 'tickets'
                  AND column_name = 'order_item_id') THEN
        CREATE INDEX IF NOT EXISTS idx_tickets_order_item ON tickets (order_item_id);
    END IF;

    -- ── group chat ──────────────────────────────────────────────────────────
    -- Dibuang `migration-manual/029_chat_dua_tabel.sql` pada database lama yang
    -- sudah pindah ke `chats`/`chat_messages`; index penggantinya ada di
    -- `029a_chat_dua_tabel_baru.sql`.
    IF to_regclass('public.group_messages') IS NOT NULL THEN
        -- Riwayat pesan: WHERE room_id=$1 ORDER BY sent_at DESC, id DESC LIMIT.
        CREATE INDEX IF NOT EXISTS idx_group_messages_room ON group_messages (room_id, sent_at DESC, id DESC);
    END IF;

    IF to_regclass('public.group_members') IS NOT NULL THEN
        -- Cek keanggotaan (room_id+user_id) — sangat sering dipanggil.
        CREATE INDEX IF NOT EXISTS idx_group_members_room_user ON group_members (room_id, user_id);
        -- "Room saya": JOIN group_members ON user_id=$1 (butuh user_id sbg leading col).
        CREATE INDEX IF NOT EXISTS idx_group_members_user ON group_members (user_id);
    END IF;

    IF to_regclass('public.group_rooms') IS NOT NULL THEN
        -- Room per event: WHERE r.event_id=$1.
        CREATE INDEX IF NOT EXISTS idx_group_rooms_event ON group_rooms (event_id);
    END IF;

    -- ── notifications ───────────────────────────────────────────────────────
    -- Tabelnya dibuat `001b_notifications.sql`.
    -- List: WHERE user_id=$1 ORDER BY created_at DESC.
    CREATE INDEX IF NOT EXISTS idx_notifications_user_date ON notifications (user_id, created_at DESC);
    -- Badge belum-dibaca: WHERE user_id=$1 AND is_read=FALSE → partial index (kecil & cepat).
    CREATE INDEX IF NOT EXISTS idx_notifications_unread ON notifications (user_id) WHERE is_read = FALSE;
END $$;

ANALYZE;

-- ── OPSIONAL: pencarian teks (ILIKE '%kata%') di explore ──────────────────────
-- ILIKE dengan wildcard depan TIDAK bisa pakai btree. pg_trgm + GIN membuatnya
-- cepat. Butuh extension (umumnya tersedia di managed Postgres). Jalankan blok
-- ini HANYA jika fitur search terasa lambat. Jika tak punya hak buat extension,
-- lewati — query tetap jalan (hanya lebih lambat saat search).
--
-- CREATE EXTENSION IF NOT EXISTS pg_trgm;
-- CREATE INDEX IF NOT EXISTS idx_events_name_trgm  ON products USING gin (name  gin_trgm_ops);
-- CREATE INDEX IF NOT EXISTS idx_events_venue_trgm ON products USING gin (venue gin_trgm_ops);
-- CREATE INDEX IF NOT EXISTS idx_events_city_trgm  ON products USING gin (city  gin_trgm_ops);
-- ANALYZE;
