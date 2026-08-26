# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

PULSE — a product/ticketing platform backend in Rust. A **single Axum binary on one port** serves four things at once:

- **Leptos SSR + WASM-hydration web app** (`/*`, server functions at `/api-fn/*`)
- **REST API** consumed by a separate Next.js frontend (`/api/*`)
- **WebSocket** chat (`/ws/*`)
- **WebRTC live streaming** control plane (`/api/live/*`) backed by an in-process SFU

## Commands

All native (server) cargo commands **must** pass `--features ssr` — the default feature is `ssr`, but be explicit when overriding. The `Makefile` wraps the common ones:

```bash
make run        # cargo run — SSR only, no WASM hydration (fast; pages not interactive)
make dev        # cargo leptos watch — full dev: SSR + WASM hydration + hot reload
make dev-fast   # SSR-only with Cranelift codegen (needs nightly + rustc-codegen-cranelift)
make build      # cargo leptos build --release — production build (compiles WASM bundle too)
make check      # cargo check --features ssr && cargo clippy --features ssr
```

To type-check the **WASM/hydrate** side (the client-only WebRTC code, web_sys usage, etc.):

```bash
cargo check --target wasm32-unknown-unknown --no-default-features --features hydrate --lib
```

gRPC stubs are generated at build time by `build.rs` from `proto/auth.proto` (tonic/prost).

**DB migrations berjalan otomatis saat start** (`config/migrate.rs`), bukan lagi manual:
- Berkas `migration/*.sql` di-embed ke binari oleh `build.rs` (urut nama), jadi container yang hanya memuat binari tetap bisa bermigrasi.
- Tiap berkas dikirim **utuh** lewat `batch_execute` — PostgreSQL yang memisah pernyataannya. Ini menghapus kelas bug yang mahal: klien SQL yang memecah berkas per titik-koma tanpa memahami komentar akan membelah `CREATE TABLE` menjadi kepingan rusak, dan errornya muncul di pernyataan LAIN yang merujuk tabel yang tak pernah lahir.
- `schema_migrations` mencatat versi + checksum; `pg_advisory_lock` menahan replika lain saat rolling deploy; tiap berkas berjalan dalam transaksinya sendiri bersama pencatatannya.
- **Baseline `021_paid_at_semantics.sql`**: pada database yang sudah berisi data tapi belum punya `schema_migrations`, berkas sampai batas itu hanya DICATAT tanpa dijalankan — sebagian di antaranya (mis. `007_seed_bulk.sql`) tidak aman diulang. Database kosong menjalankan semuanya dari nol.
- `AUTO_MIGRATE=false` mematikannya. Config comes from `.env` via dotenvy — see `.env.example`.

## The cfg-gating rule (most important architectural constraint)

The crate compiles to **two targets from one source tree**: native (SSR server) and `wasm32` (browser hydration). `src/lib.rs` enforces the split:

- `web` is compiled for **both** (it holds the universal `App`, pages, components, and server functions).
- Every backend module (`config`, `middleware`, `models`, `proto`, `repository`, `service`, `state`, `utils`, `ws`, `api`, `live`) is `#[cfg(not(target_arch = "wasm32"))]`.

Consequently in **`Cargo.toml`**: anything pulling tokio/mio/axum/native-TLS must live under `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`, **never** in the shared `[dependencies]` block. Code under `web/` that calls `web_sys`/`wasm_bindgen` must still type-check on native (it does — those crates compile everywhere; they just no-op at runtime off-wasm), so gate genuinely browser-only logic with `#[cfg(target_arch = "wasm32")]`.

When changing dependencies or `web/` code, verify **both** targets compile (see commands above).

## Backend layering

Request handlers → services → repositories → Postgres, wired through a single `AppState`:

