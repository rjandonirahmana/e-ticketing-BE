use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::state::events::{event_to_explore_pub, ExploreEvent};
use crate::web::utils::format_number;

#[component]
pub fn EventCard(
    href: String,
    img: String,
    #[prop(into)] alt: String,
    #[prop(into)] badge: String,
    #[prop(into)] title: String,
    #[prop(optional)] date: Option<String>,
    #[prop(into)] venue: String,
    #[prop(into)] price: String,
) -> impl IntoView {
    view! {
        <A href=href attr:class="explore-event-card-v2">
            <div class="explore-ecard-img-wrap">
                <img src=img alt=alt class="explore-ecard-img" />
                <span class="explore-ecard-cat">{badge}</span>
            </div>
            <div class="explore-ecard-body">
                <h3 class="explore-ecard-title">{title}</h3>
                {date
                    .map(|d| {
                        view! {
                            <div class="explore-ecard-meta">
                                <svg
                                    width="11"
                                    height="11"
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
                                <span>{d}</span>
                            </div>
                        }
                    })}
                <div class="explore-ecard-meta">
                    <svg
                        width="11"
                        height="11"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    >
                        <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0118 0z" />
                        <circle cx="12" cy="10" r="3" />
                    </svg>
                    <span>{venue}</span>
                </div>
                <div class="explore-ecard-footer">
                    <span class="explore-ecard-price">{price}</span>
                    <span class="explore-ecard-arrow">
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
                    </span>
                </div>
            </div>
        </A>
    }
}

#[component]
pub fn EventCardShimmer() -> impl IntoView {
    view! {
        <div class="shim-event-card">
            <div class="shim shim-img"></div>
            <div class="shim-body">
                <div class="shim shim-cat"></div>
                <div class="shim shim-title"></div>
                <div class="shim shim-date"></div>
                <div class="shim-foot">
                    <div class="shim shim-venue"></div>
                    <div class="shim shim-price"></div>
                </div>
            </div>
        </div>
    }
}

