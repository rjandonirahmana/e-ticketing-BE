//! csr — Leptos CSR (Client-Side Rendering) frontend.
//!
//! Modul ini berisi seluruh kode frontend yang berjalan di browser sebagai
//! WebAssembly. Diaktifkan oleh `src/lib.rs` saat feature `hydrate` aktif.
//!
//! Struktur:
//!   - app.rs        : Root App component + ProtectedRoute + router
//!   - components/   : UI components reusable (audio, story viewer, dll)
//!   - hooks/        : Custom hooks (auth, cart, theme, nav)
//!   - models/       : Data model frontend-facing (DTO)
//!   - pages/        : Semua halaman (27+ pages)
//!   - services/     : HTTP client + API services per domain
//!   - state/        : Reactive state stores (RwSignal-based)
//!   - utils/        : Utilitas bersama (format IDR, dll)

pub mod app;
pub mod components;
pub mod hooks;
pub mod models;
pub mod pages;
pub mod services;
pub mod state;
pub mod utils;
