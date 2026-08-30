use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::web::api::{login_action, register_action};
use crate::web::app::AuthResource;
use crate::web::hooks::ThemeToggle;

// ── Login Page ─────────────────────────────────────────────────────────────────

#[component]
pub fn LoginPage() -> impl IntoView {
    let phone    = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let loading  = RwSignal::new(false);
    let error    = RwSignal::new(Option::<String>::None);
    let pass_focused = RwSignal::new(false);
    let phone_focused = RwSignal::new(false);

    // Auth resource global + navigate: pakai navigasi SPA setelah login (bukan
    // full reload) supaya cepat — lihat catatan di on_submit.
    let auth = use_context::<AuthResource>();
    let navigate = use_navigate();

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let ph = phone.get();
        let pw = password.get();
        if ph.is_empty() || pw.is_empty() {
            error.set(Some("Nomor HP dan password wajib diisi.".into()));
            return;
        }
        loading.set(true);
        error.set(None);

        let navigate = navigate.clone();
        leptos::task::spawn_local(async move {
            match login_action(ph, pw).await {
                Ok(_user) => {
                    // Cookie sesi sudah di-set oleh respons server-fn. Daripada
                    // full reload (`location.replace`) yang me-re-SSR /explore
                    // (halaman TERBERAT) + re-download & re-hydrate WASM — jauh
                    // lebih lambat — kita:
                    //   1) refetch auth resource → app langsung tahu user login,
                    //   2) navigasi SPA ke /explore (WASM sudah hydrate, render
                    //      di klien). Terasa instan.
                    if let Some(auth) = auth {
                        auth.refetch();
                    }
                    navigate("/explore", Default::default());
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                    loading.set(false);
                }
            }
        });
    };

    view! {
        <Title text="Masuk — PULSE" />
        <Meta name="description" content="Masuk ke akun PULSE kamu untuk belanja dan kelola pesananmu." />
        <main class="auth-page">
            <header class="auth-header animate-fade-up">
                <span class="kinetic-logo">"KINETIC"</span>
                <div class="header-actions">
                    <A href="/" attr:class="home-link" attr:title="Home">
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                            <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2h-5v-8H9v8H5a2 2 0 0 1-2-2z"/>
                        </svg>
                    </A>
                    <span class="header-badge">"SECURE LOGIN"</span>
                    <ThemeToggle/>
                </div>
            </header>

            <section class="animate-fade-up animate-fade-up-delay-1">
                <h1 class="hero-title">"MASUK KE"<br/>"AKUN"</h1>
                <p class="hero-sub">"Akses pesanan dan belanjaanmu."</p>
            </section>

            <div class="auth-card animate-fade-up animate-fade-up-delay-2">
                <form on:submit=on_submit class="auth-form" novalidate=true>
                    {move || error.get().map(|e| view! {
                        <div class="error-banner" role="alert">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                <circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/>
                            </svg>
                            <span>{e}</span>
                        </div>
                    })}

                    <div class="field-wrap">
                        <label for="phone" class="field-label">"Nomor HP"</label>
                        <div class=move || if phone_focused.get() { "field-box field-box--focused" } else { "field-box" }>
                            <input
                                id="phone"
                                type="tel"
                                class="field-input"
                                placeholder="+62 812 xxxx xxxx"
                                autocomplete="tel"
                                disabled=move || loading.get()
                                prop:value=phone
                                on:input=move |ev| phone.set(event_target_value(&ev))
                                on:focus=move |_| phone_focused.set(true)
                                on:blur=move |_| phone_focused.set(false)
                            />
                        </div>
                    </div>

                    <div class="field-wrap">
                        <label for="password" class="field-label">"Password"</label>
                        <div class=move || if pass_focused.get() { "field-box field-box--focused" } else { "field-box" }>
                            <input
                                id="password"
                                type="password"
                                class="field-input"
                                placeholder="••••••••"
                                autocomplete="current-password"
                                disabled=move || loading.get()
                                prop:value=password
                                on:input=move |ev| password.set(event_target_value(&ev))
                                on:focus=move |_| pass_focused.set(true)
                                on:blur=move |_| pass_focused.set(false)
                            />
                        </div>
                    </div>

                    <div class="form-row">
                        <A href="/forgot-password" attr:class="forgot-link">"Lupa password?"</A>
                    </div>

                    <button type="submit" disabled=move || loading.get() class="submit-btn">
                        {move || if loading.get() {
                            view! {
                                <span class="btn-loading">
                                    <span class="spinner"></span>
                                    "MASUK..."
                                </span>
                            }.into_any()
                        } else {
                            view! {
                                <>
                                    "MASUK"
                                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                                        <line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/>
                                    </svg>
                                </>
                            }.into_any()
                        }}
                    </button>
                </form>

                <div class="divider"><span class="divider-text">"atau"</span></div>

                <p class="auth-prompt">
                    "Belum punya akun? "
                    <A href="/register" attr:class="auth-prompt-link">"Daftar sekarang →"</A>
                </p>
            </div>

            <div class="trust-row animate-fade-up animate-fade-up-delay-3">
                <div class="trust-item">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0110 0v4"/></svg>
                    <span>"SSL Aman"</span>
                </div>
                <div class="trust-item">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                    <span>"Terverifikasi"</span>
                </div>
                <div class="trust-item">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 11.08V12a10 10 0 11-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
                    <span>"Platform Terpercaya"</span>
                </div>
            </div>
        </main>
    }
}

