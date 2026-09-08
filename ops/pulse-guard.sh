#!/bin/sh
# ═══════════════════════════════════════════════════════════════════════════════
# pulse-guard.sh — pengawas host untuk container `ticketing` (:3100)
#
# Dijalankan systemd timer tiap 30 detik. Tugasnya SATU: mengembalikan layanan
# saat app membeku, dan meninggalkan bukti kenapa ia membeku.
#
# ── KENAPA DI HOST, BUKAN DOCKER HEALTHCHECK ─────────────────────────────────
# Runtime youki di mesin ini membuat `docker exec` gagal ("OCI runtime exec
# failed"), sehingga SEMUA container tampil `unhealthy` walau sehat. Apa pun
# yang mengambil keputusan dari `.State.Health.Status` — termasuk autoheal —
# akan me-restart container yang tidak sakit, terus-menerus. Uji di sini
# memakai curl dari host ke port yang dipublish: ia menempuh jalur yang sama
# dengan pingora, dan tak menyentuh `docker exec` sama sekali.
#
# ── KENAPA `/healthz` ────────────────────────────────────────────────────────
# `/healthz` (main.rs) tak menyentuh database: ia menjawab "ok" selama runtime
# tokio masih menjadwalkan task. Kalau ia diam, yang mati adalah PROSESNYA —
# persis keadaan yang restart bisa perbaiki. Kalau ia menjawab tapi halaman SSR
# lambat, masalahnya di DB/query dan restart tak menolong; skrip ini sengaja
# TIDAK bertindak untuk kasus itu.
#
# Pasang: lihat ops/README.md
# ═══════════════════════════════════════════════════════════════════════════════
set -u

NAMA="${PULSE_NAMA:-ticketing}"
URL="${PULSE_URL:-http://127.0.0.1:3100/healthz}"

# Tiga kegagalan berturut pada timer 30 detik = ±90 detik tak menjawab sebelum
# bertindak. Satu kegagalan tidak cukup: saat mesin sedang swap berat, curl
# bisa lewat 5 detik sekali lalu pulih sendiri, dan restart di situ justru
# memutus siaran yang sebenarnya masih hidup.
BATAS_GAGAL="${PULSE_BATAS_GAGAL:-3}"

# Jarak minimum antar-restart. Tanpa ini, app yang butuh 40 detik untuk siap
# akan di-restart lagi sebelum sempat berdiri — dan tak pernah berhasil.
JEDA_RESTART="${PULSE_JEDA_RESTART:-600}"

# Restart ke-4 dalam sejam berarti restart bukan obatnya. Berhenti, dan biarkan
# layanan tetap mati supaya masalahnya terlihat dan bukti terkumpul, alih-alih
# menyembunyikannya di balik loop yang tampak "menangani".
MAKS_RESTART_PER_JAM="${PULSE_MAKS_RESTART_PER_JAM:-3}"

# Pencegahan memori: bertindak hanya bila RAM mesin nyaris habis DAN app inilah
# yang gemuk. Kalau yang menghabiskan RAM adalah tetangga (Postgres, waha,
# rustfs), me-restart app tidak memperbaiki apa pun — cuma membunuh sesi
# pengguna. Dalam hal itu skrip mencatat siapa pemakannya, lalu diam.
# Berapa lama menunggu app berdiri lagi sesudah restart sebelum menyatakannya
# gagal. Migrasi di startup bisa memakan waktu, jadi jangan terlalu pendek.
VERIFIKASI_DETIK="${PULSE_VERIFIKASI_DETIK:-120}"

AMBANG_MEM_TERSEDIA_MB="${PULSE_AMBANG_MEM_MB:-250}"
AMBANG_RSS_APP_MB="${PULSE_AMBANG_RSS_MB:-2000}"
BATAS_MEM_BERTURUT="${PULSE_BATAS_MEM:-5}"

# Bisa ditimpa supaya skrip ini dapat diuji tanpa menyentuh mesin produksi
# (lihat ops/uji-pulse-guard.sh).
STATE="${PULSE_STATE:-/var/lib/pulse-guard}"
LOG="${PULSE_LOG:-/var/log/pulse-guard}"
mkdir -p "$STATE" "$LOG" || exit 0

F_GAGAL="$STATE/gagal"
F_MEM="$STATE/mem"
F_RESTART="$STATE/riwayat-restart"   # satu epoch per baris

