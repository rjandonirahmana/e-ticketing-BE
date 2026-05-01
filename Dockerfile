# Stage 1: Builder
FROM rustlang/rust:nightly-alpine AS builder

RUN apk add --no-cache \
    musl-dev g++ make perl pkgconfig \
    openssl-dev openssl-libs-static \
    zlib-dev zlib-static \
    protobuf protobuf-dev \
    curl

RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /app

ENV OPENSSL_STATIC=1
ENV OPENSSL_DIR=/usr
ENV OPENSSL_LIB_DIR=/usr/lib
ENV OPENSSL_INCLUDE_DIR=/usr/include
ENV PKG_CONFIG_ALLOW_CROSS=1

# Copy Cargo files + build.rs + proto DULU sebelum dummy build
# supaya protoc bisa jalan dan generate auth.rs saat cache deps
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY proto/ ./proto/

# Dummy main agar cargo bisa compile deps + jalankan build.rs (protoc)
RUN mkdir src && echo "fn main(){}" > src/main.rs && \
    cargo build --release --target x86_64-unknown-linux-musl && \
    rm src/main.rs

# Copy source asli dan build ulang
# deps sudah ter-cache, hanya recompile e-ticketing saja
COPY src/ ./src/
RUN cargo build --release --target x86_64-unknown-linux-musl

# Stage 2: Runtime
FROM scratch
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/e-ticketing /e-ticketing

EXPOSE 8080
ENV BIND_HOST=0.0.0.0
ENV BIND_PORT=8080
CMD ["/e-ticketing"]