- **Handlers**: `api/*` (REST for Next.js), `web/api/*` (Leptos server functions), `ws/*` (WebSocket + chat).
- **Services** (`service/*`): business logic, generic over repository traits (e.g. `BannerService<R>`); concrete aliases like `DefaultBannerSvc` are pinned in `state.rs`.
- **Repositories** (`repository/*`): `Pg*` implementations over `deadpool-postgres` with hand-written SQL.
- **`state.rs` `AppState`**: the `Arc`-shared DI container holding every service, the JWT service, the WS manager, the live-stream service, and an in-process moka TTL cache (`PublicCache`) for hot public data (events/banners/categories).

Redis is split by logical DB: app connection uses `/1`, WebSocket uses `/2`.

`AppState` is injected **two ways** in `main.rs`, and both are in use: as an Axum `Extension` (so Leptos server functions can extract it) and via `.with_state(...)` (for the REST and live routers). Match the surrounding router when adding endpoints.

## Router assembly (`main.rs`)

Everything is merged into one app, order matters for fallthrough:
`chat_router` → CORS → `web::assets::router` (CSS) → upload → `rest_router` (`/api/*`) → `live_router` (`/api/live/*`) → Leptos SSR router (catch-all `/*` + server fns + `/pkg/*` static) → `CompressionLayer`.

`pkg_no_cache` middleware forces `no-cache` on `/pkg/*` (JS/WASM): without it, a stale cached JS bundle against a new WASM blob causes "is not a function" hydration crashes after deploys.

## Web app (Leptos, universal SSR + hydration)

One `App` component (`web/app/router.rs`) renders identically on server and client — `shell()` emits full HTML on the server, `hydrate_body(App)` (in `lib.rs`) attaches reactivity to that exact DOM (true hydration, no FOUC). Routes are guarded by `AuthGuard`/`MerchantGuard`/`AdminGuard`; shared state is set up in `provide_all_app_contexts()`.

**CSS is embedded into the binary** via `include_str!` in `web/assets.rs` (compile-time), served as one bundle at `/styles/app.css` plus individual files at `/styles/{file}`. `build.rs` reruns on `styles/` changes. Add new stylesheets to the `STYLES` table in `web/assets.rs`.

## Live streaming (`src/live/`)

A command-channel actor design around the `str0m` (Sans-I/O WebRTC) SFU:

- `LiveStreamService` (`service.rs`) spawns `SfuEngine` on a **dedicated OS thread** running a blocking UDP poll loop, plus a tokio task draining SFU events. They communicate via mpsc `SfuCommand`/`SfuEvent` (defined in `sfu.rs`).
- `api.rs` is the REST control plane (`/api/live/*`): create/stop rooms, exchange publish/subscribe SDP and ICE. Handlers return `axum::response::Response` and the router ends in `.with_state(state)`.
- Browser side: `web/pages/merchant_live.rs` is the publisher (camera → SFU), `web/components/live_stream.rs` is the viewer. These are WASM-only WebRTC and use `Action::new_local` (futures hold non-`Send` `web_sys` handles).

Note: trickle-ICE candidates from clients are parsed with `Candidate::from_sdp_string` (str0m re-exports it from the `is` crate) and fed to the peer via `rtc.add_remote_candidate`. Unparseable candidates (e.g. mDNS `*.local`) are logged and skipped — connectivity still works via the host candidate exchanged in the SDP, UDP demux, and peer-reflexive candidates discovered from incoming STUN.

## Meet — video conference (`src/meet/`)

A "zoom meet" between a merchant (host) and invited users. **Unlike `live` (SFU, one-to-many), `meet` is a P2P mesh**: the server is *signaling + admission only* (no media). Browsers connect to each other directly — best for small groups (~2–6).

