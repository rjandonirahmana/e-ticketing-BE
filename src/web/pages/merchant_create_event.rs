//! merchant_create_event.rs — Halaman Buat Event Baru (SSR).

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::create_merchant_event;
use crate::web::app::AuthResource;

const CATEGORY_OPTIONS: &[&str] = &[
    "Musik", "Festival", "Konser", "Olahraga", "Teknologi",
    "Seni", "Kuliner", "Pendidikan", "Hiburan", "Bisnis",
];

#[component]
pub fn MerchantCreateEventPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let name        = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let venue       = RwSignal::new(String::new());
    let city        = RwSignal::new(String::new());
    let event_date  = RwSignal::new(String::new());
    let start_time  = RwSignal::new(String::new());
    let categories  = RwSignal::new(Vec::<String>::new());

    let loading = RwSignal::new(false);
    let error   = RwSignal::new(Option::<String>::None);

    let toggle_cat = move |cat: String| {
        categories.update(|v| {
            if let Some(i) = v.iter().position(|c| c == &cat) {
                v.remove(i);
            } else {
                v.push(cat);
            }
        });
    };

    let on_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let n = name.get();
        if n.len() < 3 {
            error.set(Some("Nama event minimal 3 karakter.".into()));
            return;
        }
        if event_date.get().is_empty() {
            error.set(Some("Tanggal event wajib diisi.".into()));
            return;
        }
        loading.set(true);
        error.set(None);
        let cats = categories.get().join(",");

        leptos::task::spawn_local(async move {
            match create_merchant_event(
                name.get(),
                description.get(),
                venue.get(),
                city.get(),
                event_date.get(),
                start_time.get(),
                cats,
            ).await {
                Ok(slug) => {
                    let _ = &slug; // used inside wasm32 cfg block
                    #[cfg(target_arch = "wasm32")]
                    if let Some(win) = web_sys::window() {
                        let path = if slug.is_empty() { "/merchant".to_string() }
                            else { format!("/events/{slug}") };
                        let _ = win.location().replace(&path);
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
        <div class="page-header">
            <div class="container">
                <p class="page-header__eyebrow">"// merchant hub"</p>
                <h1 class="page-header__title">"Buat Event Baru"</h1>
            </div>
        </div>

        <div class="container" style="padding-bottom:4rem;max-width:720px">
            <div style="margin-bottom:1.25rem">
                <A href="/merchant" attr:class="btn btn--ghost btn--sm">"← Merchant Hub"</A>
            </div>

            {move || {
                if !is_logged_in() && auth.get().is_some() {
                    return view! {
                        <div style="text-align:center;padding:4rem 0">
                            <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                        </div>
                    }.into_any();
                }

                view! {
                    <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:16px;padding:2rem">
                        {move || error.get().map(|e| view! { <div class="alert alert--error" style="margin-bottom:1.25rem">{e}</div> })}

                        <form on:submit=on_submit>
                            // ── Nama Event ──────────────────────────────────
                            <div class="form-group">
                                <label>"Nama Event *"</label>
                                <input
                                    type="text"
                                    placeholder="Contoh: Jakarta Music Festival 2025"
                                    prop:value=name
                                    on:input=move |ev| name.set(event_target_value(&ev))
                                />
                            </div>

                            // ── Deskripsi ────────────────────────────────────
                            <div class="form-group">
                                <label>"Deskripsi"</label>
                                <textarea
                                    placeholder="Deskripsikan event kamu..."
                                    rows="4"
                                    style="width:100%;background:var(--clr-bg);border:1px solid var(--clr-border);border-radius:8px;padding:.75rem;color:inherit;font-size:.875rem;resize:vertical"
                                    prop:value=description
                                    on:input=move |ev| description.set(event_target_value(&ev))
                                />
                            </div>

                            // ── Tanggal & Waktu ──────────────────────────────
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:1rem">
                                <div class="form-group">
                                    <label>"Tanggal Event *"</label>
                                    <input
                                        type="date"
                                        prop:value=event_date
                                        on:input=move |ev| event_date.set(event_target_value(&ev))
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"Waktu Mulai"</label>
                                    <input
                                        type="time"
                                        prop:value=start_time
                                        on:input=move |ev| start_time.set(event_target_value(&ev))
                                    />
                                </div>
                            </div>

                            // ── Venue & Kota ─────────────────────────────────
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:1rem">
                                <div class="form-group">
                                    <label>"Venue / Lokasi"</label>
                                    <input
                                        type="text"
                                        placeholder="Gelora Bung Karno..."
                                        prop:value=venue
                                        on:input=move |ev| venue.set(event_target_value(&ev))
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"Kota"</label>
                                    <input
                                        type="text"
                                        placeholder="Jakarta"
                                        prop:value=city
                                        on:input=move |ev| city.set(event_target_value(&ev))
                                    />
                                </div>
                            </div>

                            // ── Kategori ─────────────────────────────────────
                            <div class="form-group">
                                <label>"Kategori"</label>
                                <div style="display:flex;flex-wrap:wrap;gap:.5rem;margin-top:.375rem">
                                    {CATEGORY_OPTIONS.iter().map(|cat| {
                                        let c = cat.to_string();
                                        let c2 = c.clone();
                                        view! {
                                            <button
                                                type="button"
                                                class=move || {
                                                    if categories.get().contains(&c) {
                                                        "btn btn--accent btn--sm"
                                                    } else {
                                                        "btn btn--ghost btn--sm"
                                                    }
                                                }
                                                on:click=move |_| toggle_cat(c2.clone())
                                            >{*cat}</button>
                                        }
                                    }).collect_view()}
                                </div>
                            </div>

                            // ── Submit ───────────────────────────────────────
                            <div style="margin-top:1.5rem;display:flex;gap:.75rem">
                                <button
                                    type="submit"
                                    class="btn btn--accent btn--lg"
                                    disabled=move || loading.get()
                                >
                                    {move || if loading.get() { "Membuat Event..." } else { "Buat Event" }}
                                </button>
                                <A href="/merchant" attr:class="btn btn--ghost btn--lg">"Batal"</A>
                            </div>
                        </form>
                    </div>
                }.into_any()
            }}
        </div>
    }
}
