//! lib.rs — WASM entry point (Unified SSR + Hydration).
//!
//! Arsitektur:
//!   - Server (SSR): `web::shell()` → HTML penuh, SEO-ready, auth blocking
//!   - Client (Hydrate): `hydrate_body(App)` → attach reaktivitas ke DOM SSR
//!   - Satu App universal = zero DOM mismatch = tidak ada FOUC swap.

// View Leptos membangun tipe nested sangat dalam (terutama ExplorePage);
// batas default 128 tidak cukup → naikkan agar type-checking tidak overflow.
#![recursion_limit = "512"]

#[cfg(feature = "hydrate")]
#[global_allocator]
static ALLOC: lol_alloc::AssumeSingleThreaded<lol_alloc::FreeListAllocator> =
    unsafe { lol_alloc::AssumeSingleThreaded::new(lol_alloc::FreeListAllocator::new()) };

// Modul web berisi App universal + server functions.
// Diperlukan untuk SSR (compile server) dan Hydration (compile WASM).
#[cfg(any(feature = "ssr", feature = "hydrate"))]
pub mod web;

// Server-only modules — tidak dikompilasi untuk WASM
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod middleware;
#[cfg(not(target_arch = "wasm32"))]
pub mod models;
#[cfg(not(target_arch = "wasm32"))]
pub mod proto;
#[cfg(not(target_arch = "wasm32"))]
pub mod repository;
#[cfg(not(target_arch = "wasm32"))]
pub mod service;
#[cfg(not(target_arch = "wasm32"))]
pub mod state;
#[cfg(not(target_arch = "wasm32"))]
pub mod utils;
#[cfg(not(target_arch = "wasm32"))]
pub mod ws;
#[cfg(not(target_arch = "wasm32"))]
pub mod api;
#[cfg(not(target_arch = "wasm32"))]
pub mod live;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use web::app::App;
    console_error_panic_hook::set_once();

    // True hydration: attach event listener & signals ke DOM yang sudah
    // di-render server. Jangan clear body — struktur DOM harus identik.
    leptos::mount::hydrate_body(App);
}
