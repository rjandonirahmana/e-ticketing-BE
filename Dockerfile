# Stage 1: Build dependency cache
FROM rustlang/rust:nightly-alpine AS builder

RUN apk add --no-cache \
    musl-dev g++ make perl pkgconfig \
    openssl-dev openssl-libs-static \
    zlib-dev zlib-static

WORKDIR /app

ENV OPENSSL_STATIC=1
ENV OPENSSL_VENDORED=1

# --- Cache dependencies ---
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main(){}" > src/main.rs && \
    cargo build --release --target x86_64-unknown-linux-musl && \
    rm -rf src target/x86_64-unknown-linux-musl/release/deps/e_ticketing*

# --- Build aplikasi sebenarnya ---
COPY . .
RUN cargo build --release --target x86_64-unknown-linux-musl

# Stage 2: Runtime
FROM scratch
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/e-ticketing /e-ticketing

EXPOSE 8080
ENV BIND_HOST=0.0.0.0
ENV BIND_PORT=8080

CMD ["/e-ticketing"]