use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::csr::components::{GridBackground, ErrorBanner};
use crate::csr::hooks::ThemeToggle;
use crate::csr::services::auth as auth_svc;

#[component]
pub fn ForgotPasswordPage() -> impl IntoView {
    let email = RwSignal::new(String::new());
    let loading = RwSignal::new(false);
    let sent = RwSignal::new(false);
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let focused = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let em = email.get();
        if em.trim().is_empty() {
            error.set(Some("Please enter your email address.".into())); return;
        }
        error.set(None);
        loading.set(true);
        spawn_local(async move {
            match auth_svc::request_password_reset(&em).await {
                Ok(_) => { loading.set(false); sent.set(true); }
                Err(err) => {
                    loading.set(false);
                    let msg = if err.status == 404 { "No account found with this email.".into() }
                        else if err.status == 0 { "Network error. Check your connection.".into() }
                        else { "Something went wrong. Please try again.".into() };
                    error.set(Some(msg));
                }
            }
        });
    };

    view! {
        <GridBackground />
        <main class="auth-page">
            <header class="auth-header">
                <A href="/login" attr:class="back-btn"><svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="15 18 9 12 15 6"/></svg></A>
                <span class="auth-logo">"KINETIC"</span>
                <ThemeToggle />
            </header>
            <section>
                <h1 class="hero-title">"RESET YOUR"<br/>"ACCESS"</h1>
                <p class="hero-sub">"Enter your email and we'll send you a link to reset your password."</p>
            </section>

            {move || if sent.get() {
                let em = email.get();
                view! {
                    <div class="success-card">
                        <div class="success-icon">
                            <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="#c8ff5e" stroke-width="2">
                                <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>
                            </svg>
                        </div>
                        <h2 class="success-title">"CHECK YOUR INBOX"</h2>
                        <p class="success-text">"We sent a password reset link to "<strong>{em}</strong>". Check your spam folder if you don't see it within a minute."</p>
                        <A href="/login" attr:class="back-login-btn">"BACK TO LOGIN"</A>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="auth-card">
                        <form on:submit=on_submit class="auth-form" novalidate=true>
                            <div class="field-wrap">
                                <label for="email" class="field-label">"Email Address"</label>
                                <div class=move || if focused.get() { "field-box field-box--focused" } else { "field-box" }>
                                    <input id="email" type="email" class="field-input"
                                        placeholder="you@example.com" autocomplete="email"
                                        disabled=move || loading.get()
                                        prop:value=move || email.get()
                                        on:input=move |ev| { email.set(event_target_value(&ev)); error.set(None); }
                                        on:focus=move |_| focused.set(true)
                                        on:blur=move |_| focused.set(false) />
                                </div>
                            </div>

                            {move || error.get().map(|m| view! { <ErrorBanner message=m /> })}

                            <button type="submit" disabled=move || loading.get() class="submit-btn">
                                {move || if loading.get() {
                                    view! { <span class="btn-loading"><span class="spinner"></span>"SENDING..."</span> }.into_any()
                                } else {
                                    view! {
                                        <>"SEND RESET LINK"
                                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                                            <line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/>
                                        </svg></>
                                    }.into_any()
                                }}
                            </button>
                        </form>
                        <p class="auth-prompt">"Remembered it? "<A href="/login" attr:class="auth-prompt-link">"Sign in →"</A></p>
                    </div>
                }.into_any()
            }}
        </main>
    }
}
