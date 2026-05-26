use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::csr::components::BottomNav;
use crate::csr::hooks::ThemeToggle;
use crate::csr::state::use_venue_events_store;

#[component]
pub fn VenueLocationPage() -> impl IntoView {
    let params = use_params_map();
    let id = params.with_untracked(|p| p.get("slug").unwrap_or_default());

    let store = use_venue_events_store();
    // Panggil langsung — VenueEventsCtx::load() sudah punya loading guard internal
    store.load(id.clone());

    view! {
        <div class="page">
            <header class="page-header">
                <button class="back-btn" on:click=move |_| {
                    let _ = web_sys::window().and_then(|w| w.history().ok()).map(|h| h.back());
                }>
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </button>
                <span class="page-logo">"LOCATION"</span>
                <div class="header-actions">
                    <ThemeToggle />
                    <button class="icon-btn" aria-label="Share">
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/>
                            <line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/>
                        </svg>
                    </button>
                </div>
            </header>

            <div class="vl-map">
                <div class="vl-map-grid"></div>
                <svg class="vl-map-route" viewBox="0 0 360 280" preserveAspectRatio="none">
                    <path d="M -20 60 Q 100 30 180 130 T 380 230" fill="none" stroke="#4f6bff" stroke-width="1.5" opacity="0.6"/>
                    <path d="M 20 200 Q 120 180 200 100 T 360 40" fill="none" stroke="#7c4fff" stroke-width="1" opacity="0.4"/>
                </svg>
                {[(40.0_f32, 50.0_f32), (300.0, 70.0), (90.0, 220.0), (260.0, 230.0)].iter().map(|(x, y)| {
                    let style = format!("left: {}px; top: {}px;", x, y);
                    view! {
                        <div class="vl-pin-mini" style=style>
                            <svg width="14" height="18" viewBox="0 0 32 40">
                                <path d="M16 0C7.163 0 0 7.163 0 16c0 11 16 24 16 24s16-13 16-24C32 7.163 24.837 0 16 0z" fill="#55556a"/>
                            </svg>
                        </div>
                    }
                }).collect_view()}
                <div class="vl-pin-main">
                    <div class="vl-pin-pulse"></div>
                    <div class="vl-pin-circle">
                        <div class="vl-pin-dot"></div>
                    </div>
                    <span class="vl-pin-label">"JIS STADIUM"</span>
                </div>
            </div>

            <div class="vl-cta-wrap">
                <button class="vl-directions-btn">
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
                        <polygon points="3 11 22 2 13 21 11 13 3 11"/>
                    </svg>
                    "GET DIRECTIONS"
                </button>
            </div>

            <div class="vl-info-card">
                <div class="vl-info-head">
                    <div>
                        <span class="vl-eyebrow">"PRIMARY VENUE"</span>
                        <h2 class="vl-venue-name">"Jakarta International Stadium"</h2>
                    </div>
                    <div class="vl-distance">
                        <span class="vl-dist-val">"1.2"</span>
                        <span class="vl-dist-unit">"KM"</span>
                        <span class="vl-dist-away">"AWAY"</span>
                    </div>
                </div>
                <div class="vl-addr-row">
                    <div class="vl-addr-icon">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#c8ff5e" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0118 0z"/>
                            <circle cx="12" cy="10" r="3"/>
                        </svg>
                    </div>
                    <p class="vl-addr-text">
                        "Papanggo, Tanjung Priok, North Jakarta City, Jakarta 14340, Indonesia"
                    </p>
                </div>
            </div>

            <section class="vl-section">
                <div class="vl-section-line"></div>
                <h3 class="vl-section-title">"VENUE FACILITIES"</h3>
            </section>

            <div class="vl-facilities-grid">
                <div class="vl-facility">
                    <div class="vl-facility-icon vl-facility-icon--blue">
                        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#7da7ff" stroke-width="2.2" stroke-linecap="round">
                            <rect x="4" y="3" width="16" height="18" rx="2"/>
                            <path d="M9 8h4a3 3 0 010 6H9V8zm0 6v4"/>
                        </svg>
                    </div>
                    <span class="vl-facility-label">"Premium"<br/>"Parking"</span>
                </div>
                <div class="vl-facility">
                    <div class="vl-facility-icon vl-facility-icon--lime">
                        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#c8ff5e" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M3 2v7c0 1.1.9 2 2 2h2v11h2V2H3zm5 0v9M14 2v20h2V12h2c1.1 0 2-.9 2-2V2h-6z"/>
                        </svg>
                    </div>
                    <span class="vl-facility-label">"Food"<br/>"Court"</span>
                </div>
                <div class="vl-facility">
                    <div class="vl-facility-icon vl-facility-icon--blue">
                        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#7da7ff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <circle cx="12" cy="4" r="2"/>
                            <path d="M19 13v-2a7 7 0 00-14 0v2"/>
                            <circle cx="12" cy="17" r="5"/>
                        </svg>
                    </div>
                    <span class="vl-facility-label">"Accessibility"</span>
                </div>
                <div class="vl-facility">
                    <div class="vl-facility-icon vl-facility-icon--lime">
                        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="#c8ff5e" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <circle cx="9" cy="5" r="2"/><circle cx="15" cy="5" r="2"/>
                            <path d="M7 22V11M11 22V11M13 22V11M17 22V11"/>
                        </svg>
                    </div>
                    <span class="vl-facility-label">"Restrooms"</span>
                </div>
            </div>

            <section class="vl-section">
                <div class="vl-section-line"></div>
                <div class="vl-section-row">
                    <h3 class="vl-section-title">"EVENTS AT VENUE"</h3>
                    <button class="vl-view-all">"VIEW ALL"</button>
                </div>
            </section>

            <div class="vl-events-list">
                {move || store.items.with(|events| {
                    events.iter().cloned().map(|ev| {
                        let href = format!("/events/{}", ev.slug);
                        let cover_style = format!("background: {};", ev.grad);
                        view! {
                            <A href=href attr:class="vl-event-card">
                                <div class="vl-event-cover" style=cover_style></div>
                                <div class="vl-event-body">
                                    <span class="vl-event-date">{ev.date}</span>
                                    <h4 class="vl-event-title">{ev.title}</h4>
                                    <span class="vl-event-price">{ev.price}</span>
                                </div>
                                <svg class="vl-event-chev" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                    <polyline points="9 18 15 12 9 6"/>
                                </svg>
                            </A>
                        }
                    }).collect_view()
                })}
            </div>

            <BottomNav active="" />
        </div>
    }
}
