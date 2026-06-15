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
        store.load_cat(active_cat.get());
    });

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
                        (0..3)
                            .map(|i| {
                                view! {
                                    <div
                                        class="exp-shimmer-wrap"
                                        style=format!("animation-delay:{}ms", i * 80)
                                    >
                                        <EventCardShimmer />
                                    </div>
                                }
                            })
                            .collect_view()
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
                            list.into_iter()
                                .enumerate()
                                .map(|(i, ev)| {
                                    let href = format!("/events/{}", ev.slug);
                                    let venue_str = if ev.city.is_empty() {
                                        ev.venue.to_uppercase()
                                    } else {
                                        format!(
                                            "{} • {}",
                                            ev.venue.to_uppercase(),
                                            ev.city.to_uppercase(),
                                        )
                                    };
                                    let is_hot = i % 2 == 0;
                                    let badge_class = if is_hot {
                                        "exp-card-v2__badge exp-card-v2__badge--hot"
                                    } else {
                                        "exp-card-v2__badge exp-card-v2__badge--limited"
                                    };
                                    let badge_text = if is_hot {
                                        "⚡ SELLING FAST"
                                    } else {
                                        "LIMITED SEATS"
                                    };
                                    let cat = ev
                                        .category
                                        .first()
                                        .cloned()
                                        .unwrap_or_default()
                                        .to_uppercase();
                                    view! {
                                        <div class="exp-cascade" style=format!("--i:{}", i)>
                                            <a href=href class="exp-card-v2">
                                                <div class="exp-card-v2__eyebrow">{cat}</div>
                                                <div class="exp-card-v2__img-wrap">
                                                    <img
                                                        src=ev.cover.clone()
                                                        alt=ev.title.clone()
                                                        class="exp-card-v2__img"
                                                        loading="lazy"
                                                    />
                                                    <span class=badge_class>{badge_text}</span>
                                                </div>
                                                <div class="exp-card-v2__body">
                                                    <h2 class="exp-card-v2__title">{ev.title.clone()}</h2>
                                                    <div class="exp-card-v2__meta">
                                                        <div class="exp-card-v2__venue-date">
                                                            <span class="exp-card-v2__venue">{venue_str}</span>
                                                            <span class="exp-card-v2__date">{ev.date.clone()}</span>
                                                        </div>
                                                        <div class="exp-card-v2__price-wrap">
                                                            <span class="exp-card-v2__price-label">"STARTS FROM"</span>
                                                            <span class="exp-card-v2__price">
                                                                {ev.price_str.clone()}
                                                            </span>
                                                        </div>
                                                    </div>
                                                </div>
                                            </a>
                                        </div>
                                    }
                                })
                                .collect_view()
                                .into_any()
                        }
                    }
                }}
            </div>

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
