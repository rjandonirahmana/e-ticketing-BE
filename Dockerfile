# syntax=docker/dockerfile:1.7
# ═══════════════════════════════════════════════════════════════════════════════
# Dockerfile — e-ticketing Platform (Leptos SSR + Axum)
#
# Build pipeline:
#   Stage 1 (builder): Alpine Rust nightly
#     a. Install cargo-leptos — own image layer, only reruns on toolchain change
#     b. Pre-compile all deps (dummy src) — cached until Cargo.toml/lock changes
#     c. cargo leptos build --release — WASM + SSR in a single pass, no double compile
#   Stage 2 (runtime): debian:bookworm-slim — musl static binary runs on glibc Linux
#
# Why single Alpine stage:
#   Previous design used rustlang/rust:nightly (Debian) for frontend and
#   rustlang/rust:nightly-alpine for backend. Different toolchains forced separate
#   cache mount IDs that never share artifacts, causing the SSR backend to be
#   compiled twice from scratch (once inside cargo leptos build, once standalone).
#   Single Alpine stage with consistent cache IDs eliminates that.
#
# Cache mount strategy:
#   id=cargo-registry — shared across all RUN commands for Cargo crate downloads
#   id=target         — shared across dep pre-compile + final build; native and
#                       WASM artifacts land in non-overlapping subdirs
#                       (target/release/ vs target/wasm32-unknown-unknown/release/)
#                       so a single mount covers both without conflict
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

# Compiled once as an image layer — reruns only when the Alpine base or nightly
# toolchain version changes, not on source or dependency changes.
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

# Minimal dummy source lets Cargo compile and cache all dependencies even though
# the final binary/lib won't link. The || true discards the expected link error.
RUN mkdir -p src && \
    printf 'fn main() {}' > src/main.rs && \
    printf '' > src/lib.rs

# SSR (native musl) deps — artifacts land in target/release/deps/
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=target,target=/app/target \
    cargo build --release --features ssr 2>&1 || true

# WASM/hydrate deps — artifacts land in target/wasm32-unknown-unknown/release/deps/
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=target,target=/app/target \
    cargo build --release --target wasm32-unknown-unknown --features hydrate 2>&1 || true

# ── Final build ────────────────────────────────────────────────────────────────
COPY src/ ./src/
# Signal Cargo that source changed after the dummy→real source swap above.
RUN touch src/main.rs src/lib.rs

# Single cargo leptos build pass: compiles WASM (hydrate) + SSR binary together
# using bin-features=["ssr"] and lib-features=["hydrate"] from Cargo.toml metadata.
# Dep artifacts already in id=target cache — only changed source files recompile.
#
# FIX naming bug: cargo-leptos occasionally writes the WASM binary as
# e-ticketing.wasm instead of e-ticketing_bg.wasm. The JS loader always
# requests the _bg suffix, so a missing file → WASM 404 → hydration never starts
# → every interactive page stays in a loading state forever. Copy to the expected
# name when needed.
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=target,target=/app/target \
    cargo leptos build --release 2>&1 \
    && cp /app/target/release/e-ticketing /app/e-ticketing-bin \
    && cp -r /app/target/site /app/site-out \
    && cd /app/site-out/pkg \
    && [ -f e-ticketing.wasm ] && cp e-ticketing.wasm e-ticketing_bg.wasm \
    && (ls -la e-ticketing*.wasm || true) \
    && test -f e-ticketing_bg.wasm

# ── Runtime ───────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/e-ticketing-bin ./e-ticketing
COPY --from=builder /app/site-out        ./target/site
# Cargo.toml required at runtime: main.rs calls get_configuration(Some("Cargo.toml"))
# to read [package.metadata.leptos] for site-addr/site-root. Missing → immediate exit.
COPY --from=builder /app/Cargo.toml      ./Cargo.toml
COPY --from=builder /app/proto           ./proto

EXPOSE 3000

ENV HOST=0.0.0.0
ENV PORT=3000
ENV LEPTOS_SITE_ROOT=target/site
ENV LEPTOS_ENV=PROD

CMD ["./e-ticketing"]
