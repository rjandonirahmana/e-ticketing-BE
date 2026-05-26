//! forgot_password.rs — Halaman Lupa Password (SSR).

use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn ForgotPasswordPage() -> impl IntoView {
    let email   = RwSignal::new(String::new());
    let loading = RwSignal::new(false);
    let sent    = RwSignal::new(false);
    let error   = RwSignal::new(Option::<String>::None);

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let em = email.get();
        if em.trim().is_empty() {
            error.set(Some("Masukkan alamat email kamu.".into()));
            return;
        }
        error.set(None);
        loading.set(true);
        // Fitur reset password belum tersedia di backend — tampilkan pesan
        loading.set(false);
        error.set(Some("Reset password belum tersedia. Hubungi support kami.".into()));
    };

    view! {
        <div class="auth-page">
            <div class="auth-box">
                <div class="auth-box__logo">"PUL" <span>"SE"</span></div>

                <h2 class="auth-box__title">"Reset Password"</h2>
                <p style="font-size:.875rem;color:var(--clr-muted);text-align:center;margin-bottom:1.5rem">
                    "Masukkan email kamu untuk reset password."
                </p>

                {move || if sent.get() {
                    view! {
                        <div class="alert alert--success">
                            "Email reset telah dikirim ke " <strong>{email.get()}</strong>
                            ". Periksa folder spam."
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <div>
                            {move || error.get().map(|e| view! { <div class="alert alert--error">{e}</div> })}
                            <form on:submit=on_submit>
                                <div class="form-group">
                                    <label>"Alamat Email"</label>
                                    <input
                                        type="email"
                                        placeholder="kamu@example.com"
                                        autocomplete="email"
                                        prop:value=email
                                        on:input=move |ev| { email.set(event_target_value(&ev)); error.set(None); }
                                    />
                                </div>
                                <button
                                    type="submit"
                                    class="btn btn--accent btn--full btn--lg"
                                    disabled=move || loading.get()
                                >
                                    {move || if loading.get() { "Mengirim..." } else { "Kirim Link Reset" }}
                                </button>
                            </form>
                        </div>
                    }.into_any()
                }}

                <div class="form-divider">"atau"</div>
                <A href="/login" attr:class="btn btn--ghost btn--full">"Kembali ke Login"</A>
            </div>
        </div>
    }
}
