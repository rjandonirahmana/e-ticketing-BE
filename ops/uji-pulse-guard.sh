#!/bin/sh
# uji-pulse-guard.sh — uji logika keputusan pulse-guard TANPA server.
#
# Perintah luar (curl/docker/dll) diganti stub di PATH, dan STATE/LOG diarahkan
# ke direktori sementara. Yang diuji cuma satu hal, tapi hal yang paling sulit
# diuji di produksi: KAPAN ia memutuskan restart, dan kapan ia menahan diri.
set -u
GUARD="$(cd "$(dirname "$0")" && pwd)/pulse-guard.sh"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
BIN="$TMP/bin"; mkdir -p "$BIN"

# ── Stub ─────────────────────────────────────────────────────────────────────
cat >"$BIN/curl" <<'S'
#!/bin/sh
[ -f "$STUB_SEHAT" ] && exit 0
exit 7
S
cat >"$BIN/docker" <<'S'
#!/bin/sh
echo "$@" >>"$STUB_JEJAK"
case "$1" in inspect) echo 0 ;; esac
exit 0
S
# `timeout` tak ada di macOS; di server GNU coreutils menyediakannya.
cat >"$BIN/timeout" <<'S'
#!/bin/sh
shift; exec "$@"
S
for c in free swapon dmesg top; do printf '#!/bin/sh\nexit 0\n' >"$BIN/$c"; done
chmod +x "$BIN"/*

export STUB_JEJAK="$TMP/docker-jejak" STUB_SEHAT="$TMP/sehat"
export PATH="$BIN:$PATH" PULSE_STATE="$TMP/state" PULSE_LOG="$TMP/log"
# Verifikasi pasca-restart dipangkas: stub curl selalu gagal, dan menunggu
# 120 detik nyata di sebuah uji tak membuktikan apa pun.
export PULSE_JEDA_RESTART=600 PULSE_MAKS_RESTART_PER_JAM=3 PULSE_VERIFIKASI_DETIK=5

lolos=0; gagal=0
periksa() { # nama, harapan, kenyataan
    if [ "$2" = "$3" ]; then lolos=$((lolos+1)); echo "  ✓ $1"
    else gagal=$((gagal+1)); echo "  ✗ $1 — harap '$2', dapat '$3'"; fi
}
restart_terjadi() { grep -c '^restart ' "$STUB_JEJAK" 2>/dev/null | tr -d ' '; }

echo "── 1. App sehat: tak ada tindakan ─────────────────────────────────────"
: >"$STUB_JEJAK"; touch "$STUB_SEHAT"
sh "$GUARD" >/dev/null 2>&1
periksa "tidak restart" 0 "$(restart_terjadi)"
periksa "penghitung gagal nol" 0 "$(cat "$TMP/state/gagal")"

echo "── 2. Gagal 2x: MASIH menahan diri ───────────────────────────────────"
rm -f "$STUB_SEHAT"; : >"$STUB_JEJAK"
sh "$GUARD" >/dev/null 2>&1; sh "$GUARD" >/dev/null 2>&1
periksa "tidak restart di 2/3" 0 "$(restart_terjadi)"
periksa "penghitung = 2" 2 "$(cat "$TMP/state/gagal")"

echo "── 3. Gagal ke-3: restart ────────────────────────────────────────────"
sh "$GUARD" >/dev/null 2>&1
periksa "restart dipanggil sekali" 1 "$(restart_terjadi)"
periksa "penghitung direset" 0 "$(cat "$TMP/state/gagal")"
periksa "bukti insiden tersimpan" 1 "$(ls -d "$TMP"/log/insiden-* 2>/dev/null | wc -l | tr -d ' ')"

echo "── 4. Jeda: 3 kegagalan lagi TAK boleh restart lagi ──────────────────"
: >"$STUB_JEJAK"
sh "$GUARD" >/dev/null 2>&1; sh "$GUARD" >/dev/null 2>&1; sh "$GUARD" >/dev/null 2>&1
periksa "ditahan oleh jeda 600 dtk" 0 "$(restart_terjadi)"
periksa "log menyebut TUNGGU" 1 "$(grep -c 'TUNGGU' "$TMP/log/guard.log" | tr -d ' ')"

echo "── 5. Anggaran habis: restart ke-4 dalam sejam = MENYERAH ────────────"
# Tiga restart tercatat baru saja, tapi jeda dilewati (uji anggarannya sendiri).
now=$(date +%s); : >"$TMP/state/riwayat-restart"
for d in 3000 2000 1000; do echo $((now - d)) >>"$TMP/state/riwayat-restart"; done
: >"$STUB_JEJAK"; echo 3 >"$TMP/state/gagal"
PULSE_JEDA_RESTART=1 sh "$GUARD" >/dev/null 2>&1
periksa "tidak restart" 0 "$(restart_terjadi)"
periksa "log menyebut MENYERAH" 1 "$(grep -c 'MENYERAH' "$TMP/log/guard.log" | tr -d ' ')"

echo "── 6. Pulih: penghitung bersih lagi ──────────────────────────────────"
touch "$STUB_SEHAT"; : >"$STUB_JEJAK"
sh "$GUARD" >/dev/null 2>&1
periksa "tidak restart saat sehat" 0 "$(restart_terjadi)"
periksa "penghitung nol" 0 "$(cat "$TMP/state/gagal")"

echo
echo "lolos=$lolos gagal=$gagal"
[ "$gagal" = 0 ] || exit 1