- `MeetService` (`service.rs`) is pure in-memory state (no SFU thread): a `DashMap` of `MeetRoom`s, each holding `Peer`s with an `mpsc::UnboundedSender` to that peer's WS task.
- `api.rs`: `POST /api/meet/rooms` (auth merchant/admin → create), `GET /api/meet/rooms/{id}` (public info), and `GET /ws/meet/{room_id}` (public WS). The WS is public so invited guests can connect without login; **host identity is verified inside the handler** via the `pulse_token` cookie JWT (role + `user_id == host_id`).
- Admission (waiting room): guests land in a pending list; only the host connection may send `admit`/`deny` (enforced server-side). Signaling relay (`signal`) is restricted to admitted peers. Anti-glare: the **newly admitted peer initiates** offers to existing peers. Mic/camera on-off is broadcast via a `state` → `peer_state` message so tiles show muted/avatar indicators.
- Browser side: `web/pages/meet.rs` (`/meet/:id`), Google-Meet-style flow: **green room** (camera preview + mic/cam toggle + name before joining) → waiting → in-meet grid with a bottom **control bar** (mic, camera, host people-panel, leave). Route `/meet/host` = create+host (merchant "MEET" button on `/merchant`); `/meet/{room_id}` = guest invite link. WASM mesh manages one `RtcPeerConnection` per peer; remote `<video>` tiles are created imperatively in the DOM (reliable for binding dynamic `MediaStream`s; avatar/mic indicators toggled via class), the self tile + controls are reactive Leptos. STUN+TURN via `/api/rtc/ice` (`web/rtc.rs`).

## Auth: access token JWT + refresh token opaque

Sejak migrasi 025, keduanya adalah benda yang berbeda — dan itu poin utamanya.

| | Access token | Refresh token |
|---|---|---|
| Bentuk | JWT bertanda tangan | 32 byte acak (opaque) |
| Disimpan server | tidak (stateless) | ya, sebagai SHA-256 di `refresh_tokens` |
| Umur | `JWT_EXPIRY_HOURS` | 30 hari |
| Dicabut | tidak bisa | bisa, per token atau per keluarga |

**Kenapa opaque, bukan JWT ber-`token_type`.** Karena bentuknya berbeda sama sekali, memakai access token sebagai refresh token menjadi mustahil secara struktur — bukan sekadar dilarang oleh satu pemeriksaan yang bisa lupa ditulis. Versi sebelumnya mengembalikan token yang SAMA di kedua field dan endpoint refresh menerima apa pun yang lolos verifikasi JWT.

**Rotasi & deteksi pemakaian ulang** (`service/refresh.rs`): tiap refresh mencabut token lama dan menerbitkan yang baru dalam `family_id` yang sama. Token yang sudah dicabut muncul lagi = salinannya ada di dua tangan → **seluruh keluarga dicabut**. Pemilik sah ikut kerepotan sekali, dan itu disengaja: alternatifnya pencuri memegang rantai yang bisa diperpanjang tanpa batas.

Balapan dua refresh serentak diputus oleh `WHERE revoked_at IS NULL` pada UPDATE pencabutan — hanya satu yang mendapat baris.

**Peran diambil ulang dari database saat refresh**, tidak disalin dari token lama. Ini yang membatasi umur hak akses yang sudah dicabut. Perlu diketahui: dengan `JWT_EXPIRY_HOURS` masih 24, peran basi tetap bisa bertahan sampai 24 jam. Menurunkannya ke 15–30 menit kini AMAN untuk klien REST (mereka punya refresh), tetapi **web Leptos belum punya refresh cookie** — jalur cookie masih mengandalkan umur access token, jadi menurunkannya akan mengeluarkan pengguna web tiap 30 menit. Itu pekerjaan berikutnya.

Baris yang dicabut sengaja disimpan sampai kedaluwarsa (deteksi pemakaian ulang butuh barisnya ada); tugas latar harian di `main.rs` membuang yang sudah lewat.

## Styling: CSS lama + Tailwind, berdampingan

Dua sistem hidup bersamaan selama migrasi bertahap:

- `styles/parts/*.css` → digabung `build.rs` → di-embed binari → `/styles/app.css`. Memegang halaman yang belum dipindahkan.
- `style/tailwind.css` → di-compile cargo-leptos (`tailwind-input-file`) → `/pkg/e-ticketing.css`. Tailwind **v4, dikonfigurasi lewat CSS** — tak ada `tailwind.config.js`.

