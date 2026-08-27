# migration-manual/

Berkas di folder ini **TIDAK** dijalankan otomatis.

`build.rs` hanya meng-embed `migration/*.sql` ke dalam binari, dan `AUTO_MIGRATE`
hanya menjalankan yang ter-embed itu. Apa pun di sini harus dijalankan tangan.

## Kenapa dipisah

Isinya menghapus data dan tak bisa dibatalkan. Migrasi otomatis mengeksekusi
dirinya sendiri pada deploy berikutnya — tak ada momen di mana kekeliruan masih
bisa dihentikan. Untuk perubahan yang membuang tabel dan riwayat percakapan,
momen itu justru yang paling dibutuhkan.

## Cara menjalankan

Periksa dulu apa yang akan hilang:

```sql
SELECT COUNT(*) FROM group_rooms WHERE event_id IS NOT NULL;

SELECT COUNT(*) FROM group_messages m
  JOIN group_rooms r ON r.id = m.room_id
 WHERE r.event_id IS NOT NULL;
```

Cadangkan, lalu jalankan:

```bash
pg_dump "$DATABASE_URL" -t group_rooms -t group_messages -t group_members \
  > cadangan-chat-$(date +%F).sql

psql "$DATABASE_URL" -f migration-manual/029_chat_dua_tabel.sql
```

## Urutan deploy — PENTING

Migrasi ini mengganti nama tabel, jadi kode lama berhenti bekerja begitu ia
selesai, dan kode baru berhenti bekerja bila dijalankan sebelum ia dijalankan.
Keduanya harus berpindah bersama:

1. Hentikan container.
2. Jalankan migrasi ini.
3. Jalankan image baru.

Menjalankan migrasi selagi container lama masih hidup akan membuat setiap
pembukaan halaman pesan gagal sampai image barunya naik.
