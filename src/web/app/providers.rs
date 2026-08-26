//! web/app/providers.rs — Penyedia SEMUA context global aplikasi.
//!
//! `provide_all_app_contexts()` dipanggil di root `App` (router.rs) sehingga
//! berjalan baik di SSR maupun setelah hydration. Harus sinkron dengan CSR
//! `init_app_providers()` — setiap provider baru tambahkan di sini juga.

use leptos::prelude::*;

use crate::web::api::get_session;

use super::contexts::{AuthResource, CartContext, PendingOrderCtx, PendingSubCtx};

/// Sediakan SEMUA context yang dibutuhkan App, baik di SSR maupun client.
///
/// Perbedaan SSR vs CSR:
///   - Auth   : SSR pakai `Resource::new_blocking` (tunggu cookie); CSR pakai server fn async.
///   - Cart   : SSR kosong; CSR memuat dari server (sudah masuk) atau
///              localStorage (tamu) begitu resource auth selesai.
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

    // ── Cart ────────────────────────────────────────────────────────────────
    // Kosong di SSR (server tak tahu keranjang siapa sebelum cookie dibaca), lalu
    // diisi setelah hydration: dari server bila sudah masuk, dari localStorage
    // bila masih tamu. Efek di bawah menunggu resource auth selesai supaya
    // keputusan itu tidak diambil dua kali.
    let cart = CartContext::new();
    provide_context(cart);

    #[cfg(target_arch = "wasm32")]
    {
        // Dijalankan ulang setiap kali status auth berubah (masuk / keluar).
        // `bootstrap()` menuang keranjang tamu ke keranjang milik user sekali,
        // lalu membersihkan localStorage — lihat komentarnya di contexts.rs.
        let mut was_authed: Option<bool> = None;
        Effect::new(move |_| {
            let Some(Ok(session)) = auth.get() else {
                return;
            };
            let now_authed = session.is_some();
            if was_authed == Some(now_authed) {
                return;
            }
            was_authed = Some(now_authed);

            if now_authed {
                cart.bootstrap();
            } else {
                cart.authed.set(false);
                cart.load_local();
            }
        });

        // Kegagalan keranjang muncul sebagai toast, di halaman mana pun.
        // Sebelumnya `CartContext.error` hanya terisi tanpa pernah dibaca, jadi
        // penolakan server (mis. stok habis saat menambah) tak terlihat sama
        // sekali dan barangnya seolah hilang begitu saja.
        {
            let toast = crate::web::components::toast::use_toast();
            let mut terakhir = String::new();
            Effect::new(move |_| {
                let pesan = cart.error.get();
                if pesan.is_empty() || pesan == terakhir {
                    return;
                }
                terakhir = pesan.clone();
                toast.error(&pesan);
            });
        }

        // Sinkronisasi antar-tab untuk keranjang TAMU. Pengguna yang sudah masuk
        // tak memakai localStorage sebagai sumber kebenaran, jadi peristiwa ini
        // sengaja diabaikan bagi mereka — kalau tidak, satu tab bisa menimpa
        // keranjang server dengan sisa data tamu yang basi.
        if let Some(win) = web_sys::window() {
            let cb = wasm_bindgen::closure::Closure::<dyn Fn(web_sys::StorageEvent)>::new(
                move |e: web_sys::StorageEvent| {
                    if e.key().as_deref() == Some(super::contexts::CART_KEY)
                        && !cart.authed.get_untracked()
                    {
                        cart.load_local();
                    }
                },
            );
            use wasm_bindgen::JsCast;
            let _ = win.add_event_listener_with_callback("storage", cb.as_ref().unchecked_ref());
            cb.forget();
        }
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
