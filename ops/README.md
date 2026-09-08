# ops/ — pengawas produksi

## pulse-guard

Pengawas host untuk container `ticketing` (:3100). Ia menjawab satu kebutuhan:
**saat app membeku karena CPU/RAM habis, layanan kembali sendiri, dan buktinya
tidak ikut hilang.**

### Kenapa di host, bukan Docker HEALTHCHECK

Runtime **youki** di VPS ini membuat `docker exec` gagal (`OCI runtime exec
failed`), sehingga `docker ps` menampilkan SEMUA container `unhealthy` walau
sehat. Apa pun yang membaca `.State.Health.Status` — termasuk `autoheal` —
akan me-restart container yang tidak sakit, tanpa henti. `pulse-guard` menguji
lewat `curl` dari host ke port yang dipublish: jalur yang sama dengan pingora,
tanpa menyentuh `docker exec`.

### Apa yang memicu restart

| Pemicu | Syarat | Alasan |
|---|---|---|
| App beku | `/healthz` gagal **3× berturut** (±90 dtk) | `/healthz` tak menyentuh DB — ia diam hanya bila runtime tokio berhenti menjadwalkan. Itu persis yang restart bisa perbaiki. |
| RAM mesin nyaris habis | `MemAvailable` < 250 MB **5× berturut** DAN RSS app ≥ 2000 MB | Bertindak sebelum OOM killer global memilih korbannya sendiri (24 Agu 2026: korbannya Postgres, cluster berhenti 42 menit). |

Yang **sengaja tidak** memicu restart:

- `/healthz` menjawab tapi halaman SSR lambat → masalahnya query/DB; restart
  tak menolong dan malah memutus sesi.
- RAM habis tapi RSS app kecil → pemakannya proses lain. Guard mencatat 5
  proses terbesar lalu diam, karena me-restart app tidak mengembalikan RAM
  siapa pun.

### Pagar supaya pengawasnya sendiri tidak jadi masalah

- Jeda minimal **600 dtk** antar-restart — app butuh ±40 dtk untuk berdiri.
- Maksimal **3 restart per jam**. Yang keempat: guard **MENYERAH**, app
  dibiarkan mati, dan itu disengaja — restart ke-4 membuktikan restart bukan
  obatnya, dan loop yang "menangani" hanya menyembunyikan masalahnya.
- Restart selalu **diverifikasi** (curl sampai 120 dtk). Container yang
  crash-loop tercatat sebagai GAGAL, bukan sukses.
- Bukti insiden dibuang otomatis setelah 14 hari.

### Bukti yang direkam sebelum tiap restart

Di `/var/log/pulse-guard/insiden-<tanggal>/`:

- `app.log` — 3000 baris terakhir container
- `vonis-watchdog.txt` — baris `RUNTIME TERSENDAT` / `PROSES TAK DAPAT CPU`
  dari `src/utils/watchdog.rs`. **Ini yang paling penting**: task tokio telat
  tapi OS thread tidak = worker tokio terblokir (bug kode); keduanya telat =
  kernel tak menjalankan proses (mesin kehabisan CPU/memori).
- `memori.txt`, `proses-terbesar.txt`, `thread-app.txt` (cek thread `sfu-udp`),
  `container.txt`, `kernel.txt` (OOM kill)

### Pasang

```sh
# dari laptop
scp ops/pulse-guard.sh root@77.237.242.1:/usr/local/bin/
scp ops/pulse-guard.service ops/pulse-guard.timer root@77.237.242.1:/etc/systemd/system/

# di server
chmod +x /usr/local/bin/pulse-guard.sh
systemctl daemon-reload
systemctl enable --now pulse-guard.timer
systemctl list-timers pulse-guard.timer      # pastikan terjadwal
tail -f /var/log/pulse-guard/guard.log
```

Rotasi log (sekali saja):

```sh
scp ops/pulse-guard.logrotate root@77.237.242.1:/etc/logrotate.d/pulse-guard
```

Uji tanpa menunggu insiden — hentikan app sebentar, lalu lihat guard
mengembalikannya:

```sh
docker stop ticketing && sleep 100 && tail -20 /var/log/pulse-guard/guard.log
```

### Notifikasi Telegram (opsional)

```sh
cat >/etc/pulse-guard.env <<'ENV'
TELEGRAM_BOT_TOKEN=...
TELEGRAM_ADMIN_CHAT_ID=...
ENV
chmod 600 /etc/pulse-guard.env
```

### Ambang bisa diubah tanpa mengubah skrip

Lewat `Environment=` di `pulse-guard.service`: `PULSE_BATAS_GAGAL`,
`PULSE_JEDA_RESTART`, `PULSE_MAKS_RESTART_PER_JAM`, `PULSE_VERIFIKASI_DETIK`,
`PULSE_AMBANG_MEM_MB`, `PULSE_AMBANG_RSS_MB`, `PULSE_BATAS_MEM`, `PULSE_URL`,
`PULSE_NAMA`, `PULSE_STATE`, `PULSE_LOG`.

### Uji logikanya tanpa server

`ops/uji-pulse-guard.sh` menjalankan skrip yang sama dengan `curl`/`docker`
diganti stub dan direktori state sementara, lalu memeriksa keputusannya: sehat
= diam, gagal 2× = diam, gagal 3× = restart + bukti tersimpan, dalam jeda =
TUNGGU, anggaran habis = MENYERAH, sehat lagi = penghitung bersih.

```sh
sh ops/uji-pulse-guard.sh     # 13 pemeriksaan, ±7 detik
```

Jalankan ini setiap kali ambang atau alur keputusannya diubah — jalur restart
adalah jalur yang paling jarang dieksekusi dan paling mahal kalau salah.

### Yang TIDAK diperbaiki pengawas ini

Ia memulihkan gejala, bukan sebab. Kalau `vonis-watchdog.txt` terus berisi
`RUNTIME TERSENDAT` tanpa `PROSES TAK DAPAT CPU`, ada kode blocking di jalur
async dan `WORKER_THREADS=2` membuat dua thread saja cukup untuk membekukan
seluruh app — itu harus diperbaiki di kode. Guard hanya membelikan waktu untuk
memperbaikinya tanpa situs mati semalaman.
