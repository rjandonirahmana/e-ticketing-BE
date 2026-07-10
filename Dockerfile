# syntax=docker/dockerfile:1.7
# ═══════════════════════════════════════════════════════════════════════════════
# Dockerfile — e-ticketing Platform (Leptos SSR + Axum)
#
# Build pipeline:
#   Stage 1 (builder): Alpine Rust nightly
#     a. Install cargo-leptos — own image layer, only reruns on toolchain change
#     b. Pre-compile all deps (dummy src) — cached until Cargo.toml/lock changes
#     c. cargo leptos build --release — WASM + SSR in a single pass
#   Stage 2 (runtime): debian:bookworm-slim
#
# Required env vars at runtime:
#   DATABASE_URL, JWT_SECRET, RUSTFS_ACCESS_KEY, RUSTFS_SECRET_KEY
#   REDIS_URL, INTERNAL_JWT_SECRET (recommended)
#
# Optional env vars:
#   DB_POOL_MAX_SIZE (default 16), JWT_EXPIRY_HOURS (default 24)
#   BCRYPT_COST (default 10), RUSTFS_ENDPOINT, RUSTFS_PUBLIC_URL
#   WAHA_BASE_URL, WAHA_SESSION, WAHA_API_KEY
#   TELEGRAM_BOT_TOKEN, TELEGRAM_ADMIN_CHAT_ID
#
# Run:
#   docker build -t pulse .
#   docker run -p 3000:3000 --env-file .env pulse
# ═══════════════════════════════════════════════════════════════════════════════

# ── Builder ───────────────────────────────────────────────────────────────────
FROM rustlang/rust:nightly-alpine AS builder

RUN apk add --no-cache \
    musl-dev g++ make perl pkgconfig \
    openssl-dev openssl-libs-static \
    zlib-dev zlib-static \
    protobuf protobuf-dev \
    curl binaryen

RUN rustup target add wasm32-unknown-unknown

# cargo-leptos compiled once as its own image layer.
# Only reruns when the Alpine base or nightly toolchain version changes.
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    cargo install cargo-leptos --locked

ENV OPENSSL_STATIC=1
ENV PKG_CONFIG_ALLOW_CROSS=1
WORKDIR /app

# ── Dependency pre-compilation ─────────────────────────────────────────────────
# Copy manifests and generated inputs only. This layer is invalidated when
# Cargo.toml, Cargo.lock, build.rs, proto files, or styles change — not on
# edits to src/. That keeps dep compilation out of the hot path for code changes.
COPY Cargo.toml Cargo.lock build.rs ./
COPY proto/ ./proto/
COPY styles/ ./styles/

# Minimal dummy source lets Cargo compile and cache all dependencies.
# The || true discards the expected link error from the empty main/lib.
RUN mkdir -p src && \
    printf 'fn main() {}' > src/main.rs && \
    printf '' > src/lib.rs

# SSR (native musl) deps
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=target,target=/app/target \
    cargo build --release --features ssr 2>&1 || true

# WASM/hydrate deps
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=target,target=/app/target \
    cargo build --release --target wasm32-unknown-unknown --features hydrate 2>&1 || true

# ── Final build ────────────────────────────────────────────────────────────────
COPY src/ ./src/
# Touch to signal Cargo that source changed after the dummy→real swap.
RUN touch src/main.rs src/lib.rs

# cargo leptos build produces both WASM (hydrate) + SSR binary in one pass.
# Dep artifacts in id=target cache — only changed source files recompile.
#
# WASM naming: release builds may produce either e-ticketing.wasm or
# e-ticketing_bg.wasm depending on the cargo-leptos version.  The JS loader
# always requests the _bg suffix. Normalise here so the runtime image is always
# consistent regardless of which variant cargo-leptos chose.
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=target,target=/app/target \
    cargo leptos build --release 2>&1 \
    && cp /app/target/release/e-ticketing /app/e-ticketing-bin \
    && cp -r /app/target/site /app/site-out \
    && cd /app/site-out/pkg \
    && ([ -f e-ticketing_bg.wasm ] || cp e-ticketing.wasm e-ticketing_bg.wasm) \
    && ls -la e-ticketing*.wasm \
    && test -f e-ticketing_bg.wasm

# ── Runtime ───────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# curl: dipakai HEALTHCHECK di bawah (hit /healthz). ca-certificates: TLS keluar.
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/e-ticketing-bin ./e-ticketing
COPY --from=builder /app/site-out        ./target/site
# Cargo.toml is required at runtime: leptos::config::get_configuration(Some("Cargo.toml"))
# reads [package.metadata.leptos] for site-addr and site-root. Missing → panic on startup.
COPY --from=builder /app/Cargo.toml      ./Cargo.toml
COPY --from=builder /app/proto           ./proto

EXPOSE 3000

ENV LEPTOS_SITE_ROOT=target/site
ENV LEPTOS_ENV=PROD

# Liveness untuk Docker/compose & uptime monitor. /healthz murah (tanpa query DB),
# jadi tak ikut "unhealthy" saat DB sibuk. start-period 20s memberi waktu startup
# (migrasi/koneksi DB+Redis) sebelum healthcheck mulai dihitung.
HEALTHCHECK --interval=15s --timeout=3s --start-period=20s --retries=3 \
    CMD curl -fsS http://localhost:3000/healthz || exit 1

CMD ["./e-ticketing"]
