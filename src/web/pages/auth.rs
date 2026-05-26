//! auth.rs — Halaman Login & Register dengan SSR cookie auth.
//!
//! ## Alur Auth
//!
//! **Login:**
//!   1. User submit form → `login_action` server function
//!   2. Server validasi kredensial ke backend API
//!   3. Jika OK: server set HttpOnly cookie `pulse_token`
//!   4. Client lakukan full-page navigation ke "/" via `window.location.replace`
//!      (bukan client-side navigate) agar SSR re-baca cookie baru
//!
//! **Register:**
//!   1. User submit form → `register_action` server function
//!   2. Jika OK: tampilkan success message, tunggu 1.5s, redirect ke /login
//!   3. Timeout menggunakan `gloo_timers::future::TimeoutFuture` dalam
//!      `spawn_local` — tidak ada memory leak (tidak ada `Closure::forget`)
//!
//! ## Tidak ada memory leak
//! Pattern lama `cb.forget()` (raw wasm_bindgen Closure) diganti dengan
//! `gloo_timers::future::TimeoutFuture::new(ms).await` di dalam `spawn_local`.
//! Rust future di-drop otomatis setelah selesai — zero leaks.

use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_navigate};

use crate::web::api::{login_action, register_action};

// ── Login Page ─────────────────────────────────────────────────────────────────

#[component]
pub fn LoginPage() -> impl IntoView {
    let phone    = RwSignal::new(String::new());
    let password = RwSignal::new(String::new());
    let loading  = RwSignal::new(false);
    let error    = RwSignal::new(Option::<String>::None);

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

        leptos::task::spawn_local(async move {
            match login_action(ph, pw).await {
                Ok(_user) => {
                    // Cookie sudah di-set server-side oleh login_action.
                    // Full-page navigation agar SSR re-baca cookie baru.
                    // Tidak pakai use_navigate karena itu client-side only.
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

    view! {
        <div class="auth-page">
            <div class="auth-box">
                <div class="auth-box__logo">"PUL" <span>"SE"</span></div>
                <p class="auth-box__tagline">"Temukan event impianmu"</p>

                <h2 class="auth-box__title">"Masuk ke Akun"</h2>

                {move || error.get().map(|e| view! {
                    <div class="alert alert--error">{e}</div>
                })}

                <form on:submit=on_submit>
                    <div class="form-group">
                        <label>"Nomor HP"</label>
                        <input
                            type="tel"
                            placeholder="+62 812 xxxx xxxx"
                            prop:value=phone
                            on:input=move |ev| phone.set(event_target_value(&ev))
                        />
                    </div>

                    <div class="form-group">
                        <label>"Password"</label>
                        <input
                            type="password"
                            placeholder="••••••••"
                            prop:value=password
                            on:input=move |ev| password.set(event_target_value(&ev))
                        />
                    </div>

                    <button
                        type="submit"
                        class="btn btn--accent btn--full btn--lg"
                        disabled=move || loading.get()
                    >
                        {move || if loading.get() { "Masuk..." } else { "Masuk" }}
                    </button>
                </form>

                <div class="form-divider">"atau"</div>

                <A href="/register" attr:class="btn btn--ghost btn--full">
                    "Belum punya akun? Daftar"
                </A>
                <A href="/forgot-password" attr:class="btn btn--ghost btn--full" attr:style="margin-top:.5rem;font-size:.875rem">
                    "Lupa password?"
                </A>
            </div>
        </div>
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

    // `use_navigate` harus dipanggil di level komponen (saat render),
    // bukan di dalam closure atau async block.
    // NavigateFn bersifat Clone sehingga aman di-move ke async block.
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

        // Clone navigate sebelum move ke async block.
        let nav = navigate.clone();

        leptos::task::spawn_local(async move {
            match register_action(n, ph, role.get()).await {
                Ok(_) => {
                    success.set(true);
                    loading.set(false);

                    // Tunggu 1.5 detik agar user melihat pesan sukses,
                    // kemudian navigasi ke /login.
                    //
                    // Menggunakan gloo_timers::future::TimeoutFuture (bukan
                    // raw wasm_bindgen::Closure + set_timeout + cb.forget()).
                    //
                    // TimeoutFuture adalah Future yang resolve setelah delay;
                    // ketika di-drop (setelah .await selesai), tidak ada
                    // resource yang leaked — zero memory leak.
                    #[cfg(target_arch = "wasm32")]
                    {
                        gloo_timers::future::TimeoutFuture::new(1500).await;
                        nav("/login", Default::default());
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
        <div class="auth-page">
            <div class="auth-box">
                <div class="auth-box__logo">"PUL" <span>"SE"</span></div>
                <p class="auth-box__tagline">"Buat akun gratis sekarang"</p>

                <h2 class="auth-box__title">"Daftar"</h2>

                <Show when=move || success.get()>
                    <div class="alert alert--success">
                        "Registrasi berhasil! Mengalihkan ke halaman masuk..."
                    </div>
                </Show>

                {move || error.get().map(|e| view! {
                    <div class="alert alert--error">{e}</div>
                })}

                <form on:submit=on_submit>
                    <div class="form-group">
                        <label>"Nama Lengkap"</label>
                        <input
                            type="text"
                            placeholder="John Doe"
                            prop:value=name
                            on:input=move |ev| name.set(event_target_value(&ev))
                        />
                    </div>

                    <div class="form-group">
                        <label>"Nomor HP"</label>
                        <input
                            type="tel"
                            placeholder="+62 812 xxxx xxxx"
                            prop:value=phone
                            on:input=move |ev| phone.set(event_target_value(&ev))
                        />
                    </div>

                    <div class="form-group">
                        <label>"Daftar sebagai"</label>
                        <select on:change=move |ev| role.set(event_target_value(&ev))>
                            <option value="customer">"Pembeli Tiket"</option>
                            <option value="merchant">"Event Organizer / Merchant"</option>
                        </select>
                    </div>

                    <button
                        type="submit"
                        class="btn btn--accent btn--full btn--lg"
                        disabled=move || loading.get() || success.get()
                    >
                        {move || if loading.get() { "Mendaftar..." } else { "Daftar Sekarang" }}
                    </button>
                </form>

                <div class="form-divider">"atau"</div>

                <A href="/login" attr:class="btn btn--ghost btn--full">
                    "Sudah punya akun? Masuk"
                </A>
            </div>
        </div>
    }
}
