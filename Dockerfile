# syntax=docker/dockerfile:1.7
# ═══════════════════════════════════════════════════════════════════════════════
# Dockerfile — PULSE / e-ticketing (Leptos SSR + Axum, satu binary satu port)
#
#   Stage 1 (builder): rust:1.95-alpine (musl, STATIS)
#     a. cargo-leptos DIPIN — layer sendiri, rerun hanya saat toolchain berubah
#     b. pre-compile dependency dengan dummy src — cache s/d Cargo.toml/lock berubah
#     c. cargo leptos build --release --precompress → WASM + SSR + .br/.gz sekali jalan
#   Stage 2 (runtime): debian:bookworm-slim, NON-ROOT
#
# TIDAK ADA TEST DI SINI — disengaja. Test dijalankan di job terpisah pada CI
# (.github/workflows/master.yml), di luar `docker build`. Menaruhnya di dalam
# build image berarti membayar ulang seluruh pipeline hanya untuk mengetahui
# satu assert gagal, dan membuat kegagalan test tak bisa dibedakan dari
# kegagalan build di log Docker.
#
# Env WAJIB saat runtime:
#   DATABASE_URL, JWT_SECRET, RUSTFS_ACCESS_KEY, RUSTFS_SECRET_KEY
#   REDIS_URL, INTERNAL_JWT_SECRET (sangat disarankan)
#
# Opsional: DB_POOL_MAX_SIZE (24), JWT_EXPIRY_HOURS (24), BCRYPT_COST (10),
#   RUSTFS_ENDPOINT/PUBLIC_URL, WAHA_*, TELEGRAM_*, SFU_BIND_ADDR/PUBLIC_IP,
#   AUTO_MIGRATE, WORKER_THREADS, TOKIO_MAX_BLOCKING_THREADS,
#   UPLOAD_TMP_DIR (default /var/tmp/e-ticketing-uploads — WAJIB di disk, bukan
#   tmpfs; kalau tmpfs, streaming upload tetap memakan RAM. Lihat main.rs)
#
# Run:  docker build -t ticketing .
#       docker run -p 3000:3000 --env-file .env ticketing
# ═══════════════════════════════════════════════════════════════════════════════

# ── Builder ───────────────────────────────────────────────────────────────────
# Versi DIPIN dan HARUS sama dengan `channel` di rust-toolchain.toml. Kalau
# berbeda, rustup mengunduh toolchain kedua di dalam image — build lebih lambat
# tanpa memberi apa pun.
FROM rust:1.95-alpine AS builder

RUN apk add --no-cache \
    musl-dev g++ make perl pkgconfig \
    openssl-dev openssl-libs-static \
    zlib-dev zlib-static \
    protobuf protobuf-dev \
    curl binaryen brotli

# Target WASM untuk hydration Leptos.
#
# CATATAN: versi compiler dipatri lewat TAG IMAGE di atas (`rust:1.95-alpine`),
# BUKAN lewat `rust-toolchain.toml`. Berkas itu sempat ditambahkan lalu dicabut
# lagi: ia memaksa setiap mesin pengembang mengunduh toolchain KEDUA yang
# terpisah dari `stable` miliknya — meski angkanya kebetulan sama persis —
# dan unduhan itu gagal di tengah jalan, meninggalkan toolchain rusak yang
# membuat SEMUA perintah cargo di direktori ini ikut mati.
#
# Reprodusibilitas tetap terjaga di kedua jalur yang benar-benar membangun
# artefak: tag image di sini, dan `toolchain: "1.95.0"` yang eksplisit di
# .github/workflows/master.yml. Kalau suatu saat pin lokal memang diinginkan,
# pasang toolchainnya lebih dulu (`rustup toolchain install 1.95.0 -t
# wasm32-unknown-unknown`) BARU tambahkan berkasnya — bukan sebaliknya.
RUN rustup target add wasm32-unknown-unknown

# cargo-leptos sebagai layer sendiri (rerun hanya saat base/toolchain berubah).
#
# VERSINYA DIPIN. `--locked` hanya mengunci dependensi cargo-leptos, BUKAN versi
# cargo-leptos itu sendiri — tanpa `--version`, build bulan depan bisa memakai
# rilis baru yang mengubah tata letak keluaran (nama berkas di /pkg, ada/tidaknya
# hash.txt). Persis kelas kegagalan yang sudah pernah menimpa proyek ini: nama
# bundle WASM yang bergeser satu suku kata membuat hydration diam-diam tak pernah
# jalan, dan seluruh aplikasi jadi HTML mati yang TAMPAK normal (lihat
# `wasm_bg_alias` di main.rs).
#
# 0.3.7 = versi yang dipakai pengembang (`cargo leptos --version`). Samakan.
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    cargo install cargo-leptos --locked --version 0.3.7

ENV OPENSSL_STATIC=1
ENV PKG_CONFIG_ALLOW_CROSS=1
WORKDIR /app

