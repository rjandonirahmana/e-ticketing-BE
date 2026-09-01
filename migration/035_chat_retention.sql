-- 035 — Retensi pesan 30 hari.
--
-- Pemindaian retensi menyaring `sent_at < NOW() - INTERVAL '30 days'` LINTAS
-- SELURUH room. Indeks yang sudah ada semuanya diawali `chat_id`
-- (`idx_chat_messages_room_sent`), jadi tak satu pun bisa dipakai untuk
-- saringan yang tidak menyebut percakapan — pemindaiannya jatuh ke seq scan atas
-- seluruh tabel, tiap hari, selamanya.
--
-- Indeks ini hanya untuk pekerjaan itu. Ia tidak menggantikan indeks per-room:
-- keduanya menjawab pertanyaan yang berbeda, dan yang ini urutannya terbalik
-- (ASC — yang dicari justru yang PALING TUA).
CREATE INDEX IF NOT EXISTS idx_chat_messages_retensi
    ON chat_messages (sent_at ASC);

-- Pemindaian medianya lebih sempit lagi: hanya baris yang PUNYA berkas di
-- RustFS yang perlu disinggahi sebelum dihapus. Indeks parsial membuat
-- ukurannya sebanding dengan jumlah pesan bergambar, bukan jumlah pesan —
-- pada percakapan yang isinya hampir seluruhnya teks, selisihnya besar.
CREATE INDEX IF NOT EXISTS idx_chat_messages_retensi_media
    ON chat_messages (sent_at ASC)
    WHERE media_url IS NOT NULL;
