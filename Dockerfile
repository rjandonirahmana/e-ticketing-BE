# syntax=docker/dockerfile:1.7
# ═══════════════════════════════════════════════════════════════════════════════
# Dockerfile — PULSE Platform (Backend API + Leptos SSR Frontend)
#
# Build pipeline:
#   Stage 1: Install cargo-leptos + build WASM/CSS (frontend assets)
#   Stage 2: Build backend binary yang meng-embed SSR (musl static)
#   Stage 3: Runtime minimal (scratch)
#
# Usage:
#   docker build -t pulse .
#   docker run -p 3000:3000 --env-file .env pulse
#
# Catatan kecepatan build:
#   Cache mounts (--mount=type=cache) di bawah membuat ~/.cargo/registry dan
#   target/ persist antar build (BuildKit), jadi dependency yang sudah pernah
#   dikompilasi tidak diulang dari nol setiap kali. Tanpa ini, setiap build
#   mengompilasi ulang seluruh dependency tree (tokio/axum/leptos/tonic dst)
#   dari awal — itu sumber utama build yang lama.
# ═══════════════════════════════════════════════════════════════════════════════

# ── Stage 1: Build frontend assets (WASM + CSS) ───────────────────────────────
FROM rustlang/rust:nightly AS frontend-builder

RUN apt-get update && apt-get install -y \
    curl binaryen protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Install wasm-pack + cargo-leptos
RUN rustup target add wasm32-unknown-unknown
RUN --mount=type=cache,id=cargo-registry-debian,target=/usr/local/cargo/registry \
    cargo install cargo-leptos --locked

WORKDIR /app
COPY Cargo.toml Cargo.lock build.rs ./
COPY src/      ./src/
COPY proto/    ./proto/
COPY styles/   ./styles/

# Build WASM + CSS → target/site/
RUN --mount=type=cache,id=cargo-registry-debian,target=/usr/local/cargo/registry \
    --mount=type=cache,id=target-frontend,target=/app/target \
    cargo leptos build --release 2>&1 \
    && cp -r /app/target/site /app/site-out

# ── Stage 2: Build backend binary (musl static) ───────────────────────────────
FROM rustlang/rust:nightly-alpine AS backend-builder

RUN apk add --no-cache \
    musl-dev g++ make perl pkgconfig \
    openssl-dev openssl-libs-static \
    zlib-dev zlib-static \
    protobuf protobuf-dev \
    curl

WORKDIR /app

ENV OPENSSL_STATIC=1
ENV PKG_CONFIG_ALLOW_CROSS=1

# Copy seluruh source
COPY . .

# Build binary backend saja (default feature = ssr, lihat Cargo.toml).
# Tidak perlu cargo-leptos / wasm32 target di sini — sebelumnya step ini
# membangun ulang seluruh WASM frontend untuk kedua kalinya (padahal sudah
# dibangun di Stage 1), itu sumber pemborosan waktu terbesar di build ini.
RUN --mount=type=cache,id=cargo-registry-alpine,target=/usr/local/cargo/registry \
    --mount=type=cache,id=target-backend,target=/app/target \
    cargo build --release --bin e-ticketing 2>&1 \
    && cp /app/target/release/e-ticketing /app/e-ticketing-out

# ── Stage 3: Runtime ───────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Binary backend (SSR embedded)
COPY --from=backend-builder /app/e-ticketing-out ./e-ticketing
# Static assets frontend (WASM, JS, CSS)
COPY --from=frontend-builder /app/site-out ./target/site

# Cargo.toml dibutuhkan SAAT RUNTIME — main.rs membaca [package.metadata.leptos]
# dari sini via get_configuration(Some("Cargo.toml")) untuk site-addr/site-root dst.
# Tanpa ini binary langsung exit dengan "Cargo.toml not found in package root".
COPY --from=backend-builder /app/Cargo.toml ./Cargo.toml

# Buat direktori untuk proto jika diperlukan
COPY --from=backend-builder /app/proto ./proto

EXPOSE 3000

ENV HOST=0.0.0.0
ENV PORT=3000
ENV LEPTOS_SITE_ROOT=target/site
ENV LEPTOS_ENV=PROD

CMD ["./e-ticketing"]
