//! `/merchant/events/new` — halaman buat event baru dari sisi merchant.
//!
//! PERBAIKAN:
//! - Sesuai parameter real API di event service
//! - Proper error handling dan validasi
//! - Correct async/await + spawn_local usage

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::csr::components::detail_image_section::{
    collect_detail_drafts, DetailImageDraft, DetailImagesSection,
};
use crate::csr::components::event_story_preview::EventStoryPreviewInline;
use crate::csr::components::merchant_dashboard_event::MerchantEventSkeleton;
use crate::csr::hooks::{use_auth, use_nav, ThemeToggle};
use crate::csr::models::categories::CATEGORIES;
use crate::csr::services::event::{self as event_svc, NewVariant};
use leptos_router::components::A;

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Konversi date "2024-08-15" → "2024-08-15T00:00:00Z" (format DateTime<Utc> yang dibutuhkan BE).
fn date_to_iso_datetime(date: &str) -> String {
    let d = date.trim();
    if d.is_empty() {
        String::new()
    } else {
        format!("{}T00:00:00Z", d)
    }
}

/// Kombinasikan date "2024-08-15" + time "14:30" → "2024-08-15T14:30:00Z".
/// Return None jika salah satu kosong.
fn combine_to_iso_datetime(date: &str, time: &str) -> Option<String> {
    let d = date.trim();
    let t = time.trim();
    if d.is_empty() || t.is_empty() {
        None
    } else {
        Some(format!("{}T{}:00Z", d, t))
    }
}

// ─── Variant draft ────────────────────────────────────────────────────────────

#[derive(Clone)]
struct VariantDraft {
    name: RwSignal<String>,
    price: RwSignal<String>,
    sale_price: RwSignal<String>,
    sale_start: RwSignal<String>,
    sale_end: RwSignal<String>,
    quota: RwSignal<String>,
    max_order: RwSignal<String>,
}

impl VariantDraft {
    fn blank(name: &str) -> Self {
        Self {
            name: RwSignal::new(name.to_string()),
            price: RwSignal::new(String::new()),
            sale_price: RwSignal::new(String::new()),
            sale_start: RwSignal::new(String::new()),
            sale_end: RwSignal::new(String::new()),
            quota: RwSignal::new(String::new()),
            max_order: RwSignal::new(String::new()),
        }
    }

    fn to_new_variant(&self) -> Option<NewVariant> {
        let name = self.name.get_untracked();
        if name.trim().is_empty() {
            return None;
        }

        let price = self.price.get_untracked().trim().parse::<f64>().ok()?;
        let quota = self.quota.get_untracked().trim().parse::<i32>().ok()?;
        let sale_price = self.sale_price.get_untracked().trim().parse::<f64>().ok();

        let sale_start = {
            let s = self.sale_start.get_untracked();
            if s.trim().is_empty() {
                None
            } else {
                Some(s.trim().to_string())
            }
        };
        let sale_end = {
            let s = self.sale_end.get_untracked();
            if s.trim().is_empty() {
                None
            } else {
                Some(s.trim().to_string())
            }
        };
        let max_order = self.max_order.get_untracked().trim().parse::<i32>().ok();

        Some(NewVariant {
            name,
            price,
            quota,
            description: None,
            sale_price,
            sale_price_start_date: sale_start,
            sale_price_end_date: sale_end,
            max_per_order: max_order,
            sort_order: None,
        })
    }
}

// ─── Page ─────────────────────────────────────────────────────────────────────

