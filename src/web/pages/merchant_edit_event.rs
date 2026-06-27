//! merchant_edit_event.rs — Halaman Edit Event (SSR + medit-* design).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::web::api::{get_merchant_event_detail, update_merchant_event};
use crate::web::app::AuthResource;
use crate::web::components::event_story_preview::EventStoryPreviewInline;
use crate::web::hooks::ThemeToggle;
use crate::web::utils::{map_destroy, map_picker, map_set, DEFAULT_LAT, DEFAULT_LNG};

const CATEGORIES: &[&str] = &[
    "Musik", "Festival", "Konser", "Olahraga", "Teknologi",
    "Seni", "Kuliner", "Pendidikan", "Hiburan", "Bisnis",
];

// ── Skeleton ──────────────────────────────────────────────────────────────────

#[component]
fn EditSkeleton() -> impl IntoView {
    view! {
        <div class="medit-container">
            {(0..5).map(|_| view! {
                <div class="medit-field-group">
                    <div class="shimmer-bg" style="width:80px;height:10px;border-radius:4px;margin-bottom:8px;"></div>
                    <div class="shimmer-bg" style="width:100%;height:44px;border-radius:8px;"></div>
                </div>
            }).collect_view()}
        </div>
    }
}

// ── Main page ─────────────────────────────────────────────────────────────────

