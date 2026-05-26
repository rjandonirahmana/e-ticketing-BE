//! lib.rs — WASM entry point.
//!
//! Opsi 3 — Hybrid SSR + CSR:
//!
//!   SERVER (SSR):
//!     - Public routes (/, /explore, /events/:slug) → render penuh dengan data
//!     - Private routes → render konten berdasarkan auth cookie
//!     - SEO: meta tags, konten ter-render di HTML
//!
//!   CLIENT (WASM):
//!     - SSR DOM di-clear, CSR App asli di-mount
//!     - User mendapat UI original yang lengkap dengan semua interaktivitas
//!     - Tidak ada hydration mismatch karena CSR mount fresh

#[cfg(feature = "hydrate")]
#[global_allocator]
static ALLOC: lol_alloc::AssumeSingleThreaded<lol_alloc::FreeListAllocator> =
    unsafe { lol_alloc::AssumeSingleThreaded::new(lol_alloc::FreeListAllocator::new()) };

#[cfg(feature = "ssr")]
pub mod web;

pub mod csr;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use csr::app::App;
    console_error_panic_hook::set_once();

    // Hapus SSR shell, mount CSR App asli.
    //
    // Kenapa tidak hydrate_body?
    //   SSR (web::app::App) dan CSR (csr::app::App) berbeda struktur,
    //   sehingga true hydration akan mismatch dan error.
    //
    // Trade-off:
    //   + User mendapat UI original CSR 100% sama
    //   + Tidak ada hydration error
    //   - Ada ~50-200ms flash putih/hitam saat swap SSR→CSR
    //
    // Untuk public pages: SSR sudah kirim konten ke crawler (SEO ✓)
    // Untuk user: konten SSR terlihat sekilas, lalu CSR load
    if let Some(body) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.body())
    {
        body.set_inner_html("");
    }
    leptos::mount::mount_to_body(App);
}
