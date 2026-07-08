//! merchant_public.rs — Profil merchant publik (/m/:id), sisi user.
//!
//! Hero cover (dari event terbaru), avatar/logo, tombol Follow, statistik
//! (followers / events / rating → klik rating ke halaman reviews), tab
//! EVENTS | TENTANG. Entry point: tombol penyelenggara di event detail &
//! chip penyelenggara di kartu explore.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::{
    get_merchant_public_events, get_merchant_public_profile, set_follow_merchant,
};
use crate::web::app::AuthResource;
use crate::web::components::{BottomNav, EventCard, EventCardShimmer};
use crate::web::hooks::ThemeToggle;

/// 12500 → "12.5k", 999 → "999".
pub(crate) fn fmt_count(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}jt", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn fmt_price(p: f64) -> String {
    let n = p as i64;
    if n <= 0 {
        return "Gratis".into();
    }
    // Pemisah ribuan sederhana: 1250000 → "Rp1.250.000".
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    format!("Rp{out}")
}

#[component]
pub fn MerchantPublicPage() -> impl IntoView {
    let params = use_params_map();
    let mid = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");

    let profile = Resource::new(mid, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("not_ready".into()));
        }
        get_merchant_public_profile(id).await
    });
    let events = Resource::new(mid, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("not_ready".into()));
        }
        get_merchant_public_events(id, Some(1)).await
    });

    // State follow lokal (optimistic): diisi dari profile saat termuat.
    let following = RwSignal::new(false);
    let followers = RwSignal::new(0i64);
    let follow_init = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(Ok(p)) = profile.get() {
            if !follow_init.get_untracked() {
                following.set(p.is_following);
                followers.set(p.followers);
                follow_init.set(true);
            }
        }
    });

    let follow_busy = RwSignal::new(false);
    let on_follow = move |_| {
        // Belum login → arahkan ke login (follow butuh identitas).
        let logged_in = auth
            .get_untracked()
            .and_then(|r| r.ok())
            .flatten()
            .is_some();
        if !logged_in {
            #[cfg(target_arch = "wasm32")]
            if let Some(w) = web_sys::window() {
                let _ = w.location().assign("/login");
            }
            return;
        }
        if follow_busy.get_untracked() {
            return;
        }
        let id = mid();
        let target = !following.get_untracked();
        // Optimistic update; rollback bila server gagal.
        following.set(target);
        followers.update(|f| *f += if target { 1 } else { -1 });
        follow_busy.set(true);
        leptos::task::spawn_local(async move {
            if set_follow_merchant(id, target).await.is_err() {
                following.set(!target);
                followers.update(|f| *f -= if target { 1 } else { -1 });
            }
            follow_busy.set(false);
        });
    };

    // Tab aktif: 0 = EVENTS, 1 = TENTANG.
    let tab = RwSignal::new(0usize);

    view! {
        <div class="mp-page">
            <header class="page-header mp-header">
                <A href="/explore" attr:class="back-btn" attr:aria-label="Kembali">
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
                <span class="page-logo">"PULSE"</span>
                <div class="header-actions">
                    <ThemeToggle />
                </div>
            </header>

            <Suspense fallback=|| {
                view! { <div class="mp-hero shimmer-bg"></div> }
            }>
                {move || {
                    match profile.get() {
                        None => view! { <div class="mp-hero shimmer-bg"></div> }.into_any(),
                        Some(Err(e)) if e.to_string().contains("not_ready") => {
                            view! { <div class="mp-hero shimmer-bg"></div> }.into_any()
                        }
                        Some(Err(_)) => {
                            view! {
                                <div class="mp-container">
                                    <div class="medit-error-banner">
                                        "Merchant tidak ditemukan."
                                    </div>
                                    <A href="/explore" attr:class="medit-cancel-btn">
                                        "← Kembali"
                                    </A>
                                </div>
                            }
                                .into_any()
                        }
                        Some(Ok(p)) => {
                            let merchant_id = p.merchant_id.clone();
                            let logo = p.logo_url.clone().unwrap_or_default();
                            let initial = p
                                .store_name
                                .chars()
                                .next()
                                .unwrap_or('P')
                                .to_uppercase()
                                .to_string();
                            let desc = p.description.clone().unwrap_or_default();
                            let reviews_href = format!("/m/{}/reviews", merchant_id);
                            view! {
                                // ── Hero: cover event terbaru sebagai latar ──
                                <div class="mp-hero">
                                    {move || {
                                        events
                                            .get()
                                            .and_then(|r| r.ok())
                                            .and_then(|pe| {
                                                pe.data.first().and_then(|e| e.cover_url.clone())
                                            })
                                            .filter(|c| !c.is_empty())
                                            .map(|cover| {
                                                view! { <img src=cover alt="" loading="lazy" /> }
                                            })
                                    }} <div class="mp-hero-grad"></div>
                                </div>

                                // ── Kepala profil ─────────────────────────────
                                <div class="mp-head">
                                    {if logo.is_empty() {
                                        view! {
                                            <div class="mp-avatar mp-avatar-fallback">{initial}</div>
                                        }
                                            .into_any()
                                    } else {
                                        view! {
                                            <img class="mp-avatar" src=logo alt="Logo merchant" />
                                        }
                                            .into_any()
                                    }} <div class="mp-head-actions">
                                        <button
                                            class="mp-follow-btn"
                                            data-on=move || following.get().to_string()
                                            disabled=move || follow_busy.get()
                                            on:click=on_follow
                                        >
                                            {move || {
                                                if following.get() { "Mengikuti" } else { "Follow" }
                                            }}
                                        </button>
                                    </div>
                                </div>

                                <div class="mp-container">
                                    <div class="mp-name-row">
                                        <h1 class="mp-name">{p.store_name.clone()}</h1>
                                        {p
                                            .verified
                                            .then(|| {
                                                view! {
                                                    <span class="mp-verified" title="Terverifikasi">
                                                        <svg
                                                            width="16"
                                                            height="16"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="2.5"
                                                            stroke-linecap="round"
                                                        >
                                                            <path d="M9 12l2 2 4-4" />
                                                            <circle cx="12" cy="12" r="9" />
                                                        </svg>
                                                    </span>
                                                }
                                            })}
                                    </div>

                                    // ── Statistik ─────────────────────────────
                                    <div class="mp-stats">
                                        <div class="mp-stat">
                                            <span class="mp-stat-num">
                                                {move || fmt_count(followers.get())}
                                            </span>
                                            <span class="mp-stat-label">"FOLLOWERS"</span>
                                        </div>
                                        <div class="mp-stat">
                                            <span class="mp-stat-num">{fmt_count(p.events_count)}</span>
                                            <span class="mp-stat-label">"EVENTS"</span>
                                        </div>
                                        <a class="mp-stat mp-stat-link" href=reviews_href.clone()>
                                            <span class="mp-stat-num">
                                                {format!("{:.1}", p.rating_avg)}
                                                <span class="mp-stat-star">"★"</span>
                                            </span>
                                            <span class="mp-stat-label">"RATING"</span>
                                        </a>
                                    </div>

                                    // ── Tabs ──────────────────────────────────
                                    <div class="mp-tabs">
                                        <button
                                            class=move || {
                                                if tab.get() == 0 { "mp-tab mp-tab--on" } else { "mp-tab" }
                                            }
                                            on:click=move |_| tab.set(0)
                                        >
                                            "EVENTS"
                                        </button>
                                        <button
                                            class=move || {
                                                if tab.get() == 1 { "mp-tab mp-tab--on" } else { "mp-tab" }
                                            }
                                            on:click=move |_| tab.set(1)
                                        >
                                            "TENTANG"
                                        </button>
                                        <a class="mp-tab" href=reviews_href>
                                            "ULASAN"
                                        </a>
                                    </div>

                                    {move || {
                                        if tab.get() == 0 {
                                            view! {
                                                <Suspense fallback=|| {
                                                    view! {
                                                        <div class="mp-grid">
                                                            <EventCardShimmer />
                                                            <EventCardShimmer />
                                                        </div>
                                                    }
                                                }>
                                                    {move || {
                                                        events
                                                            .get()
                                                            .map(|r| match r {
                                                                Ok(pe) if pe.data.is_empty() => {
                                                                    view! { <p class="mp-empty">"Belum ada event aktif."</p> }
                                                                        .into_any()
                                                                }
                                                                Ok(pe) => {
                                                                    view! {
                                                                        <div class="mp-grid">
                                                                            {pe
                                                                                .data
                                                                                .iter()
                                                                                .map(|e| {
                                                                                    let badge = e.category.first().cloned().unwrap_or_default();
                                                                                    view! {
                                                                                        <EventCard
                                                                                            href=format!("/events/{}", e.slug)
                                                                                            img=e.cover_url.clone().unwrap_or_default()
                                                                                            alt=e.name.clone()
                                                                                            badge=badge
                                                                                            title=e.name.clone()
                                                                                            venue=e.venue.clone().unwrap_or_default()
                                                                                            price=format!("Mulai {}", fmt_price(e.display_price))
                                                                                        />
                                                                                    }
                                                                                })
                                                                                .collect_view()}
                                                                        </div>
                                                                    }
                                                                        .into_any()
                                                                }
                                                                Err(_) => {
                                                                    view! { <p class="mp-empty">"Gagal memuat event."</p> }
                                                                        .into_any()
                                                                }
                                                            })
                                                    }}
                                                </Suspense>
                                            }
                                                .into_any()
                                        } else {
                                            let d = desc.clone();
                                            view! {
                                                <div class="mp-about">
                                                    {if d.is_empty() {
                                                        view! {
                                                            <p class="mp-empty">"Merchant belum menulis deskripsi."</p>
                                                        }
                                                            .into_any()
                                                    } else {
                                                        view! { <p class="mp-about-text">{d}</p> }.into_any()
                                                    }}
                                                </div>
                                            }
                                                .into_any()
                                        }
                                    }}
                                </div>
                            }
                                .into_any()
                        }
                    }
                }}
            </Suspense>

            <BottomNav />
        </div>
    }
}
