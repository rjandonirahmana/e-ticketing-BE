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
# ═══════════════════════════════════════════════════════════════════════════════

# ── Stage 1: Build frontend assets (WASM + CSS) ───────────────────────────────
FROM rustlang/rust:nightly AS frontend-builder

RUN apt-get update && apt-get install -y \
    curl binaryen \
    && rm -rf /var/lib/apt/lists/*

# Install wasm-pack + cargo-leptos
RUN rustup target add wasm32-unknown-unknown
RUN cargo install cargo-leptos --locked

WORKDIR /app
COPY Cargo.toml Cargo.lock build.rs ./
COPY src/      ./src/
COPY proto/    ./proto/
COPY styles/   ./styles/

# Build WASM + CSS → target/site/
RUN cargo leptos build --release 2>&1

# ── Stage 2: Build backend binary (musl static) ───────────────────────────────
FROM rustlang/rust:nightly-alpine AS backend-builder

RUN apk add --no-cache \
    musl-dev g++ make perl pkgconfig \
    openssl-dev openssl-libs-static \
    zlib-dev zlib-static \
    protobuf protobuf-dev \
    curl

RUN rustup target add x86_64-unknown-linux-musl
RUN rustup target add wasm32-unknown-unknown
RUN cargo install cargo-leptos --locked

WORKDIR /app

ENV OPENSSL_STATIC=1
ENV PKG_CONFIG_ALLOW_CROSS=1

# Copy seluruh source
COPY . .
# Copy frontend assets yang sudah dibangun
COPY --from=frontend-builder /app/target/site ./target/site

# Build binary backend (dengan SSR embedded)
# cargo leptos build --release akan menghasilkan binary di target/server/
RUN cargo leptos build --release 2>&1

# ── Stage 3: Runtime ───────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Binary backend (SSR embedded)
COPY --from=backend-builder /app/target/release/e-ticketing ./e-ticketing
# Static assets frontend (WASM, JS, CSS)
COPY --from=backend-builder /app/target/site ./target/site

# Buat direktori untuk proto jika diperlukan
COPY --from=backend-builder /app/proto ./proto

EXPOSE 3000

ENV HOST=0.0.0.0
ENV PORT=3000
ENV LEPTOS_SITE_ROOT=target/site
ENV LEPTOS_ENV=PROD

CMD ["./e-ticketing"]
