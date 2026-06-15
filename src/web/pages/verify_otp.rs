use leptos::html::Input;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::web::api::{verify_otp_action, resend_otp_action};
use crate::web::hooks::ThemeToggle;

const OTP_LEN: usize = 6;
const EXPIRY_SECS: i32 = 5 * 60;

#[component]
pub fn VerifyOtpPage() -> impl IntoView {
    let query = use_query_map();
    let phone = Memo::new(move |_| {
        query.read().get("phone").unwrap_or_default().to_string()
    });

    let digits: [RwSignal<String>; OTP_LEN] = std::array::from_fn(|_| RwSignal::new(String::new()));
    let refs: [NodeRef<Input>; OTP_LEN] = std::array::from_fn(|_| NodeRef::<Input>::new());

    let loading  = RwSignal::new(false);
    let resending = RwSignal::new(false);
    let error    = RwSignal::new(Option::<String>::None);
    let info     = RwSignal::new(Option::<String>::None);
    let secs_left = RwSignal::new(EXPIRY_SECS);

    let countdown_handle = set_interval_with_handle(
        move || secs_left.update(|n| { if *n > 0 { *n -= 1; } }),
        std::time::Duration::from_secs(1),
    ).ok();
    on_cleanup(move || { if let Some(h) = countdown_handle { h.clear(); } });

    let mmss = Memo::new(move |_| {
        let s = secs_left.get().max(0);
        format!("{:02}:{:02}", s / 60, s % 60)
    });

    let combined = Memo::new(move |_| {
        digits.iter().map(|d| d.get()).collect::<Vec<_>>().join("")
    });

    let make_oninput = move |idx: usize| {
        move |ev: leptos::ev::Event| {
            let raw: String = event_target_value(&ev);
            let only_digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
            if only_digits.len() <= 1 {
                digits[idx].set(only_digits.clone());
                if !only_digits.is_empty() && idx + 1 < OTP_LEN {
                    if let Some(el) = refs[idx + 1].get() {
                        let _ = el.focus();
                    }
                }
            } else {
                for (i, ch) in only_digits.chars().take(OTP_LEN - idx).enumerate() {
                    digits[idx + i].set(ch.to_string());
                }
                let next = (idx + only_digits.len()).min(OTP_LEN - 1);
                if let Some(el) = refs[next].get() {
                    let _ = el.focus();
                }
            }
        }
    };

    let make_onkeydown = move |idx: usize| {
        move |ev: leptos::ev::KeyboardEvent| {
            if ev.key() == "Backspace" && digits[idx].with(|v| v.is_empty()) && idx > 0 {
                if let Some(el) = refs[idx - 1].get() {
                    let _ = el.focus();
                }
            }
        }
    };

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let code = combined.get();
        let ph   = phone.get();
        if ph.is_empty() {
            error.set(Some("Nomor HP tidak ditemukan. Ulangi pendaftaran.".into()));
            return;
        }
        if code.len() != OTP_LEN {
            error.set(Some("Masukkan 6 digit kode OTP.".into()));
            return;
        }
        loading.set(true);
        error.set(None);
        info.set(None);
        leptos::task::spawn_local(async move {
            match verify_otp_action(ph, code).await {
                Ok(_) => {
                    #[cfg(target_arch = "wasm32")]
                    if let Some(win) = web_sys::window() {
                        let _ = win.location().replace("/explore");
                    }
                }
                Err(e) => {
                    error.set(Some(e.to_string()));
                    loading.set(false);
                }
            }
        });
    };

    let on_resend = move |_| {
        let ph = phone.get();
        if ph.is_empty() { return; }
        resending.set(true);
        info.set(None);
        error.set(None);
        leptos::task::spawn_local(async move {
            match resend_otp_action("User".into(), ph).await {
                Ok(_) => info.set(Some("Kode OTP baru sudah dikirim ke WhatsApp.".into())),
                Err(_) => error.set(Some("Gagal kirim ulang. Coba lagi.".into())),
            }
            resending.set(false);
        });
    };

    let masked = Memo::new(move |_| mask_phone(&phone.get()));

    view! {
        <div class="grid-bg">
            <div class="grid-lines"></div>
            <div class="orb orb-1"></div>
            <div class="orb orb-2"></div>
        </div>
        <main class="auth-page verify-page">
            <header class="auth-header verify-header">
                <A href="/register" attr:class="back-btn">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="verify-title-mini">"Verifikasi"</span>
                <span class="auth-logo">"KINETIC"</span>
                <ThemeToggle/>
            </header>

            <section class="animate-fade-up">
                <h1 class="hero-title verify-headline">
                    "Verifikasi"<br/>
                    <span class="verify-headline-accent">"WhatsApp"</span>
                </h1>
                <p class="hero-sub">"Kami sudah mengirim kode 6 digit ke nomor WhatsApp kamu."</p>
            </section>

            <div class="auth-card verify-card animate-fade-up animate-fade-up-delay-2">
                <form on:submit=on_submit class="verify-form" novalidate=true>
                    <div class="otp-row" role="group" aria-label="6 digit OTP">
                        {(0..OTP_LEN).map(|i| {
                            let oninput = make_oninput(i);
                            let onkeydown = make_onkeydown(i);
                            view! {
                                <input
                                    class="otp-cell"
                                    node_ref=refs[i]
                                    type="tel"
                                    inputmode="numeric"
                                    maxlength="1"
                                    autocomplete=if i == 0 { "one-time-code" } else { "off" }
                                    aria-label=move || format!("digit {}", i + 1)
                                    prop:value=move || digits[i].get()
                                    on:input=oninput
                                    on:keydown=onkeydown
                                    disabled=move || loading.get()
                                />
                            }
                        }).collect_view()}
                    </div>

                    {move || error.get().map(|m| view! {
                        <div class="error-banner" role="alert">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                <circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/>
                            </svg>
                            <span>{m}</span>
                        </div>
                    })}
                    {move || info.get().map(|m| view! {
                        <div class="success-banner" role="status">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#39ff8a" stroke-width="2">
                                <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>
                            </svg>
                            <span>{m}</span>
                        </div>
                    })}

                    <button
                        type="submit"
                        class="submit-btn verify-submit"
                        disabled=move || loading.get() || combined.get().len() != OTP_LEN
                    >
                        {move || if loading.get() {
                            view! {
                                <span class="btn-loading">
                                    <span class="spinner"></span>
                                    "MEMVERIFIKASI..."
                                </span>
                            }.into_any()
                        } else {
                            view! { <span>"VERIFIKASI & LANJUT"</span> }.into_any()
                        }}
                    </button>

                    <p class="resend-row">
                        <span>"Tidak menerima kode? "</span>
                        <button
                            type="button"
                            class="resend-link"
                            on:click=on_resend
                            disabled=move || resending.get()
                        >
                            {move || if resending.get() { "Mengirim..." } else { "Kirim Ulang" }}
                        </button>
                    </p>
                </form>
            </div>

            <div class="verify-meta-row animate-fade-up animate-fade-up-delay-3">
                <div class="verify-meta-card">
                    <div class="verify-meta-icon verify-meta-icon--expiry">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                            <circle cx="12" cy="13" r="8"/>
                            <line x1="12" y1="9" x2="12" y2="13"/>
                            <line x1="12" y1="2" x2="12" y2="4"/>
                        </svg>
                    </div>
                    <div class="verify-meta-body">
                        <span class="verify-meta-label">"EXPIRES IN"</span>
                        <span class="verify-meta-value">{move || mmss.get()}</span>
                    </div>
                </div>
                <div class="verify-meta-card">
                    <div class="verify-meta-icon verify-meta-icon--sent">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                            <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z"/>
                            <polyline points="22,6 12,13 2,6"/>
                        </svg>
                    </div>
                    <div class="verify-meta-body">
                        <span class="verify-meta-label">"SENT TO"</span>
                        <span class="verify-meta-value">{move || masked.get()}</span>
                    </div>
                </div>
            </div>
        </main>
    }
}

fn mask_phone(p: &str) -> String {
    let chars: Vec<char> = p.chars().collect();
    if chars.len() < 6 {
        return p.to_string();
    }
    let head: String = chars.iter().take(4).collect();
    let tail: String = chars.iter().rev().take(3).collect::<String>().chars().rev().collect();
    let middle: String = std::iter::repeat('*').take(chars.len().saturating_sub(7).max(3)).collect();
    format!("{head}{middle}{tail}")
}
