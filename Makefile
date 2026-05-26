.PHONY: run dev build clean

## cargo run biasa — SSR + API tanpa WASM hydration
## Halaman di-render server-side, tapi form/filter tidak interaktif di browser
run:
	cargo run

## Full dev dengan WASM hydration (hot reload)
## Ini pengganti `cargo run` yang paling direkomendasikan
dev:
	cargo leptos watch

## Production build
build:
	cargo leptos build --release

## Check / lint
check:
	cargo check --features ssr
	cargo clippy --features ssr

clean:
	cargo clean
