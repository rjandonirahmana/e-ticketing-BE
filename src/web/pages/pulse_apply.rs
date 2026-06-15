use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;

use crate::web::components::BottomNav;

#[component]
pub fn PulseApplyPage() -> impl IntoView {
    let company = RwSignal::new(String::new());
    let rekening = RwSignal::new(String::new());
    let whatsapp = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let agreed = RwSignal::new(false);

    let can_submit = Memo::new(move |_| {
        agreed.get()
            && !company.get().trim().is_empty()
            && !rekening.get().trim().is_empty()
            && !whatsapp.get().trim().is_empty()
            && !email.get().trim().is_empty()
    });

    let navigate = use_navigate();
    let on_submit = move |_| {
        if !can_submit.get_untracked() {
            return;
        }
        navigate("/register", NavigateOptions::default());
    };

    view! {
        <Title text="Daftar Jadi Partner PULSE" />
        <Meta name="description" content="Daftarkan eventmu ke PULSE dan mulai jual tiket sekarang. Proses cepat dan mudah." />
        <div class="pa-page">
            <header class="pa-topbar">
                <A href="/pulse-landing" attr:class="pa-back" attr:aria-label="Kembali">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                        stroke="currentColor" stroke-width="2.5" stroke-linecap="round"
                        stroke-linejoin="round">
                        <line x1="19" y1="12" x2="5" y2="12"/>
                        <polyline points="12 19 5 12 12 5"/>
                    </svg>
                </A>
                <span class="pa-topbar-title">"PULSE"</span>
                <span class="pa-topbar-spacer"></span>
            </header>

            <main class="pa-main">
                <span class="pa-badge">"THE 1% NETWORK"</span>

                <h1 class="pa-title">
                    "PARTNER"<br/>
                    "WITH "<span class="pa-title-accent">"PULSE"</span>
                </h1>

                <p class="pa-sub">
                    "Join the most exclusive stage for live music. Benefit from our "
                    <span class="pa-hl">"1% profit-sharing model"</span>
                    " designed to scale your vision to the masses."
                </p>

                <div class="pa-stats">
                    <div class="pa-stat">
                        <span class="pa-stat-num">"500+"</span>
                        <span class="pa-stat-label">"ACTIVE ORGANIZERS"</span>
                    </div>
                    <div class="pa-stat">
                        <span class="pa-stat-num pa-stat-num--lime">"Rp0"</span>
                        <span class="pa-stat-label">"UPFRONT COSTS"</span>
                    </div>
                </div>

                <div class="pa-form-card">
                    <div class="pa-field">
                        <label class="pa-label" for="pa-company">"NAMA PERUSAHAAN / PT"</label>
                        <input
                            id="pa-company"
                            class="pa-input"
                            type="text"
                            autocomplete="organization"
                            placeholder="PULSE Entertainment Group"
                            prop:value=move || company.get()
                            on:input=move |ev| company.set(event_target_value(&ev))
                        />
                    </div>

                    <div class="pa-field">
                        <label class="pa-label" for="pa-rekening">"NOMOR REKENING"</label>
                        <div class="pa-input pa-input--prefixed">
                            <span class="pa-prefix">"Rp"</span>
                            <input
                                id="pa-rekening"
                                class="pa-input-bare"
                                type="text"
                                inputmode="numeric"
                                placeholder="000 0000 0000"
                                prop:value=move || rekening.get()
                                on:input=move |ev| rekening.set(event_target_value(&ev))
                            />
                        </div>
                    </div>

                    <div class="pa-field">
                        <label class="pa-label" for="pa-wa">"NOMOR WHATSAPP"</label>
                        <input
                            id="pa-wa"
                            class="pa-input"
                            type="tel"
                            autocomplete="tel"
                            placeholder="+62 812..."
                            prop:value=move || whatsapp.get()
                            on:input=move |ev| whatsapp.set(event_target_value(&ev))
                        />
                    </div>

                    <div class="pa-field">
                        <label class="pa-label" for="pa-email">"ALAMAT EMAIL"</label>
                        <input
                            id="pa-email"
                            class="pa-input"
                            type="email"
                            autocomplete="email"
                            placeholder="partner@pulse.id"
                            prop:value=move || email.get()
                            on:input=move |ev| email.set(event_target_value(&ev))
                        />
                    </div>

                    <button
                        type="button"
                        class="pa-agree"
                        class:pa-agree--on=move || agreed.get()
                        aria-pressed=move || if agreed.get() { "true" } else { "false" }
                        on:click=move |_| agreed.update(|v| *v = !*v)
                    >
                        <span class="pa-check">
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none"
                                stroke="currentColor" stroke-width="3.5" stroke-linecap="round"
                                stroke-linejoin="round">
                                <polyline points="20 6 9 17 4 12"/>
                            </svg>
                        </span>
                        <span class="pa-agree-text">
                            "I agree to the "
                            <span class="pa-agree-link">"Merchant Partnership Agreement"</span>
                            " and the profit-sharing terms."
                        </span>
                    </button>

                    <button
                        type="button"
                        class="pa-submit"
                        prop:disabled=move || !can_submit.get()
                        on:click=on_submit
                    >
                        "Gabung Mitra "
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                            stroke="currentColor" stroke-width="2.6" stroke-linecap="round"
                            stroke-linejoin="round">
                            <line x1="5" y1="12" x2="19" y2="12"/>
                            <polyline points="12 5 19 12 12 19"/>
                        </svg>
                    </button>
                </div>
            </main>

            <BottomNav active="profile" />
        </div>
    }
}
