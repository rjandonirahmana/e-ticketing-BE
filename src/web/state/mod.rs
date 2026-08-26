//! Reactive state stores untuk seluruh aplikasi KINETIC.
//!
//! Setiap store dibungkus `RwSignal` dan disediakan via context di `App`.
//! Komponen halaman membaca data lewat `use_<store>()` sehingga UI otomatis
//! bereaksi ketika data dimuat ulang dari backend (gRPC/REST).
//!
//! Pola umum tiap store:
//!   - `items: RwSignal<Vec<T>>`  — data utama, awalnya berisi mock data
//!   - `loading: RwSignal<bool>`  — status loading
//!   - `load()`                    — async, memanggil service (kalau ada)
//!     dan mengganti `items` dengan respons dari backend.
//!     Di-guard dengan `if is_server() { return; }` agar aman di SSR.

pub mod banners;
pub mod products;
pub mod notifications;
pub mod orders;
pub mod premium;
pub mod stories;
pub mod trending;
pub mod venue_products;
pub use banners::provide_banners_store;
pub use products::{provide_products_store, use_products_store};
pub use notifications::{provide_notifications_store, use_notifications_store};
pub use orders::{provide_orders_store, use_orders_store, Order};
pub use stories::provide_stories_store;
pub use trending::provide_trending_store;
pub use venue_products::{provide_venue_products_store, use_venue_products_store};

/// Panggil sekali di `App` untuk menyediakan semua store via context.
///
/// Setiap store yang memanggil `load()` sudah memiliki guard `if is_server() { return; }`
/// di dalam implementasinya — aman dipanggil dari SSR `App()`.
pub fn provide_all_stores() {
    provide_banners_store();
    provide_products_store();
    provide_notifications_store();
    provide_orders_store();
    provide_stories_store();
    provide_trending_store();
    provide_venue_products_store();
}
