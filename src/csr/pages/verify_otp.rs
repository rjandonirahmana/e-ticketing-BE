//! OTP verification page — matches the design from `screen_1777101276169.png`.
//!
//! Flow:
//!   1. User reaches this page right after `register` succeeds. The `phone`
//!      query param tells us which number to verify.
//!   2. We render six big code boxes. The user types a 6-digit OTP that the
//!      backend just sent to their WhatsApp.
//!   3. On submit we call `/auth/verify-register`; on success the user is
//!      logged in (token stored) and we navigate home.
//!   4. A 5-minute countdown is shown. After expiry — or on demand — the user
//!      may tap "Resend".

use leptos::ev::Event;
use leptos::html::Input;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use crate::csr::hooks::use_nav;
use leptos_router::hooks::use_query_map;

use crate::csr::components::{ErrorBanner, GridBackground};
use crate::csr::hooks::{use_auth, ThemeToggle};
use crate::csr::services::auth as auth_svc;

const OTP_LEN: usize = 6;
const EXPIRY_SECS: i32 = 5 * 60;

#[component]
pub fn VerifyOtpPage() -> impl IntoView {
    let auth = use_auth();
    let navigate = use_nav();
    let query = use_query_map();

    // Phone number we're verifying — read once from the URL.
    let phone = Memo::new(move |_| query.with(|q| q.get("phone").unwrap_or_default().to_string()));

    // 6 separate single-digit cells.
    let digits: [RwSignal<String>; OTP_LEN] = std::array::from_fn(|_| RwSignal::new(String::new()));
    let refs: [NodeRef<Input>; OTP_LEN] = std::array::from_fn(|_| NodeRef::new());

    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let info: RwSignal<Option<String>> = RwSignal::new(None);
    let loading = RwSignal::new(false);
    let resending = RwSignal::new(false);
    let secs_left = RwSignal::new(EXPIRY_SECS);

    // Already authenticated → bounce home.
    {
        let nav = navigate.clone();
        Effect::new(move |_| {
            if auth.is_authenticated() {
                nav("/", Default::default());
            }
        });
    }

    // 1-second countdown — timer is stopped automatically when the page unmounts.
    let countdown_handle = set_interval_with_handle(
        move || {
            secs_left.update(|n| {
                if *n > 0 {
                    *n -= 1;
                }
            });
        },
        std::time::Duration::from_secs(1),
    )
    .ok();
    on_cleanup(move || {
        if let Some(h) = countdown_handle {
            h.clear();
        }
    });

    let combined = Memo::new(move |_| digits.iter().map(|d| d.get()).collect::<Vec<_>>().join(""));

    // Build a single-cell input handler. Auto-advances on entry, auto-retreats
    // on backspace.
    let make_oninput = move |idx: usize| {
        move |ev: Event| {
            let raw: String = event_target_value(&ev);
            // keep only digits; if user pasted multiple, distribute across cells.
            let only_digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();

            if only_digits.len() <= 1 {
                digits[idx].set(only_digits.clone());
                if !only_digits.is_empty() && idx + 1 < OTP_LEN {
                    if let Some(input) = refs[idx + 1].get() {
                        let _ = input.focus();
                    }
                }
            } else {
                // Spread (e.g. user pasted "123456")
                for (i, ch) in only_digits.chars().take(OTP_LEN - idx).enumerate() {
                    digits[idx + i].set(ch.to_string());
                }
                let next_focus = (idx + only_digits.len()).min(OTP_LEN - 1);
                if let Some(input) = refs[next_focus].get() {
                    let _ = input.focus();
                }
            }
        }
    };

    let make_onkeydown = move |idx: usize| {
        move |ev: leptos::ev::KeyboardEvent| {
            if ev.key() == "Backspace" && digits[idx].with(|v| v.is_empty()) && idx > 0 {
                if let Some(input) = refs[idx - 1].get() {
                    let _ = input.focus();
                }
            }
        }
    };

    let nav_for_submit = navigate.clone();
    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let code = combined.get();
        let ph = phone.get();
        if ph.is_empty() {
            error.set(Some(
                "Nomor HP tidak ditemukan. Mohon ulangi pendaftaran.".into(),
            ));
            return;
        }
        if code.len() != OTP_LEN {
            error.set(Some("Masukkan 6 digit kode OTP.".into()));
            return;
        }
        error.set(None);
        info.set(None);
        loading.set(true);
        let nav = nav_for_submit.clone();
        spawn_local(async move {
            match auth.verify_otp(ph, code).await {
                Ok(()) => {
                    loading.set(false);
                    nav("/", Default::default());
                }
                Err(err) => {
                    loading.set(false);
                    let msg = match err.status {
                        401 | 400 | 422 => "Kode OTP salah atau sudah kedaluwarsa.".into(),
                        404 => "Pendaftaran tidak ditemukan. Mohon mulai ulang.".into(),
                        0 => "Jaringan bermasalah. Periksa koneksi.".into(),
                        _ => format!("Terjadi kesalahan. ({})", err.status),
                    };
                    error.set(Some(msg));
                }
            }
        });
    };

    let on_resend = move |_| {
        let ph = phone.get();
        if ph.is_empty() {
            error.set(Some("Nomor HP tidak ditemukan.".into()));
            return;
        }
        if resending.get() {
            return;
        }
        resending.set(true);
        info.set(None);
        error.set(None);
        spawn_local(async move {
            match auth_svc::resend_register_otp("Kinetic User", &ph, None).await {
                Ok(()) => {
                    info.set(Some("Kode OTP baru sudah dikirim ke WhatsApp.".into()));
                    secs_left.set(EXPIRY_SECS);
                }
                Err(err) => {
                    let msg = match err.status {
                        409 => "Akun sudah aktif. Silakan login.".into(),
                        429 => "Terlalu banyak permintaan. Coba lagi sebentar lagi.".into(),
                        0 => "Jaringan bermasalah.".into(),
                        _ => format!("Gagal kirim ulang. ({})", err.status),
                    };
                    error.set(Some(msg));
                }
            }
            resending.set(false);
        });
    };

    let masked_phone = Memo::new(move |_| mask_phone(&phone.get()));
    let mmss = Memo::new(move |_| {
        let s = secs_left.get().max(0);
        format!("{:02}:{:02}", s / 60, s % 60)
    });
    let expired = Memo::new(move |_| secs_left.get() <= 0);

    view! {
        <GridBackground />
        <main class="auth-page verify-page">
            <header class="auth-header verify-header">
                <A href="/register" attr:class="back-btn">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="verify-title-mini">"Verification"</span>
                <span class="auth-logo">"KINETIC"</span>
                <ThemeToggle />
            </header>

            <section class="animate-fade-up">
                <h1 class="hero-title verify-headline">
                    "Verify Your"<br/>
                    <span class="verify-headline-accent">"WhatsApp"</span>
                </h1>
                <p class="hero-sub">
                    "Kami sudah mengirim kode 6 digit ke nomor WhatsApp kamu."
                </p>
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

                    {move || error.get().map(|m| view! { <ErrorBanner message=m /> })}
                    {move || info.get().map(|m| view! {
                        <div class="success-banner" role="status">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="#39ff8a" stroke-width="2">
                                <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>
                            </svg>
                            <span>{m}</span>
                        </div>
                    })}

                    <button type="submit"
                        class="submit-btn verify-submit"
                        disabled=move || loading.get() || combined.get().len() != OTP_LEN>
                        {move || if loading.get() {
                            view! { <span class="btn-loading"><span class="spinner"></span>"MEMVERIFIKASI..."</span> }.into_any()
                        } else {
                            view! { <span>"Verify & Proceed"</span> }.into_any()
                        }}
                    </button>

                    <p class="resend-row">
                        <span>"Tidak menerima kode? "</span>
                        <button type="button"
                            class="resend-link"
                            on:click=on_resend
                            disabled=move || resending.get() || (!expired.get() && secs_left.get() > EXPIRY_SECS - 30)>
                            {move || if resending.get() { "Mengirim..." } else { "Resend" }}
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
                        <span class="verify-meta-value">{move || masked_phone.get()}</span>
                    </div>
                </div>
            </div>
        </main>
    }
}

/// Mask a phone like "+6281234567890" → "+628******890". Falls back to the
/// raw value if it's too short to mask meaningfully.
fn mask_phone(p: &str) -> String {
    let chars: Vec<char> = p.chars().collect();
    if chars.len() < 6 {
        return p.to_string();
    }
    let head = chars.iter().take(4).collect::<String>();
    let tail = chars
        .iter()
        .rev()
        .take(3)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let middle: String = std::iter::repeat('*')
        .take(chars.len().saturating_sub(7).max(3))
        .collect();
    format!("{head}{middle}{tail}")
}
