//! web/app — Root App universal (SSR + Hydration), dipecah per-tanggung-jawab.
//!
//! Sebelumnya satu file `app.rs` (~33 KB). Dipecah agar compile lebih nyaman &
//! mudah dirawat:
//!   - contexts.rs  : tipe context global (AuthResource, CartContext, dst.)
//!   - providers.rs : provide_all_app_contexts() — penyedia semua context
//!   - guards.rs    : AuthGuard / AdminGuard / MerchantGuard (UX, bukan keamanan)
//!   - router.rs    : komponen root `App` + tabel route + ScrollToTop
//!   - shell.rs     : `shell()` HTML SSR (server-only)
//!
//! Public API tetap sama: `crate::web::app::{App, shell, AuthResource,
//! CartContext, PendingOrderCtx, PendingSubCtx, SuccessSnapshot}`.

mod contexts;
mod guards;
mod providers;
mod router;
#[cfg(feature = "ssr")]
mod shell;

pub use contexts::{
    AuthResource, CartContext, PendingOrderCtx, PendingSubCtx, SuccessSnapshot,
};
pub use router::App;

#[cfg(feature = "ssr")]
pub use shell::shell;
