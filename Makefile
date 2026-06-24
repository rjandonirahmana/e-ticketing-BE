.PHONY: run dev dev-fast build clean check

## SSR only — tanpa WASM hydration (cepat, halaman tidak interaktif)
run:
	cargo run

## Full dev dengan WASM hydration + hot reload (direkomendasikan)
dev:
	cargo leptos watch

## SSR-only dev pakai Cranelift JIT codegen (rebuild 50-70% lebih cepat vs LLVM)
## Gunakan ini saat iterasi server-fn/backend; tidak perlu WASM hydration.
## Syarat: rustup component add rustc-codegen-cranelift --toolchain nightly
dev-fast:
	CARGO_PROFILE_DEV_CODEGEN_BACKEND=cranelift cargo +nightly run --features ssr

## Production build
build:
	cargo leptos build --release

## Check / lint
check:
	cargo check --features ssr
	cargo clippy --features ssr

clean:
	cargo clean
