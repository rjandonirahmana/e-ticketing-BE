-- ============================================================================
-- Migration: 027_direct_chat.sql
-- Chat berubah dari GRUP PER PRODUK menjadi PERCAKAPAN PENJUAL ↔ PEMBELI.
-- ============================================================================
--
-- ATURAN PENULISAN (sama dengan 022, 023, 024, 026)
--   1. TIDAK ADA titik-koma di dalam komentar.
--   2. TIDAK ADA apostrof di dalam komentar. Pakai backtick.
--   3. TIDAK ADA blok dollar-quote.
--
-- ── KENAPA ──────────────────────────────────────────────────────────────────
-- Model lama: satu produk = satu grup, dan pembeli otomatis dimasukkan ke grup
-- itu setelah pembayaran lunas. Itu masuk akal untuk tiket acara -- orang yang
-- pergi ke konser yang sama memang punya alasan saling bicara.
--
-- Untuk marketplace barang, alasan itu hilang. Yang dibutuhkan pembeli adalah
-- bertanya kepada PENJUALNYA: stok, ukuran, ongkir, kapan bisa diambil. Dan
-- pertanyaan itu tidak boleh terbaca oleh semua orang yang kebetulan membeli
-- barang yang sama -- di antaranya ada alamat, nomor pesanan, dan keluhan.
--
-- ── KENAPA TABELNYA DIPAKAI ULANG, BUKAN BIKIN BARU ─────────────────────────
-- `group_messages`, `group_members`, seluruh jalur WebSocket, paginasi riwayat
-- ber-cursor, dan indexnya sudah bekerja dan sudah teruji. Yang berubah
-- sebenarnya cuma SATU hal: bagaimana sebuah room lahir dan siapa yang ada di
-- dalamnya. Membuat tabel `dm_*` yang sejajar berarti menggandakan semua itu
-- beserta bugnya masing-masing.
--
-- Sebuah room kini punya DUA bentuk yang hidup berdampingan:
--   * `event_id` terisi          -> grup produk lama (dibiarkan apa adanya)
--   * `buyer_id` + `merchant_id` -> percakapan berdua
--
-- ── DATA LAMA TIDAK DIBUANG ─────────────────────────────────────────────────
-- Grup yang sudah ada tetap bisa dibuka dan riwayatnya tetap terbaca. Yang
-- berhenti adalah PEMBUATAN grup baru dan auto-join setelah pembayaran. Ini
-- disengaja: menghapus percakapan yang sudah terjadi adalah kehilangan yang
-- tak bisa dibatalkan, sedangkan membiarkannya hanya menyisakan beberapa baris.
--
--   psql "$DATABASE_URL" -f migration/027_direct_chat.sql
-- ============================================================================

-- ── 1. `event_id` tak lagi wajib dan tak lagi unik ──────────────────────────
-- UNIQUE-nya harus dilepas SEBELUM kolom peserta dipakai: dengan `event_id`
-- NULL pada setiap percakapan berdua, UNIQUE lama akan menolak percakapan
-- KEDUA di seluruh platform pada sebagian konfigurasi.
ALTER TABLE group_rooms DROP CONSTRAINT IF EXISTS group_rooms_event_id_key;
ALTER TABLE group_rooms ALTER COLUMN event_id DROP NOT NULL;

-- Keunikannya DIKEMBALIKAN sebagai index PARSIAL, bukan dibuang begitu saja.
-- `upsert_product_room` memakai `ON CONFLICT (event_id)`, dan klausa itu butuh
-- sebuah index unik yang cocok -- tanpa penggantinya, grup produk lama x
-- saat runtime dengan `no unique or exclusion constraint matching the ON
-- CONFLICT specification`, yaitu kegagalan yang tak akan terlihat sampai ada
-- merchant yang membuka grup lamanya.
--
-- `WHERE event_id IS NOT NULL` membuatnya hanya berlaku bagi grup produk;
-- percakapan berdua (event_id NULL) tak ikut diatur, sehingga jumlahnya bebas.
CREATE UNIQUE INDEX IF NOT EXISTS uq_group_rooms_event
    ON group_rooms (event_id)
    WHERE event_id IS NOT NULL;

-- ── 2. Peserta percakapan ───────────────────────────────────────────────────
ALTER TABLE group_rooms ADD COLUMN IF NOT EXISTS buyer_id    BYTEA NULL;
ALTER TABLE group_rooms ADD COLUMN IF NOT EXISTS merchant_id BYTEA NULL;

ALTER TABLE group_rooms DROP CONSTRAINT IF EXISTS fk_group_rooms_buyer;
ALTER TABLE group_rooms ADD CONSTRAINT fk_group_rooms_buyer
    FOREIGN KEY (buyer_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE group_rooms DROP CONSTRAINT IF EXISTS fk_group_rooms_merchant;
ALTER TABLE group_rooms ADD CONSTRAINT fk_group_rooms_merchant
    FOREIGN KEY (merchant_id) REFERENCES users(id) ON DELETE CASCADE;

-- ── 3. Satu percakapan per pasangan ─────────────────────────────────────────
-- Ini bukan sekadar kerapian. Tanpa unique, dua permintaan `ensure_dm` yang
-- tiba bersamaan -- pembeli menekan `Chat Penjual` dua kali, atau dari dua tab
-- -- akan melahirkan DUA room untuk pasangan yang sama, dan pesan-pesannya
-- terbelah di antara keduanya tanpa ada yang menyadarinya. Unique membuat yang
-- kalah balapan gagal secara terbuka, sehingga kode bisa mengambil room yang
-- sudah ada.
--
-- Partial: baris grup lama ber-`buyer_id` NULL tidak ikut diatur.
CREATE UNIQUE INDEX IF NOT EXISTS uq_group_rooms_dm
    ON group_rooms (buyer_id, merchant_id)
    WHERE buyer_id IS NOT NULL AND merchant_id IS NOT NULL;

-- Daftar percakapan milik satu toko -- dibaca merchant tiap membuka pesan.
CREATE INDEX IF NOT EXISTS idx_group_rooms_merchant
    ON group_rooms (merchant_id)
    WHERE merchant_id IS NOT NULL;