SEKARANG=$(date +%s)
CAP=$(date +%F-%H%M%S)

catat() { echo "$(date -Is) $*" >>"$LOG/guard.log"; }
baca()  { v=$([ -f "$1" ] && cat "$1" 2>/dev/null); echo "${v:-0}"; }

# ── Pembacaan mesin (murah, /proc langsung) ──────────────────────────────────
mem_tersedia_mb() { awk '/^MemAvailable:/{print int($2/1024)}' /proc/meminfo; }
swap_pakai_mb()   { awk '/^SwapTotal:/{t=$2}/^SwapFree:/{f=$2}END{print int((t-f)/1024)}' /proc/meminfo; }
pid_app()         { timeout 10 docker inspect -f '{{.State.Pid}}' "$NAMA" 2>/dev/null || echo 0; }
rss_app_mb() {
    p=$(pid_app)
    case "$p" in ''|0) echo 0; return ;; esac
    v=$(awk '/^VmRSS:/{print int($2/1024)}' "/proc/$p/status" 2>/dev/null)
    echo "${v:-0}"
}
potret() {
    echo "mem_tersedia=$(mem_tersedia_mb)MB swap_pakai=$(swap_pakai_mb)MB rss_app=$(rss_app_mb)MB load=$(cut -d' ' -f1-3 /proc/loadavg) psi_mem=$(awk '/^some/{print $3}' /proc/pressure/memory 2>/dev/null)"
}

# ── Bukti: dikumpulkan SEBELUM restart, karena restart menghapusnya ──────────
kumpulkan_bukti() {
    d="$LOG/insiden-$CAP"; mkdir -p "$d"
    timeout 20 docker logs --tail 3000 "$NAMA" >"$d/app.log" 2>&1
    # Vonis watchdog app (utils/watchdog.rs) — inilah baris yang membedakan
    # "worker tokio terblokir" (bug kode) dari "kernel tak menjalankan proses"
    # (mesin kehabisan CPU/memori). Disalin terpisah supaya tak tenggelam.
    grep -E "TERSENDAT|TAK DAPAT CPU|PULIH|DAPAT CPU LAGI" "$d/app.log" | tail -40 >"$d/vonis-watchdog.txt"
    { free -m; echo; swapon --show; echo; cat /proc/loadavg; echo
      for f in /proc/pressure/cpu /proc/pressure/memory /proc/pressure/io; do echo "$f: $(head -1 "$f" 2>/dev/null)"; done
    } >"$d/memori.txt" 2>&1
    ps -eo pid,rss,pcpu,comm --sort=-rss | head -15 >"$d/proses-terbesar.txt" 2>&1
    p=$(pid_app)
    case "$p" in ''|0) : ;; *) timeout 10 top -bH -n1 -p "$p" 2>/dev/null | head -25 >"$d/thread-app.txt" ;; esac
    timeout 15 docker ps -a --format '{{.Names}}\t{{.Status}}' >"$d/container.txt" 2>&1
    dmesg -T 2>/dev/null | grep -iE "oom|killed process|blocked for more than" | tail -25 >"$d/kernel.txt"
    # Bukti lama dibuang sendiri: disk VPS ini pernah jadi masalah, dan
    # pengawas yang mengisi disknya sendiri adalah pengawas yang bikin insiden.
    find "$LOG" -maxdepth 1 -name 'insiden-*' -type d -mtime +14 -exec rm -rf {} + 2>/dev/null
    echo "$d"
}

lapor() {
    # Telegram opsional: isi /etc/pulse-guard.env bila mau notifikasi.
    [ -f /etc/pulse-guard.env ] && . /etc/pulse-guard.env
    [ -n "${TELEGRAM_BOT_TOKEN:-}" ] && [ -n "${TELEGRAM_ADMIN_CHAT_ID:-}" ] || return 0
    curl -s -m 10 -o /dev/null \
        "https://api.telegram.org/bot$TELEGRAM_BOT_TOKEN/sendMessage" \
        --data-urlencode "chat_id=$TELEGRAM_ADMIN_CHAT_ID" \
        --data-urlencode "text=[pulse-guard] $1"
}

