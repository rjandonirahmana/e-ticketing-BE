-- 036 — Membalas pesan tertentu.
--
-- `ON DELETE SET NULL`, bukan CASCADE: balasan atas pesan yang kemudian dihapus
-- (retensi 30 hari) harus TETAP ADA — yang hilang hanya kutipannya. CASCADE akan
-- menghapus balasannya juga, dan ikut menyeret balasan atas balasan itu, sampai
-- satu pesan lama bisa melenyapkan seluruh cabang percakapan.
--
-- Karena retensi memang membuang pesan lama tiap hari, keadaan "balasan tanpa
-- kutipan" bukan kasus tepi yang langka; ia pasti terjadi. Sisi render sudah
-- menanganinya dengan menampilkan balasannya saja.
ALTER TABLE chat_messages
    ADD COLUMN IF NOT EXISTS reply_to_id BYTEA NULL
        REFERENCES chat_messages(id) ON DELETE SET NULL;

-- Postgres TIDAK membuat indeks otomatis untuk kolom yang MERUJUK. Tanpa indeks
-- ini, setiap penghapusan pesan lama harus memindai seluruh tabel untuk mencari
-- balasan yang menunjuk padanya — dan retensi menghapus ribuan baris tiap hari,
-- jadi biayanya dibayar berulang kali setiap hari.
CREATE INDEX IF NOT EXISTS idx_chat_messages_reply_to
    ON chat_messages (reply_to_id)
    WHERE reply_to_id IS NOT NULL;
