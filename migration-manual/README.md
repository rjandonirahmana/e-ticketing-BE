# migration-manual/

Berkas di folder ini **TIDAK** dijalankan otomatis.

`build.rs` hanya meng-embed `migration/*.sql` ke dalam binari, dan `AUTO_MIGRATE`
hanya menjalankan yang ter-embed itu. Apa pun di sini harus dijalankan tangan.

## Nomornya menyatakan URUTAN, bukan tanggal

Nomor di sini dibaca dengan aturan yang sama seperti `migration/`: **berkas
bernomor N dijalankan setelah SELURUH berkas `migration/` yang bernomor ≤ N.**

Karena itu tiga berkas di bawah bernomor 040+ — bukan karena baru ditulis,
melainkan karena satu-satunya saat yang benar untuk menjalankannya adalah
**setelah seluruh migrasi otomatis selesai**. Dua seed sebelumnya bernomor 007
dan 010, dan nomor itu BOHONG: keduanya menyebut tabel `events` dan
`event_variants`, yang sudah berganti nama menjadi `products` dan
`product_variants` oleh `migration/023_products_rename.sql`. Dijalankan pada
database hari ini, keduanya berhenti di `relation "events" does not exist`.
Isinya kini mengikuti nama yang berlaku, dan nomornya mengikuti isinya.

`029_chat_dua_tabel.sql` sengaja TETAP di nomor 029: ia bukan alat yang dipanggil
kapan saja, melainkan satu langkah sejarah yang menempati posisi itu — pasangan
lama dari `migration/029a_chat_dua_tabel_baru.sql`.

## Isi folder ini

| Berkas | Kenapa manual |
|---|---|
| `029_chat_dua_tabel.sql` | Membuang tabel dan riwayat percakapan — tak bisa dibatalkan |
| `040_products_slug_unique.sql` | `CREATE INDEX CONCURRENTLY` tak boleh berada di dalam transaksi, dan bisa GAGAL bila ada slug kembar |
| `041_seed_bulk.sql` | 1 juta produk + 3 juta varian. Data UJI |
| `042_seed_stories.sql` | 500 ribu story. Data UJI |
| `043_seed_fake_users.sql` | 2 juta user palsu, password sama (`123456789`). Data UJI |
| `044_seed_fake_orders.sql` | 500 ribu order palsu (paid) + cart/tiket. Data UJI — butuh 041 & 043 lebih dulu |

Dua seed itu dulu berada di `migration/` sehingga IKUT ter-embed — artinya
deployment pertama ke database kosong mana pun, termasuk produksi baru, akan
menanaminya dengan sejuta produk palsu. Baseline hanya melindungi database yang
sudah berisi, bukan yang masih kosong.

Bentuk chat dua-tabel untuk database BARU kini disediakan
`migration/029a_chat_dua_tabel_baru.sql`, yang hanya MEMBUAT dan tak pernah
membuang — jadi replay dari nol berjalan tanpa perlu menyentuh berkas di sini.

## Kenapa dipisah

Alasannya berbeda per berkas, dan hanya satu di antaranya soal data hilang:

- **029** menghapus data dan tak bisa dibatalkan. Migrasi otomatis mengeksekusi
  dirinya sendiri pada deploy berikutnya — tak ada momen di mana kekeliruan masih
  bisa dihentikan. Untuk perubahan yang membuang tabel dan riwayat percakapan,
  momen itu justru yang paling dibutuhkan.
- **040** tak BISA dijalankan penjalan otomatis: tiap berkas di sana dibungkus
  transaksi (`config/migrate.rs`), dan `CREATE INDEX CONCURRENTLY` menolak hidup
  di dalam transaksi. Selain itu ia gagal bila ada slug kembar, dan memilih
  produk mana yang berhak atas sebuah slug adalah keputusan manusia.
- **041/042/043/044** adalah data UJI. Database kosong menjalankan SEMUA
  berkas `migration/` dari nol — kalau seed ikut di sana, produksi baru lahir
  berisi sejuta produk palsu. `043` juga punya alasan tambahan: SATU password
  (`123456789`) dipakai ulang untuk jutaan akun — lubang keamanan raksasa kalau
  sampai berjalan di database yang sungguhan dipakai orang.

## Cara menjalankan

### 040 — keunikan slug

Periksa dulu apakah ada yang kembar:

```sql
SELECT slug, COUNT(*) FROM products WHERE slug <> ''
 GROUP BY slug HAVING COUNT(*) > 1;
```

Kosong → pasang indexnya. **Tanpa `-1`/`--single-transaction`**, karena
`CONCURRENTLY` tak boleh berada di dalam transaksi:

```bash
psql "$DATABASE_URL" -f migration-manual/040_products_slug_unique.sql
```

### 041 / 042 / 043 / 044 — seed data uji

Jangan pernah di produksi. Ubah dulu angka `generate_series(...)` (041, 043,
044) atau `\set n_users` / `\set n_stories` (042) sesuai skala yang ingin
diukur:

```bash
psql "$DATABASE_URL" -f migration-manual/041_seed_bulk.sql
psql "$DATABASE_URL" -f migration-manual/042_seed_stories.sql
psql "$DATABASE_URL" -f migration-manual/043_seed_fake_users.sql
psql "$DATABASE_URL" -f migration-manual/044_seed_fake_orders.sql   # butuh 041 & 043 dulu
```

Semua idempoten (`ON CONFLICT DO NOTHING`) dan punya blok rollback ter-comment
di bagian bawah berkas. `044` merangkai empat INSERT (cart → order → cart_item
→ ticket) dalam satu statement lewat `RETURNING`, jadi urutannya terjamin
walau dijalankan sebagai satu file `-f`.

### 029 — chat dua tabel (database LAMA saja)

Database yang dibangun dari nol sudah mendapat bentuk barunya dari
`migration/029a_chat_dua_tabel_baru.sql`. Berkas ini hanya untuk database yang
masih memakai `group_rooms`/`group_messages`.

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

#### Urutan deploy — PENTING

Migrasi ini mengganti nama tabel, jadi kode lama berhenti bekerja begitu ia
selesai, dan kode baru berhenti bekerja bila dijalankan sebelum ia dijalankan.
Keduanya harus berpindah bersama:

1. Hentikan container.
2. Jalankan migrasi ini.
3. Jalankan image baru.

Menjalankan migrasi selagi container lama masih hidup akan membuat setiap
pembukaan halaman pesan gagal sampai image barunya naik.