# ── Pre-compile dependency ────────────────────────────────────────────────────
# Salin HANYA input yang memengaruhi dependency → layer ini invalid saat
# Cargo.toml/lock/build.rs/proto/styles/migration berubah, BUKAN saat edit src/.
#
# `.cargo/config.toml` SENGAJA TIDAK disalin. Isinya pilihan lokal laptop:
#   • `MACOSX_DEPLOYMENT_TARGET` — macOS saja, tak berarti di Linux.
#   • `[build] jobs = 10` — di runner CI 4 core, 10 rustc paralel bersamaan
#     `codegen-units = 1` + LTO justru menekan RAM sampai risiko OOM.
#   • rustflags wasm32 (`opt-level=z`, `panic=abort`, `codegen-units=1`) — sudah
#     dinyatakan ulang di `[profile.wasm-release]`, jadi murni duplikat.
# Build image karena itu memakai default cargo (jobs = jumlah core) + profil di
# Cargo.toml, yang berlaku sama di mana pun.
COPY Cargo.toml Cargo.lock build.rs ./
COPY proto/ ./proto/
COPY styles/ ./styles/
# ── migration/ WAJIB ADA SAAT BUILD ──────────────────────────────────────────
# INI PENYEBAB BUILD GAGAL SEBELUMNYA. `build.rs` meng-embed setiap
# `migration/*.sql` ke dalam binari (`include_str!`, lihat MIGRATIONS) supaya
# container yang hanya memuat binari tetap bisa bermigrasi. Baris pertamanya:
#
#     fs::read_dir("migration").expect("read migration/")
#
# Tanpa COPY ini build.rs PANIC — dan panicnya tak terlihat di dua langkah
# pre-compile di bawah karena keduanya diakhiri `|| true`, jadi kegagalannya
# baru meledak di `cargo leptos build` dengan pesan yang tak menyebut Docker
# sama sekali.
COPY migration/ ./migration/
# style/ (tunggal) = `tailwind-input-file` di [package.metadata.leptos].
# BEDA dari styles/ (jamak) di atas, yang dibaca build.rs. Keduanya dipakai dan
# keduanya wajib — cargo-leptos gagal kalau tailwind-input-file tak ada.
COPY style/ ./style/
# public/ = `assets-dir`. cargo-leptos menyalin isinya ke site-root.
COPY public/ ./public/

# Dummy source agar Cargo meng-compile & men-cache SELURUH dependency.
RUN mkdir -p src && \
    printf 'fn main() {}' > src/main.rs && \
    printf '' > src/lib.rs

# Dua langkah di bawah HANYA memanaskan cache. `|| true` disengaja: kegagalan
# meng-compile kerangka dummy tak boleh menjatuhkan image — build yang sebenarnya
# ada di bawah dan TIDAK punya jaring pengaman semacam itu.
#
# Deps SSR (native musl). `--release` = profil yang sama dengan yang dipakai
# `cargo leptos build --release` untuk binari server.
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=target,target=/app/target \
    cargo build --release --features ssr 2>&1 || true

# Deps WASM/hydrate — HARUS meniru persis apa yang dijalankan cargo-leptos,
# kalau tidak layer ini memanaskan cache yang tak pernah dipakai siapa pun:
#
#   • `--no-default-features`: default feature crate ini adalah `ssr`, dan fitur
#     Cargo bersifat aditif — tanpa mematikannya, target wasm32 ikut menyalakan
#     `leptos/ssr` bersamaan `leptos/hydrate`. Kombinasi itu tidak sah.
#   • `--profile wasm-release`: `lib-profile-release = "wasm-release"` di
#     Cargo.toml. Artefak tiap profil tinggal di sub-direktori target yang
#     BERBEDA, jadi memanaskan `release` (seperti versi sebelumnya) menyimpan
#     ratusan berkas yang lalu diabaikan seluruhnya dan seluruh dependency
#     di-compile ulang dari nol di langkah berikutnya.
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=target,target=/app/target \
    cargo build --profile wasm-release --target wasm32-unknown-unknown \
    --no-default-features --features hydrate --lib 2>&1 || true

# ── Build final ───────────────────────────────────────────────────────────────
COPY src/ ./src/
# Sentuh agar Cargo tahu source berubah setelah swap dummy→real.
RUN touch src/main.rs src/lib.rs

