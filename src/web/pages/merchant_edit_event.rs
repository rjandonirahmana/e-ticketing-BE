//! merchant_edit_event.rs — Halaman Edit Event (SSR).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::{get_merchant_event_detail, update_merchant_event};
use crate::web::app::AuthResource;

const CATEGORY_OPTIONS: &[&str] = &[
    "Musik", "Festival", "Konser", "Olahraga", "Teknologi",
    "Seni", "Kuliner", "Pendidikan", "Hiburan", "Bisnis",
];

#[component]
pub fn MerchantEditEventPage() -> impl IntoView {
    let params = use_params_map();
    let slug = move || params.read().get("slug").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    // Load event data
    let event_data = Resource::new(
        move || (slug(), is_logged_in()),
        |(s, logged_in)| async move {
            if logged_in && !s.is_empty() {
                get_merchant_event_detail(s).await
            } else {
                Err(ServerFnError::ServerError("not_ready".into()))
            }
        },
    );

    let name        = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let venue       = RwSignal::new(String::new());
    let city        = RwSignal::new(String::new());
    let event_date  = RwSignal::new(String::new());
    let start_time  = RwSignal::new(String::new());
    let categories  = RwSignal::new(Vec::<String>::new());
    let initialized = RwSignal::new(false);

    let loading = RwSignal::new(false);
    let error   = RwSignal::new(Option::<String>::None);
    let saved   = RwSignal::new(false);

    // Populate form once data loads
    Effect::new(move |_| {
        if let Some(Ok(ev)) = event_data.get() {
            if !initialized.get() {
                name.set(ev.name.clone());
                description.set(ev.description.clone().unwrap_or_default());
                venue.set(ev.venue.clone().unwrap_or_default());
                city.set(ev.city.clone().unwrap_or_default());
                // Format date from DateTime
                use chrono::Datelike;
                let d = ev.event_date;
                event_date.set(format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()));
                if let Some(st) = ev.start_time {
                    use chrono::Timelike;
                    start_time.set(format!("{:02}:{:02}", st.hour(), st.minute()));
                }
                categories.set(ev.category.clone());
                initialized.set(true);
            }
        }
    });

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
        if name.get().len() < 3 {
            error.set(Some("Nama event minimal 3 karakter.".into()));
            return;
        }
        loading.set(true);
        error.set(None);
        let current_slug = slug();
        let cats = categories.get().join(",");

        leptos::task::spawn_local(async move {
            match update_merchant_event(
                current_slug,
                name.get(),
                description.get(),
                venue.get(),
                city.get(),
                event_date.get(),
                start_time.get(),
                cats,
            ).await {
                Ok(_) => {
                    saved.set(true);
                    loading.set(false);
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
                <h1 class="page-header__title">"Edit Event"</h1>
            </div>
        </div>

        <div class="container" style="padding-bottom:4rem;max-width:720px">
            <div style="margin-bottom:1.25rem">
                <A href="/merchant" attr:class="btn btn--ghost btn--sm">"← Merchant Hub"</A>
            </div>

            <Suspense fallback=|| view! {
                <div class="loading"><div class="loading__spinner"/><span>"Memuat event..."</span></div>
            }>
                {move || {
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <div style="text-align:center;padding:4rem 0">
                                <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                            </div>
                        }.into_any();
                    }

                    let ev_loaded = event_data.get();
                    if ev_loaded.is_none() { return view!{<div/>}.into_any(); }
                    if let Some(Err(e)) = ev_loaded {
                        if !e.to_string().contains("not_ready") {
                            return view! {
                                <div class="alert alert--error">"Event tidak ditemukan atau akses ditolak."</div>
                            }.into_any();
                        }
                        return view!{<div/>}.into_any();
                    }

                    view! {
                        <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:16px;padding:2rem">
                            {move || saved.get().then(|| view! {
                                <div class="alert alert--success" style="margin-bottom:1.25rem">"Event berhasil diperbarui! ✅"</div>
                            })}
                            {move || error.get().map(|e| view! {
                                <div class="alert alert--error" style="margin-bottom:1.25rem">{e}</div>
                            })}

                            <form on:submit=on_submit>
                                <div class="form-group">
                                    <label>"Nama Event *"</label>
                                    <input
                                        type="text"
                                        prop:value=name
                                        on:input=move |ev| name.set(event_target_value(&ev))
                                    />
                                </div>

                                <div class="form-group">
                                    <label>"Deskripsi"</label>
                                    <textarea
                                        rows="4"
                                        style="width:100%;background:var(--clr-bg);border:1px solid var(--clr-border);border-radius:8px;padding:.75rem;color:inherit;font-size:.875rem;resize:vertical"
                                        prop:value=description
                                        on:input=move |ev| description.set(event_target_value(&ev))
                                    />
                                </div>

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

                                <div style="display:grid;grid-template-columns:1fr 1fr;gap:1rem">
                                    <div class="form-group">
                                        <label>"Venue"</label>
                                        <input
                                            type="text"
                                            prop:value=venue
                                            on:input=move |ev| venue.set(event_target_value(&ev))
                                        />
                                    </div>
                                    <div class="form-group">
                                        <label>"Kota"</label>
                                        <input
                                            type="text"
                                            prop:value=city
                                            on:input=move |ev| city.set(event_target_value(&ev))
                                        />
                                    </div>
                                </div>

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
                                                        if categories.get().contains(&c) { "btn btn--accent btn--sm" }
                                                        else { "btn btn--ghost btn--sm" }
                                                    }
                                                    on:click=move |_| toggle_cat(c2.clone())
                                                >{*cat}</button>
                                            }
                                        }).collect_view()}
                                    </div>
                                </div>

                                <div style="margin-top:1.5rem;display:flex;gap:.75rem">
                                    <button
                                        type="submit"
                                        class="btn btn--accent btn--lg"
                                        disabled=move || loading.get()
                                    >
                                        {move || if loading.get() { "Menyimpan..." } else { "Simpan Perubahan" }}
                                    </button>
                                    <A href="/merchant" attr:class="btn btn--ghost btn--lg">"Batal"</A>
                                </div>
                            </form>
                        </div>
                    }.into_any()
                }}
            </Suspense>
        </div>
    }
}