/// Kartu event "marketplace" — SATU sumber tunggal untuk daftar event di
/// Explore, Event Detail, dan /m/:id. Menerima `ExploreEvent` (model tampil
/// yang sudah berisi tanggal/harga terformat, is_live, dll). `index` dipakai
/// untuk delay animasi cascade (`--i`).
#[component]
pub fn EventCardPub(ev: ExploreEvent, #[prop(default = 0)] index: usize) -> impl IntoView {
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
    let price_disp = if ev.price <= 0 {
        "Gratis".to_string()
    } else {
        ev.price_str.clone()
    };
    let org_href = format!("/m/{}", ev.merchant_id);
    view! {
        <a
            href=href
            class="exp-mkt-card exp-cascade"
            style=format!("--i:{}", (index % 20).min(5))
        >
            <div class="exp-mkt-img-wrap">
                <img
                    src=ev.cover.clone()
                    alt=ev.title.clone()
                    class="exp-mkt-img"
                    loading="lazy"
                />
                {ev
                    .is_live
                    .then(|| {
                        view! {
                            <span class="exp-mkt-live">
                                <span class="exp-mkt-live-dot"></span>
                                "LIVE"
                            </span>
                        }
                    })}
            </div>
            <div class="exp-mkt-body">
                {(!cat.is_empty())
                    .then(|| {
                        view! { <span class="exp-mkt-merchant">{cat.clone()}</span> }
                    })} <h3 class="exp-mkt-title">{ev.title.clone()}</h3>
                // Chip penyelenggara → profil merchant publik. <span> ber-click
                // (bukan <a>) karena kartu ini sendiri <a> — anchor bersarang invalid.
                <span
                    class="exp-mkt-org"
                    on:click={
                        move |e: leptos::ev::MouseEvent| {
                            e.prevent_default();
                            e.stop_propagation();
                            #[cfg(target_arch = "wasm32")]
                            if let Some(w) = web_sys::window() {
                                let _ = w.location().assign(&org_href);
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            let _ = &org_href;
                        }
                    }
                >
                    "PENYELENGGARA \u{2192}"
                </span>
                <div class="exp-mkt-meta">
                    <span class="exp-mkt-meta-row">
                        <svg
                            width="12"
                            height="12"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                        >
                            <rect x="3" y="4" width="18" height="18" rx="2" />
                            <line x1="16" y1="2" x2="16" y2="6" />
                            <line x1="8" y1="2" x2="8" y2="6" />
                            <line x1="3" y1="10" x2="21" y2="10" />
                        </svg>
                        {ev.date.clone()}
                    </span>
                    {(!loc.is_empty())
                        .then(|| {
                            view! {
                                <span class="exp-mkt-meta-row">
                                    <svg
                                        width="12"
                                        height="12"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                    >
                                        <path d="M21 10c0 7-9 12-9 12s-9-5-9-12a9 9 0 0118 0z" />
                                        <circle cx="12" cy="10" r="3" />
                                    </svg>
                                    {loc.clone()}
                                </span>
                            }
                        })}
                </div>
                <div class="exp-mkt-price-block">
                    <span class="exp-mkt-from">"Mulai Dari"</span>
                    <span class="exp-mkt-price">{price_disp}</span>
                </div>
                <div class="exp-mkt-foot">
                    <svg
                        class="exp-mkt-star"
                        width="13"
                        height="13"
                        viewBox="0 0 24 24"
                        fill="currentColor"
                        stroke="none"
                    >
                        <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
                    </svg>
                    <span class="exp-mkt-sold">{sold_label}</span>
                </div>
            </div>
        </a>
    }
}

/// Grid daftar event reusable (profil merchant, dsb). Menerima `Vec<Event>`,
/// mengonversi ke `ExploreEvent`, dan merender kartu `EventCardPub` yang sama
/// dengan Explore. Kosong → teks `empty`.
#[component]
pub fn EventGrid(
    events: Vec<crate::web::models::Event>,
    #[prop(optional, into)] empty: Option<String>,
) -> impl IntoView {
    if events.is_empty() {
        let msg = empty.unwrap_or_else(|| "Belum ada event.".into());
        return view! { <p class="event-grid-empty">{msg}</p> }.into_any();
    }
    view! {
        <div class="exp-mkt-grid">
            {events
                .into_iter()
                .enumerate()
                .map(|(i, e)| {
                    let ev = event_to_explore_pub(&e);
                    view! { <EventCardPub ev=ev index=i /> }
                })
                .collect_view()}
        </div>
    }
        .into_any()
}

/// Skeleton loading untuk `EventGrid` — `count` kartu shimmer dalam grid sama.
#[component]
pub fn EventGridShimmer(#[prop(default = 4)] count: usize) -> impl IntoView {
    view! {
        <div class="exp-mkt-grid">
            {(0..count).map(|_| view! { <EventCardShimmer /> }).collect_view()}
        </div>
    }
}

#[component]
pub fn TicketCardShimmer() -> impl IntoView {
    // Skeleton meniru layout .ticket-card asli di /tickets:
    // cover penuh → judul besar → baris venue•tanggal → meta grid 2 kolom
    // (TIER/PRICE) → QR di tengah → footer (tombol OPEN QR + kode tiket).
    view! {
        <div class="shim-ticket-card">
            <div class="shim shim-tkt-cover"></div>
            <div class="shim-tkt-body">
                <div class="shim shim-tkt-title"></div>
                <div class="shim shim-tkt-venue"></div>
                <div class="shim-tkt-meta">
                    <div class="shim shim-tkt-meta-item"></div>
                    <div class="shim shim-tkt-meta-item"></div>
                </div>
                <div class="shim-tkt-qr-wrap">
                    <div class="shim shim-tkt-qr"></div>
                    <div class="shim shim-tkt-code"></div>
                </div>
                <div class="shim-tkt-foot">
                    <div class="shim shim-tkt-badge"></div>
                    <div class="shim shim-tkt-price"></div>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn OrderCardShimmer() -> impl IntoView {
    view! {
        <div class="shim-order-card">
            <div class="shim-order-card-top">
                <div class="shim shim-ord-thumb"></div>
                <div class="shim-ord-info">
                    <div class="shim shim-ord-name"></div>
                    <div class="shim shim-ord-date"></div>
                </div>
            </div>
            <div class="shim shim-ord-divider"></div>
            <div class="shim-foot">
                <div class="shim shim-ord-amount"></div>
                <div class="shim shim-ord-btn"></div>
            </div>
        </div>
    }
}

#[component]
pub fn MerchantRowShimmer() -> impl IntoView {
    view! {
        <div class="shim-merchant-row">
            <div class="shim-mr-info">
                <div class="shim shim-mr-name"></div>
                <div class="shim shim-mr-venue"></div>
            </div>
            <div class="shim shim-mr-badge"></div>
        </div>
    }
}

/// Skeleton that mirrors the `.mhub-event-card` layout used on the merchant &
/// admin hubs (cover image, title/price row, meta, sales progress, action btn).
/// The previous `MerchantRowShimmer` looked nothing like the real cards, which
/// made the loading state jarring.
#[component]
pub fn MerchantEventCardShimmer() -> impl IntoView {
    view! {
        <div class="shim-mhub-card">
            <div class="shim shim-mhub-img"></div>
            <div class="shim-mhub-body">
                <div class="shim-mhub-row">
                    <div class="shim shim-mhub-title"></div>
                    <div class="shim shim-mhub-price"></div>
                </div>
                <div class="shim shim-mhub-meta"></div>
                <div class="shim-mhub-row">
                    <div class="shim shim-mhub-key"></div>
                    <div class="shim shim-mhub-key"></div>
                </div>
                <div class="shim shim-mhub-bar"></div>
                <div class="shim shim-mhub-btn"></div>
            </div>
        </div>
    }
}

#[component]
pub fn MessageRowShimmer() -> impl IntoView {
    view! {
        <div class="shim-msg-row">
            <div class="shim shim-avatar"></div>
            <div class="shim-msg-info">
                <div class="shim shim-msg-name"></div>
                <div class="shim shim-msg-preview"></div>
            </div>
        </div>
    }
}