# `--precompress` (-P): cargo-leptos menulis `.br` + `.gz` di samping tiap aset
# /pkg, SEKALI, di sini.
#
# Tanpa ini `CompressionLayer` mengompresi ulang bundle WASM (megabyte-an) untuk
# SETIAP klien yang belum punya salinannya. Brotli atas berkas sebesar itu adalah
# pekerjaan CPU yang terasa di VPS kecil, dan terjadi tepat pada momen paling
# genting: kunjungan pertama seseorang, sebelum satu piksel pun interaktif.
# Hasilnya selalu sama untuk berkas yang sama — tak ada alasan membayarnya
# berkali-kali. main.rs menyajikannya lewat `ServeDir::precompressed_br()`.
#
# Aman untuk pengembangan: tanpa berkas .br/.gz, ServeDir menyajikan yang asli
# dan CompressionLayer mengambil alih seperti biasa.
#
# Normalisasi nama WASM di bawah: cargo-leptos 0.3.7 menulis `<name>.wasm`
# sedangkan glue JS yang ia hasilkan sendiri memuat `<name>_bg.wasm`. Beda satu
# suku kata, akibatnya 404 diam-diam → hydration tak pernah jalan → seluruh
# tombol mati tanpa satu pun pesan galat. Salinan ikut membawa varian .br/.gz;
# kalau tidak, permintaan `_bg.wasm` ber-Accept-Encoding: br jatuh ke berkas
# mentah dan seluruh manfaat pra-kompresi hilang justru untuk berkas terbesar.
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=target,target=/app/target \
    cargo leptos build --release --precompress \
    && cp /app/target/release/e-ticketing /app/e-ticketing-bin \
    && cp -r /app/target/site /app/site-out \
    && cd /app/site-out/pkg \
    && for ext in "" .br .gz; do \
         if [ -f "e-ticketing.wasm${ext}" ] && [ ! -f "e-ticketing_bg.wasm${ext}" ]; then \
           cp "e-ticketing.wasm${ext}" "e-ticketing_bg.wasm${ext}"; \
         fi; \
       done \
    && test -f e-ticketing_bg.wasm \
    && ls -la /app/site-out/pkg/

# ── Runtime ───────────────────────────────────────────────────────────────────
# KENAPA Debian padahal builder-nya Alpine: binari target musl bersifat STATIS
# (ditambah OPENSSL_STATIC=1), jadi ia berjalan di distro mana pun — pilihan
# runtime jadi bebas, dan Debian dipilih karena curl untuk HEALTHCHECK serta
# ca-certificates-nya sudah teruji di sini.
#
# Yang TIDAK boleh dilakukan: mengganti target builder ke glibc sambil
# membiarkan runtime ini. Binari glibc tak jalan di musl dan sebaliknya, dan
# gagalnya baru terlihat saat container start.
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# ── Jalan sebagai NON-ROOT ────────────────────────────────────────────────────
# Proses ini memegang kredensial Postgres, Redis, S3/RustFS, WAHA, dan token
# Telegram, serta MENULIS berkas dari data yang dikirim pengguna (streaming
# upload story/gambar merchant ke UPLOAD_TMP_DIR). Berjalan sebagai root berarti
# setiap celah — RCE, path traversal saat menulis temp, container escape —
# langsung mendapat root. UID tetap (10001) supaya kepemilikan berkas pada
# volume ter-mount tetap sama antar rebuild.
RUN useradd --system --uid 10001 --create-home --shell /usr/sbin/nologin pulse

WORKDIR /app

COPY --from=builder /app/e-ticketing-bin ./e-ticketing
COPY --from=builder /app/site-out        ./target/site
# Cargo.toml WAJIB saat runtime: `get_configuration(Some("Cargo.toml"))` membaca
# [package.metadata.leptos] untuk site-addr & site-root. Hilang → panic saat start.
COPY --from=builder /app/Cargo.toml      ./Cargo.toml
# CATATAN: `proto/` TIDAK ikut, dan itu benar. `tonic::include_proto!`
# (src/proto/mod.rs) menyisipkan kode hasil generate dari OUT_DIR pada saat
# COMPILE; tak ada satu pun berkas .proto yang dibaca saat runtime. Versi
# sebelumnya menyalinnya — muatan mati yang menyesatkan siapa pun yang mengira
# runtime masih butuh skema itu.

# Direktori temp upload (default UPLOAD_TMP_DIR). Dibuat & dimiliki `pulse` di
# sini supaya tak bergantung pada mode /var/tmp di host: main.rs melakukan
# canary-write saat start dan FAIL-FAST bila tak bisa menulis, jadi salah
# kepemilikan berarti container tak pernah naik.
RUN mkdir -p /var/tmp/e-ticketing-uploads \
    && chown -R pulse:pulse /app /var/tmp/e-ticketing-uploads

USER pulse

EXPOSE 3000
# SFU WebRTC (UDP). Media mengalir di sini, TERPISAH dari port HTTP di atas —
# publikasikan dengan `-p 4000:4000/udp` dan buka inbound UDP di firewall, kalau
# tidak penonton di luar LAN tak pernah tersambung (ICE gagal diam-diam).
EXPOSE 4000/udp

ENV LEPTOS_SITE_ROOT=target/site
ENV LEPTOS_ENV=PROD

# /healthz murah (tanpa query DB) → tak ikut "unhealthy" saat DB sibuk.
#
# start-period 45s (dulu 20s): selama jendela ini kegagalan probe TIDAK dihitung.
# Startup menyambung Postgres + Redis DAN menjalankan migrasi (AUTO_MIGRATE) di
# bawah advisory lock. Di VPS sibuk atau saat DB baru bangun, 20 detik cukup
# ketat untuk membuat container di-restart tepat sebelum ia sempat siap — lalu
# mengulanginya terus. Melebihkan jendela ini tak berbiaya apa pun: begitu
# /healthz menjawab, probe langsung berlaku normal.
HEALTHCHECK --interval=15s --timeout=3s --start-period=45s --retries=3 \
    CMD curl -fsS http://localhost:3000/healthz || exit 1

CMD ["./e-ticketing"]
