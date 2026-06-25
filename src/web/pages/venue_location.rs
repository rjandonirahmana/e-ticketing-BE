//! venue_location.rs — Halaman Lokasi Venue Event (SSR + peta OpenStreetMap).
//!
//! Memakai sistem desain aplikasi (.page / .page-header / back-btn / ThemeToggle)
//! agar lebar & gaya konsisten dengan halaman lain.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::web::api::get_event_detail;
use crate::web::hooks::ThemeToggle;
use crate::web::utils::{map_destroy, map_viewer};

/// Minimal percent-encoding untuk query string (space → +, special → %XX).
fn encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b));
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[component]
fn LocationSkeleton() -> impl IntoView {
    view! {
        <div style="padding:16px;display:flex;flex-direction:column;gap:16px">
            <div
                class="shim"
                style="height:120px;border-radius:16px"
            ></div>
            <div
                class="shim"
                style="height:340px;border-radius:16px"
            ></div>
            <div style="display:flex;gap:10px">
                <div class="shim" style="flex:1;height:48px;border-radius:12px"></div>
                <div class="shim" style="flex:1;height:48px;border-radius:12px"></div>
            </div>
        </div>
    }
}

#[component]
pub fn VenueLocationPage() -> impl IntoView {
    let params = use_params_map();
    let slug = move || params.read().get("slug").unwrap_or_default();

    let event = Resource::new(slug, |s| get_event_detail(s));

    // Render peta OpenStreetMap begitu koordinat event tersedia.
    Effect::new(move |_| {
        if let Some(Ok(ev)) = event.get() {
            if let (Some(la), Some(lo)) = (ev.latitude, ev.longitude) {
                let label = ev.venue.clone().unwrap_or_else(|| ev.name.clone());
                map_viewer("venue-map-mount", la, lo, &label);
            }
        }
    });
    on_cleanup(|| map_destroy("venue-map-mount"));

    view! {
        <div class="page vloc-page">

            // ── Header (konsisten dengan halaman lain) ─────────────────────────
            <header class="page-header">
                <button
                    class="back-btn"
                    aria-label="Kembali"
                    on:click=move |_| {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(win) = web_sys::window() {
                            let _ = win.history().ok().map(|h| h.back());
                        }
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
                <span class="page-logo">"LOKASI"</span>
                <div class="header-actions">
                    <ThemeToggle />
                </div>
            </header>

            <Suspense fallback=|| view! { <LocationSkeleton /> }>
                {move || {
                    match event.get() {
                        Some(Ok(ev)) => {
                            let venue = ev.venue.clone().unwrap_or_else(|| "Venue TBA".into());
                            let city = ev.city.clone().unwrap_or_default();
                            let full_addr = if city.is_empty() {
                                venue.clone()
                            } else {
                                format!("{}, {}", venue, city)
                            };
                            let has_coords = ev.latitude.is_some() && ev.longitude.is_some();
                            let maps_url = match (ev.latitude, ev.longitude) {
                                (Some(la), Some(lo)) => {
                                    format!("https://www.google.com/maps?q={},{}", la, lo)
                                }
                                _ => format!("https://maps.google.com/?q={}", encode_query(&full_addr)),
                            };
                            let waze_url = format!(
                                "https://www.waze.com/ul?q={}",
                                encode_query(&full_addr),
                            );
                            let addr_for_card = full_addr.clone();

                            view! {
                                <div style="padding:16px;display:flex;flex-direction:column;gap:16px">

                                    // ── Kartu info venue ───────────────────────
                                    <div style="background:var(--bg-card);border:1px solid var(--border);border-radius:16px;padding:18px">
                                        <p style="font-size:11px;letter-spacing:0.1em;text-transform:uppercase;color:var(--text-muted);margin:0 0 6px">
                                            "Lokasi Acara"
                                        </p>
                                        <h1 style="font-family:var(--font-display);font-size:22px;line-height:1.15;color:var(--text-primary);margin:0 0 14px">
                                            {ev.name.clone()}
                                        </h1>
                                        <div style="display:flex;align-items:flex-start;gap:10px">
                                            <svg
                                                width="18"
                                                height="18"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="var(--accent-lime)"
                                                stroke-width="2"
                                                stroke-linecap="round"
                                                stroke-linejoin="round"
                                                style="flex-shrink:0;margin-top:1px"
                                            >
                                                <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0 1 18 0z" />
                                                <circle cx="12" cy="10" r="3" />
                                            </svg>
                                            <span style="color:var(--text-secondary);font-size:14px;line-height:1.4">
                                                {addr_for_card}
                                            </span>
                                        </div>
                                    </div>

                                    // ── Peta OpenStreetMap / placeholder ───────
                                    {if has_coords {
                                        view! {
                                            <div
                                                id="venue-map-mount"
                                                style="width:100%;height:340px;border-radius:16px;overflow:hidden;border:1px solid var(--border);background:var(--bg-elevated)"
                                            ></div>
                                        }
                                            .into_any()
                                    } else {
                                        view! {
                                            <div style="width:100%;height:300px;border-radius:16px;overflow:hidden;border:1px solid var(--border);background:var(--bg-elevated);display:flex;align-items:center;justify-content:center">
                                                <div style="text-align:center;color:var(--text-muted);padding:0 24px">
                                                    <div style="font-size:2rem;margin-bottom:8px">"🗺"</div>
                                                    <p style="font-size:14px;font-weight:600;color:var(--text-secondary);margin:0 0 4px">
                                                        "Titik peta belum diatur"
                                                    </p>
                                                    <p style="font-size:12px;margin:0">{full_addr.clone()}</p>
                                                </div>
                                            </div>
                                        }
                                            .into_any()
                                    }}

                                    // ── Aksi navigasi ──────────────────────────
                                    <div style="display:flex;gap:10px;flex-wrap:wrap">
                                        <a
                                            href=maps_url
                                            target="_blank"
                                            rel="noopener"
                                            style="flex:1;min-width:150px;display:flex;align-items:center;justify-content:center;gap:8px;padding:13px;border-radius:12px;background:var(--accent-lime);color:var(--text-on-accent);font-weight:700;font-size:14px;text-decoration:none;border:none"
                                        >
                                            <svg
                                                width="16"
                                                height="16"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2.2"
                                                stroke-linecap="round"
                                                stroke-linejoin="round"
                                            >
                                                <polygon points="3 11 22 2 13 21 11 13 3 11" />
                                            </svg>
                                            "Google Maps"
                                        </a>
                                        <a
                                            href=waze_url
                                            target="_blank"
                                            rel="noopener"
                                            style="flex:1;min-width:150px;display:flex;align-items:center;justify-content:center;gap:8px;padding:13px;border-radius:12px;background:var(--bg-card);color:var(--text-primary);font-weight:700;font-size:14px;text-decoration:none;border:1px solid var(--border)"
                                        >
                                            <svg
                                                width="16"
                                                height="16"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2.2"
                                                stroke-linecap="round"
                                                stroke-linejoin="round"
                                            >
                                                <circle cx="12" cy="12" r="10" />
                                                <polygon points="16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76" />
                                            </svg>
                                            "Waze"
                                        </a>
                                    </div>
                                </div>
                            }
                                .into_any()
                        }
                        Some(Err(_)) => {
                            view! {
                                <div style="padding:48px 24px;text-align:center;color:var(--text-muted)">
                                    <div style="font-size:2.5rem;margin-bottom:10px">"📍"</div>
                                    <p style="font-size:15px;font-weight:600;color:var(--text-secondary);margin:0 0 4px">
                                        "Lokasi belum tersedia"
                                    </p>
                                    <p style="font-size:13px;margin:0">
                                        "Event tidak ditemukan atau lokasi belum diatur."
                                    </p>
                                </div>
                            }
                                .into_any()
                        }
                        None => view! { <LocationSkeleton /> }.into_any(),
                    }
                }}
            </Suspense>
        </div>
    }
}
