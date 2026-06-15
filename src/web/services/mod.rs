//! web/services — tipe data klien (pengganti `csr::services`).
//!
//! Catatan arsitektur SSR:
//!   Pengambilan data tidak lagi memakai `gloo_net` ke `/api/*` (yang kini
//!   wajib `X-App-Token` server-side). Semua fetch dialihkan ke server
//!   functions di `web::api`. Module ini hanya menyimpan tipe data yang
//!   masih dipakai komponen (mis. payload upload detail image).

pub mod event;
