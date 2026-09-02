-- ============================================================================
-- 029a_chat_dua_tabel_baru.sql  --  Bentuk chat DUA TABEL untuk database BARU.
-- ============================================================================
--
-- ── KENAPA BERKAS INI ADA ───────────────────────────────────────────────────
-- `029_chat_dua_tabel.sql` mengubah tiga tabel lama menjadi dua, dan ia sengaja
-- TIDAK otomatis: isinya membuang tabel beserta riwayat percakapan, dan migrasi
-- otomatis menjalankan dirinya sendiri pada deploy berikutnya -- tak ada momen
-- di mana kekeliruan masih bisa dihentikan.
--
-- Tetapi keputusan itu meninggalkan lubang: `034` dan sesudahnya menyebut
-- `chats` dan `chat_messages`, sedangkan pada database BARU kedua tabel itu
-- tak pernah lahir -- yang membuatnya lahir hanya `029`, yang tak pernah
-- dijalankan otomatis. Jadi replay dari nol selalu berhenti di `034`.
--
-- Berkas ini menutup lubang itu tanpa mengembalikan bahayanya: ia hanya
-- MEMBUAT, tak pernah membuang. Pada database yang sudah menjalankan `029`
-- secara manual, seluruh isinya jadi tanpa-efek.
--
-- ATURAN PENULISAN (sama dengan berkas lain)
--   1. TIDAK ADA titik-koma di dalam komentar.
--   2. TIDAK ADA apostrof di dalam komentar. Pakai backtick.
--   3. TIDAK ADA blok dollar-quote.

BEGIN;

-- Percakapan berdua: satu pembeli, satu toko. Tak ada tabel anggota terpisah --
-- baris ini SUDAH memuat kedua anggotanya, dan sumber kebenaran kedua untuk
-- hal yang sama selalu bisa berselisih dengan yang pertama.
CREATE TABLE IF NOT EXISTS chats (
    id               BYTEA       PRIMARY KEY,
    buyer_id         BYTEA       NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    merchant_id      BYTEA       NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Inilah yang membuat urutan inbox bermakna. NOT NULL dengan default:
    -- percakapan yang belum berpesan tetap punya tempat yang pasti di urutan.
    last_message_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    buyer_read_at    TIMESTAMPTZ,
    merchant_read_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id          BYTEA        PRIMARY KEY,
    chat_id     BYTEA        NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    sender_id   BYTEA        NOT NULL REFERENCES users(id),
    sender_name TEXT         NOT NULL DEFAULT '',
    msg_type    TEXT         NOT NULL DEFAULT 'text'
                             CHECK (msg_type IN ('text','image','shared_ticket','system')),
    content     TEXT         NOT NULL DEFAULT '',
    media_url   TEXT,
    ticket_card JSONB,
    is_system   BOOLEAN      NOT NULL DEFAULT FALSE,
    sent_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Satu percakapan per pasangan. Tanpa ini, dua permintaan yang tiba bersamaan
-- bisa melahirkan dua percakapan untuk pasangan yang sama, dan sesudahnya tak
-- ada cara memilih mana yang benar.
CREATE UNIQUE INDEX IF NOT EXISTS uq_chats_pasangan
    ON chats (buyer_id, merchant_id);

-- Inbox: `WHERE buyer_id = $1 ORDER BY last_message_at DESC`. Kolom urutnya
-- ikut masuk index supaya pembacaannya tak perlu menyortir apa pun.
CREATE INDEX IF NOT EXISTS idx_chats_pembeli
    ON chats (buyer_id, last_message_at DESC);
CREATE INDEX IF NOT EXISTS idx_chats_toko
    ON chats (merchant_id, last_message_at DESC);

-- Riwayat per percakapan, paginasi ber-cursor.
CREATE INDEX IF NOT EXISTS idx_chat_messages_riwayat
    ON chat_messages (chat_id, sent_at DESC, id DESC);

COMMIT;
