//! `/merchant/events/:slug/edit` — halaman edit event dari sisi merchant.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;

use crate::csr::components::detail_image_section::{
    collect_detail_drafts, DetailImageDraft, DetailImagesSection,
};
use crate::csr::components::event_story_preview::EventStoryPreviewInline;
use crate::csr::components::merchant_dashboard_event::MerchantEventSkeleton;
use crate::csr::hooks::{use_auth, use_nav, ThemeToggle};
use crate::csr::models::categories::CATEGORIES;
use crate::csr::models::Event;
use crate::csr::services::event::{self as event_svc, UpdateVariantPayload};
use leptos_router::components::A;

// ─── helpers ─────────────────────────────────────────────────────────────────

fn iso_to_date(s: &str) -> String {
    if s.len() >= 10 {
        s[..10].to_string()
    } else {
        s.to_string()
    }
}

fn iso_to_time(s: &str) -> String {
    s.split('T')
        .nth(1)
        .and_then(|t| t.get(..5))
        .unwrap_or("00:00")
        .to_string()
}

/// Kombinasikan date "2024-08-15" + time "14:30" → "2024-08-15T14:30:00Z".
fn combine_to_iso_datetime(date: &str, time: &str) -> Option<String> {
    let d = date.trim();
    let t = time.trim();
    if d.is_empty() || t.is_empty() {
        None
    } else {
        Some(format!("{}T{}:00Z", d, t))
    }
}

/// Konversi date "2024-08-15" → "2024-08-15T00:00:00Z".
fn date_to_iso_datetime(date: &str) -> Option<String> {
    let d = date.trim();
    if d.is_empty() {
        None
    } else {
        Some(format!("{}T00:00:00Z", d))
    }
}

// ─── Variant draft ────────────────────────────────────────────────────────────

#[derive(Clone)]
struct VariantDraft {
    id: Option<String>,
    name: RwSignal<String>,
    price: RwSignal<String>,
    sale_price: RwSignal<String>,
    sale_start: RwSignal<String>,
    sale_end: RwSignal<String>,
    quota: RwSignal<String>,
    is_active: RwSignal<bool>,
}

impl VariantDraft {
    fn from_tier(t: &crate::csr::models::TicketTier) -> Self {
        Self {
            id: Some(t.id.clone()),
            name: RwSignal::new(t.name.clone()),
            price: RwSignal::new(t.price_idr.to_string()),
            sale_price: RwSignal::new(t.sale_price_idr.map(|p| p.to_string()).unwrap_or_default()),
            sale_start: RwSignal::new(
                t.sale_start_date
                    .as_deref()
                    .map(iso_to_date)
                    .unwrap_or_default(),
            ),
            sale_end: RwSignal::new(
                t.sale_end_date
                    .as_deref()
                    .map(iso_to_date)
                    .unwrap_or_default(),
            ),
            quota: RwSignal::new(t.total.to_string()),
            is_active: RwSignal::new(t.is_active),
        }
    }

    fn blank() -> Self {
        Self {
            id: None,
            name: RwSignal::new(String::new()),
            price: RwSignal::new(String::new()),
            sale_price: RwSignal::new(String::new()),
            sale_start: RwSignal::new(String::new()),
            sale_end: RwSignal::new(String::new()),
            quota: RwSignal::new(String::new()),
            is_active: RwSignal::new(true),
        }
    }

    fn to_payload(&self) -> Option<UpdateVariantPayload> {
        let name_s = self.name.get_untracked();
        if name_s.trim().is_empty() {
            return None;
        }

        let price_f = self.price.get_untracked().trim().parse::<f64>().ok();
        let sale_f = self.sale_price.get_untracked().trim().parse::<f64>().ok();
        let quota_i = self.quota.get_untracked().trim().parse::<i32>().ok();
        let s_start = self.sale_start.get_untracked();
        let s_end = self.sale_end.get_untracked();

        Some(UpdateVariantPayload {
            id: self.id.clone(),
            name: Some(name_s),
            price: price_f,
            sale_price: sale_f,
            sale_price_start_date: if s_start.trim().is_empty() {
                None
            } else {
                Some(s_start.trim().to_string())
            },
            sale_price_end_date: if s_end.trim().is_empty() {
                None
            } else {
                Some(s_end.trim().to_string())
            },
            quota: quota_i,
            description: None,
            max_per_order: None,
            is_active: Some(self.is_active.get_untracked()),
            sort_order: None,
        })
    }
}

