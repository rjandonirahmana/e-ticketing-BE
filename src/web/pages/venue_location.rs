//! venue_location.rs — Halaman Lokasi Venue Event (SSR + client map).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::{get_event_location, get_event_detail};

#[component]
pub fn VenueLocationPage() -> impl IntoView {
    let params = use_params_map();
    let slug = move || params.read().get("slug").unwrap_or_default();

    let event = Resource::new(slug, |s| get_event_detail(s));

    let location_data = Resource::new(slug, |s| get_event_location(s));

    view! {
        <div class="container" style="padding-top:2rem;padding-bottom:4rem">
            <div style="margin-bottom:1.5rem">
                <Suspense fallback=|| view!{<span/>}>
                    {move || event.get().map(|res| match res {
                        Ok(ev) => view! {
                            <A href=format!("/events/{}", ev.slug) attr:class="btn btn--ghost btn--sm">
                                "← " {ev.name}
                            </A>
                        }.into_any(),
                        Err(_) => view! {
                            <A href="/explore" attr:class="btn btn--ghost btn--sm">"← Kembali"</A>
                        }.into_any(),
                    }).unwrap_or_else(|| view!{<span/>}.into_any())}
                </Suspense>
            </div>

            <div class="page-header">
                <div>
                    <p class="page-header__eyebrow">"// lokasi venue"</p>
                    <h1 class="page-header__title">"Lokasi Event"</h1>
                </div>
            </div>

            <Suspense fallback=|| view! {
                <div class="loading"><div class="loading__spinner"/></div>
            }>
                {move || {
                    let ev_res = event.get();
                    let loc_res = location_data.get();
                    match (ev_res, loc_res) {
                        (Some(Ok(ev)), _) => {
                            let venue = ev.venue.clone().unwrap_or_else(|| "Venue TBA".into());
                            let city  = ev.city.clone().unwrap_or_default();
                            let full_addr = if city.is_empty() { venue.clone() } else { format!("{}, {}", venue, city) };
                            let maps_url  = format!("https://maps.google.com/?q={}", urlencoding::encode(&full_addr));

                            view! {
                                <div>
                                    // Info card
                                    <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:12px;padding:1.25rem;margin-bottom:1.25rem">
                                        <h2 style="font-weight:700;font-size:1rem;margin-bottom:.5rem">{ev.name}</h2>
                                        <p style="color:var(--clr-muted);font-size:.875rem;margin-bottom:.25rem">"📍 "{full_addr.clone()}</p>
                                        {if !city.is_empty() {
                                            view! { <p style="color:var(--clr-muted);font-size:.875rem">"🏙 "{city}</p> }.into_any()
                                        } else { view!{<span/>}.into_any() }}
                                    </div>

                                    // Map placeholder (Google Maps embed atau OpenStreetMap)
                                    <div style="width:100%;height:300px;border-radius:12px;overflow:hidden;border:1px solid var(--clr-border);margin-bottom:1.25rem;background:var(--clr-surface);display:flex;align-items:center;justify-content:center"
                                        id="venue-map-mount"
                                    >
                                        <div style="text-align:center;color:var(--clr-muted)">
                                            <div style="font-size:2rem;margin-bottom:.5rem">"🗺"</div>
                                            <p style="font-size:.875rem">"Peta venue"</p>
                                            <p style="font-size:.75rem">{full_addr.clone()}</p>
                                        </div>
                                    </div>

                                    <div style="display:flex;gap:.75rem;flex-wrap:wrap">
                                        <a href=maps_url target="_blank" class="btn btn--accent">
                                            "🗺 Buka di Google Maps"
                                        </a>
                                        <a
                                            href=format!("https://www.waze.com/ul?q={}", urlencoding::encode(&full_addr))
                                            target="_blank"
                                            class="btn btn--ghost"
                                        >
                                            "Navigate via Waze"
                                        </a>
                                    </div>
                                </div>
                            }.into_any()
                        }
                        _ => view! {
                            <div class="empty">
                                <div class="empty__icon">"📍"</div>
                                <div class="empty__title">"Lokasi belum tersedia"</div>
                            </div>
                        }.into_any(),
                    }
                }}
            </Suspense>
        </div>
    }
}
