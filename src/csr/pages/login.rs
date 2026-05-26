use crate::csr::hooks::use_nav;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::csr::components::{ErrorBanner, GridBackground, KineticInput};
use crate::csr::hooks::{use_auth, ThemeToggle};

#[component]
pub fn LoginPage() -> impl IntoView {
    let auth = use_auth();
    let navigate = use_nav();
    let query = use_query_map();

    let phone = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let loading = RwSignal::new(false);
    let just_registered =
        Memo::new(move |_| query.with(|q| q.get("registered").map(|v| v == "1").unwrap_or(false)));

    // Redirect if already authenticated
    {
        let nav = navigate.clone();
        Effect::new(move |_| {
            if auth.is_authenticated() {
                nav("/", Default::default());
            }
        });
    }

    let nav_for_submit = navigate.clone();
    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let ph = phone.get().trim().to_string();
        let p = password.get();
        if ph.is_empty() || p.is_empty() {
            error.set(Some("Nomor HP dan password wajib diisi.".into()));
            return;
        }
        error.set(None);
        loading.set(true);
        let nav = nav_for_submit.clone();
        spawn_local(async move {
            match auth.login(ph, p).await {
                Ok(()) => {
                    loading.set(false);
                    nav("/", Default::default());
                }
                Err(err) => {
                    loading.set(false);
                    let msg = match err.status {
                        401 => "Nomor HP atau password salah.".into(),
                        404 => "Akun dengan nomor ini tidak ditemukan.".into(),
                        400 | 422 => "Nomor HP tidak valid.".into(),
                        0 => "Jaringan bermasalah. Periksa koneksi.".into(),
                        _ => format!("Terjadi kesalahan. ({})", err.status),
                    };
                    error.set(Some(msg));
                }
            }
        });
    };

    view! {
        <GridBackground />
        <main class="auth-page">
            <header class="auth-header animate-fade-up">
                <span class="kinetic-logo">"KINETIC"</span>
                <div class="header-actions">
                    // Home button
                    <A href="/" attr:class="home-link" attr:title="Home">
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                            <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2h-5v-8H9v8H5a2 2 0 0 1-2-2z"/>
                        </svg>
                    </A>
                    <span class="header-badge">"SECURE LOGIN"</span>
                    <ThemeToggle />
                </div>
            </header>

            <section class="animate-fade-up animate-fade-up-delay-1">
                <h1 class="hero-title">"ENTER THE"<br/>"STAGE"</h1>
                <p class="hero-sub">"Access your tickets, bookings, and live experiences."</p>
            </section>

            <div class="auth-card animate-fade-up animate-fade-up-delay-2">
                <form on:submit=on_submit class="auth-form" novalidate=true>
                    {move || just_registered.get().then(|| view! {
                        <div class="success-banner" role="status">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#39ff8a" stroke-width="2">
                                <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>
                            </svg>
                            <span>"Account created! Sign in to continue."</span>
                        </div>
                    })}

                    <KineticInput id="phone" label="Phone Number" input_type="tel" value=phone
                        placeholder="+62 8xx-xxxx-xxxx" autocomplete="tel" disabled=loading.get() />
                    <KineticInput id="password" label="Password" input_type="password" value=password
                        placeholder="••••••••" autocomplete="current-password" disabled=loading.get() />

                    <div class="form-row">
                        <A href="/forgot-password" attr:class="forgot-link">"Forgot password?"</A>
                    </div>

                    {move || error.get().map(|m| view! { <ErrorBanner message=m /> })}

                    <button type="submit" disabled=move || loading.get() class="submit-btn">
                        {move || if loading.get() {
                            view! { <span class="btn-loading"><span class="spinner"></span>"AUTHENTICATING..."</span> }.into_any()
                        } else {
                            view! {
                                <>
                                    "ACCESS YOUR TICKETS"
                                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                                        <line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/>
                                    </svg>
                                </>
                            }.into_any()
                        }}
                    </button>
                </form>

                <div class="divider"><span class="divider-text">"or continue with"</span></div>

                <div class="social-row">
                    <button class="social-btn" type="button">
                        <svg width="20" height="20" viewBox="0 0 24 24">
                            <path d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" fill="#4285F4"/>
                            <path d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" fill="#34A853"/>
                            <path d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" fill="#FBBC05"/>
                            <path d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z" fill="#EA4335"/>
                        </svg>
                        "Google"
                    </button>
                    <button class="social-btn" type="button">
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="#1877F2"><path d="M24 12.073c0-6.627-5.373-12-12-12s-12 5.373-12 12c0 5.99 4.388 10.954 10.125 11.854v-8.385H7.078v-3.47h3.047V9.43c0-3.007 1.792-4.669 4.533-4.669 1.312 0 2.686.235 2.686.235v2.953H15.83c-1.491 0-1.956.925-1.956 1.874v2.25h3.328l-.532 3.47h-2.796v8.385C19.612 23.027 24 18.062 24 12.073z"/></svg>
                        "Facebook"
                    </button>
                </div>

                <p class="auth-prompt">
                    "New to Kinetic? "
                    <A href="/register" attr:class="auth-prompt-link">"Create an account →"</A>
                </p>
            </div>

            <div class="trust-row animate-fade-up animate-fade-up-delay-3">
                <div class="trust-item">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0110 0v4"/></svg>
                    <span>"256-bit SSL"</span>
                </div>
                <div class="trust-item">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/></svg>
                    <span>"Secure REST"</span>
                </div>
                <div class="trust-item">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M22 11.08V12a10 10 0 11-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>
                    <span>"Verified Platform"</span>
                </div>
            </div>
        </main>
    }
}