#[component]
pub fn MerchantEditEventPage() -> impl IntoView {
    let params = use_params_map();
    let slug = move || params.read().get("slug").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let event_data = Resource::new(
        move || (slug(), is_logged_in()),
        |(s, logged_in)| async move {
            if logged_in && !s.is_empty() { get_merchant_event_detail(s).await }
            else { Err(ServerFnError::ServerError("not_ready".into())) }
        },
    );

    let f_name     = RwSignal::new(String::new());
    let f_desc     = RwSignal::new(String::new());
    let f_venue    = RwSignal::new(String::new());
    let f_city     = RwSignal::new(String::new());
    let f_date     = RwSignal::new(String::new());
    let f_time     = RwSignal::new(String::new());
    let f_cat: RwSignal<Vec<String>> = RwSignal::new(vec![]);
    let f_lat      = RwSignal::new(DEFAULT_LAT);
    let f_lng      = RwSignal::new(DEFAULT_LNG);
    let loc_touched = RwSignal::new(false);
    let initialized = RwSignal::new(false);

    // Inisialisasi peta picker setelah data event ter-populate & div ter-render.
    Effect::new(move |_| {
        if initialized.get() {
            map_picker("edit-loc-map", "edit-lat", "edit-lng");
        }
    });
    on_cleanup(|| map_destroy("edit-loc-map"));

    let cover_preview: RwSignal<Option<String>> = RwSignal::new(None);
    let navigate = use_navigate();
    let saving   = RwSignal::new(false);
    let error_msg = RwSignal::new(String::new());
    let saved    = RwSignal::new(false);

    // Populate form when data loads
    Effect::new(move |_| {
        if let Some(Ok(ev)) = event_data.get() {
            if !initialized.get() {
                f_name.set(ev.name.clone());
                f_desc.set(ev.description.clone().unwrap_or_default());
                f_venue.set(ev.venue.clone().unwrap_or_default());
                f_city.set(ev.city.clone().unwrap_or_default());
                use chrono::Datelike;
                let d = ev.event_date;
                f_date.set(format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()));
                if let Some(st) = ev.start_time {
                    use chrono::Timelike;
                    f_time.set(format!("{:02}:{:02}", st.hour(), st.minute()));
                }
                f_cat.set(ev.category.clone());
                if let (Some(la), Some(lo)) = (ev.latitude, ev.longitude) {
                    f_lat.set(la);
                    f_lng.set(lo);
                    loc_touched.set(true);
                }
                if let Some(url) = ev.cover_url {
                    if !url.is_empty() { cover_preview.set(Some(url)); }
                }
                initialized.set(true);
            }
        }
    });

    let on_cover_change = move |ev: leptos::ev::Event| {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            if let Some(input) = ev.target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            {
                if let Some(files) = input.files() {
                    if let Some(file) = files.get(0) {
                        if let Ok(url) = web_sys::Url::create_object_url_with_blob(&file) {
                            cover_preview.set(Some(url));
                        }
                    }
                }
            }
        }
        let _ = ev;
    };

    let do_save = move |_: leptos::ev::MouseEvent| {
        error_msg.set(String::new());
        saved.set(false);

        let name = f_name.get_untracked();
        if name.trim().len() < 3 { error_msg.set("Nama event minimal 3 karakter.".into()); return; }
        let desc  = f_desc.get_untracked();
        let venue = f_venue.get_untracked();
        let city  = f_city.get_untracked();
        let date  = f_date.get_untracked();
        if date.is_empty() { error_msg.set("Tanggal event wajib diisi.".into()); return; }
        let time  = f_time.get_untracked();
        let cats  = f_cat.get_untracked().join(",");
        let current_slug = slug();

        // Combine date + time into RFC3339 format that the server can parse.
        // Server expects chrono::DateTime<Utc>, so "2024-01-15" alone fails.
        let date_iso = if !time.is_empty() {
            format!("{}T{}:00Z", date, time)
        } else {
            format!("{}T00:00:00Z", date)
        };

        let (lat, lng) = if loc_touched.get_untracked() {
            (Some(f_lat.get_untracked()), Some(f_lng.get_untracked()))
        } else {
            (None, None)
        };

        saving.set(true);
        leptos::task::spawn_local(async move {
            match update_merchant_event(current_slug, name, desc, venue, city, date_iso.clone(), date_iso, cats, lat, lng).await {
                Ok(_) => { saved.set(true); saving.set(false); }
                Err(e) => { error_msg.set(e.to_string()); saving.set(false); }
            }
        });
    };

    view! {
        <div class="medit-page">
            <header class="page-header medit-page-header">
                <A href="/merchant" attr:class="back-btn" attr:aria-label="Kembali">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="page-logo">"EDIT EVENT"</span>
                <div class="header-actions">
                    <ThemeToggle/>
                    <A href="/notifications" attr:class="bell-btn" attr:aria-label="Notifikasi">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"/>
                            <path d="M13.73 21a2 2 0 0 1-3.46 0"/>
                        </svg>
                    </A>
                </div>
            </header>

            <Suspense fallback=|| view! { <EditSkeleton/> }>
                {move || {
                    let ev_data = event_data.get();
                    if ev_data.is_none() { return view! { <EditSkeleton/> }.into_any(); }

                    match ev_data.unwrap() {
                        Err(e) if e.to_string().contains("not_ready") =>
                            view! { <EditSkeleton/> }.into_any(),
                        Err(_) => view! {
                            <div class="medit-container">
                                <div class="medit-error-banner">
                                    "Event tidak ditemukan atau akses ditolak."
                                </div>
                                <A href="/merchant" attr:class="medit-cancel-btn">"← Kembali"</A>
                            </div>
                        }.into_any(),
                        Ok(_) => view! {
                            <div class="medit-container">

                                // ── Feedback ──────────────────────────────────
                                {move || saved.get().then(|| view! {
                                    <div class="medit-success-banner">
                                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                            <polyline points="20 6 9 17 4 12"/>
                                        </svg>
                                        "Event berhasil diperbarui!"
                                    </div>
                                })}
                                {move || (!error_msg.get().is_empty()).then(|| view! {
                                    <div class="medit-error-banner">
                                        {move || error_msg.get()}
                                    </div>
                                })}

                                // ── INFO DASAR ────────────────────────────────
                                <div class="medit-section-header">
                                    <span class="medit-section-label">"INFO DASAR"</span>
                                </div>

                                <div class="medit-field-group">
                                    <label class="medit-field-label">"NAMA EVENT"</label>
                                    <input type="text" class="medit-input"
                                           placeholder="Nama event"
                                           prop:value=move || f_name.get()
                                           on:input=move |e| f_name.set(event_target_value(&e))/>
                                </div>

                                <div class="medit-field-group">
                                    <label class="medit-field-label">"DESKRIPSI"</label>
                                    <textarea class="medit-input medit-textarea"
                                              placeholder="Deskripsi event..."
                                              prop:value=move || f_desc.get()
                                              on:input=move |e| f_desc.set(event_target_value(&e))>
                                    </textarea>
                                </div>

                                // ── KATEGORI ──────────────────────────────────
                                <div class="medit-field-group">
                                    <label class="medit-field-label">"KATEGORI"</label>
                                    <div class="medit-category-grid">
                                        {CATEGORIES.iter().map(|cat| {
                                            let c = cat.to_string();
                                            let c2 = c.clone();
                                            let c3 = c.clone();
                                            view! {
                                                <label class="medit-checkbox-label">
                                                    <input type="checkbox"
                                                           prop:checked=move || f_cat.get().contains(&c2)
                                                           on:change=move |_| {
                                                               f_cat.update(|cats| {
                                                                   if cats.contains(&c3) { cats.retain(|x| x != &c3); }
                                                                   else { cats.push(c3.clone()); }
                                                               });
                                                           }/>
                                                    <span>{c}</span>
                                                </label>
                                            }
                                        }).collect_view()}
                                    </div>
                                </div>

                                // ── FOTO COVER ────────────────────────────────
                                <div class="medit-field-group">
                                    <label class="medit-field-label">"FOTO COVER"</label>
                                    {move || cover_preview.get().map(|url| view! {
                                        <div class="medit-cover-preview">
                                            <img src=url alt="Cover preview"/>
                                        </div>
                                    })}
                                    <div class="medit-file-input-wrapper">
                                        <input type="file" class="medit-file-input" accept="image/*"
                                               on:change=on_cover_change/>
                                        <span class="medit-file-input-label">"GANTI FOTO"</span>
                                    </div>
                                </div>

                                // ── STORY PREVIEW (sama seperti create event) ──
                                <EventStoryPreviewInline
                                    title=Signal::derive(move || f_name.get())
                                    cover_url=Signal::derive(move || cover_preview.get())
                                    description=Signal::derive(move || f_desc.get())
                                    on_share_click=Callback::new({
                                        let navigate = navigate.clone();
                                        move |_| {
                                            #[cfg(target_arch = "wasm32")]
                                            {
                                                let params = web_sys::UrlSearchParams::new()
                                                    .expect("UrlSearchParams");
                                                params.append("event_title", &f_name.get_untracked());
                                                params.append(
                                                    "event_cover",
                                                    &cover_preview.get_untracked().unwrap_or_default(),
                                                );
                                                params.append("event_desc", &f_desc.get_untracked());
                                                params.append("event_slug", &slug());
                                                let qs = params.to_string();
                                                navigate(
                                                    &format!("/story?{}", qs),
                                                    Default::default(),
                                                );
                                            }
                                            #[cfg(not(target_arch = "wasm32"))]
                                            let _ = &navigate;
                                        }
                                    })
                                />

                                // ── TANGGAL & WAKTU ───────────────────────────
                                <div class="medit-section-header">
                                    <span class="medit-section-label">"WAKTU & TEMPAT"</span>
                                </div>

                                <div class="medit-field-group">
                                    <label class="medit-field-label">"TANGGAL EVENT"</label>
                                    <input type="date" class="medit-input"
                                           prop:value=move || f_date.get()
                                           on:input=move |e| f_date.set(event_target_value(&e))/>
                                </div>

                                <div class="medit-grid-2">
                                    <div class="medit-field-group">
                                        <label class="medit-field-label">"WAKTU MULAI"</label>
                                        <input type="time" class="medit-input"
                                               prop:value=move || f_time.get()
                                               on:input=move |e| f_time.set(event_target_value(&e))/>
                                    </div>
                                    <div class="medit-field-group">
                                        <label class="medit-field-label">"KOTA"</label>
                                        <input type="text" class="medit-input"
                                               placeholder="Jakarta"
                                               prop:value=move || f_city.get()
                                               on:input=move |e| f_city.set(event_target_value(&e))/>
                                    </div>
                                </div>

                                <div class="medit-field-group">
                                    <label class="medit-field-label">"NAMA VENUE"</label>
                                    <input type="text" class="medit-input"
                                           placeholder="Gelora Bung Karno"
                                           prop:value=move || f_venue.get()
                                           on:input=move |e| f_venue.set(event_target_value(&e))/>
                                </div>

                                // ── LOKASI DI PETA ────────────────────────────
                                <div class="medit-section-header">
                                    <span class="medit-section-label">"LOKASI DI PETA"</span>
                                </div>
                                <p style="font-size:12px;color:var(--text-muted);margin:0 0 10px">
                                    "Klik peta atau geser pin untuk menandai lokasi venue."
                                </p>
                                <div id="edit-loc-map"
                                     style="width:100%;height:300px;border-radius:12px;overflow:hidden;border:1px solid var(--border);margin-bottom:12px;background:var(--bg-elevated)">
                                </div>
                                <div class="medit-grid-2">
                                    <div class="medit-field-group">
                                        <label class="medit-field-label">"LATITUDE"</label>
                                        <input id="edit-lat" type="number" step="any" class="medit-input"
                                               placeholder="-6.2088"
                                               prop:value=move || f_lat.get().to_string()
                                               on:input=move |e| {
                                                   loc_touched.set(true);
                                                   if let Ok(v) = event_target_value(&e).parse::<f64>() {
                                                       f_lat.set(v);
                                                       map_set("edit-loc-map", v, f_lng.get_untracked());
                                                   }
                                               }/>
                                    </div>
                                    <div class="medit-field-group">
                                        <label class="medit-field-label">"LONGITUDE"</label>
                                        <input id="edit-lng" type="number" step="any" class="medit-input"
                                               placeholder="106.8456"
                                               prop:value=move || f_lng.get().to_string()
                                               on:input=move |e| {
                                                   loc_touched.set(true);
                                                   if let Ok(v) = event_target_value(&e).parse::<f64>() {
                                                       f_lng.set(v);
                                                       map_set("edit-loc-map", f_lat.get_untracked(), v);
                                                   }
                                               }/>
                                    </div>
                                </div>

                                // ── SUBMIT ────────────────────────────────────
                                <div class="medit-actions">
                                    <button class="medit-submit-btn"
                                            disabled=move || saving.get()
                                            on:click=do_save>
                                        {move || if saving.get() { "Menyimpan..." } else { "SIMPAN PERUBAHAN" }}
                                    </button>
                                    <A href="/merchant" attr:class="medit-cancel-btn">"BATAL"</A>
                                </div>

                            </div>
                        }.into_any(),
                    }
                }}
            </Suspense>
        </div>
    }
}
