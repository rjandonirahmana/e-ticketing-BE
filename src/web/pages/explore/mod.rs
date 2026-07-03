//! web/pages/explore — Explore page (unified SSR + hydration).

mod search_overlay;

use search_overlay::SearchOverlay;

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::web::components::story_bars::StoryBar;
use crate::web::components::story_viewer::StoryViewer;
use crate::web::components::{BottomNav, EmptyState, EventCardShimmer};
use crate::web::components::ThemeToggle;
use crate::web::state::use_events_store;
use crate::web::utils::format_number;

#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
#[cfg(feature = "hydrate")]
use wasm_bindgen::JsCast;
#[cfg(feature = "hydrate")]
use send_wrapper::SendWrapper;
#[cfg(feature = "hydrate")]
use wasm_bindgen::prelude::*;

// ── Banner model (unused until API is wired) ──────────────────────────────────
#[allow(dead_code)]
#[derive(Clone, Debug, serde::Deserialize)]
struct ApiBanner {
    image_url: String,
    click_url: String,
}

// ── Main Explore Page ─────────────────────────────────────────────────────────
#[component]
pub fn ExplorePage() -> impl IntoView {
    let params = use_query_map();
    let initial_q = params.with_untracked(|p| p.get("q").unwrap_or_default());
    let initial_cat = params.with_untracked(|p| p.get("cat").unwrap_or("All".into()));

    let query = RwSignal::new(initial_q);
    let active_cat = RwSignal::new(initial_cat);
    let show_overlay = RwSignal::new(false);
    let overlay_visible = RwSignal::new(false);

    let store = use_events_store();

    Effect::new(move |_| {
        let cat = active_cat.get();
        leptos::logging::log!("[ExplorePage] Effect fired: cat={:?}", cat);
        store.load_cat(cat);
    });

    // Rekomendasi implisit "Untuk Kamu": ambil kategori favorit user (dari
    // perilaku browsing di localStorage, tanpa perlu "like") → fetch event-nya.
    let rec_events = RwSignal::new(Vec::<crate::web::state::events::ExploreEvent>::new());
    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        let cats = crate::web::behavior::top_categories(1);
        if let Some(cat) = cats.into_iter().next() {
            spawn_local(async move {
                if let Ok(res) =
                    crate::web::api::get_events(Some(1), None, Some(cat), None, Some(10)).await
                {
                    rec_events.set(
                        res.data
                            .iter()
                            .map(crate::web::state::events::event_to_explore_pub)
                            .collect(),
                    );
                }
            });
        }
    });

    // Infinite scroll: pasang listener "scroll" di window. Saat jarak ke bawah
    // dokumen < 700px, panggil load_more() (yang menaikkan OFFSET & append).
    // load_more() sudah punya guard (loading/loading_more/has_more) → aman dipanggil
    // berkali-kali tiap event scroll tanpa fetch ganda.
    #[cfg(feature = "hydrate")]
    {
        let scroll_cb: StoredValue<Option<SendWrapper<Closure<dyn Fn()>>>> =
            StoredValue::new(None);
        Effect::new(move |_| {
            let cb = Closure::<dyn Fn()>::new(move || {
                let Some(win) = web_sys::window() else { return };
                let inner_h = win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
                let scroll_y = win.scroll_y().unwrap_or(0.0);
                let doc_h = win
                    .document()
                    .and_then(|d| d.document_element())
                    .map(|e| e.scroll_height() as f64)
                    .unwrap_or(0.0);
                if doc_h - (scroll_y + inner_h) < 700.0 {
                    store.load_more();
                }
            });
            if let Some(win) = web_sys::window() {
                let _ = win.add_event_listener_with_callback(
                    "scroll",
                    cb.as_ref().unchecked_ref(),
                );
            }
            scroll_cb.set_value(Some(SendWrapper::new(cb)));
        });
        on_cleanup(move || {
            if let Some(Some(cb)) = scroll_cb.try_update_value(|o| o.take()) {
                if let Some(win) = web_sys::window() {
                    let _ = win.remove_event_listener_with_callback(
                        "scroll",
                        cb.as_ref().unchecked_ref(),
                    );
                }
                drop(cb);
            }
        });
    }

    let close_gen = RwSignal::new(0u32);

    let open_overlay = move || {
        close_gen.update(|n| *n = n.wrapping_add(1));
        show_overlay.set(true);
        overlay_visible.set(true);
    };

    let close_overlay = move || {
        overlay_visible.set(false);
        let gen = close_gen.get_untracked().wrapping_add(1);
        close_gen.set(gen);
        #[cfg(feature = "hydrate")]
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(380).await;
            if close_gen.get_untracked() == gen {
                show_overlay.set(false);
            }
        });
        #[cfg(not(feature = "hydrate"))]
        {
            show_overlay.set(false);
        }
    };

    let filtered = Memo::new(move |_| {
        let q = query.get().to_lowercase();
        store.items.with(|events| {
            events
                .iter()
                .filter(|e| {
                    q.is_empty()
                        || e.title.to_lowercase().contains(&q)
                        || e.city.to_lowercase().contains(&q)
                        || e.venue.to_lowercase().contains(&q)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
    });

    let close_c = StoredValue::new(close_overlay);

    let placeholders = vec!["search events, artists...", "cari tiket konser", "tiket stand up"];
    let _ph_idx = RwSignal::new(0usize);
    let ph_text = RwSignal::new(placeholders[0].to_string());
    let ph_show = RwSignal::new(true);

    // Placeholder rotator — client only
    #[cfg(feature = "hydrate")]
    {
        let ph_timer: StoredValue<Option<leptos::prelude::IntervalHandle>> = StoredValue::new(None);
        let phs = placeholders.clone();
        ph_timer.set_value(
            set_interval_with_handle(
                move || {
                    ph_show.set(false);
                    let phs2 = phs.clone();
                    spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(300).await;
                        let next = (_ph_idx.get_untracked() + 1) % phs2.len();
                        _ph_idx.set(next);
                        ph_text.set(phs2[next].to_string());
                        ph_show.set(true);
                    });
                },
                std::time::Duration::from_millis(2200),
            )
            .ok(),
        );
        on_cleanup(move || {
            if let Some(Some(h)) = ph_timer.try_update_value(|o| o.take()) {
                h.clear();
            }
        });
    }
    #[cfg(not(feature = "hydrate"))]
    {
        ph_text.set(placeholders[0].to_string());
        ph_show.set(true);
    }

    // Lock body scroll saat overlay — client only
    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        if let Some(body) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.body())
        {
            if show_overlay.get() {
                let _ = body.class_list().add_1("body-scroll-locked");
            } else {
                let _ = body.class_list().remove_1("body-scroll-locked");
            }
        }
    });

    // ⌘K / Escape keybind — client only
    #[cfg(feature = "hydrate")]
    {
        let kb_handler: StoredValue<Option<SendWrapper<Closure<dyn Fn(web_sys::KeyboardEvent)>>>> =
            StoredValue::new(None);
        Effect::new(move |_| {
            let handler = Closure::new({
                let open_overlay = open_overlay.clone();
                let close_overlay = close_overlay.clone();
                let show_overlay = show_overlay.clone();
                move |ev: web_sys::KeyboardEvent| {
                    if (ev.meta_key() || ev.ctrl_key()) && ev.key().eq_ignore_ascii_case("k") {
                        ev.prevent_default();
                        open_overlay();
                    } else if ev.key() == "Escape" && show_overlay.get_untracked() {
                        ev.prevent_default();
                        close_overlay();
                    }
                }
            });
            if let Some(win) = web_sys::window() {
                let _ = win.add_event_listener_with_callback(
                    "keydown",
                    handler.as_ref().unchecked_ref::<js_sys::Function>(),
                );
            }
            kb_handler.set_value(Some(SendWrapper::new(handler)));
        });
        on_cleanup(move || {
            if let Some(Some(old)) = kb_handler.try_update_value(|o| o.take()) {
                if let Some(win) = web_sys::window() {
                    let _ = win.remove_event_listener_with_callback(
                        "keydown",
                        old.as_ref().unchecked_ref::<js_sys::Function>(),
                    );
                }
                drop(old);
            }
        });
    }

    view! {
        <Title text="Jelajahi Event — PULSE" />
        <Meta
            name="description"
            content="Temukan konser, festival, dan event seru di kotamu. Beli tiket sekarang di PULSE."
        />
        <div class="page explore-page exp-page">
            <header class="page-header exp-header">
                <A
                    href="/pulse-landing"
                    attr:class="exp-partner-btn"
                    attr:aria-label="Jadi Partner"
                >
                    <svg
                        width="12"
                        height="12"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <path d="M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2" />
                        <circle cx="9" cy="7" r="4" />
                        <line x1="19" y1="8" x2="19" y2="14" />
                        <line x1="22" y1="11" x2="16" y2="11" />
                    </svg>
                    "Jadi Partner"
                </A>
                <span class="page-logo">"PULSE"</span>
                <div class="header-actions">
                    <ThemeToggle />
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
                        <span class="bell-dot"></span>
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

            <div class="exp-searchbar-row">
                <button
                    class="exp-searchbar"
                    on:click=move |_| open_overlay()
                    aria-label="Cari acara"
                >
                    <svg
                        width="15"
                        height="15"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.2"
                        stroke-linecap="round"
                    >
                        <circle cx="11" cy="11" r="8" />
                        <line x1="21" y1="21" x2="16.65" y2="16.65" />
                    </svg>
                    <span class=move || {
                        format!(
                            "exp-searchbar-ph {}",
                            if ph_show.get() { "ph-in" } else { "ph-out" },
                        )
                    }>{move || ph_text.get()}</span>
                </button>
                <button class="exp-filter-btn" aria-label="Filter">
                    <svg
                        width="17"
                        height="17"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    >
                        <line x1="4" y1="6" x2="20" y2="6" />
                        <line x1="8" y1="12" x2="16" y2="12" />
                        <line x1="11" y1="18" x2="13" y2="18" />
                    </svg>
                </button>
            </div>

            <div class="exp-promo-wrap">
                <div class="exp-promo">
                    <span class="exp-promo-tag">"SPONSORED"</span>
                    <h2 class="exp-promo-heading">"UPGRADE TO VIP" <br /> "PULSE PASS"</h2>
                    <p class="exp-promo-desc">
                        "Early access, backstage tours, dan premium lounge untuk semua festival musim panas."
                    </p>
                    <button class="exp-promo-cta">"Claim Offer"</button>
                </div>
            </div>

            // ── Untuk Kamu (rekomendasi implisit dari perilaku) ──────────────
            {move || {
                let list = rec_events.get();
                (!list.is_empty()).then(|| {
                    let cards = list.into_iter().take(10).map(|ev| {
                        let href = format!("/events/{}", ev.slug);
                        view! {
                            <a href=href class="exp-fy-card">
                                <div class="exp-fy-img-wrap">
                                    <img src=ev.cover.clone() alt=ev.title.clone()
                                        class="exp-fy-img" loading="lazy" />
                                    {ev.is_live.then(|| view! {
                                        <span class="exp-fy-live">"LIVE"</span>
                                    })}
                                </div>
                                <div class="exp-fy-body">
                                    <div class="exp-fy-title">{ev.title.clone()}</div>
                                    <div class="exp-fy-price">{ev.price_str.clone()}</div>
                                </div>
                            </a>
                        }
                    }).collect_view();
                    view! {
                        <div class="exp-fy-section">
                            <div class="exp-fy-head">
                                <span class="exp-section-eyebrow">"REKOMENDASI"</span>
                                <h2 class="exp-fy-title-h">"Untuk Kamu"</h2>
                            </div>
                            <div class="exp-fy-rail">{cards}</div>
                        </div>
                    }
                })
            }}

            <div class="exp-section-hdr-row">
                <div class="exp-section-hdr-left">
                    <span class="exp-section-eyebrow">"TRENDING NOW"</span>
                    <h2 class="exp-section-title">"Live Events"</h2>
                </div>
                <A href="/events" attr:class="exp-view-all">
                    "View All →"
                </A>
            </div>

            <StoryBar />

            <div class="exp-chips">
                {move || {
                    store
                        .categories
                        .with(|cats| {
                            cats.iter()
                                .map(|label| {
                                    let lc = label.clone();
                                    let lk = label.clone();
                                    view! {
                                        <button
                                            class=move || {
                                                if active_cat.get() == lc {
                                                    "exp-chip exp-chip--on"
                                                } else {
                                                    "exp-chip"
                                                }
                                            }
                                            on:click=move |_| active_cat.set(lk.clone())
                                        >
                                            {label.to_uppercase()}
                                        </button>
                                    }
                                })
                                .collect_view()
                        })
                }}
            </div>

            <div class="exp-results-bar">
                <div class="exp-results-left">
                    <span class="exp-results-eyebrow">"Acara Tersedia"</span>
                    <span class="exp-results-count">
                        {move || filtered.with(|f| f.len())} " acara tersedia"
                    </span>
                </div>
                <div class="exp-results-right">
                    {move || {
                        (active_cat.get() != "All")
                            .then(|| {
                                view! {
                                    <button
                                        class="exp-clear-btn"
                                        on:click=move |_| active_cat.set("All".into())
                                    >
                                        <svg
                                            width="11"
                                            height="11"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2.5"
                                        >
                                            <line x1="18" y1="6" x2="6" y2="18" />
                                            <line x1="6" y1="6" x2="18" y2="18" />
                                        </svg>
                                        "Atur Ulang"
                                    </button>
                                }
                            })
                    }}
                </div>
            </div>

            <div class="exp-feed">
                {move || {
                    if store.loading.get() {
                        let shims = (0..6)
                            .map(|i| {
                                view! {
                                    <div
                                        class="exp-shimmer-wrap"
                                        style=format!("animation-delay:{}ms", i * 60)
                                    >
                                        <EventCardShimmer />
                                    </div>
                                }
                            })
                            .collect_view();
                        view! { <div class="exp-mkt-grid">{shims}</div> }.into_any()
                    } else if !store.error.with(|e| e.is_empty()) {
                        view! {
                            <div class="exp-empty">
                                <EmptyState
                                    icon="⚠️"
                                    title="Gagal Memuat"
                                    body="Tidak bisa terhubung ke server. Coba muat ulang halaman."
                                />
                                <button
                                    class="exp-reset-btn"
                                    on:click=move |_| store.load_cat(active_cat.get_untracked())
                                >
                                    "Coba Lagi"
                                </button>
                            </div>
                        }
                        .into_any()
                    } else {
                        let list = filtered.get();
                        if list.is_empty() {
                            view! {
                                <div class="exp-empty">
                                    <EmptyState
                                        icon="🔍"
                                        title="Belum Ada Acara"
                                        body="Coba pilih kategori lain atau ubah filter."
                                    />
                                    <button
                                        class="exp-reset-btn"
                                        on:click=move |_| active_cat.set("All".into())
                                    >
                                        "Atur Ulang Filter"
                                    </button>
                                </div>
                            }
                                .into_any()
                        } else {
                            let cards = list.into_iter()
                                .enumerate()
                                .map(|(i, ev)| {
                                    let href = format!("/events/{}", ev.slug);
                                    let loc = if !ev.city.is_empty() {
                                        ev.city.clone()
                                    } else {
                                        ev.venue.clone()
                                    };
                                    let cat = ev.category.first().cloned().unwrap_or_default();
                                    let sold = ev.total_sold.max(0);
                                    let sold_label = if sold > 0 {
                                        format!("{} Terjual", format_number(sold as i64))
                                    } else {
                                        "Baru".to_string()
                                    };
                                    let is_free = ev.price <= 0;
                                    let price_disp = if is_free { "Gratis".to_string() } else { ev.price_str.clone() };
                                    view! {
                                        <a
                                            href=href
                                            class="exp-mkt-card exp-cascade"
                                            style=format!("--i:{}", i)
                                        >
                                            <div class="exp-mkt-img-wrap">
                                                <img
                                                    src=ev.cover.clone()
                                                    alt=ev.title.clone()
                                                    class="exp-mkt-img"
                                                    loading="lazy"
                                                />
                                                {ev.is_live.then(|| view! {
                                                    <span class="exp-mkt-live">
                                                        <span class="exp-mkt-live-dot"></span>
                                                        "LIVE"
                                                    </span>
                                                })}
                                            </div>
                                            <div class="exp-mkt-body">
                                                {(!cat.is_empty()).then(|| view! {
                                                    <span class="exp-mkt-merchant">{cat.clone()}</span>
                                                })}
                                                <h3 class="exp-mkt-title">{ev.title.clone()}</h3>
                                                <div class="exp-mkt-meta">
                                                    <span class="exp-mkt-meta-row">
                                                        <svg
                                                            width="12" height="12" viewBox="0 0 24 24"
                                                            fill="none" stroke="currentColor"
                                                            stroke-width="2" stroke-linecap="round"
                                                            stroke-linejoin="round"
                                                        >
                                                            <rect x="3" y="4" width="18" height="18" rx="2" />
                                                            <line x1="16" y1="2" x2="16" y2="6" />
                                                            <line x1="8" y1="2" x2="8" y2="6" />
                                                            <line x1="3" y1="10" x2="21" y2="10" />
                                                        </svg>
                                                        {ev.date.clone()}
                                                    </span>
                                                    {(!loc.is_empty()).then(|| view! {
                                                        <span class="exp-mkt-meta-row">
                                                            <svg
                                                                width="12" height="12" viewBox="0 0 24 24"
                                                                fill="none" stroke="currentColor"
                                                                stroke-width="2" stroke-linecap="round"
                                                                stroke-linejoin="round"
                                                            >
                                                                <path d="M21 10c0 7-9 12-9 12s-9-5-9-12a9 9 0 0118 0z" />
                                                                <circle cx="12" cy="10" r="3" />
                                                            </svg>
                                                            {loc.clone()}
                                                        </span>
                                                    })}
                                                </div>
                                                <div class="exp-mkt-price-block">
                                                    <span class="exp-mkt-from">"Mulai Dari"</span>
                                                    <span class="exp-mkt-price">{price_disp}</span>
                                                </div>
                                                <div class="exp-mkt-foot">
                                                    <svg
                                                        class="exp-mkt-star"
                                                        width="13" height="13" viewBox="0 0 24 24"
                                                        fill="currentColor" stroke="none"
                                                    >
                                                        <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
                                                    </svg>
                                                    <span class="exp-mkt-sold">{sold_label}</span>
                                                </div>
                                            </div>
                                        </a>
                                    }
                                })
                                .collect_view();
                            view! { <div class="exp-mkt-grid">{cards}</div> }.into_any()
                        }
                    }
                }}
            </div>

            // Auto-load (infinite scroll): saat load_more berjalan, tampilkan
            // kartu shimmer (bukan spinner) — konsisten dgn loading awal. Scroll
            // listener (Effect di atas) yang memicu load_more otomatis.
            {move || (store.has_more.get() && store.loading_more.get()).then(|| {
                let shims = (0..4)
                    .map(|i| view! {
                        <div class="exp-shimmer-wrap" style=format!("animation-delay:{}ms", i * 60)>
                            <EventCardShimmer />
                        </div>
                    })
                    .collect_view();
                view! { <div class="exp-mkt-grid exp-mkt-grid--more">{shims}</div> }
            })}

            <div class="exp-genre-section">
                <span class="exp-section-eyebrow">"EXPLORE BY GENRE"</span>
                <div class="exp-genre-chips">
                    {move || {
                        store
                            .categories
                            .with(|cats| {
                                cats.iter()
                                    .filter(|c| *c != "All")
                                    .map(|label| {
                                        let lc = label.clone();
                                        let lk = label.clone();
                                        view! {
                                            <button
                                                class=move || {
                                                    if active_cat.get() == lc {
                                                        "exp-genre-chip exp-genre-chip--on"
                                                    } else {
                                                        "exp-genre-chip"
                                                    }
                                                }
                                                on:click=move |_| active_cat.set(lk.clone())
                                            >
                                                {label.to_uppercase()}
                                            </button>
                                        }
                                    })
                                    .collect_view()
                            })
                    }}
                </div>
            </div>

            <BottomNav active="explore" />

            {move || {
                show_overlay
                    .get()
                    .then(|| {
                        let cc = close_c.get_value();
                        view! {
                            <div
                                class=move || {
                                    if overlay_visible.get() {
                                        "exp-sovl-backdrop exp-sovl-backdrop--open"
                                    } else {
                                        "exp-sovl-backdrop"
                                    }
                                }
                                on:click=move |_| cc()
                            ></div>
                        }
                    })
            }}

            {move || {
                show_overlay
                    .get()
                    .then(|| {
                        let cc = close_c.get_value();
                        view! {
                            <div class=move || {
                                if overlay_visible.get() {
                                    "exp-sovl-wrap exp-sovl-wrap--open"
                                } else {
                                    "exp-sovl-wrap"
                                }
                            }>
                                <SearchOverlay
                                    query=query
                                    active_cat=active_cat
                                    on_close=cc
                                    store=store
                                    ph_text=ph_text
                                />
                            </div>
                        }
                    })
            }}

            <StoryViewer />
        </div>
    }
}
