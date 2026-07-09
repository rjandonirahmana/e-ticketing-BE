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
use crate::web::components::{EventGrid, EventGridShimmer};
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

/// Skeleton profil merchant (hero + avatar + nama + statistik) selama loading.
#[component]
fn MerchantProfileShimmer() -> impl IntoView {
    view! {
        <div class="mp-hero shimmer-bg"></div>
        <div class="mp-head">
            <div class="mp-avatar-wrap">
                <div class="mp-avatar shimmer-bg"></div>
            </div>
            <div class="mp-head-actions">
                <div
                    class="shimmer-bg"
                    style="width:112px;height:42px;border-radius:999px;"
                ></div>
                <div class="shimmer-bg" style="width:42px;height:42px;border-radius:50%;"></div>
            </div>
        </div>
        <div class="mp-container">
            <div
                class="shimmer-bg"
                style="width:62%;height:26px;border-radius:8px;margin-top:14px;"
            ></div>
            <div
                class="shimmer-bg"
                style="width:38%;height:14px;border-radius:6px;margin-top:10px;"
            ></div>
            <div class="mp-stats">
                {(0..3)
                    .map(|_| {
                        view! {
                            <div class="mp-stat">
                                <span
                                    class="shimmer-bg"
                                    style="width:44px;height:18px;border-radius:6px;"
                                ></span>
                                <span
                                    class="shimmer-bg"
                                    style="width:52px;height:9px;border-radius:4px;margin-top:6px;"
                                ></span>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
            <EventGridShimmer count=4 />
        </div>
    }
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

    // Feedback "tautan disalin" untuk tombol share.
    let share_ok = RwSignal::new(false);

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
                view! { <MerchantProfileShimmer /> }
            }>
                {move || {
                    match profile.get() {
                        None => view! { <MerchantProfileShimmer /> }.into_any(),
                        Some(Err(e)) if e.to_string().contains("not_ready") => {
                            view! { <MerchantProfileShimmer /> }.into_any()
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
                            let store_name = p.store_name.clone();
                            let logo = p.logo_url.clone().unwrap_or_default();
                            let verified = p.verified;
                            let initial = p
                                .store_name
                                .chars()
                                .next()
                                .unwrap_or('P')
                                .to_uppercase()
                                .to_string();
                            let desc = p.description.clone().unwrap_or_default();
                            let reviews_href = format!("/m/{}/reviews", merchant_id);
                            // Dipakai hanya di jalur wasm (clipboard); no-op di native.
                            #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
                            let share_url = format!("/m/{}", merchant_id);
                            let on_share = move |_| {
                                #[cfg(target_arch = "wasm32")]
                                if let Some(w) = web_sys::window() {
                                    let origin = w.location().origin().unwrap_or_default();
                                    let full = format!("{origin}{share_url}");
                                    let _ = w.navigator().clipboard().write_text(&full);
                                }
                                share_ok.set(true);
                                #[cfg(target_arch = "wasm32")]
                                gloo_timers::callback::Timeout::new(
                                        1600,
                                        move || share_ok.set(false),
                                    )
                                    .forget();
                            };
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
                                    <div class="mp-avatar-wrap">
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
                                        }}
                                        {verified
                                            .then(|| {
                                                view! {
                                                    <span class="mp-avatar-badge" title="Terverifikasi">
                                                        <svg
                                                            width="14"
                                                            height="14"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="3"
                                                            stroke-linecap="round"
                                                            stroke-linejoin="round"
                                                        >
                                                            <polyline points="20 6 9 17 4 12" />
                                                        </svg>
                                                    </span>
                                                }
                                            })}
                                    </div>
                                    <div class="mp-head-actions">
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
                                        <button
                                            class="mp-icon-btn"
                                            on:click=on_share
                                            aria-label="Bagikan"
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
                                                <circle cx="18" cy="5" r="3" />
                                                <circle cx="6" cy="12" r="3" />
                                                <circle cx="18" cy="19" r="3" />
                                                <line x1="8.6" y1="13.5" x2="15.4" y2="17.5" />
                                                <line x1="15.4" y1="6.5" x2="8.6" y2="10.5" />
                                            </svg>
                                        </button>
                                        {move || {
                                            share_ok
                                                .get()
                                                .then(|| {
                                                    view! { <span class="mp-share-toast">"Tautan disalin"</span> }
                                                })
                                        }}
                                    </div>
                                </div>

                                <div class="mp-container">
                                    <div class="mp-name-row">
                                        <h1 class="mp-name">{store_name.clone()}</h1>
                                    </div>

                                    // ── Lokasi (kota event terbaru) ───────────
                                    {move || {
                                        events
                                            .get()
                                            .and_then(|r| r.ok())
                                            .and_then(|pe| {
                                                pe.data.first().and_then(|e| e.city.clone())
                                            })
                                            .filter(|c| !c.is_empty())
                                            .map(|city| {
                                                view! {
                                                    <p class="mp-loc">
                                                        <svg
                                                            width="14"
                                                            height="14"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="2"
                                                            stroke-linecap="round"
                                                            stroke-linejoin="round"
                                                        >
                                                            <path d="M21 10c0 7-9 12-9 12s-9-5-9-12a9 9 0 0 1 18 0z" />
                                                            <circle cx="12" cy="10" r="3" />
                                                        </svg>
                                                        {city}
                                                    </p>
                                                }
                                            })
                                    }}

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
                                                    view! { <EventGridShimmer count=4 /> }
                                                }>
                                                    {move || {
                                                        events
                                                            .get()
                                                            .map(|r| match r {
                                                                Ok(pe) => {
                                                                    view! {
                                                                        <EventGrid
                                                                            events=pe.data
                                                                            empty="Belum ada event aktif."
                                                                        />
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
        </div>
    }
}
