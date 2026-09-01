-- ============================================================================
-- Migration: 034_chat_unread.sql — hitungan pesan belum dibaca per percakapan
-- ============================================================================
-- Kolom `chats.buyer_read_at` dan `chats.merchant_read_at` sudah ada sejak
-- percakapan dibuat, tetapi TAK PERNAH dibaca maupun ditulis satu baris kode
-- pun. Akibatnya daftar percakapan tak bisa membedakan yang sudah dibaca dari
-- yang belum, dan pesan baru hanya terlihat bila seseorang kebetulan membuka
-- percakapannya.
--
-- Migrasi ini tidak menambah kolom — hanya indeks yang membuat hitungannya
-- murah. Tanpa indeks ini, satu subkueri COUNT per baris daftar berarti
-- pemindaian `chat_messages` sekali untuk SETIAP percakapan yang dipunyai
-- seseorang; dengan indeks, ia menjadi pembacaan rentang.
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_chat_messages_chat_waktu
    ON chat_messages (chat_id, sent_at DESC);

-- Pengirim ikut di indeks karena hitungannya SELALU mengecualikan pesan
-- sendiri: pesan yang baru saja kita kirim bukan pesan yang belum kita baca.
CREATE INDEX IF NOT EXISTS idx_chat_messages_chat_pengirim
    ON chat_messages (chat_id, sender_id);
