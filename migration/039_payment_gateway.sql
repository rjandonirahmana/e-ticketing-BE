-- ═══════════════════════════════════════════════════════════════════════════
-- 039_payment_gateway.sql — transaksi gateway + jurnal webhook
-- ═══════════════════════════════════════════════════════════════════════════
--
-- Sampai migrasi ini, satu-satunya yang menandai order lunas adalah permintaan
-- dari BROWSER PEMBELI sendiri (`confirm_order_payment`). Tak ada verifikasi
-- apa pun di belakangnya. Dua tabel di sini memindahkan kewenangan itu ke
-- tempat yang benar: gateway yang memberi tahu server, dan server yang
-- memeriksa tanda tangannya.
--
-- ── KENAPA DUA TABEL, BUKAN KOLOM TAMBAHAN DI `orders` ────────────────────
-- Satu order bisa punya BANYAK percobaan bayar: VA kedaluwarsa lalu pembeli
-- memilih QRIS, kartu ditolak lalu diulang. Menyimpannya sebagai kolom di
-- `orders` berarti percobaan berikutnya menimpa jejak percobaan sebelumnya —
-- dan jejak itulah yang dibutuhkan saat ada sengketa "saya sudah bayar".
--
-- `payment_webhook_events` terpisah dari `payment_transactions` karena ia
-- menjawab pertanyaan yang berbeda: bukan "berapa status transaksi ini
-- sekarang", melainkan "pesan apa saja yang pernah kita terima, dan sudahkah
-- yang ini kita proses". Yang kedua adalah syarat IDEMPOTENSI, dan tanpanya
-- satu callback yang diulang gateway (Flip mengulang 5 kali, Midtrans
-- berkali-kali sampai dapat 200) akan menerbitkan tiket dua kali.

-- ── Transaksi gateway ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS payment_transactions (
    id              BYTEA PRIMARY KEY,
    order_id        BYTEA NOT NULL REFERENCES orders(id) ON DELETE CASCADE,

    -- 'midtrans' | 'stripe' | 'flip' | 'faspay'
    provider        VARCHAR(32)  NOT NULL,
    -- Kode kanal di sisi kita (`payment_methods.code`), disimpan apa adanya
    -- supaya laporan per-kanal tak bergantung pada istilah tiap gateway.
    payment_code    VARCHAR(50)  NOT NULL DEFAULT '',

    -- Identitas transaksi DI SISI GATEWAY. Inilah yang dikirim balik lewat
    -- webhook, jadi ia harus bisa dicari cepat — lihat index di bawah.
    provider_ref    VARCHAR(191) NOT NULL,

    -- 'pending' | 'paid' | 'failed' | 'expired' | 'refunded'
    -- Sengaja BUKAN istilah gateway: tiap gateway punya kosakata sendiri
    -- (Midtrans 'settlement', Faspay '2', Flip 'SUCCESSFUL'), dan
    -- menerjemahkannya di satu tempat berarti sisa aplikasi tak perlu tahu
    -- satu pun dari kosakata itu.
    status          VARCHAR(24)  NOT NULL DEFAULT 'pending',

    amount          DECIMAL(12,2) NOT NULL,
    currency        VARCHAR(8)   NOT NULL DEFAULT 'IDR',

    -- Nomor VA / tautan bayar / URL QRIS — apa pun yang harus ditunjukkan ke
    -- pembeli untuk menyelesaikan pembayarannya.
    pay_reference   VARCHAR(255),
    pay_url         TEXT,
    expires_at      TIMESTAMPTZ,

    -- Muatan terakhir dari gateway, apa adanya. Saat ada sengketa, yang
    -- menyelesaikannya adalah apa yang MEREKA kirim, bukan tafsiran kita.
    raw_response    JSONB,

    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    paid_at         TIMESTAMPTZ
);

-- Jalur panas webhook: cari transaksi dari (provider, provider_ref).
-- UNIQUE, bukan sekadar index: satu referensi gateway hanya boleh menunjuk
-- satu transaksi, dan basis data yang menegakkannya jauh lebih dapat
-- dipercaya daripada kode yang berjanji melakukannya.
CREATE UNIQUE INDEX IF NOT EXISTS idx_paytx_provider_ref
    ON payment_transactions (provider, provider_ref);

CREATE INDEX IF NOT EXISTS idx_paytx_order
    ON payment_transactions (order_id, created_at DESC);

-- Penyapu transaksi kedaluwarsa.
CREATE INDEX IF NOT EXISTS idx_paytx_pending_expiry
    ON payment_transactions (expires_at)
    WHERE status = 'pending';

-- ── Jurnal webhook — penegak idempotensi ──────────────────────────────────
CREATE TABLE IF NOT EXISTS payment_webhook_events (
    id              BYTEA PRIMARY KEY,
    provider        VARCHAR(32)  NOT NULL,

    -- Identitas peristiwa MENURUT GATEWAY.
    --
    -- Stripe memberi `evt_…` yang benar-benar unik per peristiwa. Midtrans,
    -- Faspay, dan Flip tidak memberi apa pun semacam itu — untuk mereka kunci
    -- ini disusun dari (referensi transaksi + status), yang cukup: callback
    -- ulang untuk status yang SAMA memang harus diabaikan, sedangkan
    -- perpindahan status berikutnya (pending → paid) menghasilkan kunci baru
    -- dan tetap diproses.
    event_key       VARCHAR(191) NOT NULL,

    -- Hasil pemeriksaan tanda tangan. Peristiwa yang GAGAL verifikasi tetap
    -- dicatat, dan itu disengaja: rentetan baris `FALSE` adalah satu-satunya
    -- tanda bahwa ada yang sedang mencoba memalsukan pembayaran.
    signature_ok    BOOLEAN      NOT NULL,

    payload         JSONB        NOT NULL,
    received_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    processed_at    TIMESTAMPTZ
);

-- Inilah penjaga idempotensinya. INSERT yang kalah balapan akan gagal di sini,
-- dan kegagalan itu yang memberi tahu pemroses bahwa peristiwanya sudah
-- ditangani — bukan SELECT-lalu-INSERT yang punya celah di antara keduanya.
CREATE UNIQUE INDEX IF NOT EXISTS idx_webhook_event_key
    ON payment_webhook_events (provider, event_key);

CREATE INDEX IF NOT EXISTS idx_webhook_recent
    ON payment_webhook_events (received_at DESC);
