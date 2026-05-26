//! verify_otp.rs — Halaman Verifikasi OTP setelah Register (SSR).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::web::api::{verify_otp_action, resend_otp_action};

#[component]
pub fn VerifyOtpPage() -> impl IntoView {
    let query = use_query_map();
    let phone = move || query.read().get("phone").unwrap_or_default();

    let otp    = RwSignal::new(String::new());
    let loading  = RwSignal::new(false);
    let resending = RwSignal::new(false);
    let error  = RwSignal::new(Option::<String>::None);
    let info   = RwSignal::new(Option::<String>::None);
    let success = RwSignal::new(false);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let code = otp.get();
        let ph   = phone();
        if ph.is_empty() {
            error.set(Some("Nomor HP tidak ditemukan. Ulangi pendaftaran.".into()));
            return;
        }
        if code.len() < 4 {
            error.set(Some("Masukkan kode OTP yang valid.".into()));
            return;
        }
        loading.set(true);
        error.set(None);
        leptos::task::spawn_local(async move {
            match verify_otp_action(ph, code).await {
                Ok(_) => {
                    success.set(true);
                    #[cfg(target_arch = "wasm32")]
                    if let Some(win) = web_sys::window() {
                        let _ = win.location().replace("/");
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
        let ph = phone();
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

    // Mask phone display
    let masked = move || {
        let p = phone();
        if p.len() < 7 { return p; }
        let head: String = p.chars().take(4).collect();
        let tail: String = p.chars().rev().take(3).collect::<String>().chars().rev().collect();
        let stars: String = std::iter::repeat('*').take(p.len().saturating_sub(7).max(3)).collect();
        format!("{head}{stars}{tail}")
    };

    view! {
        <div class="auth-page">
            <div class="auth-box">
                <div class="auth-box__logo">"PUL" <span>"SE"</span></div>
                <p class="auth-box__tagline">"Verifikasi nomor WhatsApp kamu"</p>

                <h2 class="auth-box__title">"Masukkan Kode OTP"</h2>

                <p style="font-size:.875rem;color:var(--clr-muted);margin-bottom:1.25rem;text-align:center">
                    "Kode 6 digit telah dikirim ke " <strong>{masked}</strong>
                </p>

                <Show when=move || success.get()>
                    <div class="alert alert--success">"Verifikasi berhasil! Mengalihkan..."</div>
                </Show>

                {move || error.get().map(|e| view! { <div class="alert alert--error">{e}</div> })}
                {move || info.get().map(|m| view! { <div class="alert alert--success">{m}</div> })}

                <form on:submit=on_submit>
                    <div class="form-group">
                        <label>"Kode OTP"</label>
                        <input
                            type="text"
                            inputmode="numeric"
                            maxlength="6"
                            placeholder="123456"
                            autocomplete="one-time-code"
                            prop:value=otp
                            on:input=move |ev| otp.set(event_target_value(&ev))
                            style="font-size:1.5rem;letter-spacing:.5rem;text-align:center"
                        />
                    </div>

                    <button
                        type="submit"
                        class="btn btn--accent btn--full btn--lg"
                        disabled=move || loading.get() || success.get()
                    >
                        {move || if loading.get() { "Memverifikasi..." } else { "Verifikasi" }}
                    </button>
                </form>

                <div style="margin-top:1.25rem;text-align:center">
                    <button
                        class="btn btn--ghost btn--sm"
                        on:click=on_resend
                        disabled=move || resending.get()
                    >
                        {move || if resending.get() { "Mengirim..." } else { "Kirim Ulang OTP" }}
                    </button>
                </div>

                <div class="form-divider">"atau"</div>
                <A href="/register" attr:class="btn btn--ghost btn--full">"Kembali ke Daftar"</A>
            </div>
        </div>
    }
}