// ── Register Page ──────────────────────────────────────────────────────────────

#[component]
pub fn RegisterPage() -> impl IntoView {
    let name    = RwSignal::new(String::new());
    let phone   = RwSignal::new(String::new());
    let role    = RwSignal::new("customer".to_string());
    let loading = RwSignal::new(false);
    let error   = RwSignal::new(Option::<String>::None);
    let success = RwSignal::new(false);
    let name_focused  = RwSignal::new(false);
    let phone_focused = RwSignal::new(false);

    let navigate = use_navigate();

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let n  = name.get();
        let ph = phone.get();
        if n.len() < 2 || ph.is_empty() {
            error.set(Some(
                "Nama minimal 2 karakter dan nomor HP wajib diisi.".into(),
            ));
            return;
        }
        loading.set(true);
        error.set(None);

        let _nav = navigate.clone();

        leptos::task::spawn_local(async move {
            match register_action(n, ph.clone(), role.get()).await {
                Ok(_) => {
                    success.set(true);
                    loading.set(false);
                    #[cfg(target_arch = "wasm32")]
                    {
                        gloo_timers::future::TimeoutFuture::new(1200).await;
                        let target = format!("/verify-otp?phone={}", urlencoding_minimal(&ph));
                        _nav(&target, Default::default());
                    }
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                    loading.set(false);
                }
            }
        });
    };

    view! {
        <Title text="Daftar — PULSE" />
        <Meta name="description" content="Buat akun PULSE gratis dan mulai belanja product favoritmu." />
        <main class="auth-page">
            <header class="auth-header">
                <A href="/login" attr:class="back-btn">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="auth-logo">"KINETIC"</span>
                <ThemeToggle/>
            </header>

            <section>
                <h1 class="hero-title">"BUAT AKUN"<br/>"GRATIS"</h1>
                <p class="hero-sub">"Daftar dan temukan ribuan product pilihan di seluruh Indonesia."</p>
            </section>

            <div class="auth-card">
                <form on:submit=on_submit class="auth-form" novalidate=true>
                    <Show when=move || success.get()>
                        <div class="success-banner" role="status">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#39ff8a" stroke-width="2">
                                <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>
                            </svg>
                            <span>"Registrasi berhasil! Mengarahkan ke verifikasi..."</span>
                        </div>
                    </Show>

                    {move || error.get().map(|e| view! {
                        <div class="error-banner" role="alert">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                <circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/>
                            </svg>
                            <span>{e}</span>
                        </div>
                    })}

                    <div class="field-wrap">
                        <label for="name" class="field-label">"Nama Lengkap"</label>
                        <div class=move || if name_focused.get() { "field-box field-box--focused" } else { "field-box" }>
                            <input
                                id="name"
                                type="text"
                                class="field-input"
                                placeholder="John Doe"
                                autocomplete="name"
                                disabled=move || loading.get()
                                prop:value=name
                                on:input=move |ev| name.set(event_target_value(&ev))
                                on:focus=move |_| name_focused.set(true)
                                on:blur=move |_| name_focused.set(false)
                            />
                        </div>
                    </div>

                    <div class="field-wrap">
                        <label for="phone" class="field-label">"Nomor WhatsApp"</label>
                        <div class=move || if phone_focused.get() { "field-box field-box--focused" } else { "field-box" }>
                            <input
                                id="phone"
                                type="tel"
                                class="field-input"
                                placeholder="+62 812 xxxx xxxx"
                                autocomplete="tel"
                                disabled=move || loading.get()
                                prop:value=phone
                                on:input=move |ev| phone.set(event_target_value(&ev))
                                on:focus=move |_| phone_focused.set(true)
                                on:blur=move |_| phone_focused.set(false)
                            />
                        </div>
                    </div>

                    <div class="field-wrap">
                        <label for="role" class="field-label">"Daftar sebagai"</label>
                        <div class="field-box">
                            <select
                                id="role"
                                class="field-input"
                                on:change=move |ev| role.set(event_target_value(&ev))
                            >
                                <option value="customer">"Pembeli"</option>
                                <option value="merchant">"Penjual / Merchant"</option>
                            </select>
                        </div>
                    </div>

                    <button
                        type="submit"
                        disabled=move || loading.get() || success.get()
                        class="submit-btn"
                    >
                        {move || if loading.get() {
                            view! {
                                <span class="btn-loading">
                                    <span class="spinner"></span>
                                    "MENDAFTAR..."
                                </span>
                            }.into_any()
                        } else {
                            view! {
                                <>
                                    "DAFTAR SEKARANG"
                                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                                        <line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/>
                                    </svg>
                                </>
                            }.into_any()
                        }}
                    </button>
                </form>

                <div class="divider"><span class="divider-text">"atau"</span></div>

                <p class="auth-prompt">
                    "Sudah punya akun? "
                    <A href="/login" attr:class="auth-prompt-link">"Masuk →"</A>
                </p>
            </div>
        </main>
    }
}

#[allow(dead_code)]
fn urlencoding_minimal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | '+' => out.push(c),
            _ => {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).bytes() {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    out
}
