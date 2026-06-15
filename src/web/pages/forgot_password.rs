use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::hooks::ThemeToggle;

#[component]
pub fn ForgotPasswordPage() -> impl IntoView {
    let email   = RwSignal::new(String::new());
    let loading = RwSignal::new(false);
    let sent    = RwSignal::new(false);
    let error   = RwSignal::new(Option::<String>::None);
    let focused = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let em = email.get();
        if em.trim().is_empty() {
            error.set(Some("Masukkan alamat email kamu.".into()));
            return;
        }
        error.set(None);
        loading.set(true);
        loading.set(false);
        error.set(Some("Reset password belum tersedia. Hubungi support kami.".into()));
    };

    view! {
        <div class="grid-bg">
            <div class="grid-lines"></div>
            <div class="orb orb-1"></div>
            <div class="orb orb-2"></div>
        </div>
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
                <h1 class="hero-title">"RESET"<br/>"PASSWORD"</h1>
                <p class="hero-sub">"Masukkan email kamu untuk reset password."</p>
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
                        <h2 class="success-title">"CEK EMAIL KAMU"</h2>
                        <p class="success-text">"Link reset password sudah dikirim ke "<strong>{em}</strong>". Cek folder spam jika tidak ada."</p>
                        <A href="/login" attr:class="back-login-btn">"KEMBALI KE LOGIN"</A>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="auth-card">
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
                                <label for="email" class="field-label">"Alamat Email"</label>
                                <div class=move || if focused.get() { "field-box field-box--focused" } else { "field-box" }>
                                    <input
                                        id="email"
                                        type="email"
                                        class="field-input"
                                        placeholder="kamu@example.com"
                                        autocomplete="email"
                                        disabled=move || loading.get()
                                        prop:value=move || email.get()
                                        on:input=move |ev| { email.set(event_target_value(&ev)); error.set(None); }
                                        on:focus=move |_| focused.set(true)
                                        on:blur=move |_| focused.set(false)
                                    />
                                </div>
                            </div>

                            <button type="submit" disabled=move || loading.get() class="submit-btn">
                                {move || if loading.get() {
                                    view! {
                                        <span class="btn-loading">
                                            <span class="spinner"></span>
                                            "MENGIRIM..."
                                        </span>
                                    }.into_any()
                                } else {
                                    view! {
                                        <>
                                            "KIRIM LINK RESET"
                                            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                                                <line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/>
                                            </svg>
                                        </>
                                    }.into_any()
                                }}
                            </button>
                        </form>
                        <p class="auth-prompt">
                            "Ingat password? "
                            <A href="/login" attr:class="auth-prompt-link">"Masuk →"</A>
                        </p>
                    </div>
                }.into_any()
            }}
        </main>
    }
}
