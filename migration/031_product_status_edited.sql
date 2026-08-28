-- ============================================================================
-- Migration: 031_product_status_edited.sql
-- Status produk: TIGA keadaan nyata, dan database yang menegakkannya.
-- ============================================================================
--
-- ATURAN PENULISAN (sama dengan 022, 023, 024, 026, 027, 030)
--   1. TIDAK ADA titik-koma di dalam komentar.
--   2. TIDAK ADA apostrof di dalam komentar. Pakai backtick.
--   3. TIDAK ADA blok dollar-quote.
--
-- ── MASALAH PERTAMA: `edited` TIDAK PERNAH DIIZINKAN ────────────────────────
--
-- `migration/001.sql` memasang pagar berikut pada tabel yang saat itu bernama
-- `events`:
--
--     status VARCHAR(20) DEFAULT `active`
--            CHECK (status IN (`active`, `cancelled`, `completed`))
--
-- Sejak itu kode Rust menambahkan status `edited` sebagai penanda bahwa produk
-- sedang ditahan menunggu tinjauan admin. Lihat
-- `models::products::STATUS_MENUNGGU_REVIEW`, yang dipakai `repository/product/
-- write.rs` pada CREATE maupun UPDATE.
--
-- Pagarnya tidak pernah ikut diperbarui. Akibatnya bukan sekadar tidak rapi:
-- setiap merchant yang menekan SIMPAN memicu CHECK CONSTRAINT VIOLATION, dan
-- suntingannya ditolak seluruhnya. Fitur `tahan produk saat disunting` tidak
-- setengah bekerja — ia tidak pernah bisa bekerja sama sekali di database yang
-- benar-benar menjalankan berkas 001.
--
-- ── MASALAH KEDUA: `completed` TIDAK PERNAH DIPAKAI ─────────────────────────
--
-- `completed` adalah sisa dari masa ketika produk di sini masih berarti ACARA:
-- sebuah konser bisa selesai, dan sesudahnya ia bukan lagi barang dagangan.
-- Untuk marketplace barang, keadaan itu tidak punya arti apa pun.
--
-- Dan memang tidak pernah dipakai. Tak satu pun jalur tulis di seluruh kode
-- menuliskannya: tidak `create`, tidak `update`, tidak satu pun tombol admin.
-- Ia hanya hidup di daftar nilai yang diizinkan — sebuah pilihan yang ada
-- tetapi tak pernah bisa dipilih.
--
-- Status yang tak pernah ditulis bukan cuma mubazir. Ia memaksa setiap orang
-- yang membaca skema ini menebak-nebak kapan ia muncul dan apa bedanya dengan
-- `cancelled`, lalu menulis penanganan untuk keadaan yang tidak pernah terjadi.
--
-- Yang tersisa adalah tiga keadaan yang benar-benar ada:
--   * `active`    -- terbit, terlihat pembeli, bisa dibeli
--   * `edited`    -- ditahan, menunggu tinjauan admin
--   * `cancelled` -- tidak dijual lagi
--
-- ── BAGAIMANA BARIS LAMA DIPERLAKUKAN ───────────────────────────────────────
--
-- Aturannya satu, dan dipilih supaya TIDAK ADA yang berubah di mata pembeli:
-- apa yang terlihat hari ini tetap terlihat, apa yang tersembunyi tetap
-- tersembunyi.
--
-- Seluruh jalur publik menyaring `status = active`. Artinya baris ber-status
-- apa pun SELAIN `active` -- termasuk `completed`, termasuk NULL, termasuk nilai
-- nyasar yang entah dari mana -- saat ini TIDAK terlihat siapa pun. Semuanya
-- karena itu dipetakan ke `cancelled`, yang juga tidak terlihat.
--
-- Godaan untuk memetakannya ke `active` sengaja dihindari. Itu akan MENERBITKAN
-- produk yang selama ini sengaja tidak terbit -- diam-diam, tanpa ada yang
-- menekan tombol, pada saat deploy. Sebuah migrasi tidak boleh menjual apa pun
-- atas nama orang lain.
--
-- Idempotent — aman dijalankan berulang.
-- ============================================================================

-- Nama constraint-nya lahir saat tabelnya masih `events`, jadi PostgreSQL
-- menamainya `events_status_check`. Migrasi 023 me-rename TABEL-nya menjadi
-- `products`, dan rename tabel TIDAK ikut mengubah nama constraint. Karena itu
-- kedua nama yang mungkin dibuang di sini.
ALTER TABLE products DROP CONSTRAINT IF EXISTS events_status_check;
ALTER TABLE products DROP CONSTRAINT IF EXISTS products_status_check;

-- Semua yang bukan `active` dan bukan `edited` menjadi `cancelled`. Menangkap
-- `completed`, NULL, dan nilai nyasar sekaligus — tanpa perlu menyebut satu per
-- satu nilai yang mungkin pernah ada di sana.
UPDATE products
   SET status = 'cancelled'
 WHERE status IS NULL
    OR status NOT IN ('active', 'edited');

ALTER TABLE products ADD CONSTRAINT products_status_check
    CHECK (status IN ('active', 'edited', 'cancelled'));