#[component]
pub fn MerchantCreateEventPage() -> impl IntoView {
    let auth = use_auth();
    let navigate = use_nav();

    // Auth gate
    {
        let nav = navigate.clone();
        Effect::new(move |_| {
            if auth.is_loading.get() {
                return;
            }
            if !auth.is_authenticated() {
                nav("/login", Default::default());
                return;
            }
            let ok = auth.user.with(|u| {
                u.as_ref()
                    .map(|p| p.membership_tier == "MERCHANT")
                    .unwrap_or(false)
            });
            if !ok {
                nav("/merchant", Default::default());
            }
        });
    }

    // Form state
    let f_name = RwSignal::new(String::new());
    let f_desc = RwSignal::new(String::new());
    let f_cat: RwSignal<Vec<String>> = RwSignal::new(vec![]);
    let f_date = RwSignal::new(String::new());
    let f_time = RwSignal::new(String::new());
    let f_end_time = RwSignal::new(String::new());
    let f_venue = RwSignal::new(String::new());
    let f_city = RwSignal::new(String::new());

    // Cover image
    let cover_file: RwSignal<Option<web_sys::File>> = RwSignal::new(None);
    let cover_preview: RwSignal<Option<String>> = RwSignal::new(None);

    let on_cover_change = move |ev: leptos::ev::Event| {
        use leptos::wasm_bindgen::JsCast;
        let input: web_sys::HtmlInputElement = ev.target().unwrap().unchecked_into();
        let files = input.files().unwrap();
        if let Some(file) = files.get(0) {
            let url = leptos::web_sys::Url::create_object_url_with_blob(&file).unwrap_or_default();
            cover_preview.set(Some(url));
            cover_file.set(Some(file));
        }
    };

    // Variants
    let variants: RwSignal<Vec<VariantDraft>> =
        RwSignal::new(vec![VariantDraft::blank("General Admission")]);

    // Detail images
    let detail_drafts: RwSignal<Vec<DetailImageDraft>> = RwSignal::new(vec![]);

    let submitting = RwSignal::new(false);
    let upload_progress = RwSignal::new(String::new());
    let error_msg = RwSignal::new(String::new());
    let success_msg = RwSignal::new(String::new());

    let nav_store = StoredValue::new(navigate.clone());

    let do_submit = move |_: leptos::ev::MouseEvent| {
        error_msg.set(String::new());
        success_msg.set(String::new());
        upload_progress.set(String::new());

        let name = f_name.get_untracked();
        if name.trim().is_empty() {
            error_msg.set("Nama event wajib diisi.".into());
            return;
        }

        let desc = f_desc.get_untracked();
        if desc.trim().is_empty() {
            error_msg.set("Deskripsi event wajib diisi.".into());
            return;
        }

        let cats = f_cat.get_untracked();
        if cats.is_empty() {
            error_msg.set("Pilih minimal satu kategori.".into());
            return;
        }

        let date = f_date.get_untracked();
        if date.trim().is_empty() {
            error_msg.set("Tanggal event wajib diisi.".into());
            return;
        }

        let time = f_time.get_untracked();
        let end_time = f_end_time.get_untracked();
        if time.trim().is_empty() || end_time.trim().is_empty() {
            error_msg.set("Waktu mulai & selesai event wajib diisi.".into());
            return;
        }

        let venue = f_venue.get_untracked();
        if venue.trim().is_empty() {
            error_msg.set("Nama venue wajib diisi.".into());
            return;
        }

        let city = f_city.get_untracked();
        if city.trim().is_empty() {
            error_msg.set("Kota wajib diisi.".into());
            return;
        }

        let cover_file_val = cover_file.get_untracked();
        if cover_file_val.is_none() {
            error_msg.set("Foto cover event wajib diisi.".into());
            return;
        }

        let parsed_variants: Vec<NewVariant> = variants
            .get_untracked()
            .iter()
            .filter_map(|v| v.to_new_variant())
            .collect();

        if parsed_variants.is_empty() {
            error_msg.set("Minimal satu variant tiket dengan harga & kuota valid.".into());
            return;
        }

        submitting.set(true);

        let event_name = name.clone();
        let nav = nav_store.get_value();

        spawn_local(async move {
            use wasm_bindgen_futures::JsFuture;

            upload_progress.set("Mengupload foto...".into());

            // Collect detail images
            let (detail_upload, _) = collect_detail_drafts(&detail_drafts.get_untracked()).await;

            // Read cover file bytes
            let image_bytes = match &cover_file_val {
                Some(file) => match JsFuture::from(file.array_buffer()).await {
                    Ok(ab) => {
                        let arr = web_sys::js_sys::Uint8Array::new(&ab);
                        Some(arr.to_vec())
                    }
                    Err(_) => {
                        error_msg.set("Gagal membaca file cover image.".into());
                        submitting.set(false);
                        return;
                    }
                },
                None => None,
            };

            let image_type = cover_file_val.as_ref().map(|f| f.type_());

            // Get merchant name dari auth user
            let merchant_name = auth.user.with(|u| {
                u.as_ref()
                    .map(|p| p.full_name.clone())
                    .map(|p| p)
                    .unwrap_or_else(|| "Unknown Merchant".to_string())
            });

            match event_svc::create_event(
                merchant_name,                             // merchant_name: String
                event_name,                                // name: String
                desc,                                      // description: String
                cats,                                      // category: Vec<String>
                date_to_iso_datetime(&date),               // event_date: String (ISO 8601)
                combine_to_iso_datetime(&date, &time),     // start_time: Option<String>
                combine_to_iso_datetime(&date, &end_time), // end_time: Option<String>
                venue,                                     // venue: String
                city,                                      // city: String
                parsed_variants,                           // variants: Vec<NewVariant>
                detail_upload, // detail_items: Vec<DetailImageUploadItem>
                image_bytes,   // image_bytes: Option<Vec<u8>>
                image_type,    // image_type: Option<String>
            )
            .await
            {
                Ok(event) => {
                    success_msg.set("Event berhasil dibuat!".into());
                    upload_progress.set(String::new());
                    submitting.set(false);

                    spawn_local(async move {
                        gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
                        nav(
                            &format!("/merchant/events/{}/edit", event.slug),
                            Default::default(),
                        );
                    });
                }
                Err(e) => {
                    error_msg.set(format!("Gagal membuat event: {}", e));
                    upload_progress.set(String::new());
                    submitting.set(false);
                }
            }
        });
    };

    let avatar_url = move || {
        auth.user
            .with(|u| u.as_ref().map(|p| p.avatar_url.clone()).unwrap_or_default())
    };

    view! {
        {move || {
            if auth.is_loading.get() {
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
                                <A
                                    href="/notifications"
                                    attr:class="bell-btn"
                                    attr:aria-label="Notifikasi"
                                >
                                    <svg
                                        width="18"
                                        height="18"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                    >
                                        <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
                                        <path d="M13.73 21a2 2 0 0 1-3.46 0" />
                                    </svg>
                                    <span class="bell-dot"></span>
                                </A>
                                <div class="nav-avatar">
                                    {move || {
                                        let url = avatar_url();
                                        if url.is_empty() {
                                            view! {
                                                <svg
                                                    width="16"
                                                    height="16"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2"
                                                    stroke-linecap="round"
                                                >
                                                    <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
                                                    <circle cx="12" cy="7" r="4" />
                                                </svg>
                                            }
                                                .into_any()
                                        } else {
                                            view! { <img src=url alt="" /> }.into_any()
                                        }
                                    }}
                                </div>
                            </div>
                        </header>
                        <MerchantEventSkeleton />
                    </div>
                }
                    .into_any()
            } else if auth.is_authenticated() {
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
                                <A
                                    href="/notifications"
                                    attr:class="bell-btn"
                                    attr:aria-label="Notifikasi"
                                >
                                    <svg
                                        width="18"
                                        height="18"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                    >
                                        <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
                                        <path d="M13.73 21a2 2 0 0 1-3.46 0" />
                                    </svg>
                                    <span class="bell-dot"></span>
                                </A>
                                <div class="nav-avatar">
                                    {move || {
                                        let url = avatar_url();
                                        if url.is_empty() {
                                            view! {
                                                <svg
                                                    width="16"
                                                    height="16"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2"
                                                    stroke-linecap="round"
                                                >
                                                    <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
                                                    <circle cx="12" cy="7" r="4" />
                                                </svg>
                                            }
                                                .into_any()
                                        } else {
                                            view! { <img src=url alt="" /> }.into_any()
                                        }
                                    }}
                                </div>
                            </div>
                        </header>
                        <div class="medit-container">

                            // ── BASIC INFO ────────────────────────────────────────────────
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
                                />
                            </div>

                            // ── CATEGORY ──────────────────────────────────────────────────
                            <div class="medit-field-group">
                                <label class="medit-field-label">"KATEGORI"</label>
                                <div class="medit-category-grid">
                                    {CATEGORIES
                                        .iter()
                                        .map(|cat| {
                                            let cat_owned = cat.to_string();
                                            let cat_for_change = cat_owned.clone();
                                            // FIX: to_string() agar dapat String untuk contains(&String)
                                            view! {
                                                <label class="medit-checkbox-label">
                                                    <input
                                                        type="checkbox"
                                                        on:change={
                                                            let c = cat_for_change.clone();
                                                            move |ev| {
                                                                use leptos::wasm_bindgen::JsCast;
                                                                let is_checked = ev
                                                                    .target()
                                                                    .and_then(|t| {
                                                                        t.dyn_into::<web_sys::HtmlInputElement>().ok()
                                                                    })
                                                                    .map(|el| el.checked())
                                                                    .unwrap_or(false);
                                                                f_cat
                                                                    .update(|cats| {
                                                                        if is_checked {
                                                                            if !cats.contains(&c) {
                                                                                cats.push(c.clone());
                                                                            }
                                                                        } else {
                                                                            cats.retain(|x| x != &c);
                                                                        }
                                                                    });
                                                            }
                                                        }
                                                    />
                                                    <span>{cat_owned}</span>
                                                </label>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            </div>

                            // ── COVER IMAGE ───────────────────────────────────────────────
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
                                on_share_click=Callback::new({
                                    let nav = use_nav();
                                    move |_| {
                                        let params = web_sys::UrlSearchParams::new().unwrap();
                                        params.append("event_title", &f_name.get_untracked());
                                        params
                                            .append(
                                                "event_cover",
                                                &cover_preview.get_untracked().unwrap_or_default(),
                                            );
                                        params.append("event_desc", &f_desc.get_untracked());
                                        params.append("event_slug", "draft");
                                        params.append("from_create", "1");
                                        nav(
                                            &format!("/stories/new?{}", params.to_string()),
                                            Default::default(),
                                        );
                                    }
                                })
                            />

                            // ── DATE & TIME ───────────────────────────────────────────────
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
                                    placeholder="cth. Studio Musik Internasional"
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

                            // ── DETAIL IMAGES ─────────────────────────────────────────────
                            <div class="medit-section-header">
                                <span class="medit-section-label">"FOTO DETAIL"</span>
                            </div>

                            <DetailImagesSection drafts=detail_drafts />

                            // ── TICKET VARIANTS ───────────────────────────────────────────
                            <div class="medit-section-header">
                                <span class="medit-section-label">"TICKET VARIANTS"</span>
                            </div>

                            {move || {
                                variants
                                    .with(|vs| {
                                        vs.iter()
                                            .enumerate()
                                            .map(|(i, v)| {
                                                let v = v.clone();
                                                view! {
                                                    <div class="medit-variant-card">
                                                        <div class="medit-variant-header">
                                                            <span class="medit-variant-cat-label">"CATEGORY"</span>
                                                            <button
                                                                class="medit-variant-delete-btn"
                                                                aria-label="Hapus"
                                                                on:click=move |_| {
                                                                    variants
                                                                        .update(|vs| {
                                                                            if vs.len() > 1 && i < vs.len() {
                                                                                vs.remove(i);
                                                                            }
                                                                        });
                                                                }
                                                            >
                                                                <svg
                                                                    width="14"
                                                                    height="14"
                                                                    viewBox="0 0 24 24"
                                                                    fill="none"
                                                                    stroke="currentColor"
                                                                    stroke-width="2"
                                                                    stroke-linecap="round"
                                                                >
                                                                    <polyline points="3 6 5 6 21 6" />
                                                                    <path d="M19 6v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6m3 0V4a1 1 0 011-1h4a1 1 0 011 1v2" />
                                                                </svg>
                                                            </button>
                                                        </div>

                                                        <input
                                                            type="text"
                                                            class="medit-variant-name-input"
                                                            placeholder="Nama tiket (cth. VIP Backstage)"
                                                            prop:value=move || v.name.get()
                                                            on:input={
                                                                let vn = v.name;
                                                                move |e| vn.set(event_target_value(&e))
                                                            }
                                                        />

                                                        <div class="medit-field-group" style="margin-top:4px">
                                                            <label class="medit-field-label">"PRICE (IDR)"</label>
                                                            <div class="medit-rp-input-wrap">
                                                                <span class="medit-rp-prefix">"Rp"</span>
                                                                <input
                                                                    type="number"
                                                                    class="medit-input medit-input--rp"
                                                                    placeholder="0"
                                                                    prop:value=move || v.price.get()
                                                                    on:input={
                                                                        let vp = v.price;
                                                                        move |e| vp.set(event_target_value(&e))
                                                                    }
                                                                />
                                                            </div>
                                                        </div>

                                                        <div class="medit-field-group">
                                                            <label class="medit-field-label medit-field-label--sale">
                                                                "SALE PRICE (IDR)"
                                                            </label>
                                                            <div class="medit-rp-input-wrap">
                                                                <span class="medit-rp-prefix">"Rp"</span>
                                                                <input
                                                                    type="number"
                                                                    class="medit-input medit-input--rp"
                                                                    placeholder="Optional"
                                                                    prop:value=move || v.sale_price.get()
                                                                    on:input={
                                                                        let vs = v.sale_price;
                                                                        move |e| vs.set(event_target_value(&e))
                                                                    }
                                                                />
                                                            </div>
                                                        </div>

                                                        <div class="medit-field-group">
                                                            <label class="medit-field-label">"TOTAL QUOTA"</label>
                                                            <div class="medit-input-icon-row">
                                                                <input
                                                                    type="number"
                                                                    class="medit-input"
                                                                    placeholder="0"
                                                                    prop:value=move || v.quota.get()
                                                                    on:input={
                                                                        let vq = v.quota;
                                                                        move |e| vq.set(event_target_value(&e))
                                                                    }
                                                                />
                                                                <svg
                                                                    class="medit-input-icon"
                                                                    width="15"
                                                                    height="15"
                                                                    viewBox="0 0 24 24"
                                                                    fill="none"
                                                                    stroke="currentColor"
                                                                    stroke-width="2"
                                                                    stroke-linecap="round"
                                                                >
                                                                    <path d="M16 21v-2a4 4 0 00-4-4H6a4 4 0 00-4 4v2" />
                                                                    <circle cx="9" cy="7" r="4" />
                                                                    <line x1="19" y1="8" x2="19" y2="14" />
                                                                    <line x1="22" y1="11" x2="16" y2="11" />
                                                                </svg>
                                                            </div>
                                                        </div>

                                                        <div class="medit-grid-2">
                                                            <div class="medit-field-group">
                                                                <label class="medit-field-label">"SALE START"</label>
                                                                <input
                                                                    type="date"
                                                                    class="medit-input"
                                                                    prop:value=move || v.sale_start.get()
                                                                    on:input={
                                                                        let vs = v.sale_start;
                                                                        move |e| vs.set(event_target_value(&e))
                                                                    }
                                                                />
                                                            </div>
                                                            <div class="medit-field-group">
                                                                <label class="medit-field-label">"SALE END"</label>
                                                                <input
                                                                    type="date"
                                                                    class="medit-input"
                                                                    prop:value=move || v.sale_end.get()
                                                                    on:input={
                                                                        let ve = v.sale_end;
                                                                        move |e| ve.set(event_target_value(&e))
                                                                    }
                                                                />
                                                            </div>
                                                        </div>

                                                        <div class="medit-field-group">
                                                            <label class="medit-field-label">
                                                                "MAKS PER ORDER (opsional)"
                                                            </label>
                                                            <input
                                                                type="number"
                                                                class="medit-input"
                                                                placeholder="cth. 4"
                                                                prop:value=move || v.max_order.get()
                                                                on:input={
                                                                    let vm = v.max_order;
                                                                    move |e| vm.set(event_target_value(&e))
                                                                }
                                                            />
                                                        </div>
                                                    </div>
                                                }
                                            })
                                            .collect_view()
                                    })
                            }}

                            <button
                                class="medit-add-variant-row"
                                on:click=move |_| {
                                    variants.update(|vs| vs.push(VariantDraft::blank("")))
                                }
                            >
                                <span class="medit-add-variant-row-icon">
                                    <svg
                                        width="14"
                                        height="14"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2.5"
                                        stroke-linecap="round"
                                    >
                                        <line x1="12" y1="5" x2="12" y2="19" />
                                        <line x1="5" y1="12" x2="19" y2="12" />
                                    </svg>
                                </span>
                                "TAMBAH VARIANT"
                            </button>

                            {move || {
                                (!upload_progress.get().is_empty())
                                    .then(|| {
                                        view! {
                                            <div class="medit-success-banner" style="opacity:0.8">
                                                <svg
                                                    width="14"
                                                    height="14"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2.5"
                                                >
                                                    <polyline points="1 4 1 10 7 10" />
                                                    <path d="M3.51 15a9 9 0 1 0 .49-4" />
                                                </svg>
                                                {upload_progress.get()}
                                            </div>
                                        }
                                    })
                            }}

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
                                                    stroke-width="2.5"
                                                >
                                                    <circle cx="12" cy="12" r="10" />
                                                    <line x1="12" y1="8" x2="12" y2="12" />
                                                    <line x1="12" y1="16" x2="12.01" y2="16" />
                                                </svg>
                                                {error_msg.get()}
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
                                                    stroke-width="2.5"
                                                >
                                                    <polyline points="20 6 9 17 4 12" />
                                                </svg>
                                                {success_msg.get()}
                                            </div>
                                        }
                                    })
                            }}

                            <div style="height:90px"></div>
                        </div>

                        <div class="medit-sticky-footer">
                            <button
                                class="medit-update-btn"
                                disabled=move || submitting.get()
                                on:click=do_submit
                            >
                                {move || {
                                    if submitting.get() {
                                        view! { <span>"MENYIMPAN..."</span> }.into_any()
                                    } else {
                                        view! {
                                            <span>"TERBITKAN EVENT"</span>
                                            <svg
                                                width="16"
                                                height="16"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2.5"
                                                stroke-linecap="round"
                                            >
                                                <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
                                            </svg>
                                        }
                                            .into_any()
                                    }
                                }}
                            </button>
                        </div>
                    </div>
                }
                    .into_any()
            } else {
                view! { <div>"Tidak terautentikasi"</div> }.into_any()
            }
        }}
    }
}
