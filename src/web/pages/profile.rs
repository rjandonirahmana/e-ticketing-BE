//! profile.rs — Halaman Profil dengan SSR auth context.
//!
//! Menggunakan AuthResource dari context — tidak ada localStorage.
//! Jika user belum login, tampilkan redirect ke /login.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::logout_action;
use crate::web::app::AuthResource;

#[component]
pub fn ProfilePage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");

    let on_logout = move |_: web_sys::MouseEvent| {
        leptos::task::spawn_local(async move {
            let _ = logout_action().await;
            #[cfg(target_arch = "wasm32")]
            {
                if let Some(win) = web_sys::window() {
                    let _ = win.location().replace("/");
                }
            }
        });
    };

    view! {
        <Suspense fallback=|| view! {
            <div class="loading">
                <div class="loading__spinner"/>
                <span>"Memuat profil..."</span>
            </div>
        }>
            {move || auth.get().map(|res| {
                match res.ok().flatten() {
                    None => view! {
                        // Belum login — redirect ke /login
                        <div class="container" style="padding:4rem 0;text-align:center">
                            <p style="color:var(--clr-muted);margin-bottom:1.5rem">
                                "Kamu harus masuk untuk melihat profil."
                            </p>
                            <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                        </div>
                    }.into_any(),

                    Some(u) => {
                        let initial  = u.name.chars().next().unwrap_or('P').to_uppercase().to_string();
                        let name     = u.name.clone();
                        let phone    = u.phone.clone();
                        let email    = u.email.clone().unwrap_or_else(|| "-".into());
                        let role     = u.role.clone();
                        let role_label = match role.as_str() {
                            "merchant" => "Merchant / EO",
                            "admin"    => "Administrator",
                            _          => "Pembeli",
                        };
                        let is_merchant = role == "merchant";

                        view! {
                            <div class="page-header">
                                <div class="container">
                                    <p class="page-header__eyebrow">"// akun kamu"</p>
                                    <h1 class="page-header__title">"Profil"</h1>
                                    <p class="page-header__sub">"Kelola informasi dan preferensi akunmu"</p>
                                </div>
                            </div>

                            <div class="container" style="padding-bottom:4rem;max-width:720px">
                                <div class="profile-section">
                                    <div class="profile-section__avatar">{initial}</div>
                                    <div class="profile-section__name">{name}</div>
                                    <div class="profile-section__phone">{phone.clone()}</div>
                                </div>

                                <div class="info-card">
                                    <div class="info-card__title">"Informasi Akun"</div>
                                    <div class="info-card__row">
                                        <span class="info-card__label">"Email"</span>
                                        <span class="info-card__value">{email}</span>
                                    </div>
                                    <div class="info-card__row">
                                        <span class="info-card__label">"Nomor HP"</span>
                                        <span class="info-card__value mono">{phone}</span>
                                    </div>
                                    <div class="info-card__row">
                                        <span class="info-card__label">"Peran"</span>
                                        <span class="badge badge--accent">{role_label}</span>
                                    </div>
                                </div>

                                <div style="margin-top:1.5rem;display:flex;gap:1rem;flex-wrap:wrap">
                                    <a href="/tickets" class="btn btn--ghost">"🎟 Tiket Ku"</a>
                                    <Show when=move || is_merchant>
                                        <a href="/merchant" class="btn btn--ghost">
                                            "🏪 Merchant Hub"
                                        </a>
                                    </Show>
                                    <button
                                        class="btn btn--ghost"
                                        on:click=on_logout
                                        style="color:var(--clr-error);border-color:var(--clr-error)"
                                    >
                                        "Keluar"
                                    </button>
                                </div>
                            </div>
                        }.into_any()
                    }
                }
            })}
        </Suspense>
    }
}
