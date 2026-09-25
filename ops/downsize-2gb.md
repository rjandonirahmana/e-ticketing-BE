# downsize-2gb.md — pindah dari box 7,8 GB/4 core ke 2 GB/2 CPU

Runbook ini untuk migrasi server produksi (Sep 2026): dari VPS 7,8 GB/4 core
(Contabo, `77.237.242.1` — lihat memory `vps-oom-swap`) ke box **2 GB RAM / 2
CPU**, topologi SAMA (app + Postgres + Redis + RustFS nebeng satu mesin).

Bagian yang bisa diubah lewat repo ini SUDAH dikerjakan (lihat commit terkait):
`deploy.example.txt` (`--memory=896m`, `DB_POOL_MAX_SIZE=8`,
`MIMALLOC_PURGE_DELAY=0`), `src/live/service.rs` (plafon siaran live serentak
CPU-aware), `ops/pulse-guard.sh` (ambang RSS/memori diturunkan proporsional).

Bagian di BAWAH ini hidup di server, bukan di repo — Postgres native/systemd,
RustFS di `~/Desktop/personal/rustfs/docker-compose.yml` (repo terpisah).
Tidak ada akses SSH dari sesi yang menulis dokumen ini; semua angka adalah
**titik awal berbasis perhitungan, WAJIB diukur ulang** dengan `docker stats`
dan `free -m` setelah deploy nyata — sama seperti prinsip yang sudah ditulis
di `deploy.example.txt` untuk box lama.

## Anggaran kasar 2 GB (perkiraan, ukur ulang!)

| Pemakai | Perkiraan | Catatan |
|---|---|---|
| App (container) | ~896 MB | `--memory=896m` di `deploy.example.txt` |
| Postgres | ~300-400 MB | `shared_buffers` + overhead koneksi, lihat di bawah |
| Redis | ~100-150 MB | `maxmemory 96mb` + overhead proses |
| RustFS | ~150-250 MB | tak ada tuning resmi upstream; batasi via `mem_limit` |
| OS + Docker daemon + youki | ~200-300 MB | tak bisa dikecilkan lebih jauh |

Jumlahnya sudah mepet 2 GB — **swap WAJIB aktif** sebagai jaring pengaman
terakhir (bukan solusi utama; swap thrashing tetap buruk, tapi lebih baik
daripada OOM killer global memilih korban acak seperti insiden 24 Agu 2026).

## Postgres — `postgresql.conf`

```conf
shared_buffers = 192MB
effective_cache_size = 512MB
work_mem = 4MB
maintenance_work_mem = 32MB
max_connections = 25
```

`max_connections = 25` memberi ruang: 8 dari `DB_POOL_MAX_SIZE` app + sisa
untuk koneksi manual (psql, migrasi, monitoring). App TIDAK BOLEH sendirian
minta lebih dari jatah ini — kalau `DB_POOL_MAX_SIZE` dinaikkan lagi di masa
depan, naikkan `max_connections` bersamaan, jangan salah satu saja.

`work_mem` kecil sengaja — dikalikan per operasi sort/hash per koneksi, bukan
sekali per server; di box RAM kecil ini gampang jadi sumber OOM diam-diam
kalau dibiarkan bawaan/besar.

Terapkan lalu `systemctl restart postgresql@<versi>-main` (sesuaikan nama
unit, lihat `systemctl list-units | grep postgres`).

## Redis

```conf
maxmemory 96mb
maxmemory-policy allkeys-lru
```

Redis di proyek ini dipakai untuk pub/sub fanout WS lintas-instance (lihat
memory `eticketing-perf-audit` — bagian `WsManager` fanout) — bukan
penyimpanan yang perlu bertahan; `allkeys-lru` aman karena kehilangan entri
lama hanya berarti fanout mengulang, bukan kehilangan data pengguna.

## RustFS

Di `~/Desktop/personal/rustfs/docker-compose.yml`, tambah batas resource pada
service-nya:

```yaml
services:
  rustfs:
    mem_limit: 256m
```

(`docker compose` v2 sintaks lama `mem_limit` di top-level service masih
didukung sebagai shorthand; kalau proyek ini sudah pakai `deploy.resources`
format Swarm, pindahkan ke situ sesuai versi compose yang dipakai.)

## Swap — verifikasi, JANGAN pasang baru tanpa cek dulu

Insiden 24 Agu 2026 (memory `vps-oom-swap`) sudah memasang swapfile 12 GB +
`vm.swappiness=10` di box LAMA. Kalau box baru adalah mesin fisik BARU (bukan
sekadar resize VM yang sama), swap itu TIDAK ikut pindah — cek dari nol:

```sh
swapon --show          # harus menampilkan swapfile aktif
cat /etc/fstab | grep swap   # harus terdaftar, bukan cuma file yang ada
sysctl vm.swappiness         # harus 10
```

Kalau box baru RAM-nya 2 GB, swapfile 12 GB proporsinya terlalu besar
(swap 6x RAM fisik) — pertimbangkan turunkan ke ~2-4 GB. Prinsipnya sama
seperti sebelumnya: swap ada sebagai jaring pengaman lambat, BUKAN untuk
menopang beban kerja sehari-hari.

## Verifikasi pasca-deploy (WAJIB, jangan asumsi)

1. `docker stats --no-stream` saat trafik normal (~10-30 menit setelah
   deploy) — bandingkan RSS app terhadap `--memory=896m`. Kalau mepet/lewat,
   turunkan `DB_POOL_MAX_SIZE` lebih jauh atau naikkan `--memory` (dan kurangi
   jatah Postgres/Redis/RustFS sepadan — total tetap ≤2GB).
2. Tab **Analitik admin** (`service/server_status.rs`, sudah ada di app) —
   kartu kolam DB (peringatan otomatis saat `pool_idle == 0 && pool_size >=
   pool_max`, gejala paling awal insiden halaman-putih) dan kartu memori.
3. `free -m` + `swapon --show` langsung di host — pastikan swap TERPAKAI
   sesekali (tanda anggaran mepet) tapi TIDAK terus-menerus tinggi (tanda
   thrashing).
4. Coba nyalakan satu siaran live sambil ada trafik lain — pastikan pesan
   "Ada N siaran lain sedang berlangsung" muncul saat mencoba siaran KEDUA
   (plafon `LIVE_MAX_CONCURRENT_ROOMS`, default 1 untuk 2 CPU — lihat
   `src/live/service.rs`), dan SSR halaman lain tetap responsif selama satu
   siaran berjalan.
5. `ops/pulse-guard.sh` — ambang baru (`AMBANG_RSS_APP_MB=700`,
   `AMBANG_MEM_TERSEDIA_MB=150`) aktif otomatis begitu unit systemd-nya
   di-reload; tak perlu langkah tambahan kecuali override lewat
   `/etc/pulse-guard.env` sebelumnya menimpa nilai ini secara eksplisit —
   cek isi file itu di server sebelum deploy.
