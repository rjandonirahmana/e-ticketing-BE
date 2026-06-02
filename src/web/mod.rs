//! web — Leptos SSR frontend, terintegrasi langsung dalam binary e-ticketing.
//!
//! Module ini berisi semua komponen Leptos:
//!   - app.rs      : Root App + AuthResource context
//!   - models.rs   : Data models (JSON/client-facing)
//!   - api/        : Server functions (cookie-based auth)
//!   - components/ : UI components (Navbar, EventCard)
//!   - pages/      : Semua halaman aplikasi

pub mod api;
pub mod app;
#[cfg(feature = "ssr")]
pub mod assets;
pub mod components;
pub mod models;
pub mod pages;
pub mod state;
