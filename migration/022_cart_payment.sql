-- ============================================================================
-- Migration: 022_cart_payment.sql
-- Keranjang, metode pembayaran, dan promo pindah dari browser ke database.
-- ============================================================================
--
-- ATURAN PENULISAN BERKAS INI (jangan dilanggar saat menyunting)
--   1. TIDAK ADA titik-koma di dalam komentar.
--   2. TIDAK ADA apostrof di dalam komentar. Pakai backtick.
--   3. TIDAK ADA blok dollar-quote.
--   4. TIDAK ADA foreign key di dalam CREATE TABLE. Semua FK ditambahkan
--      terpisah di BAGIAN 5, paling akhir.
--
-- Aturan 1-3 melindungi dari klien SQL yang memecah berkas dengan memotong
-- pada setiap titik-koma tanpa memahami komentar maupun string.
--
-- Aturan 4 melindungi dari hal yang lebih berbahaya. FOREIGN KEY di dalam
-- CREATE TABLE membuat tabelnya GAGAL LAHIR bila salah satu acuannya
-- bermasalah -- tipe kolom tak cocok, tabelnya bernama lain, atau belum ada.
-- Dan kegagalan itu muncul sebagai error di tempat LAIN:
--     ERROR: relation "promo_redemptions" does not exist
-- padahal CREATE TABLE-nya jelas ada di berkas ini. Dengan FK dipisah, tabelnya
-- tetap lahir, aplikasi tetap jalan, dan bila ada FK yang gagal, pesannya
-- menunjuk persis ke acuan yang bermasalah.
--
-- ── MASALAH YANG DISELESAIKAN ───────────────────────────────────────────────
-- Keranjang PULSE selama ini hanya hidup di `localStorage` browser
-- (`web/app/providers.rs`, kunci "pulse_cart"). Konsekuensinya:
--   • Keranjang hilang saat ganti perangkat, ganti browser, atau bersihkan situs.
--   • Harga dan nama tiket di halaman checkout berasal dari klien -- server tak
--     punya catatan apa pun tentang apa yang dilihat pembeli.
--   • Tak ada tempat menyimpan kode promo yang sudah divalidasi.
--   • Metode pembayaran di-hardcode sebagai konstanta Rust, dan biaya adminnya
--     dua angka gelondongan yang sama untuk semua kanal.
--
-- ── URUTAN ──────────────────────────────────────────────────────────────────
--   BAGIAN 1  tabel, tanpa satu pun foreign key
--   BAGIAN 2  index
--   BAGIAN 3  data awal metode pembayaran
--   BAGIAN 4  data awal promo
--   BAGIAN 5  foreign key
--
-- Kolom pembayaran pada tabel `orders` ada di berkas terpisah,
-- `022a_orders_payment_repair.sql`. Jalankan berkas ini lebih dulu.
--
-- Idempotent. Aman dijalankan berkali-kali.
--   psql "$DATABASE_URL" -f migration/022_cart_payment.sql
-- ============================================================================


-- ════════════════════════════════════════════════════════════════════════════
-- BAGIAN 1 -- TABEL
-- ════════════════════════════════════════════════════════════════════════════

-- Satu keranjang aktif per user. Barisnya tidak pernah dihapus: setelah jadi
-- order ia ditutup lewat `deleted_at`, dan `orders.cart_id` menunjuk ke sana
-- sebagai bukti isi keranjang saat pesanan lahir.

CREATE TABLE IF NOT EXISTS carts (
    id              BYTEA        PRIMARY KEY,
    user_id         BYTEA        NOT NULL,

    promo_code      VARCHAR(50),
    discount_amount DECIMAL(12,2) NOT NULL DEFAULT 0 CHECK (discount_amount >= 0),

    payment_code    VARCHAR(50),
    position        VARCHAR(30)  NOT NULL DEFAULT 'cart',

    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at      TIMESTAMPTZ
);