Empat hal yang harus dijaga saat menyentuh styling:

1. **Warna & font memakai token, bukan hex.** Blok `@theme` memetakan `--color-brand: var(--color-primary)`, `--font-title: var(--font-display)`, dst. Utility karena itu ikut berubah saat tema terang/gelap diganti, tanpa satu pun varian `dark:`. Nama Tailwind sengaja BERBEDA dari nama token aplikasi (`brand` bukan `primary`) — nama yang sama menghasilkan `var()` melingkar dan properti itu batal diam-diam.
2. **Preflight tidak diimpor** (hanya `theme.css` + `utilities.css`). Reset Tailwind akan menggeser jarak di seluruh CSS lama yang ditulis tanpanya. Konsekuensi: `border-2` tak memberi `border-style`, jadi tulis `border border-solid`.
3. **Kelas harus literal di markup.** `@source "../src/**/*.rs"` memindai teks apa adanya. Kelas yang dirakit (`format!("bg-{…}")`) lolos purge dan gayanya hilang senyap di produksi. Untuk kelas kondisional, tulis dua rangkaian LENGKAP lalu pilih salah satu (lihat `cart_row` di `web/pages/cart.rs`).
4. **`/pkg/*.css` hanya dihasilkan `cargo leptos build|watch`.** `make run` biasa tidak membuatnya.

**Status migrasi:** `web/pages/cart.rs` sudah pindah (42 kelas eksklusifnya dihapus dari CSS). Kelas yang dipakai bersama banyak halaman — `page`, `page-header`, `back-btn`, `page-logo`, `header-actions`, `shim`, `item-*` — sengaja DIBIARKAN sebagai CSS: memindahkannya berarti menyentuh 25-58 berkas sekaligus, yang bukan lagi bertahap.

## Penamaan domain: products, bukan events

Migrasi 023 me-rename `events` → `products` dan `event_variants` → `product_variants`, beserta seluruh identifier Rust (`Product`, `ProductVariant`, `ProductService`, `PgProductRepository`, …), nama modul/berkas (`models/products.rs`, `repository/product/`, `web/pages/product_detail.rs`, …), dan URL publik (`/products/:slug`, `/api/products*`).

**Nama KOLOM sengaja tidak ikut di-rename** — `product_variants.event_id`, `products.event_date`, `banners.event_id`, `group_rooms.event_id` tetap seperti semula, dan field Rust yang memetakannya juga. Alasannya operasional: `ALTER TABLE IF EXISTS … RENAME TO` aman dijalankan ulang, sedangkan `RENAME COLUMN` tak punya padanan `IF EXISTS` dan akan menggagalkan seluruh berkas pada percobaan kedua. Di database ini yang riwayat migrasinya sering berhenti separuh jalan, itu risiko yang tak sebanding.

Identifier browser (`MouseEvent`, `StorageEvent`, `str0m::Event`, `add_event_listener_*`, `prevent_default`) jelas TIDAK termasuk rename — kalau menyunting massal lagi, kecualikan mereka.

## Keranjang, order, pembayaran (DB-backed)

Keranjang **tidak lagi hidup di `localStorage`**. Sejak `migration/022_cart_payment.sql`:

