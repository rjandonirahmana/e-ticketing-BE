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
    // Sandi sedang ditampilkan apa adanya.
    //
    // Nyaris seluruh salah-ketik sandi di ponsel berasal dari mengetik buta:
    // papan ketik kecil, huruf besar-kecil bercampur, dan tak ada satu pun
    // cara memeriksa selain mengirimkannya lalu ditolak. Tombol ini menjadikan
    // pemeriksaan itu mungkin.
    let lihat_sandi = RwSignal::new(false);
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
                // ── Orang sandi ─────────────────────────────────────────────
                // Tangannya menutup mata selama sandi tersembunyi dan turun ke
                // dada saat sandinya ditampilkan — satu-satunya penanda keadaan
                // itu yang terlihat tanpa harus dicari. Digerakkan oleh sinyal
                // yang SAMA dengan tombol mata di dalam kolom sandi, jadi
                // keduanya mustahil bercerita hal yang berbeda.
                //
                // `aria-hidden`: ia mengulang apa yang sudah dinyatakan
                // `aria-pressed` pada tombol mata. Dibacakan pembaca layar, ia
                // hanya menambah satu benda tak bernama untuk dilewati.
                <div
                    class="pw-orang"
                    class:pw-orang--lihat=move || lihat_sandi.get()
                    aria-hidden="true"
                >
                    <svg class="pw-orang__svg" viewBox="0 0 120 120">
                        // Badan digambar PALING DULU supaya kepala dan tangan
                        // menimpanya. Bahunya menyentuh tepi bawah viewBox: yang
                        // membuat sosok ini terbaca sebagai orang — bukan kepala
                        // melayang — adalah pundak yang terpotong bingkai,
                        // persis seperti foto profil.
                        <path class="pw-orang__badan"
                              d="M20 120 Q22 97 44 91 L76 91 Q98 97 100 120 Z"/>
                        <path class="pw-orang__leher" d="M52 76 h16 v18 h-16 Z"/>

                        // Telinga LEBIH DULU daripada kepala supaya pangkalnya
                        // tertimpa — itu yang membuatnya menempel, bukan
                        // tertempel-tempel di sampingnya.
                        <ellipse class="pw-orang__telinga" cx="31" cy="58" rx="6" ry="8"/>
                        <ellipse class="pw-orang__telinga" cx="89" cy="58" rx="6" ry="8"/>

                        <ellipse class="pw-orang__kepala" cx="60" cy="54" rx="29" ry="32"/>

                        // Rambut digambar SESUDAH kepala dan hanya menutupi
                        // dahi. Tanpa ini yang tersisa hanyalah bulatan dengan
                        // wajah — bentuk yang sama persis dipakai emoji, hewan,
                        // dan bola; rambutlah yang menjadikannya manusia.
                        <path class="pw-orang__rambut"
                              d="M31 52 Q30 22 60 22 Q90 22 89 52 Q84 36 60 34 Q36 34 31 52 Z"/>

                        <path class="pw-orang__alis" d="M43 44 Q50 40 57 44"/>
                        <path class="pw-orang__alis" d="M63 44 Q70 40 77 44"/>

                        <circle class="pw-orang__mata" cx="50" cy="54" r="3.6"/>
                        <circle class="pw-orang__mata" cx="70" cy="54" r="3.6"/>
                        <path class="pw-orang__mata-pejam" d="M45 55 Q50 59 55 55"/>
                        <path class="pw-orang__mata-pejam" d="M65 55 Q70 59 75 55"/>

                        // Hidung sebagai SATU guratan, bukan dua titik. Dua
                        // titik berdampingan di tengah wajah terbaca sebagai
                        // sepasang mata begitu mata yang asli tertutup tangan —
                        // orangnya tampak punya empat mata.
                        <path class="pw-orang__hidung" d="M59 60 Q57 68 61.5 68"/>
                        <path class="pw-orang__mulut" d="M53 74 Q60 80 67 74"/>

                        // Tangan digambar TERAKHIR: di SVG tak ada `z-index`,
                        // urutan dokumen itulah urutan tumpukannya.
                        // EMPAT guratan jari, bukan tiga, dan tak satu pun
                        // menyentuh tepi telapaknya. Tiga guratan sepanjang
                        // penuh membelah telapak menjadi pita-pita sama lebar —
                        // yang terbaca bukan tangan, melainkan permukaan
                        // bergaris.
                        <g class="pw-orang__tangan pw-orang__tangan--kiri">
                            <ellipse cx="46" cy="54" rx="15" ry="12"/>
                            <path class="pw-orang__jari" d="M37 50 L38 60"/>
                            <path class="pw-orang__jari" d="M43 48 L43 61"/>
                            <path class="pw-orang__jari" d="M49 48 L49 61"/>
                            <path class="pw-orang__jari" d="M55 50 L54 60"/>
                        </g>
                        <g class="pw-orang__tangan pw-orang__tangan--kanan">
                            <ellipse cx="74" cy="54" rx="15" ry="12"/>
                            <path class="pw-orang__jari" d="M65 50 L66 60"/>
                            <path class="pw-orang__jari" d="M71 48 L71 61"/>
                            <path class="pw-orang__jari" d="M77 48 L77 61"/>
                            <path class="pw-orang__jari" d="M83 50 L82 60"/>
                        </g>
                    </svg>
                </div>

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
                                type=move || if lihat_sandi.get() { "text" } else { "password" }
                                class="field-input"
                                placeholder="••••••••"
                                autocomplete="current-password"
                                disabled=move || loading.get()
                                prop:value=password
                                on:input=move |ev| password.set(event_target_value(&ev))
                                on:focus=move |_| pass_focused.set(true)
                                on:blur=move |_| pass_focused.set(false)
                            />
                            // `type="button"` WAJIB. Tombol di dalam <form>
                            // tanpa atribut ini bertipe `submit` — mengetuknya
                            // akan MENGIRIM formulir login alih-alih menampilkan
                            // sandinya.
                            //
                            // `on:mousedown` dicegah supaya kolomnya tak
                            // kehilangan fokus saat tombol ditekan: tanpa itu
                            // bingkai fokus berkedip dan posisi kursor teks
                            // hilang tepat saat orang sedang membetulkan ketikan.
                            <button
                                type="button"
                                class="field-eye"
                                aria-label=move || {
                                    if lihat_sandi.get() { "Sembunyikan password" } else { "Tampilkan password" }
                                }
                                aria-pressed=move || if lihat_sandi.get() { "true" } else { "false" }
                                on:mousedown=move |ev| ev.prevent_default()
                                on:click=move |_| lihat_sandi.update(|v| *v = !*v)
                            >
                                {move || if lihat_sandi.get() {
                                    view! {
                                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="2"
                                             stroke-linecap="round" stroke-linejoin="round">
                                            <path d="M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94"/>
                                            <path d="M9.9 4.24A9.12 9.12 0 0112 4c7 0 11 8 11 8a18.5 18.5 0 01-2.16 3.19"/>
                                            <path d="M14.12 14.12a3 3 0 11-4.24-4.24"/>
                                            <line x1="1" y1="1" x2="23" y2="23"/>
                                        </svg>
                                    }.into_any()
                                } else {
                                    view! {
                                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="2"
                                             stroke-linecap="round" stroke-linejoin="round">
                                            <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
                                            <circle cx="12" cy="12" r="3"/>
                                        </svg>
                                    }.into_any()
                                }}
                            </button>
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
