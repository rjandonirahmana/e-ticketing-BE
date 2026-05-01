# Stage 1
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

# 1. Cache deps only
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main(){}" > src/main.rs
RUN cargo build --release --target x86_64-unknown-linux-musl
RUN rm -rf src

# 2. Copy full source (including proto + build.rs)
COPY . .

# 3. Build final (build.rs WILL run)
RUN cargo build --release --target x86_64-unknown-linux-musl