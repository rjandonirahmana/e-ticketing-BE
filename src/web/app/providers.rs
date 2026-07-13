//! web/app/providers.rs — Penyedia SEMUA context global aplikasi.
//!
//! `provide_all_app_contexts()` dipanggil di root `App` (router.rs) sehingga
//! berjalan baik di SSR maupun setelah hydration. Harus sinkron dengan CSR
//! `init_app_providers()` — setiap provider baru tambahkan di sini juga.

use leptos::prelude::*;

use crate::web::api::get_session;
use crate::web::models::CartItem;

use super::contexts::{AuthResource, CartContext, PendingOrderCtx, PendingSubCtx};

/// Sediakan SEMUA context yang dibutuhkan App, baik di SSR maupun client.
///
/// Perbedaan SSR vs CSR:
///   - Auth   : SSR pakai `Resource::new_blocking` (tunggu cookie); CSR pakai server fn async.
///   - Cart   : SSR pakai `CartContext` kosong (tidak perlu fetch); CSR sama.
///   - Stores : SSR panggil provide_*_store() yang sudah di-guard `if is_server()` di load().
///   - Theme  : Sama persis — `provide_theme()` aman di SSR (web_sys::window() → None).
///   - Premium: Sama persis — `provide_premium_store()` tidak ada spawn_local di provide.
///   - PaySuc : Sama persis — hanya RwSignal kosong.
pub(crate) fn provide_all_app_contexts() {
    // ── Auth ────────────────────────────────────────────────────────────────
    // new_blocking di SEMUA target: SSR blocks render; client baca serialized
    // state dari HTML → langsung resolved, tidak ada Suspense fallback flash.
    let auth: AuthResource = Resource::new_blocking(|| (), |_| get_session());
    provide_context(auth);

    // ── UI / theming ────────────────────────────────────────────────────────
    // provide_theme() aman di SSR: web_sys::window() → None, Effect::new aman.
    crate::web::hooks::provide_theme();

    // ── Toast global (notifikasi UI) ────────────────────────────────────────
    // Daftar toast kosong di SSR (toast hanya ditambah di client) → aman.
    crate::web::components::toast::provide_toast();

    // ── Cart: init dari localStorage di client, kosong di SSR ──────────────
    let initial_cart: Vec<CartItem> = {
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .and_then(|w| w.local_storage().ok())
                .flatten()
                .and_then(|s| s.get_item("pulse_cart").ok())
                .flatten()
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            vec![]
        }
    };
    let cart_signal = RwSignal::new(initial_cart);
    provide_context(CartContext { items: cart_signal });

    // Cross-tab sync: storage event fires in OTHER tabs when localStorage changes.
    #[cfg(target_arch = "wasm32")]
    if let Some(win) = web_sys::window() {
        let cb = wasm_bindgen::closure::Closure::<dyn Fn(web_sys::StorageEvent)>::new(
            move |e: web_sys::StorageEvent| {
                if e.key().as_deref() == Some("pulse_cart") {
                    let new_items = e
                        .new_value()
                        .and_then(|json| serde_json::from_str::<Vec<CartItem>>(&json).ok())
                        .unwrap_or_default();
                    cart_signal.set(new_items);
                }
            },
        );
        use wasm_bindgen::JsCast;
        let _ = win.add_event_listener_with_callback("storage", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    // ── PendingOrderCtx (dipakai checkout / order_created / payment_success) ──
    provide_context(PendingOrderCtx {
        pending_order: RwSignal::new(None),
        success_order: RwSignal::new(None),
    });

    // ── PendingSubCtx (subscription → subscription/checkout) ──────────────
    provide_context(PendingSubCtx {
        order: RwSignal::new(None),
    });

    // ── Data stores (web) ───────────────────────────────────────────────────
    // Setiap store sudah di-guard `if is_server() { return; }` di load().
    crate::web::state::provide_all_stores();

    // ── Premium subscription status ─────────────────────────────────────────
    // provide_premium_store() hanya setup signal — tidak ada spawn_local.
    crate::web::state::premium::provide_premium_store();
}
