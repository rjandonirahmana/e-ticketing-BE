//! event_detail.rs — Halaman Detail Event (SSR + hydration).
//!
//! Cart-based ticket selection matching CSR design:
//! Add button → qty ctrl, footer subtotal → Secure Tickets → /cart.

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::web::api::get_event_detail;
use crate::web::app::{AuthResource, CartContext};
use crate::web::hooks::ThemeToggle;
use crate::web::models::{CartItem, format_date, format_price};

#[component]
pub fn EventDetailPage() -> impl IntoView {
    let params    = use_params_map();
    let slug      = Memo::new(move |_| params.read().get("slug").unwrap_or_default());
    let event_res = Resource::new_blocking(move || slug.get(), |s| get_event_detail(s));

    let navigate  = use_navigate();
    let auth     = use_context::<AuthResource>().expect("AuthResource missing");
    let cart_ctx = use_context::<CartContext>().expect("CartContext not provided");

    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let shimmer = move || view! {
        // No .page wrapper here — .page is hoisted outside Suspense below
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
            {(0..2i32).map(|_| view! {
                <div style="display:flex;justify-content:space-between;align-items:center;
                            padding:14px 0;border-bottom:1px solid var(--border-soft)">
                    <div style="display:flex;flex-direction:column;gap:8px">
                        <div class="shim" style="height:16px;width:130px"></div>
                        <div class="shim" style="height:12px;width:80px"></div>
                    </div>
                    <div class="shim" style="height:36px;width:80px;border-radius:100px"></div>
                </div>
            }).collect_view()}
        </div>
    };

    view! {
        // .page is always rendered — Suspense only replaces inner content
        <div class="page ed-page">
        <Suspense fallback=shimmer>
            {move || {
                let navigate = navigate.clone();
                Suspend::new(async move {
                match event_res.await {
                    Err(_) => view! {
                        <header class="page-header ed-header">
                            <A href="/explore" attr:class="back-btn" attr:aria-label="Back">
                                <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                    stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                    <polyline points="15 18 9 12 15 6" />
                                </svg>
                            </A>
                            <span class="page-logo">"KINETIC"</span>
                            <div style="width:36px"></div>
                        </header>
                        <div style="display:flex;flex-direction:column;align-items:center;
                                    justify-content:center;min-height:60vh;padding:20px;text-align:center">
                            <div style="font-size:3rem;margin-bottom:12px">"🔍"</div>
                            <h2 style="font-size:1.1rem;font-weight:700;text-transform:uppercase;
                                       letter-spacing:.06em;margin-bottom:8px">
                                "EVENT TIDAK DITEMUKAN"
                            </h2>
                            <p style="color:var(--text-muted);font-size:.85rem;margin-bottom:20px">
                                "Event ini mungkin sudah selesai atau telah dihapus."
                            </p>
                            <A href="/explore" attr:class="tier-add-btn">"JELAJAHI EVENT"</A>
                        </div>
                    }.into_any(),

                    Ok(ev) => {
                        let title      = ev.name.clone();
                        let cover      = ev.cover_url.clone().unwrap_or_default();
                        let date_str   = format_date(&ev.event_date);
                        let venue_str  = match (&ev.venue, &ev.city) {
                            (Some(v), Some(c)) => format!("{}, {}", v, c),
                            (Some(v), None)    => v.clone(),
                            (None, Some(c))    => c.clone(),
                            (None, None)       => String::new(),
                        };
                        let cats       = ev.category.clone();
                        let desc       = ev.description.clone().unwrap_or_default();
                        let base_price = ev.display_price;
                        let ev_slug    = ev.slug.clone();
                        let ev_id      = ev.id.clone();
                        let ev_name    = ev.name.clone();
                        let ev_cover   = ev.cover_url.clone().unwrap_or_default();
                        let variants   = ev.event_variants.clone();

                        // ── Cart total helpers: inline in each closure ─────────
                        // cart_ctx is Copy. Each closure gets a distinct clone of ev_id.
                        let ev_id_price = ev_id.clone();
                        let ev_id_btn   = ev_id.clone();

                        // ── Share to story handler ────────────────────────────
                        let _share_slug      = ev_slug.clone();
                        let _share_title     = title.clone();
                        let _share_cover     = cover.clone();
                        let _share_id        = ev_id.clone();
                        let _share_desc      = desc.clone();
                        let _share_venue     = venue_str.clone();
                        let _share_price_str = format_price(base_price);
                        let _share_date_str  = date_str.clone();

                        let _nav_story = navigate.clone();
                        let share_to_story = move |_: web_sys::MouseEvent| {
                            #[cfg(target_arch = "wasm32")]
                            {
                                let params = web_sys::UrlSearchParams::new()
                                    .expect("UrlSearchParams");
                                params.append("event_id",    &_share_id);
                                params.append("event_slug",  &_share_slug);
                                params.append("event_title", &_share_title);
                                params.append("event_cover", &_share_cover);
                                params.append("event_desc",  &_share_desc);
                                params.append("event_date",  &_share_date_str);
                                params.append("event_venue", &_share_venue);
                                params.append("event_price", &_share_price_str);
                                if let Some(win) = web_sys::window() {
                                    if let Ok(Some(storage)) = win.session_storage() {
                                        let _ = storage.set_item("story_hero_transition", "event");
                                        let _ = storage.set_item("story_hero_cover", &_share_cover);
                                    }
                                }
                                let qs = params.to_string();
                                _nav_story(&format!("/story?{}", qs), Default::default());
                            }
                        };

                        // ── Tier cards (cart-based like CSR) ──────────────────
                        let tiers_view = variants.into_iter().map(|v| {
                            let vid         = v.id.clone();
                            let vid_add     = v.id.clone();
                            let vid_plus    = v.id.clone();
                            let vid_minus   = v.id.clone();
                            let vname       = v.name.clone();
                            let vdesc       = v.description.clone();
                            let vprice      = format_price(v.display_price);
                            let vprice_val  = v.display_price as i64;
                            let rem         = v.remaining;
                            let is_vip      = v.name.to_lowercase().contains("vip");
                            let card_cls    = if is_vip { "tier-card tier-card--vip" } else { "tier-card" };

                            let ev_id_a  = ev_id.clone();
                            let ev_nm_a  = ev_name.clone();
                            let venue_a  = venue_str.clone();
                            let cover_a  = ev_cover.clone();
                            let tname_a  = vname.clone();
                            let vid_qty  = vid.clone();

                            let on_add = {
                                let ev_id_a  = ev_id_a.clone();
                                let ev_nm_a  = ev_nm_a.clone();
                                let venue_a  = venue_a.clone();
                                let cover_a  = cover_a.clone();
                                let tname_a  = tname_a.clone();
                                let vid_add  = vid_add.clone();
                                move |e: web_sys::MouseEvent| {
                                    e.stop_propagation();
                                    cart_ctx.add_item(CartItem {
                                        event_id:    ev_id_a.clone(),
                                        tier_id:     vid_add.clone(),
                                        event_title: ev_nm_a.clone(),
                                        tier_name:   tname_a.clone(),
                                        venue_name:  venue_a.clone(),
                                        event_cover: cover_a.clone(),
                                        quantity:    1,
                                        unit_price:  vprice_val,
                                    });
                                }
                            };
                            let on_minus = move |e: web_sys::MouseEvent| {
                                e.stop_propagation();
                                let q = cart_ctx.get_qty(&vid_minus);
                                if q > 0 { cart_ctx.update_qty(&vid_minus, q - 1); }
                            };
                            let on_plus = move |e: web_sys::MouseEvent| {
                                e.stop_propagation();
                                let q = cart_ctx.get_qty(&vid_plus);
                                cart_ctx.update_qty(&vid_plus, q + 1);
                            };

                            view! {
                                <div class=card_cls>
                                    <div class="tier-top">
                                        <div class="tier-name">{vname.clone()}</div>
                                        {vdesc.map(|d| view! {
                                            <p class="tier-desc">{d}</p>
                                        })}
                                        {(is_vip && rem <= 15).then(|| view! {
                                            <span class="tier-scarcity">
                                                <span class="scarcity-dot"></span>
                                                {format!("Only {} Left", rem)}
                                            </span>
                                        })}
                                    </div>
                                    <div class="tier-bottom">
                                        <span class="tier-price">{vprice}</span>
                                        {move || {
                                            let q = cart_ctx.get_qty(&vid_qty);
                                            if q == 0 {
                                                view! {
                                                    <button class="tier-add-btn"
                                                        on:click=on_add.clone()>
                                                        "Add"
                                                    </button>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="qty-ctrl">
                                                        <button class="qty-btn qty-btn--minus"
                                                            on:click=on_minus.clone()>
                                                            "−"
                                                        </button>
                                                        <span class="qty-val">{q}</span>
                                                        <button class="qty-btn qty-btn--plus"
                                                            on:click=on_plus.clone()>
                                                            "+"
                                                        </button>
                                                    </div>
                                                }.into_any()
                                            }
                                        }}
                                    </div>
                                </div>
                            }
                        }).collect_view();

                        let meta_title = format!("{} — PULSE", title);
                        let meta_desc = format!(
                            "{} | {} | {}",
                            if desc.is_empty() { "Event seru di Indonesia" } else { &desc },
                            venue_str,
                            date_str,
                        );

                        view! {
                                <Title text=meta_title />
                                <Meta name="description" content=meta_desc />
                                // ── Header ───────────────────────────────────────────
                                <header class="page-header ed-header">
                                    <button class="back-btn" aria-label="Back"
                                        on:click=move |_| {
                                            #[cfg(target_arch = "wasm32")]
                                            if let Some(win) = web_sys::window() {
                                                let _ = win.history().ok().map(|h| h.back());
                                            }
                                        }>
                                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                            stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                            <polyline points="15 18 9 12 15 6" />
                                        </svg>
                                    </button>
                                    <span class="page-logo">"KINETIC"</span>
                                    <div class="header-actions">
                                        <ThemeToggle />
                                        <button class="icon-btn" on:click=share_to_story
                                            aria-label="Bagikan ke Cerita">
                                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                                stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
                                                <circle cx="18" cy="5" r="3" />
                                                <circle cx="6" cy="12" r="3" />
                                                <circle cx="18" cy="19" r="3" />
                                                <line x1="8.59" y1="13.51" x2="15.42" y2="17.49" />
                                                <line x1="15.41" y1="6.51" x2="8.59" y2="10.49" />
                                            </svg>
                                        </button>
                                        <A href="/notifications" attr:class="bell-btn">
                                            <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                                                stroke="currentColor" stroke-width="2"
                                                stroke-linecap="round" stroke-linejoin="round">
                                                <path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9" />
                                                <path d="M13.73 21a2 2 0 01-3.46 0" />
                                            </svg>
                                        </A>
                                        <A href="/profile" attr:class="nav-avatar">
                                            <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                                                stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                                <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
                                                <circle cx="12" cy="7" r="4" />
                                            </svg>
                                        </A>
                                    </div>
                                </header>

                                // ── Hero ─────────────────────────────────────────────
                                <div class="ed-hero">
                                    {if cover.is_empty() {
                                        view! {
                                            <div class="ed-hero-img"
                                                style="background:var(--bg-elevated)" />
                                        }.into_any()
                                    } else {
                                        view! {
                                            <img src=cover alt=title.clone() class="ed-hero-img" />
                                        }.into_any()
                                    }}
                                    <div class="ed-hero-gradient"></div>
                                    <div class="ed-hero-overlay-content">
                                        <div class="ed-hero-badges">
                                            <span class="ed-live-badge">
                                                <span class="ed-live-dot"></span>
                                                "LIVE NOW"
                                            </span>
                                            {cats.first().map(|c| view! {
                                                <span class="ed-cat-badge">{c.clone()}</span>
                                            })}
                                        </div>
                                        <h1 class="ed-hero-title">{title.clone()}</h1>
                                        <div class="ed-hero-meta">
                                            <div class="ed-hero-meta-item">
                                                <svg width="13" height="13" viewBox="0 0 24 24"
                                                    fill="none" stroke="currentColor" stroke-width="2"
                                                    stroke-linecap="round">
                                                    <rect x="3" y="4" width="18" height="18" rx="2" />
                                                    <line x1="16" y1="2" x2="16" y2="6" />
                                                    <line x1="8" y1="2" x2="8" y2="6" />
                                                    <line x1="3" y1="10" x2="21" y2="10" />
                                                </svg>
                                                {date_str}
                                            </div>
                                            {(!venue_str.is_empty()).then({
                                                let vs = venue_str.clone();
                                                move || view! {
                                                    <div class="ed-hero-meta-item">
                                                        <svg width="13" height="13" viewBox="0 0 24 24"
                                                            fill="none" stroke="currentColor"
                                                            stroke-width="2" stroke-linecap="round">
                                                            <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0118 0z" />
                                                            <circle cx="12" cy="10" r="3" />
                                                        </svg>
                                                        {vs.clone()}
                                                    </div>
                                                }
                                            })}
                                        </div>
                                    </div>
                                </div>

                                // ── Body ─────────────────────────────────────────────
                                <div class="ed-body">
                                    <div class="ed-main">

                                        // About
                                        {(!desc.is_empty()).then({
                                            let d = desc.clone();
                                            move || view! {
                                                <section class="section">
                                                    <p class="ed-section-eyebrow">"ABOUT THE EVENT"</p>
                                                    <p class="about-text">{d.clone()}</p>
                                                </section>
                                            }
                                        })}

                                        // Categories
                                        {(!cats.is_empty()).then({
                                            let c2 = cats.clone();
                                            move || view! {
                                                <div class="ed-categories-section">
                                                    <p class="ed-section-eyebrow">"CATEGORIES"</p>
                                                    <div class="ed-chips-row">
                                                        {c2.iter().map(|c| view! {
                                                            <span class="ed-chip">{c.clone()}</span>
                                                        }).collect_view()}
                                                    </div>
                                                </div>
                                            }
                                        })}

                                        // Select Tickets
                                        <div class="ed-tickets-header">
                                            <span class="ed-tickets-title">"Select Tickets"</span>
                                            <span class="ed-tickets-avail">"Available until sale ends"</span>
                                        </div>
                                        <section class="section ed-mobile-tiers">
                                            {tiers_view}
                                        </section>

                                        // Venue
                                        <section class="section">
                                            <h2 class="section-title">"The Venue"</h2>
                                        </section>
                                        <div class="map-card">
                                            <div class="map-visual">
                                                <div class="map-grid"></div>
                                                <div class="map-pin">
                                                    <svg width="28" height="36" viewBox="0 0 32 40">
                                                        <path d="M16 0C7.163 0 0 7.163 0 16c0 11 16 24 16 24s16-13 16-24C32 7.163 24.837 0 16 0z"
                                                            fill="#c8ff5e" />
                                                        <circle cx="16" cy="16" r="6" fill="#0d0d1a" />
                                                    </svg>
                                                </div>
                                            </div>
                                            <div class="map-info">
                                                {(!ev_slug.is_empty()).then({
                                                    let es = ev_slug.clone();
                                                    move || view! {
                                                        <A href=format!("/events/{}/location", es)
                                                            attr:class="directions-btn">
                                                            <svg width="14" height="14" viewBox="0 0 24 24"
                                                                fill="none" stroke="currentColor"
                                                                stroke-width="2" stroke-linecap="round">
                                                                <line x1="5" y1="12" x2="19" y2="12" />
                                                                <polyline points="12 5 19 12 12 19" />
                                                            </svg>
                                                            "Get Directions"
                                                        </A>
                                                    }
                                                })}
                                            </div>
                                        </div>
                                    </div>
                                </div>

                                // ── Sticky footer: starting price + Secure Tickets ───
                                <div class="sticky-footer ed-mobile-footer">
                                    <div class="ed-footer-starting">
                                        {move || {
                                            let qty: i32 = cart_ctx.items.with(|v| {
                                                v.iter()
                                                    .filter(|i| i.event_id == ev_id_price)
                                                    .map(|i| i.quantity)
                                                    .sum()
                                            });
                                            if qty > 0 {
                                                view! {
                                                    <span class="footer-label">"IN CART"</span>
                                                    <span class="footer-cart-qty">
                                                        {format!("{} TICKET{}", qty, if qty == 1 { "" } else { "S" })}
                                                    </span>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <span class="footer-label">"STARTING FROM"</span>
                                                    <span class="starting-price">{format_price(base_price)}</span>
                                                }.into_any()
                                            }
                                        }}
                                    </div>
                                    {move || {
                                        let ti: i32 = cart_ctx.items.with(|v| {
                                            v.iter()
                                                .filter(|i| i.event_id == ev_id_btn)
                                                .map(|i| i.quantity)
                                                .sum()
                                        });
                                        if is_logged_in() {
                                            view! {
                                                <A href="/cart"
                                                    attr:class=if ti == 0 {
                                                        "ed-secure-btn ed-secure-btn--disabled"
                                                    } else {
                                                        "ed-secure-btn"
                                                    }
                                                    attr:aria-disabled=if ti == 0 { "true" } else { "false" }
                                                >
                                                    "Secure Tickets"
                                                </A>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <a href="/login" class="ed-secure-btn">
                                                    "Masuk untuk Beli"
                                                </a>
                                            }.into_any()
                                        }
                                    }}
                                </div>
                        }.into_any()
                    }
                }
            })
            }}
        </Suspense>
        </div>
    }
}
