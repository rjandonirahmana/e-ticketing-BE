//! navbar.rs — Navigasi utama dengan SSR auth context.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::logout_action;
use crate::web::app::AuthResource;

#[component]
pub fn Navbar() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource tidak di-provide oleh App");

    let on_logout = move |_: leptos::ev::MouseEvent| {
        leptos::task::spawn_local(async move {
            let _ = logout_action().await;
            #[cfg(target_arch = "wasm32")]
            if let Some(win) = web_sys::window() {
                let _ = win.location().replace("/");
            }
        });
    };

    view! {
        <nav class="navbar">
            <div class="container">
                <div class="navbar__inner">
                    <A href="/" attr:class="navbar__logo">
                        "PUL" <span>"SE"</span>
                    </A>

                    // Semua yang butuh auth dibungkus Suspense agar tidak
                    // terjadi hydration mismatch (resource dibaca sebelum ready)
                    <Suspense fallback=|| view! { <ul class="navbar__links"/> }>
                        {move || {
                            let logged_in = auth.get()
                                .and_then(|r| r.ok())
                                .flatten()
                                .is_some();
                            view! {
                                <ul class="navbar__links">
                                    <li><A href="/">"Beranda"</A></li>
                                    <li><A href="/explore">"Jelajahi"</A></li>
                                    {logged_in.then(|| view! {
                                        <li><A href="/tickets">"Tiket Ku"</A></li>
                                        <li><A href="/merchant">"Merchant Hub"</A></li>
                                    })}
                                </ul>
                            }
                        }}
                    </Suspense>

                    <Suspense fallback=|| view! { <div class="navbar__actions"/> }>
                        {move || {
                            let logged_in = auth.get()
                                .and_then(|r| r.ok())
                                .flatten()
                                .is_some();
                            view! {
                                <div class="navbar__actions">
                                    {if logged_in {
                                        view! {
                                            <A href="/profile" attr:class="btn btn--ghost btn--sm">"Profil"</A>
                                            <button class="btn btn--ghost btn--sm" on:click=on_logout>"Keluar"</button>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <A href="/login" attr:class="btn btn--ghost btn--sm">"Masuk"</A>
                                            <A href="/register" attr:class="btn btn--accent btn--sm">"Daftar"</A>
                                        }.into_any()
                                    }}
                                </div>
                            }
                        }}
                    </Suspense>
                </div>
            </div>
        </nav>
    }
}
