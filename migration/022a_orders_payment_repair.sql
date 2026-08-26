-- ============================================================================
-- Migration: 022a_orders_payment_repair.sql
-- Kolom pembayaran pada tabel `orders`.
-- ============================================================================
--
-- ATURAN PENULISAN BERKAS INI (jangan dilanggar saat menyunting)
--   1. TIDAK ADA titik-koma di dalam komentar.
--   2. TIDAK ADA apostrof di dalam komentar. Pakai backtick.
--   3. TIDAK ADA blok dollar-quote.
--   4. TIDAK ADA foreign key menempel pada ALTER TABLE ADD COLUMN. FK ditulis
--      terpisah di bagian paling akhir.
--
-- Ketiganya bukan selera. Sebagian klien SQL memecah berkas menjadi pernyataan
-- dengan cara memotong pada setiap titik-koma, tanpa memahami komentar maupun
-- string. Pada klien seperti itu, satu titik-koma di dalam komentar akan
-- memotong pernyataan di sekitarnya menjadi dua kepingan rusak, dan pernyataan
-- itu HILANG tanpa pesan yang jelas. Itulah yang membuat sebagian kolom
-- terpasang dan sebagian tidak, lalu muncul error yang membingungkan seperti
--     ERROR 42703: column "payment_expired_at" does not exist   (indexcmds.c)
-- padahal ALTER TABLE untuk kolom itu memang ada di berkas.
--
-- Blok dollar-quote dilarang karena alasan yang sama, isinya penuh titik-koma.
--
-- Aturan 4 melindungi dari hal lain: `ADD COLUMN ... REFERENCES carts(id)` akan
-- GAGAL SELURUHNYA bila tabel `carts` belum ada, sehingga kolomnya pun tak
-- terpasang. Dipisah, kolomnya tetap masuk dan hanya batasannya yang tertunda.
--
-- TABEL BERISI DATA: AMAN
-- Tiga kolom NOT NULL di bawah semuanya punya DEFAULT 0, jadi baris lama terisi
-- sendiri. Sisanya NULL-able. Tidak ada kolom NOT NULL tanpa default, dan hanya
-- bentuk itulah yang benar-benar gagal pada tabel yang sudah ada isinya.
--
-- Idempotent. Aman dijalankan berkali-kali.
--   psql "$DATABASE_URL" -f migration/022a_orders_payment_repair.sql
-- ============================================================================


-- ── Kolom rincian harga ─────────────────────────────────────────────────────
-- Invarian: total_amount = subtotal_amount - discount_amount + payment_charge

ALTER TABLE orders ADD COLUMN IF NOT EXISTS subtotal_amount DECIMAL(12,2) NOT NULL DEFAULT 0;

ALTER TABLE orders ADD COLUMN IF NOT EXISTS discount_amount DECIMAL(12,2) NOT NULL DEFAULT 0;

ALTER TABLE orders ADD COLUMN IF NOT EXISTS promo_code VARCHAR(50);


-- ── Kolom kanal pembayaran ──────────────────────────────────────────────────
-- `payment_method` adalah kolom lama. Ia dipertahankan karena jalur REST dan
-- data lawas memakainya, dan kini selalu diisi nilai yang sama dengan
-- `payment_code`. Kolom ini ditegakkan ulang di sini karena `ORDER_COLS` di
-- `repository/order.rs` menyebutnya, sehingga bila ia hilang, SETIAP pembacaan
-- order gagal.

ALTER TABLE orders ADD COLUMN IF NOT EXISTS payment_method VARCHAR(50);

ALTER TABLE orders ADD COLUMN IF NOT EXISTS payment_vendor VARCHAR(50);

ALTER TABLE orders ADD COLUMN IF NOT EXISTS payment_code VARCHAR(50);

ALTER TABLE orders ADD COLUMN IF NOT EXISTS payment_charge DECIMAL(12,2) NOT NULL DEFAULT 0;

-- Batas waktu bayar dari sisi KANAL. Berbeda dari `expired_at` yang menahan
-- stok. Stok harus dilepas cepat supaya tidak ada tiket yang tersandera,
-- sedangkan tenggat kanal mengikuti kebiasaan kanalnya. Menyatukan keduanya
-- membuat salah satu dari dua janji itu pasti dilanggar.

ALTER TABLE orders ADD COLUMN IF NOT EXISTS payment_expired_at TIMESTAMPTZ;

-- Nomor Virtual Account atau referensi QRIS yang ditunjukkan ke pembeli.

ALTER TABLE orders ADD COLUMN IF NOT EXISTS payment_reference VARCHAR(100);

-- URL halaman bayar milik gateway, bila kanalnya memakai redirect.

ALTER TABLE orders ADD COLUMN IF NOT EXISTS link_pay TEXT;


-- ── Kunci idempotensi ───────────────────────────────────────────────────────
-- `repository/order.rs` sudah lama memakai kolom ini beserta unique index
-- parsialnya, tetapi keduanya tidak pernah tercatat di folder migration. Kasus
-- yang sama persis dengan rename tabel di 005a.

ALTER TABLE orders ADD COLUMN IF NOT EXISTS idempotency_key VARCHAR(64);


-- ── Keranjang asal ──────────────────────────────────────────────────────────
-- Audit: isi keranjang persis saat pesanan lahir. Kolomnya NULL-able, jadi
-- seluruh order lama yang sudah ada di tabel ini tetap sah tanpa perlu diisi
-- apa pun.

ALTER TABLE orders ADD COLUMN IF NOT EXISTS cart_id BYTEA;


-- ── Backfill ────────────────────────────────────────────────────────────────
-- Order lama tidak punya rincian, tetapi totalnya benar. Menyalin total ke
-- subtotal membuat invarian di atas berlaku untuk SEMUA baris, sehingga
-- laporan tidak perlu mengurus dua bentuk data.

UPDATE orders SET subtotal_amount = total_amount
 WHERE subtotal_amount = 0 AND total_amount <> 0;

UPDATE orders SET payment_code = payment_method
 WHERE payment_code IS NULL AND payment_method IS NOT NULL;


-- ── Index ───────────────────────────────────────────────────────────────────
-- Parsial: hanya order yang MENYEBUT kunci idempotensi yang dijaga unik. Order
-- tanpa kunci tetap boleh berulang.

CREATE UNIQUE INDEX IF NOT EXISTS uniq_orders_customer_idem
    ON orders (customer_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;

-- Halaman menunggu pembayaran mengurutkan order pending milik satu user
-- menurut batas waktu.

CREATE INDEX IF NOT EXISTS idx_orders_pending_expiry
    ON orders (customer_id, payment_expired_at)
    WHERE status = 'pending';


-- ── Foreign key, paling akhir ───────────────────────────────────────────────
-- Butuh tabel `carts` dari migrasi 022. Bila 022 belum dijalankan, HANYA dua
-- baris di bawah ini yang gagal -- sebelas kolom di atas tetap terpasang dan
-- aplikasi sudah bisa membaca order lagi. Jalankan 022 lalu ulangi berkas ini.
--
-- DROP lalu ADD dipakai sebagai ganti "ADD CONSTRAINT IF NOT EXISTS" yang tidak
-- ada di PostgreSQL. Efeknya idempoten.

ALTER TABLE orders DROP CONSTRAINT IF EXISTS fk_orders_cart;
ALTER TABLE orders ADD CONSTRAINT fk_orders_cart
    FOREIGN KEY (cart_id) REFERENCES carts(id) ON DELETE SET NULL;