-- Baris keranjang. Selain HARGA, tak ada yang disalin ke sini -- nama event,
-- nama varian, venue, dan cover di-JOIN hidup dari `events` dan `event_variants`
-- setiap kali keranjang dibaca, jadi mustahil basi.
--
-- Harga adalah satu-satunya pengecualian, justru karena ia BOLEH berbeda.
-- Dibanding harga berlaku, kolom ini menjawab "harga berubah sejak Anda
-- menambahkan" -- pertanyaan yang lenyap kalau harganya ikut di-JOIN.

CREATE TABLE IF NOT EXISTS cart_items (
    id                BYTEA        PRIMARY KEY,
    cart_id           BYTEA        NOT NULL,
    ticket_variant_id BYTEA        NOT NULL,

    quantity          INTEGER      NOT NULL DEFAULT 1
                                   CHECK (quantity > 0 AND quantity <= 100),
    unit_price        DECIMAL(12,2) NOT NULL DEFAULT 0 CHECK (unit_price >= 0),

    created_at        TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Buang salinan tampilan bila tabelnya sempat lahir dengan bentuk lama.
-- `CREATE TABLE IF NOT EXISTS` tidak menyentuh tabel yang sudah ada, jadi tanpa
-- baris-baris ini kolom lamanya akan menetap.

ALTER TABLE cart_items DROP COLUMN IF EXISTS event_id;
ALTER TABLE cart_items DROP COLUMN IF EXISTS event_name;
ALTER TABLE cart_items DROP COLUMN IF EXISTS variant_name;
ALTER TABLE cart_items DROP COLUMN IF EXISTS venue;
ALTER TABLE cart_items DROP COLUMN IF EXISTS cover_url;
ALTER TABLE cart_items DROP COLUMN IF EXISTS event_date;
ALTER TABLE cart_items DROP COLUMN IF EXISTS merchant_id;
ALTER TABLE cart_items DROP COLUMN IF EXISTS merchant_name;


-- Kanal pembayaran sebagai DATA, bukan kode. Menambah kanal atau mengubah biaya
-- admin cukup satu INSERT atau UPDATE, tanpa deploy ulang.

CREATE TABLE IF NOT EXISTS payment_methods (
    code        VARCHAR(50)  PRIMARY KEY,
    name        VARCHAR(100) NOT NULL DEFAULT '',
    vendor      VARCHAR(50)  NOT NULL DEFAULT '',
    category    VARCHAR(20)  NOT NULL DEFAULT 'other'
                             CHECK (category IN ('qris','ewallet','va','cc','cash','other')),
    image_url   TEXT         NOT NULL DEFAULT '',
    description VARCHAR(255) NOT NULL DEFAULT '',

    -- Biaya admin: tetap ditambah persentase. Keduanya boleh nol. Dipisah
    -- karena kanal nyata memang memungut dua-duanya, misalnya VA Rp4.000
    -- sementara kartu kredit 2,9 persen.
    charge          INTEGER      NOT NULL DEFAULT 0 CHECK (charge >= 0),
    charge_percent  NUMERIC(5,2) NOT NULL DEFAULT 0 CHECK (charge_percent >= 0),

    min_amount  BIGINT       NOT NULL DEFAULT 0,
    -- 0 berarti tanpa batas atas.
    max_amount  BIGINT       NOT NULL DEFAULT 0,

    -- Boleh dipakai bersamaan dengan kode promo.
    allow_promo BOOLEAN      NOT NULL DEFAULT TRUE,
    -- Lunas seketika tanpa gateway, misalnya tunai di lokasi.
    is_instant  BOOLEAN      NOT NULL DEFAULT FALSE,

    va_prefix   VARCHAR(10)  NOT NULL DEFAULT '',
    instruction TEXT         NOT NULL DEFAULT '',

    sort_order  INTEGER      NOT NULL DEFAULT 0,
    is_active   BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at  TIMESTAMPTZ
);


CREATE TABLE IF NOT EXISTS promos (
    id              BIGSERIAL    PRIMARY KEY,
    code            VARCHAR(50)  NOT NULL,
    name            VARCHAR(150) NOT NULL DEFAULT '',

    -- `fixed` memotong rupiah, `percent` memotong persen dari subtotal dan
    -- dibatasi `max_discount`.
    discount_type   VARCHAR(10)  NOT NULL DEFAULT 'fixed'
                                 CHECK (discount_type IN ('fixed','percent')),
    amount          DECIMAL(12,2) NOT NULL DEFAULT 0 CHECK (amount >= 0),
    -- 0 berarti tanpa plafon. Hanya bermakna untuk tipe `percent`.
    max_discount    DECIMAL(12,2) NOT NULL DEFAULT 0 CHECK (max_discount >= 0),

    min_cart_amount DECIMAL(12,2) NOT NULL DEFAULT 0,
    -- 0 berarti tanpa batas atas.
    max_cart_amount DECIMAL(12,2) NOT NULL DEFAULT 0,
    min_qty         INTEGER      NOT NULL DEFAULT 0,
    max_qty         INTEGER      NOT NULL DEFAULT 0,

    -- Kuota global. 0 berarti tanpa batas.
    quota_total     INTEGER      NOT NULL DEFAULT 0 CHECK (quota_total >= 0),
    quota_used      INTEGER      NOT NULL DEFAULT 0 CHECK (quota_used >= 0),
    -- Berapa kali SATU user boleh memakainya. 0 berarti tanpa batas.
    per_user_limit  INTEGER      NOT NULL DEFAULT 1 CHECK (per_user_limit >= 0),

    premium_only    BOOLEAN      NOT NULL DEFAULT FALSE,
    -- NULL berarti berlaku untuk semua kanal. Bila diisi, hanya kanal di daftar.
    payment_codes   TEXT[],

    starts_at       TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    ends_at         TIMESTAMPTZ,
    is_active       BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    deleted_at      TIMESTAMPTZ
);


-- Siapa memakai promo apa di order mana. Penegak `per_user_limit`.

CREATE TABLE IF NOT EXISTS promo_redemptions (
    id              BIGSERIAL   PRIMARY KEY,
    promo_id        BIGINT      NOT NULL,
    user_id         BYTEA       NOT NULL,
    order_id        BYTEA       NOT NULL,
    discount_amount DECIMAL(12,2) NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);


-- ════════════════════════════════════════════════════════════════════════════
-- BAGIAN 2 -- INDEX
-- ════════════════════════════════════════════════════════════════════════════

-- SATU keranjang aktif per user. Ini yang membuat "ambil keranjang saya" cukup
-- satu baris tanpa ORDER BY, dan mencegah dua tab membuat keranjang kembar.

CREATE UNIQUE INDEX IF NOT EXISTS uniq_carts_user_active
    ON carts (user_id)
    WHERE deleted_at IS NULL;

-- Satu varian sama dengan satu baris. Menambah varian yang sama menaikkan
-- `quantity`, bukan membuat baris kedua. Inilah yang membuat UPSERT di
-- repository bisa jadi satu perjalanan ke database.

CREATE UNIQUE INDEX IF NOT EXISTS uniq_cart_items_variant
    ON cart_items (cart_id, ticket_variant_id);

CREATE INDEX IF NOT EXISTS idx_cart_items_cart
    ON cart_items (cart_id, created_at);

CREATE INDEX IF NOT EXISTS idx_cart_items_variant
    ON cart_items (ticket_variant_id);

CREATE INDEX IF NOT EXISTS idx_payment_methods_active
    ON payment_methods (sort_order, code)
    WHERE is_active AND deleted_at IS NULL;

-- Kode promo dibandingkan dalam huruf besar supaya "pulse10" dan "PULSE10"
-- adalah promo yang SAMA. Kalau tidak, satu kuota bisa dipakai dua kali lewat
-- perbedaan huruf belaka.

CREATE UNIQUE INDEX IF NOT EXISTS uniq_promos_code
    ON promos (UPPER(code))
    WHERE deleted_at IS NULL;

-- Satu order memakai paling banyak satu promo.

CREATE UNIQUE INDEX IF NOT EXISTS uniq_promo_redemption_order
    ON promo_redemptions (order_id);

CREATE INDEX IF NOT EXISTS idx_promo_redemptions_user
    ON promo_redemptions (promo_id, user_id);


-- ════════════════════════════════════════════════════════════════════════════
-- BAGIAN 3 -- DATA AWAL METODE PEMBAYARAN
-- ════════════════════════════════════════════════════════════════════════════
-- ON CONFLICT DO NOTHING: menjalankan ulang migrasi tidak menimpa penyetelan
-- biaya yang mungkin sudah diubah operator di produksi.

INSERT INTO payment_methods
    (code, name, vendor, category, description, charge, charge_percent,
     allow_promo, is_instant, va_prefix, sort_order, instruction)
VALUES
    ('qris',        'QRIS',            'internal', 'qris',    'Scan sekali, semua e-wallet dan mobile banking', 0,    0.70, TRUE,  FALSE, '',      10,
     'Buka aplikasi e-wallet atau mobile banking, pilih Scan QRIS, lalu pindai kode yang tampil.'),
    ('gopay',       'GoPay',           'midtrans', 'ewallet', 'Bayar dari saldo GoPay',                        2000, 0,    TRUE,  FALSE, '',      20,
     'Buka aplikasi Gojek, konfirmasi pembayaran pada notifikasi yang masuk.'),
    ('ovo',         'OVO',             'midtrans', 'ewallet', 'Bayar dari saldo OVO',                          2000, 0,    TRUE,  FALSE, '',      21,
     'Buka aplikasi OVO, konfirmasi permintaan pembayaran yang masuk.'),
    ('dana',        'DANA',            'midtrans', 'ewallet', 'Bayar dari saldo DANA',                         2000, 0,    TRUE,  FALSE, '',      22,
     'Buka aplikasi DANA, konfirmasi permintaan pembayaran yang masuk.'),
    ('shopeepay',   'ShopeePay',       'midtrans', 'ewallet', 'Bayar dari saldo ShopeePay',                    2000, 0,    TRUE,  FALSE, '',      23,
     'Buka aplikasi Shopee, konfirmasi permintaan pembayaran yang masuk.'),
    ('va_bca',      'BCA Virtual Account',     'midtrans', 'va', 'Transfer via ATM, m-BCA, atau KlikBCA',   4000, 0, TRUE, FALSE, '39001', 30,
     'Transfer ke nomor Virtual Account di atas melalui ATM atau m-Banking BCA. Pembayaran terverifikasi otomatis.'),
    ('va_mandiri',  'Mandiri Virtual Account', 'midtrans', 'va', 'Transfer via Livin by Mandiri',           4000, 0, TRUE, FALSE, '88801', 31,
     'Buka Livin by Mandiri, pilih Bayar lalu Multipayment, masukkan nomor Virtual Account di atas.'),
    ('va_bni',      'BNI Virtual Account',     'midtrans', 'va', 'Transfer via ATM atau BNI Mobile',        4000, 0, TRUE, FALSE, '98801', 32,
     'Transfer ke nomor Virtual Account di atas melalui ATM atau BNI Mobile Banking.'),
    ('va_bri',      'BRI Virtual Account',     'midtrans', 'va', 'Transfer via BRImo atau ATM BRI',         4000, 0, TRUE, FALSE, '26215', 33,
     'Transfer ke nomor Virtual Account di atas melalui BRImo atau ATM BRI.'),
    ('credit_card', 'Kartu Kredit atau Debit', 'midtrans', 'cc', 'Visa, Mastercard, JCB',                   0, 2.90, FALSE, FALSE, '',      40,
     'Masukkan data kartu pada halaman pembayaran, lalu selesaikan verifikasi 3D Secure dari bank Anda.'),
    ('cash',        'Bayar di Lokasi',         'internal', 'cash','Bayar tunai saat penukaran tiket',       0, 0,    FALSE, TRUE,  '',      90,
     'Tunjukkan kode pesanan di loket lokasi acara, lalu bayar tunai kepada petugas.')
ON CONFLICT (code) DO NOTHING;


-- ════════════════════════════════════════════════════════════════════════════
-- BAGIAN 4 -- DATA AWAL PROMO
-- ════════════════════════════════════════════════════════════════════════════

INSERT INTO promos (code, name, discount_type, amount, max_discount,
                    min_cart_amount, quota_total, per_user_limit, ends_at)
VALUES
    ('PULSE10',  'Diskon 10 persen pengguna baru', 'percent', 10, 50000, 100000, 1000, 1, NOW() + INTERVAL '90 days'),
    ('HEMAT25K', 'Potongan Rp25.000',              'fixed',   25000, 0,  150000, 500,  2, NOW() + INTERVAL '60 days')
ON CONFLICT DO NOTHING;


-- ════════════════════════════════════════════════════════════════════════════
-- BAGIAN 5 -- FOREIGN KEY
-- ════════════════════════════════════════════════════════════════════════════
-- Ditambahkan terpisah, satu per satu, PALING AKHIR. Semua tabel di atas sudah
-- ada sebelum baris pertama di bagian ini dijalankan, jadi kegagalan FK apa pun
-- tidak lagi menghapus tabel dari muka bumi -- ia hanya membuat satu batasan
-- tidak terpasang, dengan pesan yang menunjuk persis ke acuannya.
--
-- DROP lalu ADD dipakai sebagai ganti "ADD CONSTRAINT IF NOT EXISTS" yang tidak
-- ada di PostgreSQL. Efeknya idempoten.
--
-- Perhatikan perbedaan aturan hapus, dan itu disengaja:
--   cart_items  ke event_variants  CASCADE  -- varian dihapus, baris keranjang
--                                              orang harus ikut lenyap
--   order_items ke event_variants  RESTRICT -- varian yang pernah TERJUAL tidak
--                                              boleh bisa dihapus
-- Kolom yang sama, aturan berlawanan, dan keduanya benar untuk perannya.
-- Itulah sebabnya `order_items` tidak bisa digantikan `cart_items`.

ALTER TABLE carts DROP CONSTRAINT IF EXISTS fk_carts_user;
ALTER TABLE carts ADD CONSTRAINT fk_carts_user
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE cart_items DROP CONSTRAINT IF EXISTS fk_cart_items_cart;
ALTER TABLE cart_items ADD CONSTRAINT fk_cart_items_cart
    FOREIGN KEY (cart_id) REFERENCES carts(id) ON DELETE CASCADE;

ALTER TABLE cart_items DROP CONSTRAINT IF EXISTS fk_cart_items_variant;
ALTER TABLE cart_items ADD CONSTRAINT fk_cart_items_variant
    FOREIGN KEY (ticket_variant_id) REFERENCES event_variants(id) ON DELETE CASCADE;

ALTER TABLE promo_redemptions DROP CONSTRAINT IF EXISTS fk_promo_red_promo;
ALTER TABLE promo_redemptions ADD CONSTRAINT fk_promo_red_promo
    FOREIGN KEY (promo_id) REFERENCES promos(id) ON DELETE CASCADE;

ALTER TABLE promo_redemptions DROP CONSTRAINT IF EXISTS fk_promo_red_user;
ALTER TABLE promo_redemptions ADD CONSTRAINT fk_promo_red_user
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

ALTER TABLE promo_redemptions DROP CONSTRAINT IF EXISTS fk_promo_red_order;
ALTER TABLE promo_redemptions ADD CONSTRAINT fk_promo_red_order
    FOREIGN KEY (order_id) REFERENCES orders(id) ON DELETE CASCADE;
