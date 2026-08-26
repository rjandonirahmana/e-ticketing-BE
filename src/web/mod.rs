//! web — Leptos SSR frontend, terintegrasi langsung dalam binary e-ticketing.
//!
//! Module ini berisi semua komponen Leptos:
//!   - app.rs      : Root App + AuthResource context
//!   - models.rs   : Data models (JSON/client-facing)
//!   - api/        : Server functions (cookie-based auth)
//!   - components/ : UI components (Navbar, ProductCard)
//!   - pages/      : Semua halaman aplikasi

pub mod api;
pub mod app;
#[cfg(feature = "ssr")]
pub mod assets;
pub mod behavior;
pub mod components;
pub mod hooks;
pub mod models;
pub mod pages;
pub mod rtc;
pub mod seo;
pub mod services;
pub mod state;
pub mod utils;
