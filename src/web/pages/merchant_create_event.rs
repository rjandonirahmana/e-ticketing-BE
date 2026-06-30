//! merchant_create_event.rs — Halaman Buat Event Baru (SSR + medit-* design).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::web::api::create_merchant_event;
use crate::web::app::AuthResource;
use crate::web::components::event_story_preview::EventStoryPreviewInline;
use crate::web::hooks::ThemeToggle;
use crate::web::utils::{map_picker, map_set, DEFAULT_LAT, DEFAULT_LNG};

const CATEGORIES: &[&str] = &[
    "Musik", "Festival", "Konser", "Olahraga", "Teknologi",
    "Seni", "Kuliner", "Pendidikan", "Hiburan", "Bisnis",
];

#[component]
pub fn MerchantCreateEventPage() -> impl IntoView {
    let _auth = use_context::<AuthResource>().expect("AuthResource missing");
    let _navigate = use_navigate();

    let f_name     = RwSignal::new(String::new());
    let f_desc     = RwSignal::new(String::new());
    let f_cat: RwSignal<Vec<String>> = RwSignal::new(vec![]);
    let f_date     = RwSignal::new(String::new());
    let f_time     = RwSignal::new(String::new());
    let f_end_time = RwSignal::new(String::new());
    let f_venue    = RwSignal::new(String::new());
    let f_city     = RwSignal::new(String::new());
    let f_lat      = RwSignal::new(DEFAULT_LAT);
    let f_lng      = RwSignal::new(DEFAULT_LNG);
    let loc_touched = RwSignal::new(false);

    // Peta di-init dua jalur (idempoten, guard `_leaflet_id` di shell):
    // (1) skrip auto-init di shell via data-attribute — jalan tanpa hydration;
    // (2) Effect ini — jalur hydration/SPA sebagai cadangan.
    Effect::new(move |_| {
        map_picker(
            "create-loc-map",
            "create-lat",
            "create-lng",
            f_lat.get_untracked(),
            f_lng.get_untracked(),
        );
    });

    let cover_preview: RwSignal<Option<String>> = RwSignal::new(None);

    let submitting = RwSignal::new(false);
    let error_msg  = RwSignal::new(String::new());
    let success_msg = RwSignal::new(String::new());

    // Cover image preview (WASM-only)
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

    let do_submit = move |_: leptos::ev::MouseEvent| {
        error_msg.set(String::new());
        success_msg.set(String::new());

        let name = f_name.get_untracked();
        if name.trim().is_empty() { error_msg.set("Nama event wajib diisi.".into()); return; }
        let desc = f_desc.get_untracked();
        if desc.trim().is_empty() { error_msg.set("Deskripsi event wajib diisi.".into()); return; }
        let cats = f_cat.get_untracked();
        if cats.is_empty() { error_msg.set("Pilih minimal satu kategori.".into()); return; }
        let date = f_date.get_untracked();
        if date.trim().is_empty() { error_msg.set("Tanggal event wajib diisi.".into()); return; }
        let time = f_time.get_untracked();
        if time.trim().is_empty() {
            error_msg.set("Waktu mulai wajib diisi.".into()); return;
        }
        let venue = f_venue.get_untracked();
        if venue.trim().is_empty() { error_msg.set("Nama venue wajib diisi.".into()); return; }
        let city = f_city.get_untracked();
        if city.trim().is_empty() { error_msg.set("Kota wajib diisi.".into()); return; }

        let cats_str = cats.join(",");
        let start_iso = format!("{}T{}:00Z", date, time);
        let (lat, lng) = if loc_touched.get_untracked() {
            (Some(f_lat.get_untracked()), Some(f_lng.get_untracked()))
        } else {
            (None, None)
        };
        submitting.set(true);

        leptos::task::spawn_local(async move {
            match create_merchant_event(name, desc, venue, city, start_iso.clone(), start_iso, cats_str, lat, lng).await {
                Ok(_slug) => {
                    success_msg.set("Event berhasil dibuat!".into());
                    submitting.set(false);
                    #[cfg(target_arch = "wasm32")]
                    if let Some(win) = web_sys::window() {
                        let path = if _slug.is_empty() { "/merchant".to_string() }
                            else { format!("/merchant/events/{}/edit", _slug) };
                        let _ = win.location().replace(&path);
                    }
                }
                Err(e) => {
                    error_msg.set(format!("Gagal membuat event: {}", e));
                    submitting.set(false);
                }
            }
        });
    };

    view! {
        <div class="medit-page">
            <header class="page-header medit-page-header">
                <A href="/merchant" attr:class="back-btn" attr:aria-label="Kembali">
                    <svg
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                    >
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </A>
                <span class="page-logo">"BUAT EVENT"</span>
                <div class="header-actions">
                    <ThemeToggle />
                    <A href="/notifications" attr:class="bell-btn" attr:aria-label="Notifikasi">
                        <svg
                            width="18"
                            height="18"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                        >
                            <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
                            <path d="M13.73 21a2 2 0 0 1-3.46 0" />
                        </svg>
                    </A>
                </div>
            </header>

            <div class="medit-container">

                // ── Feedback ──────────────────────────────────────────────────
                {move || {
                    (!error_msg.get().is_empty())
                        .then(|| {
                            view! {
                                <div class="medit-error-banner">
                                    <svg
                                        width="14"
                                        height="14"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                    >
                                        <circle cx="12" cy="12" r="10" />
                                        <line x1="12" y1="8" x2="12" y2="12" />
                                        <line x1="12" y1="16" x2="12.01" y2="16" />
                                    </svg>
                                    {move || error_msg.get()}
                                </div>
                            }
                        })
                }}
                {move || {
                    (!success_msg.get().is_empty())
                        .then(|| {
                            view! {
                                <div class="medit-success-banner">
                                    <svg
                                        width="14"
                                        height="14"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                    >
                                        <polyline points="20 6 9 17 4 12" />
                                    </svg>
                                    {move || success_msg.get()}
                                </div>
                            }
                        })
                }}
                // ── INFO DASAR ────────────────────────────────────────────────
                <div class="medit-section-header">
                    <span class="medit-section-label">"INFO DASAR"</span>
                </div>
                <div class="medit-field-group">
                    <label class="medit-field-label">"NAMA EVENT"</label>
                    <input
                        type="text"
                        class="medit-input"
                        placeholder="cth. Konser Jazz Malam Akhir Pekan"
                        prop:value=move || f_name.get()
                        on:input=move |e| f_name.set(event_target_value(&e))
                    />
                </div>
                <div class="medit-field-group">
                    <label class="medit-field-label">"DESKRIPSI"</label>
                    <textarea
                        class="medit-input medit-textarea"
                        placeholder="Ceritakan tentang event Anda..."
                        prop:value=move || f_desc.get()
                        on:input=move |e| f_desc.set(event_target_value(&e))
                    ></textarea>
                </div>
                // ── KATEGORI ──────────────────────────────────────────────────
                <div class="medit-field-group">
                    <label class="medit-field-label">"KATEGORI"</label>
                    <div class="medit-category-grid">
                        {CATEGORIES
                            .iter()
                            .map(|cat| {
                                let c = cat.to_string();
                                let c2 = c.clone();
                                view! {
                                    <label class="medit-checkbox-label">
                                        <input
                                            type="checkbox"
                                            on:change=move |_| {
                                                f_cat
                                                    .update(|cats| {
                                                        if cats.contains(&c) {
                                                            cats.retain(|x| x != &c);
                                                        } else {
                                                            cats.push(c.clone());
                                                        }
                                                    });
                                            }
                                        />
                                        <span>{c2}</span>
                                    </label>
                                }
                            })
                            .collect_view()}
                    </div>
                </div>
                // ── FOTO COVER ────────────────────────────────────────────────
                <div class="medit-field-group">
                    <label class="medit-field-label">"FOTO COVER"</label>
                    <div class="medit-file-input-wrapper">
                        <input
                            type="file"
                            class="medit-file-input"
                            accept="image/*"
                            on:change=on_cover_change
                        />
                        <span class="medit-file-input-label">"PILIH FOTO"</span>
                    </div>
                    {move || {
                        cover_preview
                            .get()
                            .map(|url| {
                                view! {
                                    <div class="medit-cover-preview">
                                        <img src=url alt="Cover preview" />
                                    </div>
                                }
                            })
                    }}
                </div>
                // ── STORY PREVIEW ─────────────────────────────────────────────
                <EventStoryPreviewInline
                    title=Signal::derive(move || f_name.get())
                    cover_url=Signal::derive(move || cover_preview.get())
                    description=Signal::derive(move || f_desc.get())
                    on_share_click=Callback::new(move |_| {
                        #[cfg(target_arch = "wasm32")]
                        {
                            let params = web_sys::UrlSearchParams::new().expect("UrlSearchParams");
                            params.append("event_title", &f_name.get_untracked());
                            params
                                .append(
                                    "event_cover",
                                    &cover_preview.get_untracked().unwrap_or_default(),
                                );
                            params.append("event_desc", &f_desc.get_untracked());
                            params.append("event_slug", "draft");
                            params.append("from_create", "1");
                            let qs = params.to_string();
                            _navigate(&format!("/story?{}", qs), Default::default());
                        }
                    })
                />
                // ── TANGGAL & WAKTU ───────────────────────────────────────────
                <div class="medit-field-group">
                    <label class="medit-field-label">"TANGGAL EVENT"</label>
                    <input
                        type="date"
                        class="medit-input"
                        prop:value=move || f_date.get()
                        on:input=move |e| f_date.set(event_target_value(&e))
                    />
                </div>
                <div class="medit-grid-2">
                    <div class="medit-field-group">
                        <label class="medit-field-label">"WAKTU MULAI"</label>
                        <input
                            type="time"
                            class="medit-input"
                            prop:value=move || f_time.get()
                            on:input=move |e| f_time.set(event_target_value(&e))
                        />
                    </div>
                    <div class="medit-field-group">
                        <label class="medit-field-label">"WAKTU SELESAI"</label>
                        <input
                            type="time"
                            class="medit-input"
                            prop:value=move || f_end_time.get()
                            on:input=move |e| f_end_time.set(event_target_value(&e))
                        />
                    </div>
                </div>
                // ── VENUE ─────────────────────────────────────────────────────
                <div class="medit-field-group">
                    <label class="medit-field-label">"NAMA VENUE"</label>
                    <input
                        type="text"
                        class="medit-input"
                        placeholder="cth. Gelora Bung Karno"
                        prop:value=move || f_venue.get()
                        on:input=move |e| f_venue.set(event_target_value(&e))
                    />
                </div>
                <div class="medit-field-group">
                    <label class="medit-field-label">"KOTA"</label>
                    <input
                        type="text"
                        class="medit-input"
                        placeholder="cth. Jakarta Pusat"
                        prop:value=move || f_city.get()
                        on:input=move |e| f_city.set(event_target_value(&e))
                    />
                </div>
                // ── LOKASI DI PETA ────────────────────────────────────────────
                <div class="medit-section-header">
                    <span class="medit-section-label">"LOKASI DI PETA"</span>
                </div>
                <p style="font-size:12px;color:var(--text-muted);margin:0 0 10px">
                    "Klik peta atau geser pin untuk menandai lokasi venue. Koordinat terisi otomatis."
                </p>
                <div
                    id="create-loc-map"
                    data-map-picker="1"
                    data-lat-input="create-lat"
                    data-lng-input="create-lng"
                    style="width:100%;height:300px;border-radius:12px;overflow:hidden;border:1px solid var(--border);margin-bottom:12px;background:var(--bg-elevated)"
                ></div>
                <div class="medit-grid-2">
                    <div class="medit-field-group">
                        <label class="medit-field-label">"LATITUDE"</label>
                        <input
                            id="create-lat"
                            type="number"
                            step="any"
                            class="medit-input"
                            placeholder="-6.2088"
                            prop:value=move || f_lat.get().to_string()
                            on:input=move |e| {
                                loc_touched.set(true);
                                if let Ok(v) = event_target_value(&e).parse::<f64>() {
                                    f_lat.set(v);
                                    map_set("create-loc-map", v, f_lng.get_untracked());
                                }
                            }
                        />
                    </div>
                    <div class="medit-field-group">
                        <label class="medit-field-label">"LONGITUDE"</label>
                        <input
                            id="create-lng"
                            type="number"
                            step="any"
                            class="medit-input"
                            placeholder="106.8456"
                            prop:value=move || f_lng.get().to_string()
                            on:input=move |e| {
                                loc_touched.set(true);
                                if let Ok(v) = event_target_value(&e).parse::<f64>() {
                                    f_lng.set(v);
                                    map_set("create-loc-map", f_lat.get_untracked(), v);
                                }
                            }
                        />
                    </div>
                </div>
                // ── SUBMIT ────────────────────────────────────────────────────
                <div class="medit-actions">
                    <button
                        class="medit-submit-btn"
                        disabled=move || submitting.get()
                        on:click=do_submit
                    >
                        {move || if submitting.get() { "Membuat Event..." } else { "BUAT EVENT" }}
                    </button>
                    <A href="/merchant" attr:class="medit-cancel-btn">
                        "BATAL"
                    </A>
                </div>

            </div>
        </div>
    }
}