// ─── Page component ───────────────────────────────────────────────────────────

#[component]
pub fn MerchantEditEventPage() -> impl IntoView {
    let params = use_params_map();
    let auth = use_auth();
    let navigate = use_nav();

    // FIX: Simpan slug sebagai StoredValue<String> (yang impl Copy).
    // Tanpa ini, `event_slug: String` di-move ke dalam `do_submit`, lalu
    // `do_submit` di-move ke reactive closure → closure jadi FnOnce, bukan FnMut.
    // StoredValue<T> selalu Copy, jadi do_submit bisa dipanggil berkali-kali.
    let event_slug: StoredValue<String> =
        StoredValue::new(params.with_untracked(|p| p.get("slug").unwrap_or_default()));

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
                    .map(|p| p.membership_tier == "MERCHANT" || p.role == "admin")
                    .unwrap_or(false)
            });
            if !ok {
                nav("/merchant", Default::default());
            }
        });
    }

    let event_sig: RwSignal<Option<Event>> = RwSignal::new(None);
    let loading = RwSignal::new(true);
    let submitting = RwSignal::new(false);
    let upload_progress = RwSignal::new(String::new());
    let error_msg = RwSignal::new(String::new());
    let success_msg = RwSignal::new(String::new());

    // Form fields — semua RwSignal, semua Copy
    let f_name = RwSignal::new(String::new());
    let f_id = RwSignal::new(String::new());
    let f_desc = RwSignal::new(String::new());
    let f_cat: RwSignal<Vec<String>> = RwSignal::new(vec![]);
    let f_date = RwSignal::new(String::new());
    let f_time = RwSignal::new(String::new());
    let f_end_time = RwSignal::new(String::new());
    let f_venue = RwSignal::new(String::new());
    let f_city = RwSignal::new(String::new());

    let cover_file: RwSignal<Option<web_sys::File>> = RwSignal::new(None);
    let cover_preview: RwSignal<Option<String>> = RwSignal::new(None);

    let on_cover_change = move |ev: leptos::ev::Event| {
        use leptos::wasm_bindgen::JsCast;
        let input: web_sys::HtmlInputElement = ev.target().unwrap().unchecked_into();
        let files = input.files().unwrap();
        if let Some(file) = files.get(0) {
            let url = web_sys::Url::create_object_url_with_blob(&file).unwrap_or_default();
            cover_preview.set(Some(url));
            cover_file.set(Some(file));
        }
    };

    let variants: RwSignal<Vec<VariantDraft>> = RwSignal::new(vec![]);
    let detail_drafts: RwSignal<Vec<DetailImageDraft>> = RwSignal::new(vec![]);
    let nav_store = StoredValue::new(navigate.clone());

    // Fetch data on mount — event_slug.get_value() klon String tiap kali
    Effect::new(move |_| {
        let slug = event_slug.get_value();
        spawn_local(async move {
            match event_svc::get_event(&slug).await {
                Ok(event) => {
                    f_name.set(event.title.clone());
                    f_id.set(event.id.clone());
                    f_desc.set(event.description.clone());
                    f_cat.set(event.category.clone());
                    f_date.set(iso_to_date(&event.start_time));
                    f_time.set(iso_to_time(&event.start_time));
                    f_end_time.set(
                        event
                            .end_time
                            .as_deref()
                            .map(iso_to_time)
                            .unwrap_or_default(),
                    );
                    f_venue.set(event.venue.name.clone());
                    f_city.set(event.venue.city.clone());
                    cover_preview.set(Some(event.cover_url.clone()));
                    variants.set(event.tiers.iter().map(VariantDraft::from_tier).collect());
                    detail_drafts.set(
                        event
                            .detail_images
                            .iter()
                            .map(DetailImageDraft::from_existing)
                            .collect(),
                    );
                    event_sig.set(Some(event));
                    loading.set(false);
                }
                Err(e) => {
                    error_msg.set(format!("Gagal memuat event: {}", e));
                    loading.set(false);
                }
            }
        });
    });

    // do_submit sekarang hanya capture:
    //   - RwSignal<T>          → Copy ✓
    //   - StoredValue<T>       → Copy ✓
    // Tidak ada String/Vec/non-Copy → closure jadi Fn, bukan FnOnce.
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

        let parsed_variants: Vec<UpdateVariantPayload> = variants
            .get_untracked()
            .iter()
            .filter_map(|v| v.to_payload())
            .collect();
        if parsed_variants.is_empty() {
            error_msg.set("Minimal satu variant tiket dengan harga & kuota valid.".into());
            return;
        }

        submitting.set(true);

        // Klon dari StoredValue tiap klik — tidak consume closure
        let _slug = event_slug.get_value();
        let nav = nav_store.get_value();
        let image_file = cover_file.get_untracked();
        let id = f_id.get_untracked();

        spawn_local(async move {
            upload_progress.set("Mengupload foto...".into());

            let (detail_upload, detail_retain) =
                collect_detail_drafts(&detail_drafts.get_untracked()).await;

            match event_svc::update_event(
                &id,
                Some(name),
                Some(desc),
                Some(venue),
                Some(city),
                date_to_iso_datetime(&date), // event_date → ISO datetime
                combine_to_iso_datetime(&date, &time), // start_time → ISO datetime
                combine_to_iso_datetime(&date, &end_time), // end_time → ISO datetime
                Some(cats),
                Some(parsed_variants),
                detail_upload,
                if detail_retain.is_empty() {
                    None
                } else {
                    Some(detail_retain)
                },
                image_file,
            )
            .await
            {
                Ok(_) => {
                    success_msg.set("Event berhasil diperbarui!".into());
                    upload_progress.set(String::new());
                    submitting.set(false);
                    spawn_local(async move {
                        gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
                        nav(&format!("/merchant"), Default::default());
                    });
                }
                Err(e) => {
                    error_msg.set(format!("Gagal memperbarui event: {}", e));
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
                view! { <div>"Loading auth..."</div> }.into_any()
            } else if auth.is_authenticated() {
                if loading.get() {
                    view! {
                        <div class="medit-page">
                            <header class="page-header medit-page-header">
                                <button
                                    class="back-btn"
                                    aria-label="Kembali"
                                    on:click=move |_| {
                                        nav_store
                                            .get_value()(&format!("/merchant"), Default::default());
                                    }
                                >
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
                                </button>
                                <span class="page-logo">"EDIT EVENT"</span>
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
                } else {
                    view! {
                        <div class="medit-page">
                            <header class="page-header medit-page-header">
                                <button
                                    class="back-btn"
                                    aria-label="Kembali"
                                    on:click=move |_| {
                                        nav_store
                                            .get_value()(&format!("/merchant"), Default::default());
                                    }
                                >
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
                                </button>
                                <span class="page-logo">"EDIT EVENT"</span>
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

                                <div class="medit-field-group">
                                    <label class="medit-field-label">"KATEGORI"</label>
                                    <div class="medit-category-grid">
                                        {CATEGORIES
                                            .iter()
                                            .map(|cat| {
                                                let cat_owned = cat.to_string();
                                                let cat_for_check = cat_owned.clone();
                                                let cat_for_change = cat_owned.clone();
                                                view! {
                                                    <label class="medit-checkbox-label">
                                                        <input
                                                            type="checkbox"
                                                            prop:checked=move || {
                                                                f_cat.with(|cats| cats.contains(&cat_for_check))
                                                            }
                                                            on:change={
                                                                let c = cat_for_change.clone();
                                                                move |ev| {
                                                                    use leptos::wasm_bindgen::JsCast;
                                                                    let checked = ev
                                                                        .target()
                                                                        .and_then(|t| {
                                                                            t.dyn_into::<web_sys::HtmlInputElement>().ok()
                                                                        })
                                                                        .map(|el| el.checked())
                                                                        .unwrap_or(false);
                                                                    f_cat
                                                                        .update(|cats| {
                                                                            if checked {
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

                                <div class="medit-field-group">
                                    <label class="medit-field-label">"FOTO COVER"</label>
                                    <div class="medit-file-input-wrapper">
                                        <input
                                            type="file"
                                            class="medit-file-input"
                                            accept="image/*"
                                            on:change=on_cover_change
                                        />
                                        <span class="medit-file-input-label">
                                            "PILIH FOTO BARU (OPSIONAL)"
                                        </span>
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
                                            params.append("event_slug", &event_slug.get_value());
                                            nav(
                                                &format!("/stories/new?{}", params.to_string()),
                                                Default::default(),
                                            );
                                        }
                                    })
                                />

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

                                <div class="medit-section-header">
                                    <span class="medit-section-label">"FOTO DETAIL"</span>
                                </div>

                                <DetailImagesSection drafts=detail_drafts />

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
                                                    let sold_count = event_sig
                                                        .with(|e| {
                                                            e.as_ref()
                                                                .and_then(|evt| evt.tiers.get(i))
                                                                .map(|t| t.sold)
                                                                .unwrap_or(0)
                                                        });
                                                    let sold_display = if sold_count > 0 {
                                                        format!("{} terjual", sold_count)
                                                    } else {
                                                        String::new()
                                                    };
                                                    let show_sold = !sold_display.is_empty();
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
                                                                        width="16"
                                                                        height="16"
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

                                                            {show_sold
                                                                .then(move || {
                                                                    view! {
                                                                        <div class="medit-sold-badge">
                                                                            <svg
                                                                                width="11"
                                                                                height="11"
                                                                                viewBox="0 0 24 24"
                                                                                fill="none"
                                                                                stroke="currentColor"
                                                                                stroke-width="2.5"
                                                                            >
                                                                                <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
                                                                            </svg>
                                                                            {sold_display.clone()}
                                                                        </div>
                                                                    }
                                                                })}

                                                            <div class="medit-field-group" style="margin-top:14px">
                                                                <label class="medit-field-label">"HARGA (IDR)"</label>
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
                                                                    "HARGA PROMO (IDR)"
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
                                                                <label class="medit-field-label">"TOTAL KUOTA"</label>
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
                                                                        width="16"
                                                                        height="16"
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
                                                                    <label class="medit-field-label">"MULAI PROMO"</label>
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
                                                                    <label class="medit-field-label">"AKHIR PROMO"</label>
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

                                                            <div class="medit-active-row">
                                                                <span class="medit-field-label">"AKTIFKAN"</span>
                                                                <label class="mhub-tier-toggle">
                                                                    <input
                                                                        type="checkbox"
                                                                        prop:checked=move || v.is_active.get()
                                                                        on:change={
                                                                            let va = v.is_active;
                                                                            move |e| {
                                                                                use leptos::wasm_bindgen::JsCast;
                                                                                let c = e
                                                                                    .target()
                                                                                    .and_then(|t| {
                                                                                        t.dyn_into::<web_sys::HtmlInputElement>().ok()
                                                                                    })
                                                                                    .map(|el| el.checked())
                                                                                    .unwrap_or(true);
                                                                                va.set(c);
                                                                            }
                                                                        }
                                                                    />
                                                                    <span class="mhub-tier-toggle-label">
                                                                        {move || if v.is_active.get() { "Ya" } else { "Tidak" }}
                                                                    </span>
                                                                </label>
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
                                        variants.update(|vs| vs.push(VariantDraft::blank()))
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
                                    "TAMBAH VARIAN"
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
                                                        width="15"
                                                        height="15"
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
                                                        width="15"
                                                        height="15"
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
                                    disabled=move || loading.get() || submitting.get()
                                    on:click=do_submit
                                >
                                    {move || {
                                        if submitting.get() {
                                            view! { <span>"MENYIMPAN..."</span> }.into_any()
                                        } else {
                                            view! {
                                                <span>"PERBARUI ACARA"</span>
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
                }
            } else {
                view! { <div>"Tidak terautentikasi"</div> }.into_any()
            }
        }}
    }
}
