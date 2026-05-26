use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use crate::csr::hooks::use_nav;

use crate::csr::components::{ErrorBanner, GridBackground, KineticInput};
use crate::csr::hooks::{use_auth, ThemeToggle};
use crate::csr::models::RegisterRequest;
use crate::csr::services::auth as auth_svc;

#[component]
pub fn RegisterPage() -> impl IntoView {
    let auth = use_auth();
    let navigate = use_nav();

    let full_name = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let phone = RwSignal::new(String::new());
    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let loading = RwSignal::new(false);

    {
        let nav = navigate.clone();
        Effect::new(move |_| {
            if auth.is_authenticated() {
                nav("/", Default::default());
            }
        });
    }

    let nav_submit = navigate.clone();
    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let fname = full_name.get().trim().to_string();
        let em = email.get().trim().to_string();
        let ph = phone.get().trim().to_string();
        if fname.is_empty() || ph.is_empty() {
            error.set(Some("Nama dan nomor HP wajib diisi.".into()));
            return;
        }
        if ph.len() < 8 {
            error.set(Some("Nomor HP minimal 8 digit.".into()));
            return;
        }
        error.set(None);
        loading.set(true);
        let nav = nav_submit.clone();
        let phone_for_redirect = ph.clone();
        spawn_local(async move {
            let req = RegisterRequest {
                full_name: fname,
                email: em,
                phone: ph,
            };
            match auth_svc::register(&req).await {
                Ok(_) => {
                    loading.set(false);
                    // Backend telah mengirim OTP via WhatsApp + password.
                    // Pindah ke halaman verifikasi.
                    let target = format!(
                        "/verify-otp?phone={}",
                        urlencoding_minimal(&phone_for_redirect)
                    );
                    nav(&target, Default::default());
                }
                Err(err) => {
                    loading.set(false);
                    let msg = match err.status {
                        409 => "Nomor HP atau email sudah terdaftar.".into(),
                        400 | 422 => "Data tidak valid. Periksa kembali.".into(),
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
            <header class="auth-header">
                <A href="/login" attr:class="back-btn">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="auth-logo">"KINETIC"</span>
                <ThemeToggle />
            </header>
            <section>
                <h1 class="hero-title">"JOIN THE"<br/>"STAGE"</h1>
                <p class="hero-sub">"Daftar dengan nomor WhatsApp kamu — kode OTP & password awal akan kami kirim langsung."</p>
            </section>
            <div class="auth-card">
                <form on:submit=on_submit class="auth-form" novalidate=true>
                    <KineticInput id="fullName" label="Full Name" value=full_name
                        placeholder="Nama lengkap" autocomplete="name" disabled=loading.get() />
                    <KineticInput id="email" label="Email (opsional)" input_type="email" value=email
                        placeholder="you@example.com" autocomplete="email" disabled=loading.get() />
                    <KineticInput id="phone" label="WhatsApp Number" input_type="tel" value=phone
                        placeholder="+62 8xx-xxxx-xxxx" autocomplete="tel" disabled=loading.get() />

                    {move || error.get().map(|m| view! { <ErrorBanner message=m /> })}

                    <button type="submit" disabled=move || loading.get() class="submit-btn">
                        {move || if loading.get() {
                            view! { <span class="btn-loading"><span class="spinner"></span>"MENGIRIM OTP..."</span> }.into_any()
                        } else {
                            view! {
                                <>"KIRIM OTP"
                                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                                    <line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/>
                                </svg></>
                            }.into_any()
                        }}
                    </button>
                </form>
                <p class="auth-prompt">
                    "Sudah punya akun? "
                    <A href="/login" attr:class="auth-prompt-link">"Sign in →"</A>
                </p>
            </div>
        </main>
    }
}

/// Minimal percent-encoding for query string values (keeps `+` and digits as-is
/// while escaping unsafe characters). Avoids pulling in an extra crate.
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
