-- ============================================================================
-- 029_chat_dua_tabel.sql   —   JALANKAN MANUAL, TIDAK OTOMATIS
-- Chat: TIGA tabel menjadi DUA, plus urutan inbox yang benar.
-- ============================================================================
--
-- Berkas ini sengaja TIDAK berada di `migration/`. Isinya membuang tabel dan
-- riwayat percakapan, dan itu tak bisa dibatalkan. Baca `migration-manual/README.md`
-- untuk urutan deploy-nya -- tabel diganti nama di sini, jadi kode lama dan kode
-- baru tak bisa hidup bersamaan.
--
-- ATURAN PENULISAN (sama dengan 022, 023, 024, 026, 027)
--   1. TIDAK ADA titik-koma di dalam komentar.
--   2. TIDAK ADA apostrof di dalam komentar. Pakai backtick.
--   3. TIDAK ADA blok dollar-quote.
--
-- ── APA YANG DIBUANG, DAN KENAPA ────────────────────────────────────────────
--
-- 1. `group_members` -- SELURUH TABEL.
--    Sejak chat menjadi percakapan berdua, baris room sudah memuat `buyer_id`
--    dan `merchant_id`, dan mereka MEMANG kedua anggotanya. Tabel terpisah
--    menjadi sumber kebenaran KEDUA untuk hal yang sudah diketahui baris
--    pertama, dan dua sumber kebenaran selalu bisa berselisih. Bentuk
--    selisihnya yang paling buruk sudah nyata: room yang ada tetapi tak bisa
--    dimasuki siapa pun, termasuk oleh yang barusan membuatnya.
--
-- 2. Grup produk lama beserta pesannya.
--
-- 3. Empat kolom mati di tabel room:
--    * `event_id`   -- tak ada lagi room yang terikat produk.
--    * `created_by` -- selalu sama dengan `merchant_id`.
--    * `name` dan `cover_url` -- salinan `merchant_details.store_name` dan
--      `logo_url`. Salinan itu bukan sekadar mubazir, ia BASI: toko yang
--      berganti nama atau logo tetap tampil dengan nama lamanya di daftar
--      pesan selamanya, karena tak ada yang pernah memperbaruinya. Sekarang
--      di-JOIN saat dibaca -- lookup primary key, praktis gratis.
--
-- ── APA YANG DITAMBAH ───────────────────────────────────────────────────────
--
-- `last_message_at` -- ini memperbaiki cacat, bukan sekadar mempercepat.
-- Daftar percakapan sebelumnya diurutkan `created_at DESC`, yaitu kapan
-- percakapan DIBUAT. Percakapan yang baru menerima pesan tak pernah naik ke
-- atas, sehingga pertanyaan baru dari pembeli tenggelam di bawah percakapan
-- lama yang sudah selesai. Untuk sebuah inbox, itu urutan yang salah.
--
-- Alternatifnya menghitung `MAX(sent_at)` saat membaca daftar: satu subquery
-- berkorelasi ke tabel yang tumbuh tanpa batas, untuk setiap percakapan, setiap
-- kali inbox dibuka. Kolom yang diperbarui saat menulis memindahkan biaya itu
-- ke satu UPDATE per pesan.
--
-- `buyer_read_at` / `merchant_read_at` -- penanda belum dibaca tanpa tabel
-- ketiga. Jumlah belum dibaca = COUNT pesan ber-`sent_at` lebih baru, memakai
-- index riwayat yang memang sudah ada.
-- ============================================================================

BEGIN;

-- ── 1. Buang grup produk lama ───────────────────────────────────────────────
-- Pesannya ikut terhapus lewat ON DELETE CASCADE pada `group_messages.room_id`.
DELETE FROM group_rooms WHERE event_id IS NOT NULL;

-- ── 2. Buang tabel anggota ──────────────────────────────────────────────────
DROP TABLE IF EXISTS group_members;

-- ── 3. Ganti nama tabel agar sesuai maknanya ────────────────────────────────
-- `group_rooms` untuk percakapan berdua adalah nama yang menyesatkan siapa pun
-- yang membacanya nanti. Pola `ALTER TABLE IF EXISTS ... RENAME TO` sama dengan
-- yang dipakai migrasi 023, dan aman diulang.
ALTER TABLE IF EXISTS group_rooms    RENAME TO chats;
ALTER TABLE IF EXISTS group_messages RENAME TO chat_messages;
ALTER TABLE IF EXISTS chat_messages  RENAME COLUMN room_id TO chat_id;

