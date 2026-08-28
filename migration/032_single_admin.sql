-- ============================================================================
-- Migration: 032_single_admin.sql
-- Hanya boleh ada SATU admin. Ditegakkan database, bukan sekadar disepakati.
-- ============================================================================
--
-- ATURAN PENULISAN (sama dengan 022, 023, 024, 026, 027, 030, 031)
--   1. TIDAK ADA titik-koma di dalam komentar.
--   2. TIDAK ADA apostrof di dalam komentar. Pakai backtick.
--   3. TIDAK ADA blok dollar-quote.
--
-- ── KENAPA DI DATABASE, PADAHAL KODE SUDAH MENJAGA ──────────────────────────
--
-- Di sisi aplikasi peran hanya bisa ditulis di SATU tempat: INSERT saat
-- registrasi (`repository/user.rs`). `UpdateProfileRequest` tidak punya field
-- `role` sama sekali, jadi tak ada jalur promosi, dan `initiate_register`
-- sudah menolak permintaan mendaftar sebagai admin.
--
-- Artinya admin kedua tidak mungkin lahir lewat aplikasi. Ia hanya bisa lahir
-- lewat SQL langsung — psql, GUI database, skrip pemulihan, atau seed yang
-- disalin dari lingkungan lain. Dan justru di situlah penjagaan kode tidak
-- berlaku sama sekali.
--
-- Index inilah satu-satunya bentuk penjagaan yang ikut hadir di jalur itu.
-- `UPDATE users SET role = admin` yang kedua akan ditolak PostgreSQL, apa pun
-- yang menjalankannya.
--
-- ── ⚠️  YANG DILAKUKAN TERHADAP ADMIN YANG SUDAH TERLANJUR BANYAK ───────────
--
-- Index unik tidak bisa dipasang selagi barisnya masih melanggar. Ada dua
-- pilihan, dan keduanya punya harga:
--
--   * Gagalkan migrasinya. Karena migrasi berjalan saat start dan kegagalannya
--     menggagalkan start, ini berarti situs MATI sampai ada manusia yang
--     memperbaiki datanya. Terlalu mahal untuk aturan tata kelola.
--
--   * Turunkan yang berlebih. Dipilih yang ini — tetapi ia MENGUBAH SIAPA YANG
--     PUNYA AKSES, jadi aturannya dibuat sederhana dan bisa ditebak:
--     yang DIPERTAHANKAN adalah admin yang PALING TUA (`created_at` terkecil,
--     `id` sebagai pemutus seri supaya hasilnya sama di setiap percobaan).
--     Sisanya menjadi `customer`.
--
-- PERIKSA DULU SEBELUM DEPLOY. Kalau akun admin yang Anda pakai sehari-hari
-- BUKAN yang paling tua, Anda akan kehilangan aksesnya:
--
--     SELECT encode(id, `hex`), email, name, role, created_at
--       FROM users WHERE role = `admin` ORDER BY created_at ASC, id ASC
--
-- Baris PERTAMA dari hasil itulah yang akan bertahan. Bila bukan yang Anda
-- inginkan, jadikan akun itu yang tertua lebih dulu, atau turunkan sendiri
-- yang lain sebelum menjalankan migrasi ini.
--
-- Idempotent — aman dijalankan berulang. Sesudah index terpasang, UPDATE di
-- bawah tak lagi menemukan baris untuk diubah.
-- ============================================================================

-- Turunkan setiap admin KECUALI yang paling tua.
--
-- Sub-query-nya sengaja memakai `ORDER BY ... LIMIT 1`, bukan `MIN(created_at)`:
-- dua admin yang dibuat pada milidetik yang sama akan membuat `MIN` cocok
-- dengan KEDUANYA, dan keduanya lolos — lalu index di bawah gagal dan seluruh
-- migrasi batal. `LIMIT 1` selalu menyisakan tepat satu baris.
UPDATE users
   SET role = 'customer',
       updated_at = NOW()
 WHERE role = 'admin'
   AND id <> (
        SELECT id FROM users
         WHERE role = 'admin'
         ORDER BY created_at ASC, id ASC
         LIMIT 1
   );

-- Partial unique index: barisnya hanya yang ber-peran `admin`, dan di dalam
-- himpunan itu `role` wajib unik. Karena seluruh isinya bernilai sama, `unik`
-- di sana berarti `paling banyak satu baris`.
--
-- Pengguna non-admin tidak ikut ter-index sama sekali, jadi tak ada biaya tulis
-- tambahan pada pendaftaran biasa.
CREATE UNIQUE INDEX IF NOT EXISTS uniq_users_single_admin
    ON users (role)
 WHERE role = 'admin';
