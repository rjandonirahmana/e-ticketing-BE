-- 037 — Hitungan belum-dibaca disimpan, bukan dihitung ulang.
--
-- Sebelum ini, daftar percakapan menghitung `COUNT(*)` pesan belum dibaca untuk
-- SETIAP percakapan, tiap kali inbox dibuka. Migrasi 034 mengindeksnya sehingga
-- tiap hitungan menjadi murah — tapi jumlah hitungannya tumbuh bersama jumlah
-- percakapan yang dipunyai orang itu. Pengguna dengan dua ratus percakapan
-- membayar dua ratus subkueri untuk satu halaman.
--
-- Skema ini selalu berdua (pembeli & merchant), jadi dua kolom sudah cukup —
-- tak perlu tabel peserta tersendiri. Hitungannya dinaikkan pada pernyataan
-- yang SAMA dengan yang menyimpan pesannya, jadi tak ada perjalanan tambahan
-- ke basis data sama sekali.
ALTER TABLE chats ADD COLUMN IF NOT EXISTS buyer_unread    INT NOT NULL DEFAULT 0;
ALTER TABLE chats ADD COLUMN IF NOT EXISTS merchant_unread INT NOT NULL DEFAULT 0;

-- Isi dari keadaan yang ada sekarang. Ini SEKALI JALAN dan bisa berat pada
-- tabel yang sudah besar — ia mengerjakan persis kueri yang selama ini
-- dijalankan tiap kali inbox dibuka, tapi untuk seluruh percakapan sekaligus.
-- Jalankan saat sepi.
UPDATE chats c
   SET buyer_unread = (
           SELECT COUNT(*) FROM chat_messages m
            WHERE m.chat_id = c.id
              AND m.sender_id <> c.buyer_id
              AND m.sent_at > COALESCE(c.buyer_read_at, c.created_at)
       ),
       merchant_unread = (
           SELECT COUNT(*) FROM chat_messages m
            WHERE m.chat_id = c.id
              AND m.sender_id <> c.merchant_id
              AND m.sent_at > COALESCE(c.merchant_read_at, c.created_at)
       );