-- ── 4. Buang kolom mati ─────────────────────────────────────────────────────
ALTER TABLE chats DROP COLUMN IF EXISTS event_id;
ALTER TABLE chats DROP COLUMN IF EXISTS created_by;
ALTER TABLE chats DROP COLUMN IF EXISTS name;
ALTER TABLE chats DROP COLUMN IF EXISTS cover_url;

-- ── 5. Peserta kini WAJIB ───────────────────────────────────────────────────
-- Sesudah grup lama hilang, tak ada lagi baris sah yang boleh kosong. NOT NULL
-- membuat percakapan tanpa peserta mustahil di tingkat basis data, bukan
-- sekadar tak diharapkan oleh kode.
DELETE FROM chats WHERE buyer_id IS NULL OR merchant_id IS NULL;
ALTER TABLE chats ALTER COLUMN buyer_id    SET NOT NULL;
ALTER TABLE chats ALTER COLUMN merchant_id SET NOT NULL;

-- ── 6. Urutan inbox & penanda dibaca ────────────────────────────────────────
ALTER TABLE chats ADD COLUMN IF NOT EXISTS last_message_at  TIMESTAMPTZ;
ALTER TABLE chats ADD COLUMN IF NOT EXISTS buyer_read_at    TIMESTAMPTZ;
ALTER TABLE chats ADD COLUMN IF NOT EXISTS merchant_read_at TIMESTAMPTZ;

-- `COALESCE` ke `created_at` untuk percakapan yang belum berisi pesan sama
-- sekali. Dibiarkan NULL, ia akan hilang dari inbox yang mengurutkan kolom ini.
UPDATE chats c
SET last_message_at = COALESCE(
        (SELECT MAX(m.sent_at) FROM chat_messages m WHERE m.chat_id = c.id),
        c.created_at)
WHERE c.last_message_at IS NULL;

ALTER TABLE chats ALTER COLUMN last_message_at SET NOT NULL;
ALTER TABLE chats ALTER COLUMN last_message_at SET DEFAULT NOW();

-- ── 7. Index ────────────────────────────────────────────────────────────────
-- Index lama menyebut nama tabel lama -- PostgreSQL tidak mengganti namanya
-- sendiri saat tabel di-rename, jadi ia dibuang dan dibuat ulang dengan nama
-- yang jujur.
DROP INDEX IF EXISTS uq_group_rooms_dm;
DROP INDEX IF EXISTS uq_group_rooms_event;
DROP INDEX IF EXISTS idx_group_rooms_merchant;
DROP INDEX IF EXISTS idx_group_rooms_event;
DROP INDEX IF EXISTS idx_group_rooms_creator;

-- Satu percakapan per pasangan. Boleh UNIQUE penuh sekarang (bukan parsial)
-- karena kedua kolomnya sudah NOT NULL.
CREATE UNIQUE INDEX IF NOT EXISTS uq_chats_pasangan
    ON chats (buyer_id, merchant_id);

-- Inbox: `WHERE buyer_id = $1 ORDER BY last_message_at DESC`. Kolom urutnya
-- ikut masuk index supaya pembacaannya tak perlu menyortir apa pun.
CREATE INDEX IF NOT EXISTS idx_chats_pembeli
    ON chats (buyer_id, last_message_at DESC);
CREATE INDEX IF NOT EXISTS idx_chats_toko
    ON chats (merchant_id, last_message_at DESC);

-- Riwayat per percakapan, paginasi ber-cursor.
DROP INDEX IF EXISTS idx_group_messages_room_sent;
DROP INDEX IF EXISTS idx_group_messages_room;
DROP INDEX IF EXISTS idx_group_messages_sender;
CREATE INDEX IF NOT EXISTS idx_chat_messages_riwayat
    ON chat_messages (chat_id, sent_at DESC, id DESC);

COMMIT;

-- ── Sesudah COMMIT ──────────────────────────────────────────────────────────
-- Tabel `chats` baru saja kehilangan empat kolom dan tabel `chat_messages`
-- kehilangan sebagian besar barisnya. Ruangnya belum kembali ke sistem berkas
-- sampai VACUUM berjalan, dan perencana query masih memakai statistik lama.
--
-- Dijalankan di LUAR transaksi karena VACUUM memang tak bisa di dalamnya:
--
--   VACUUM (ANALYZE) chats;
--   VACUUM (ANALYZE) chat_messages;
