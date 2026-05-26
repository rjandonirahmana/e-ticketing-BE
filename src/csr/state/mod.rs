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

pub mod banners;
pub mod events;
pub mod notifications;
pub mod orders;
pub mod premium;
pub mod stories;
pub mod trending;
pub mod venue_events;
pub use banners::provide_banners_store;
pub use events::{provide_events_store, use_events_store};
pub use notifications::{provide_notifications_store, use_notifications_store};
pub use orders::{provide_orders_store, use_orders_store, Order};
pub use trending::provide_trending_store;
pub use venue_events::{provide_venue_events_store, use_venue_events_store};

/// Panggil sekali di `App` untuk menyediakan semua store via context.
pub fn provide_all_stores() {
    provide_banners_store();
    provide_events_store();
    provide_notifications_store();
    provide_orders_store();
    provide_trending_store();
    provide_venue_events_store();
}
pub use stories::provide_stories_store;
