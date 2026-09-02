-- 038 — Luruskan constraint `reviews.order_id`.
--
-- Migrasi 013 menulisnya:
--
--     order_id BYTEA NOT NULL REFERENCES orders(id) ON DELETE SET NULL
--
-- Dua bagian itu saling menyangkal. Saat sebuah order dihapus, Postgres mencoba
-- menulis NULL ke kolom yang menolak NULL -- penghapusannya gagal dengan pesan
-- yang menunjuk ke constraint, bukan ke sebabnya. Jadi perilaku yang berlaku
-- selama ini SEBENARNYA sudah `RESTRICT`, hanya saja lewat kegagalan alih-alih
-- lewat pernyataan.
--
-- Yang diubah di sini cuma pernyataannya. `RESTRICT` memang yang benar: ulasan
-- adalah bukti pembelian, dan ulasan yang kehilangan ordernya kehilangan pula
-- alasan ia boleh dipercaya. Menolak menghapus order yang punya ulasan lebih
-- jujur daripada menyimpan ulasan yatim.
--
-- ATURAN PENULISAN: tanpa titik-koma di komentar, tanpa apostrof, tanpa
-- dollar-quote.

BEGIN;

ALTER TABLE reviews DROP CONSTRAINT IF EXISTS reviews_order_id_fkey;

ALTER TABLE reviews
    ADD CONSTRAINT reviews_order_id_fkey
    FOREIGN KEY (order_id) REFERENCES orders(id) ON DELETE RESTRICT;

COMMIT;