restart_app() {
    alasan="$1"
    riwayat=$(awk -v b=$((SEKARANG - 3600)) '$1 > b' "$F_RESTART" 2>/dev/null | wc -l | tr -d ' ')
    terakhir=$(tail -1 "$F_RESTART" 2>/dev/null)
    riwayat="${riwayat:-0}"; terakhir="${terakhir:-0}"

    if [ "$((SEKARANG - terakhir))" -lt "$JEDA_RESTART" ]; then
        catat "TUNGGU  masih dalam jeda $JEDA_RESTART dtk sejak restart terakhir ($alasan)"
        return 0
    fi
    if [ "$riwayat" -ge "$MAKS_RESTART_PER_JAM" ]; then
        catat "MENYERAH  sudah $riwayat restart dalam sejam — restart bukan obatnya, butuh manusia ($alasan)"
        lapor "MENYERAH: $riwayat restart dalam sejam, app dibiarkan mati. $(potret)"
        return 0
    fi

    bukti=$(kumpulkan_bukti)
    catat "RESTART  $alasan | $(potret) | bukti=$bukti"
    lapor "restart $NAMA — $alasan. $(potret). Bukti: $bukti"
    timeout 120 docker restart "$NAMA" >>"$LOG/guard.log" 2>&1
    echo "$SEKARANG" >>"$F_RESTART"
    echo 0 >"$F_GAGAL"; echo 0 >"$F_MEM"

    # Verifikasi, jangan asumsi. Container yang "berhasil di-restart" tapi
    # crash-loop tetap harus terbaca sebagai gagal di log ini.
    batas=$((VERIFIKASI_DETIK / 5)); i=0
    while [ "$i" -lt "$batas" ]; do
        sleep 5; i=$((i + 1))
        if curl -sf -m 5 -o /dev/null "$URL"; then
            catat "PULIH   $NAMA menjawab lagi setelah $((i * 5)) dtk"
            return 0
        fi
    done
    catat "GAGAL   $NAMA tak menjawab $VERIFIKASI_DETIK dtk setelah restart"
    lapor "restart TIDAK memulihkan $NAMA setelah 120 dtk."
}

# ── 1. Uji hidup ─────────────────────────────────────────────────────────────
if curl -sf -m 5 -o /dev/null "$URL"; then
    [ "$(baca "$F_GAGAL")" != "0" ] && catat "OK      menjawab lagi setelah $(baca "$F_GAGAL")x gagal"
    echo 0 >"$F_GAGAL"
else
    n=$(( $(baca "$F_GAGAL") + 1 ))
    echo "$n" >"$F_GAGAL"
    catat "GAGAL   $URL tak menjawab ($n/$BATAS_GAGAL) | $(potret)"
    [ "$n" -ge "$BATAS_GAGAL" ] && restart_app "tak menjawab $n pemeriksaan berturut"
    exit 0
fi

# ── 2. Pencegahan: RAM mesin nyaris habis ────────────────────────────────────
# Hanya berlaku saat app MASIH menjawab (cabang di atas sudah exit). Gunanya
# bertindak sebelum OOM killer global memilih korbannya sendiri — 24 Agu 2026
# korbannya Postgres, dan seluruh cluster ikut berhenti selama 42 menit.
tersedia=$(mem_tersedia_mb)
if [ "$tersedia" -lt "$AMBANG_MEM_TERSEDIA_MB" ]; then
    m=$(( $(baca "$F_MEM") + 1 ))
    echo "$m" >"$F_MEM"
    rss=$(rss_app_mb)
    catat "MEMORI  tersedia ${tersedia}MB < ${AMBANG_MEM_TERSEDIA_MB}MB ($m/$BATAS_MEM_BERTURUT) rss_app=${rss}MB"
    if [ "$m" -ge "$BATAS_MEM_BERTURUT" ]; then
        if [ "$rss" -ge "$AMBANG_RSS_APP_MB" ]; then
            restart_app "RAM mesin tinggal ${tersedia}MB dan app memakai ${rss}MB"
        else
            # Bukan app ini yang gemuk — restart tak akan mengembalikan RAM.
            catat "MEMORI  BUKAN app ini (rss ${rss}MB < ${AMBANG_RSS_APP_MB}MB) — tidak restart. Pemakan terbesar:"
            ps -eo pid,rss,comm --sort=-rss | head -6 >>"$LOG/guard.log"
            lapor "RAM mesin tinggal ${tersedia}MB tapi app cuma ${rss}MB — pemakannya proses lain, tidak di-restart."
            echo 0 >"$F_MEM"
        fi
    fi
else
    echo 0 >"$F_MEM"
fi