- `carts` (satu baris aktif per user, dijaga unique index parsial `WHERE deleted_at IS NULL`) + `cart_items` yang hanya menyimpan varian, jumlah, dan **harga saat dimasukkan**. Nama event/varian, venue, dan cover TIDAK disalin — di-JOIN hidup dari `events`/`event_variants` tiap pembacaan, jadi mustahil basi. Harga satu-satunya pengecualian karena justru perbedaannya yang bermakna: harga yang MENGIKAT dihitung ulang di dalam transaksi order, dan selisihnya menandai "harga berubah sejak Anda menambahkan".
- **`order_items` DIHAPUS** (migrasi 023). Baris pesanan kini tinggal di `cart_items`, dihubungkan lewat `orders.cart_id`, dan `tickets.cart_item_id` menggantikan `order_item_id`. Konsekuensinya yang harus dijaga:
  - `cart_items.unit_price` berganti arti saat keranjang ditutup: selama `carts.deleted_at IS NULL` ia "harga yang dilihat pembeli", sesudahnya "harga yang ditagihkan". Transaksi order menimpanya dengan harga yang baru dikunci (`OrderTx::freeze_cart_items`) lalu menutup keranjang — keduanya di dalam transaksi yang sama.
  - FK `cart_items → product_variants` menjadi `ON DELETE RESTRICT` (dulu CASCADE), karena tabel itu kini juga memuat pesanan berbayar. Agar merchant tetap bisa menghapus varian yang belum laku, `exec_delete_variant` lebih dulu melepasnya dari keranjang yang masih terbuka (`DETACH_VARIANT_FROM_OPEN_CARTS`).
  - Jalur beli-langsung (`POST /api/orders`) tak punya keranjang, jadi transaksi order membuatkan **keranjang sekali-pakai yang lahir sudah tertutup** (`STMT_INSERT_CLOSED_CART`) — tidak menyerempet unique index "satu keranjang aktif per user".
- `payment_methods` — kanal + biayanya sebagai DATA (`charge` tetap + `charge_percent`, `min/max_amount`, `allow_promo`, `is_instant`, `va_prefix`, `instruction`). Menambah kanal = satu baris INSERT, bukan deploy.
- `promos` + `promo_redemptions` — kuota global (`quota_total`/`quota_used`) dan batas per user (`per_user_limit`).
- `orders` bertambah `cart_id`, `subtotal_amount`, `discount_amount`, `promo_code`, `payment_vendor/code/charge`, `payment_expired_at`, `payment_reference`, `link_pay`. Invarian: `total_amount = subtotal_amount − discount_amount + payment_charge`.

Alur & lapisannya (`repository/cart.rs`, `repository/payment.rs` → `service/cart.rs`, `service/payment.rs` → `service/order/checkout.rs`):

- `CartService::view()` **menulis**, bukan sekadar membaca: barang yang tak bisa dibeli lagi (varian nonaktif, event tutup, stok habis, acara lewat) dibuang dan alasannya dikembalikan lewat `CartView.notif` — mengikuti `GET /cart/view` kiddoapi. Jumlah yang melebihi stok TIDAK dipotong diam-diam; barisnya ditandai `exceeds_stock` dan halaman mengunci tombol bayar.
- `OrderService::checkout()` menerima **hanya** `payment_code` (+ promo opsional + idempotency key). Isi keranjang, harga, potongan, dan biaya kanal dihitung server. Kuota promo dipesan sebelum order dibuat dan dikembalikan bila order gagal lahir. Order dibuat lewat `create_inner` yang sama dengan jalur lain, jadi penguncian varian + penjaga oversell + retry + idempotensi berlaku identik.
- Kanal `is_instant` (dan order nol rupiah) langsung dibayar sehingga tiket terbit tanpa langkah tambahan; sisanya lahir `pending` dengan nomor VA / referensi QRIS deterministik dari `order_code`.

Endpoint REST sepadan dengan kiddoapi (`api/cart.rs`): `/api/cart/{view,count,create,add,quantity,item/:variant_id,clear,promo,payment}`, `/api/payments`, `/api/checkout`, `/api/orders/:id/{pay,cancel}`. Server function-nya di `web/api/server_fns/cart.rs` dan `checkout.rs`; halaman memakai `CartContext` (`web/app/contexts.rs`) yang **local-first**: perubahan diterapkan optimistis lalu dikoreksi jawaban server. Tamu tetap memakai `localStorage`, dan `bootstrap()` menuangnya ke keranjang server sekali saat login.

## External integrations

Telegram (error-alert notifier, `utils::error::init_telegram_notifier`), WAHA (WhatsApp), RustFS (S3-compatible object storage for uploads/stories), and an `auth.proto` gRPC service. All are configured via env vars in `AppConfig::from_env()`.
