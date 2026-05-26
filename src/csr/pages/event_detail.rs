use crate::csr::hooks::use_nav;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::csr::components::{EmptyState, EventCard, EventCardShimmer};
use crate::csr::hooks::{format_idr, use_cart, ThemeToggle};
use crate::csr::models::{Artist, CartItem, Event, ListEventsRequest, TicketTier};
use crate::csr::services::event as event_svc;
use crate::csr::state::events::ExploreEvent;

fn render_artist(artist: Artist, active_img: RwSignal<String>) -> impl IntoView {
    let img_url = artist.image_url.clone();
    view! {
        <div
            class="artist-card"
            style="cursor:pointer"
            on:click=move |_| active_img.set(img_url.clone())
        >
            <img src=artist.image_url.clone() alt=artist.name.clone() class="artist-img" />
            <div class="artist-overlay">
                <span class="artist-role">{artist.role}</span>
                <span class="artist-name">{artist.name}</span>
            </div>
        </div>
    }
}

fn render_tier(
    tier: TicketTier,
    cart: crate::csr::hooks::CartCtx,
    ev_id: String,
    ev_title: String,
    ev_venue: String,
    ev_cover: String,
) -> impl IntoView {
    let is_vip = tier.r#type == "VIP";
    let card_cls = if is_vip {
        "tier-card tier-card--vip"
    } else {
        "tier-card"
    };

    // FIX #6: invece di 6 StoredValue separati (= 6 heap alloc per tier),
    // cattura tutto in una struct clonabile.  Una sola alloc, nessuna
    // frammentazione extra dell'heap WASM.
    #[derive(Clone)]
    struct TierCap {
        tier_id: String,
        tier_name: String,
        ev_id: String,
        ev_title: String,
        ev_venue: String,
        ev_cover: String,
    }
    let cap = TierCap {
        tier_id: tier.id.clone(),
        tier_name: tier.name.clone(),
        ev_id,
        ev_title,
        ev_venue,
        ev_cover,
    };
    let tier_price = tier.price_idr;

    // Ciascuna closure ha bisogno di una propria copia (String è Clone)
    let cap_add = cap.clone();
    let cap_rm = cap.clone();
    let cap_qty = cap.clone();

    let on_add = move |_| {
        cart.add_item(CartItem {
            event_id: cap_add.ev_id.clone(),
            tier_id: cap_add.tier_id.clone(),
            event_title: cap_add.ev_title.clone(),
            tier_name: cap_add.tier_name.clone(),
            venue_name: cap_add.ev_venue.clone(),
            event_cover: cap_add.ev_cover.clone(),
            quantity: 1,
            unit_price: tier_price,
        });
    };
    let on_remove = move |_| {
        let q = cart.get_qty(&cap_rm.tier_id);
        if q > 0 {
            cart.update_qty(&cap_rm.tier_id, q - 1);
        }
    };
    let qty = move || cart.get_qty(&cap_qty.tier_id);
    let value_qty = qty.clone();

    // Kloning on_add / on_remove agar bisa dipakai di dua cabang
    let on_add_add = on_add.clone();
    let on_add_plus = on_add.clone();

    view! {
        <div class=card_cls>
            <div class="tier-top">
                <div class="tier-name">{tier.name.clone()}</div>
                {(!tier.description.is_empty())
                    .then(|| view! { <p class="tier-desc">{tier.description.clone()}</p> })}
                {(is_vip && tier.available <= 15)
                    .then(|| {
                        view! {
                            <span class="tier-scarcity">
                                <span class="scarcity-dot"></span>
                                {format!("Only {} Left", tier.available)}
                            </span>
                        }
                    })}
            </div>
            {(!tier.perks.is_empty())
                .then(|| {
                    let perks = tier.perks.clone();
                    view! {
                        <div class="tier-perks">
                            {perks
                                .into_iter()
                                .map(|p| {
                                    view! {
                                        <span class="perk-tag">
                                            <svg
                                                width="12"
                                                height="12"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2.5"
                                                stroke-linecap="round"
                                            >
                                                <polyline points="20 6 9 17 4 12" />
                                            </svg>
                                            {p}
                                        </span>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                })}
            <div class="tier-bottom">
                <div>
                    <span class="tier-price">{format_idr(tier.price_idr)}</span>
                    {(!tier.zone.is_empty())
                        .then(|| view! { <span class="tier-zone">{tier.zone.clone()}</span> })}
                </div>
                // Saat qty = 0 → tombol Add (pill lime), saat qty > 0 → qty ctrl
                {move || {
                    if value_qty() == 0 {
                        view! {
                            <button class="tier-add-btn" on:click=on_add_add.clone()>
                                "Add"
                            </button>
                        }
                            .into_any()
                    } else {
                        view! {
                            <div class="qty-ctrl">
                                <button class="qty-btn qty-btn--minus" on:click=on_remove.clone()>
                                    "−"
                                </button>
                                <span class="qty-val">{value_qty()}</span>
                                <button class="qty-btn qty-btn--plus" on:click=on_add_plus.clone()>
                                    "+"
                                </button>
                            </div>
                        }
                            .into_any()
                    }
                }}
            </div>
        </div>
    }
}

#[component]
pub fn EventDetailPage() -> impl IntoView {
    let params = use_params_map();

    // Memo reaktif — berubah otomatis saat URL/params berubah
    let event_id = Memo::new(move |_| params.with(|p| p.get("slug").unwrap_or_default()));

    // FIX DOUBLE SHIMMER: Fetch event + related SECARA PARALEL dalam satu LocalResource.
    let combined_res = LocalResource::new(move || {
        let id = event_id.get();
        async move {
            if id.is_empty() {
                return (None::<Event>, None::<Vec<ExploreEvent>>);
            }

            let ev = match event_svc::get_event(&id).await.ok() {
                Some(e) => e,
                None => return (None, None),
            };

            let related = if let Some(cat) = ev.category.first().cloned() {
                let ev_id = ev.id.clone();
                let req = ListEventsRequest {
                    category: cat,
                    query: String::new(),
                    page: 1,
                    page_size: 6,
                };
                event_svc::list_events(&req).await.ok().map(|res| {
                    res.events
                        .into_iter()
                        .filter(|e| e.id != ev_id)
                        .take(5)
                        .map(|e| crate::csr::state::events::event_to_explore_pub(&e))
                        .collect::<Vec<_>>()
                })
            } else {
                None
            };

            (Some(ev), related)
        }
    });

    // ── FIX NAVIGASI: shimmer muncul ULANG saat pindah ke event lain ─────────
    //
    // Masalah sebelumnya: Suspense fallback hanya muncul saat initial load.
    // Setelah resource pernah resolved, Leptos tidak kembali ke fallback —
    // data lama tetap tampil sampai data baru ready → UI tiba-tiba berubah.
    //
    // Solusi: is_loading Memo yang membandingkan slug yang di-*request*
    // (event_id) dengan slug yang sudah ter-*load* (dari combined_res).
    // Saat navigasi: event_id berubah ke slug baru, combined_res masih
    // mengembalikan data lama → slugs tidak cocok → is_loading = true → shimmer.
    // Saat data baru ready: slug cocok → is_loading = false → konten muncul.
    let is_loading = Memo::new(move |_| {
        let requested = event_id.get();
        if requested.is_empty() {
            return true;
        }
        match combined_res.get() {
            None => true,
            Some(sw) => {
                let (ev_opt, _) = sw;
                match ev_opt {
                    Some(ev) if ev.slug == requested => false,
                    None => false,
                    _ => true,
                }
            }
        }
    });

    // Scroll otomatis ke atas saat mulai loading event baru
    Effect::new(move |_| {
        if is_loading.get() {
            let _ = web_sys::window().map(|w| w.scroll_to_with_x_and_y(0.0, 0.0));
        }
    });

    // Derived memo — signature render_event_detail tidak berubah
    let related_res_data = Memo::new(move |_| {
        combined_res.get().map(|sw| {
            let (_, rel) = sw;
            rel.clone()
        })
    });

    // Shimmer identik dengan fallback Suspense sebelumnya — di-extract
    // supaya tidak duplikat antara initial load dan navigasi
    let shimmer = move || {
        view! {
            <div class="page event-detail-page">
                <div class="shim" style="width:100%;height:280px;border-radius:0"></div>
                <div style="padding:20px 16px;display:flex;flex-direction:column;gap:14px">
                    <div class="shim" style="height:18px;width:80px;border-radius:100px"></div>
                    <div class="shim" style="height:32px;width:85%"></div>
                    <div class="shim" style="height:32px;width:60%"></div>
                    <div style="display:flex;gap:10px;margin-top:4px">
                        <div class="shim" style="height:13px;width:130px"></div>
                        <div class="shim" style="height:13px;width:100px"></div>
                    </div>
                    <div style="height:1px;background:var(--border-soft);margin:8px 0"></div>
                    <div class="shim" style="height:14px;width:120px"></div>
                    {(0..2)
                        .map(|_| {
                            view! {
                                <div style="display:flex;justify-content:space-between;align-items:center;padding:14px 0;border-bottom:1px solid var(--border-soft)">
                                    <div style="display:flex;flex-direction:column;gap:8px">
                                        <div class="shim" style="height:16px;width:130px"></div>
                                        <div class="shim" style="height:12px;width:80px"></div>
                                    </div>
                                    <div
                                        class="shim"
                                        style="height:36px;width:80px;border-radius:100px"
                                    ></div>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            </div>
        }
    };

    // Ganti Suspense dengan conditional render berbasis is_loading.
    // FIX BUG: sebelumnya arm None → EmptyState, padahal None berarti
    // combined_res masih loading (belum pernah resolved), bukan "not found".
    // Dengan is_loading, case tersebut sudah di-handle → tidak akan salah
    // tampil EmptyState saat sebenarnya sedang loading.
    view! {
        {move || {
            if is_loading.get() {
                shimmer().into_any()
            } else {
                match combined_res.get() {
                    None => {
                        view! {
                            <div class="page event-detail-page">
                                <EmptyState
                                    icon="🔍"
                                    title="EVENT TIDAK DITEMUKAN"
                                    body="Event ini mungkin sudah selesai atau telah dihapus."
                                    cta_label="JELAJAHI EVENT"
                                    cta_href="/"
                                />
                            </div>
                        }
                            .into_any()
                    }
                    Some(sw) => {
                        let (ev_opt, _) = sw;
                        match ev_opt {
                            None => {
                                view! {
                                    <div class="page event-detail-page">
                                        <EmptyState
                                            icon="🔍"
                                            title="EVENT TIDAK DITEMUKAN"
                                            body="Event ini mungkin sudah selesai atau telah dihapus."
                                            cta_label="JELAJAHI EVENT"
                                            cta_href="/"
                                        />
                                    </div>
                                }
                                .into_any()
                            }
                            Some(event) => {
                                render_event_detail(event.clone(), related_res_data).into_any()
                            }
                        }
                    }
                }
            }
        }}
    }
}

fn render_event_detail(
    event: Event,
    related_res_data: Memo<Option<Option<Vec<ExploreEvent>>>>,
) -> impl IntoView {
    let cart = use_cart();
    let navigate = use_nav();

    // ── FIX: hanya hitung item dari event ini, bukan seluruh cart ────────────
    // Kumpulkan tier_id milik event ini sebagai StoredValue (tidak reaktif,
    // statis sepanjang halaman, tidak buat subscription baru).
    let this_event_id = StoredValue::new(event.id.clone());

    let total_items = move || {
        cart.items.with(|v| {
            v.iter()
                .filter(|i| i.event_id == this_event_id.get_value())
                .map(|i| i.quantity)
                .sum::<i32>()
        })
    };
    let subtotal = move || {
        cart.items.with(|v| {
            v.iter()
                .filter(|i| i.event_id == this_event_id.get_value())
                .map(|i| i.unit_price * i.quantity as i64)
                .sum::<i64>()
        })
    };
    let navigateclone = navigate.clone();
    let go_cart = move |_| navigateclone("/cart", Default::default());

    // hero_venue dan short_date dipakai di overlay hero
    let short_date = {
        let parts: Vec<&str> = event.start_time.split('T').collect();
        let d = parts.first().copied().unwrap_or("");
        const MONTHS: &[&str] = &[
            "",
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ];
        let dp: Vec<&str> = d.split('-').collect();
        if dp.len() >= 3 {
            let day: u32 = dp[2].parse().unwrap_or(0);
            let mi: usize = dp[1].parse().unwrap_or(0);
            format!(
                "{} {} {}",
                day,
                MONTHS.get(mi).copied().unwrap_or(""),
                dp[0]
            )
        } else {
            d.to_string()
        }
    };
    // Venue pendek untuk hero: "Nama, Kota"
    let hero_venue = format!("{}, {}", event.venue.name, event.venue.city);
    let base_price = event.base_price_idr;

    let ev_id = event.id.clone();
    let ev_slug = event.slug.clone();
    let ev_title = event.title.clone();
    let ev_venue = format!("{}, {}", event.venue.name, event.venue.city);
    let ev_cover = event.cover_url.clone();

    let active_img = RwSignal::new(event.cover_url.clone());

    // ── Share to Story handler ──────────────────────────────────────────────
    let share_slug = ev_slug.clone();
    let share_title = ev_title.clone();
    let share_cover = ev_cover.clone();
    let share_id = ev_id.clone();
    let share_desc = event.description.clone();

    let detail_slide = RwSignal::new(0usize);
    let detail_img_count = event.detail_images.len();
    let detail_track_ref = NodeRef::<leptos::html::Div>::new();

    // Format tanggal singkat "24 Oct" untuk card story
    let share_date = {
        let parts: Vec<&str> = event.start_time.split('T').collect();
        let d = parts.first().copied().unwrap_or("");
        const MONTHS: &[&str] = &[
            "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let dp: Vec<&str> = d.split('-').collect();
        if dp.len() >= 3 {
            let day: u32 = dp[2].parse().unwrap_or(0);
            let mi: usize = dp[1].parse().unwrap_or(0);
            format!("{} {}", day, MONTHS.get(mi).copied().unwrap_or(""))
        } else {
            d.to_string()
        }
    };
    let share_venue_name = event.venue.name.clone();
    let share_price_str = format_idr(base_price);

    let share_to_story = move |_| {
        if let Some(win) = web_sys::window() {
            let params = web_sys::UrlSearchParams::new().expect("new UrlSearchParams");
            params.append("event_id", &share_id);
            params.append("event_slug", &share_slug);
            params.append("event_title", &share_title);
            params.append("event_cover", &share_cover);
            params.append("event_desc", &share_desc);
            params.append("event_date", &share_date);
            params.append("event_venue", &share_venue_name);
            params.append("event_price", &share_price_str);

            // ── Hero transition flag ───────────────────────────────────────
            // StoryCreatorPage membaca flag ini untuk memainkan animasi
            // "foto event melayang masuk ke frame" saat pertama mount.
            // sessionStorage karena: (1) persist across SPA navigate,
            // (2) otomatis bersih saat tab ditutup,
            // (3) tidak perlu cleanup manual.
            if let Ok(Some(storage)) = win.session_storage() {
                let _ = storage.set_item("story_hero_transition", "event");
                let _ = storage.set_item("story_hero_cover", &share_cover);
            }

            // Pakai SPA navigate (bukan set_href) agar state tidak full-reset
            let query_string = params.to_string();
            navigate(
                &format!("/stories/new?{}", query_string),
                Default::default(),
            );
        }
    };

    let mobile_tiers = event
        .tiers
        .iter()
        .cloned()
        .map(|tier| {
            render_tier(
                tier,
                cart,
                ev_id.clone(),
                ev_title.clone(),
                ev_venue.clone(),
                ev_cover.clone(),
            )
        })
        .collect_view();

    view! {
        <div class="page ed-page">
            // ── Mobile Header ────────────────────────────────────────────────
            <header class="page-header ed-header">
                <button
                    class="back-btn"
                    aria-label="Back"
                    on:click=move |_| {
                        let _ = web_sys::window().and_then(|w| w.history().ok()).map(|h| h.back());
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
                <span class="page-logo">"PULSE"</span>
                <div class="header-actions">
                    <ThemeToggle />
                    // ── Tombol Share ke Cerita (tetap di header) ─────────────
                    <button
                        class="icon-btn"
                        on:click=share_to_story
                        aria-label="Bagikan ke Cerita"
                        title="Bagikan ke Cerita"
                    >
                        <svg
                            width="16"
                            height="16"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2.2"
                            stroke-linecap="round"
                        >
                            <circle cx="18" cy="5" r="3" />
                            <circle cx="6" cy="12" r="3" />
                            <circle cx="18" cy="19" r="3" />
                            <line x1="8.59" y1="13.51" x2="15.42" y2="17.49" />
                            <line x1="15.41" y1="6.51" x2="8.59" y2="10.49" />
                        </svg>
                    </button>
                    <A href="/notifications" attr:class="bell-btn">
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
                            <path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9" />
                            <path d="M13.73 21a2 2 0 01-3.46 0" />
                        </svg>
                    </A>
                    <A href="/profile" attr:class="nav-avatar">
                        <svg
                            width="18"
                            height="18"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                        >
                            <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
                            <circle cx="12" cy="7" r="4" />
                        </svg>
                    </A>
                </div>
            </header>

            <div class="ed-hero">
                <img src=move || active_img.get() alt=event.title.clone() class="ed-hero-img" />
                <div class="ed-hero-gradient"></div>
                <div class="ed-hero-overlay-content">
                    // ── Badge row: LIVE NOW + kategori pertama ────────────────
                    <div class="ed-hero-badges">
                        <span class="ed-live-badge">
                            <span class="ed-live-dot"></span>
                            "LIVE NOW"
                        </span>
                        {event
                            .category
                            .first()
                            .map(|c| view! { <span class="ed-cat-badge">{c.clone()}</span> })}
                    </div>
                    <h1 class="ed-hero-title">{event.title.clone()}</h1>
                    // ── Meta row: tanggal + venue ─────────────────────────────
                    <div class="ed-hero-meta">
                        <div class="ed-hero-meta-item">
                            <svg
                                width="13"
                                height="13"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                                stroke-linecap="round"
                            >
                                <rect x="3" y="4" width="18" height="18" rx="2" />
                                <line x1="16" y1="2" x2="16" y2="6" />
                                <line x1="8" y1="2" x2="8" y2="6" />
                                <line x1="3" y1="10" x2="21" y2="10" />
                            </svg>
                            {short_date}
                        </div>
                        <div class="ed-hero-meta-item">
                            <svg
                                width="13"
                                height="13"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                                stroke-linecap="round"
                            >
                                <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0118 0z" />
                                <circle cx="12" cy="10" r="3" />
                            </svg>
                            {hero_venue}
                        </div>
                    </div>
                </div>
            </div>

            <div class="ed-body">
                <div class="ed-main">
                    // ── About the Event ──────────────────────────────────────
                    <section class="section">
                        <p class="ed-section-eyebrow">"ABOUT THE EVENT"</p>
                        <p class="about-text">{event.description.clone()}</p>
                    </section>

                    // ── Categories chips ─────────────────────────────────────
                    {(!event.category.is_empty())
                        .then({
                            let cats = event.category.clone();
                            move || {
                                view! {
                                    <div class="ed-categories-section">
                                        <p class="ed-section-eyebrow">"CATEGORIES"</p>
                                        <div class="ed-chips-row">
                                            {cats
                                                .iter()
                                                .map(|c| view! { <span class="ed-chip">{c.clone()}</span> })
                                                .collect_view()}
                                        </div>
                                    </div>
                                }
                            }
                        })}

                    // ── Select Tickets ───────────────────────────────────────
                    <div class="ed-tickets-header">
                        <span class="ed-tickets-title">"Select Tickets"</span>
                        <span class="ed-tickets-avail">"Available until sale ends"</span>
                    </div>
                    <section class="section ed-mobile-tiers">{mobile_tiers}</section>

                    // ── The Venue ────────────────────────────────────────────
                    <section class="section">
                        <h2 class="section-title">"The Venue"</h2>
                    </section>
                    <div class="map-card">
                        <div class="map-visual">
                            <div class="map-grid"></div>
                            <div class="map-pin">
                                <svg width="28" height="36" viewBox="0 0 32 40">
                                    <path
                                        d="M16 0C7.163 0 0 7.163 0 16c0 11 16 24 16 24s16-13 16-24C32 7.163 24.837 0 16 0z"
                                        fill="#c8ff5e"
                                    />
                                    <circle cx="16" cy="16" r="6" fill="#0d0d1a" />
                                </svg>
                            </div>
                        </div>
                        <div class="map-info">
                            <div class="map-name">{event.venue.name.clone()}</div>
                            <div class="map-addr">{event.venue.address.clone()}</div>
                            <A
                                href=format!("/events/{}/location", ev_slug)
                                attr:class="directions-btn"
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
                                    <line x1="5" y1="12" x2="19" y2="12" />
                                    <polyline points="12 5 19 12 12 19" />
                                </svg>
                                "Get Directions"
                            </A>
                        </div>
                    </div>

                    // ── Lineup (jika ada) ────────────────────────────────────
                    {(!event.lineup.is_empty())
                        .then({
                            let lineup = event.lineup.clone();
                            move || {
                                view! {
                                    <section class="section">
                                        <h2 class="section-title">"The Lineup"</h2>
                                        <div class="lineup-grid">
                                            {lineup
                                                .into_iter()
                                                .map(|a| render_artist(a, active_img))
                                                .collect_view()}
                                        </div>
                                    </section>
                                }
                            }
                        })}

                    // ── Detail Images ────────────────────────────────────────
                    {(!event.detail_images.is_empty())
                        .then({
                            let imgs = event.detail_images.clone();
                            move || {
                                view! {
                                    <section class="section">
                                        <h2 class="section-title">"Informasi Tambahan"</h2>
                                        <div class="detail-img-wrap">
                                            <div
                                                class="detail-img-track"
                                                node_ref=detail_track_ref
                                                on:scroll=move |_| {
                                                    if let Some(el) = detail_track_ref.get() {
                                                        let sl = el.scroll_left() as f64;
                                                        let cw = el.client_width() as f64;
                                                        if cw > 0.0 {
                                                            let idx = (sl / cw).round() as usize;
                                                            detail_slide
                                                                .set(idx.min(detail_img_count.saturating_sub(1)));
                                                        }
                                                    }
                                                }
                                            >
                                                {imgs
                                                    .iter()
                                                    .map(|img| {
                                                        let type_label = match img.image_type.as_str() {
                                                            "map" => "Denah Lokasi",
                                                            "seat" => "Peta Kursi",
                                                            "price" => "Info Harga",
                                                            _ => "Informasi",
                                                        };
                                                        let badge_style = match img.image_type.as_str() {
                                                            "map" => "background:#1e3a5f;color:#93c5fd",
                                                            "seat" => "background:#134e38;color:#6ee7b7",
                                                            "price" => "background:#4a2c06;color:#fcd34d",
                                                            _ => "background:var(--bg-elevated);color:var(--text-muted)",
                                                        };
                                                        let badge_full = format!(
                                                            "position:absolute;top:8px;left:8px;\
                                                             font-size:9px;font-weight:700;letter-spacing:.1em;\
                                                             padding:3px 8px;border-radius:100px;{}",
                                                            badge_style,
                                                        );
                                                        view! {
                                                            <div class="detail-img-slide">
                                                                <div style="position:relative;border-radius:14px;overflow:hidden">
                                                                    <img
                                                                        src=img.url.clone()
                                                                        alt=img.caption.clone()
                                                                        style="width:100%;aspect-ratio:16/9;\
                                                                         object-fit:cover;display:block"
                                                                    />
                                                                    <span style=badge_full>{type_label}</span>
                                                                </div>
                                                                {(!img.caption.is_empty())
                                                                    .then({
                                                                        let cap = img.caption.clone();
                                                                        move || {
                                                                            view! { <p class="detail-img-caption">{cap.clone()}</p> }
                                                                        }
                                                                    })}
                                                            </div>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </div>
                                            {(detail_img_count > 1)
                                                .then(move || {
                                                    view! {
                                                        <div class="detail-img-dots">
                                                            {(0..detail_img_count)
                                                                .map(|i| {
                                                                    view! {
                                                                        <span class=move || {
                                                                            if detail_slide.get() == i {
                                                                                "detail-dot detail-dot--active"
                                                                            } else {
                                                                                "detail-dot"
                                                                            }
                                                                        }></span>
                                                                    }
                                                                })
                                                                .collect_view()}
                                                        </div>
                                                    }
                                                })}
                                        </div>
                                    </section>
                                }
                            }
                        })}
                </div>
            </div>

            // ── Mobile sticky footer ─────────────────────────────────────────
            <div class="sticky-footer ed-mobile-footer">
                <div class="ed-footer-starting">
                    <span class="footer-label">"STARTING FROM"</span>
                    <span class="starting-price">
                        {move || format_idr(
                            if total_items() == 0 { base_price } else { subtotal() },
                        )}
                    </span>
                </div>
                <button class="ed-secure-btn" disabled=move || total_items() == 0 on:click=go_cart>
                    "Secure Tickets"
                </button>
            </div>

            // ── Pulse Discoveries ─────────────────────────────────────────────
            <section class="event-related-section">
                <div class="ed-related-header">
                    <div class="ed-related-header-left">
                        <span class="ed-related-eyebrow">"KATEGORI SERUPA"</span>
                        <h2 class="ed-related-heading">"Pulse Discoveries"</h2>
                    </div>
                    <A href="/explore" attr:class="ed-related-see-all">
                        "SEE ALL"
                        <svg
                            width="14"
                            height="14"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2.5"
                            stroke-linecap="round"
                        >
                            <line x1="5" y1="12" x2="19" y2="12" />
                            <polyline points="12 5 19 12 12 19" />
                        </svg>
                    </A>
                </div>
                <div class="ed-related-grid">
                    {move || {
                        match related_res_data.get() {
                            None => {
                                (0..3)
                                    .map(|_| {
                                        // Masih loading
                                        view! { <EventCardShimmer /> }
                                    })
                                    .collect_view()
                                    .into_any()
                            }
                            Some(result) => {
                                let list = result.unwrap_or_default();
                                if list.is_empty() {
                                    // Selesai load
                                    view! {
                                        <div style="grid-column:1/-1">
                                            <EmptyState
                                                icon="🎫"
                                                title="TIDAK ADA EVENT TERKAIT"
                                                body="Belum ada event lain dalam kategori yang sama."
                                                cta_label="JELAJAHI SEMUA"
                                                cta_href="/explore"
                                            />
                                        </div>
                                    }
                                        .into_any()
                                } else {
                                    list.into_iter()
                                        .map(|ev| {
                                            let venue_str = if ev.city.is_empty() {
                                                ev.venue.clone()
                                            } else {
                                                format!("{}, {}", ev.venue, ev.city)
                                            };
                                            view! {
                                                <EventCard
                                                    href=format!("/events/{}", ev.slug)
                                                    img=ev.cover.clone()
                                                    alt=ev.title.clone()
                                                    badge=ev
                                                        .category
                                                        .first()
                                                        .cloned()
                                                        .unwrap_or_default()
                                                        .to_uppercase()
                                                    title=ev.title.clone()
                                                    date=ev.date.clone()
                                                    venue=venue_str
                                                    price=ev.price_str.clone()
                                                />
                                            }
                                        })
                                        .collect_view()
                                        .into_any()
                                }
                            }
                        }
                    }}
                </div>
            </section>
        </div>
    }
}
