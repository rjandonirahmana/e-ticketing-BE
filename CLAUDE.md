# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

PULSE — an event ticketing platform backend in Rust. A **single Axum binary on one port** serves four things at once:

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

There is no test suite in this repo. gRPC stubs are generated at build time by `build.rs` from `proto/auth.proto` (tonic/prost). DB migrations are raw SQL in `migration/` applied manually (no runner). Config comes from `.env` via dotenvy — see `.env.example`.

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
- Admission (waiting room): guests land in a pending list; only the host connection may send `admit`/`deny` (enforced server-side). Signaling relay (`signal`) is restricted to admitted peers. Anti-glare: the **newly admitted peer initiates** offers to existing peers.
- Browser side: `web/pages/meet.rs` (`/meet/:id`). Route `/meet/host` = create+host (merchant "MEET" button on `/merchant`); `/meet/{room_id}` = guest invite link. WASM mesh manages one `RtcPeerConnection` per peer; remote `<video>` tiles are created imperatively in the DOM (reliable for binding dynamic `MediaStream`s), reactive Leptos drives the waiting-room UI. STUN-only like `live` (TURN needed for cross-NAT production).

## External integrations

Telegram (error-alert notifier, `utils::error::init_telegram_notifier`), WAHA (WhatsApp), RustFS (S3-compatible object storage for uploads/stories), and an `auth.proto` gRPC service. All are configured via env vars in `AppConfig::from_env()`.
