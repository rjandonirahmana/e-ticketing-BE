-- Migration: 020_cover_focus.sql — titik fokus foto cover event.
--
-- ── MASALAH ─────────────────────────────────────────────────────────────────
-- Foto cover TIDAK melar; ia terpotong di tempat yang salah. `object-fit: cover`
-- sudah dipakai di 85 tempat sehingga foto selalu memenuhi bingkainya, tapi
-- `object-position` hampir tak pernah diatur — bawaannya `center center`.
--
-- Satu foto cover ditampilkan di BEBERAPA rasio sekaligus: kartu 1:1 di grid,
-- hero lebar di halaman detail, thumbnail sempit di tiket. Dengan titik potong
-- yang selalu di tengah, foto potret kehilangan kepala orangnya di bingkai
-- lebar, dan poster yang judulnya di atas kehilangan judulnya.
--
-- ── KENAPA TITIK FOKUS, BUKAN MEMOTONG SAAT UNGGAH ──────────────────────────
-- Memotong permanen berarti memilih SATU rasio dan mengorbankan sisanya, atau
-- menyimpan beberapa salinan per foto. Menyimpan titik fokus membuat satu
-- berkas asli melayani semua bingkai.
--
-- ── KENAPA SATU KOLOM TEKS ──────────────────────────────────────────────────
-- Nilainya dipakai apa adanya sebagai `object-position` di CSS. Isinya
-- DIVALIDASI di sisi Rust sebelum ditulis (`models::events::normalisasi_fokus`):
-- hanya "X% Y%" dengan kedua angka 0–100. Tanpa validasi itu, kolom ini jadi
-- jalan menyuntikkan CSS.
--
-- Foto DETAIL tidak perlu migrasi: `events.detail_images` sudah JSONB, jadi
-- field `focus` cukup ditambahkan ke bentuk entrinya dengan serde default.
--
-- ── KENAPA SATU BLOK `DO`, BUKAN BEBERAPA PERINTAH ──────────────────────────
-- Isinya dulu tiga perintah terpisah, dan itu MENJEBAK di klien GUI: tombol
-- "Run Statement" hanya menjalankan perintah yang sedang disentuh kursor. Yang
-- terjadi: `ADD CONSTRAINT` jalan sendirian tanpa `ADD COLUMN` di atasnya, lalu
-- PostgreSQL menjawab — dengan benar — `column "cover_focus" does not exist`,
-- dan galat itu tampak seperti bug pada migrasinya.
--
-- Sebagai satu blok, ia mustahil dijalankan setengah: tak peduli tombol mana
-- yang ditekan, yang terkirim adalah seluruhnya. Sekaligus jadi satu transaksi.
--
-- Idempotent — aman dijalankan berulang.
--   psql "$DATABASE_URL" -f migration/020_cover_focus.sql
--   (di GUI: cukup tekan Run Statement — seluruh berkas ini satu perintah)

DO $$
BEGIN
    -- 1. Kolomnya. `IF NOT EXISTS` → menjalankan ulang tak melakukan apa-apa,
    --    termasuk bila percobaan sebelumnya sudah sempat menambahkannya.
    ALTER TABLE events
        ADD COLUMN IF NOT EXISTS cover_focus TEXT NOT NULL DEFAULT '50% 50%';

    -- 2. Baris lama yang entah bagaimana kosong/tak berbentuk dirapikan DULU —
    --    kalau tidak, CHECK di bawah akan menolak seluruh migrasi karena data
    --    yang sudah telanjur ada, dan itu menggagalkan pemasangan pagarnya
    --    justru pada database yang paling membutuhkannya.
    UPDATE events
       SET cover_focus = '50% 50%'
     WHERE cover_focus IS NULL
        OR cover_focus !~ '^[0-9]{1,3}% [0-9]{1,3}%$';

    -- 3. Pagar terakhir bila suatu saat ada jalur tulis yang melewatkan
    --    validasi Rust. Sengaja hanya memeriksa BENTUK, bukan rentang: rentang
    --    0–100 dijaga di Rust, dan CHECK yang terlalu rinci di sini hanya akan
    --    menolak baris pada saat yang paling tak berguna — ketika merchant
    --    menekan simpan.
    ALTER TABLE events DROP CONSTRAINT IF EXISTS chk_events_cover_focus;
    ALTER TABLE events ADD CONSTRAINT chk_events_cover_focus
        CHECK (cover_focus ~ '^[0-9]{1,3}% [0-9]{1,3}%$');

    RAISE NOTICE 'events.cover_focus siap.';
END $$;

-- =============================================================================
-- VERIFIKASI MANUAL (jalankan terpisah sesudahnya):
--   SELECT cover_focus, count(*) FROM events GROUP BY 1 ORDER BY 2 DESC;
--   -- Semua baris lama harus '50% 50%' (sama dengan perilaku sebelum ini).
-- =============================================================================
