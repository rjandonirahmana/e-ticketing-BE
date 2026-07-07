//! profile.rs — Halaman Profil (unified SSR + hydration).
//!
//! Port dari `csr/pages/profile.rs` ke pendekatan SSR:
//!   - `use_auth()` hook  → `AuthResource` dari context (Suspense, blocking SSR)
//!   - `use_premium_store()` (spawn_local, client-only) → `get_premium_status` server fn
//!   - `auth.user.active_tickets` (mock field) → dihitung dari `get_my_tickets` server fn
//!   - `use_nav()` redirect → halaman dibungkus `AuthGuard` di router, plus
//!     fallback "harus login" jika AuthResource None.
//!   - Layout/identitas visual dipertahankan identik dengan CSR.
//!
//! Catatan model: `UserResponse` (web) lebih ramping dari profil CSR — tidak ada
//! `avatar_url`/`points`. `points` ditandai sebagai placeholder hingga backend
//! menyediakan field-nya; `active_tickets` & "Active Experiences" diisi dari
//! tiket nyata via `get_my_tickets`.

use leptos::either::Either;
use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::hooks::ThemeToggle;
use crate::web::api::{
    delete_my_story, get_my_story_group, get_my_tickets, get_premium_status, logout_action,
};
use crate::web::app::AuthResource;
use crate::web::components::story_viewer::StoryViewer;
use crate::web::components::BottomNav;
use crate::web::models::{format_date, format_price};
use crate::web::state::stories::{use_stories_store, StoryMediaType};

#[component]
pub fn ProfilePage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    // Premium status — hanya fetch bila sudah login. Banner upgrade muncul
    // ketika user BUKAN premium (sama seperti `Show when !is_premium` di CSR).
    let premium = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in {
                get_premium_status().await.unwrap_or(false)
            } else {
                false
            }
        },
    );

    // Tiket aktif → jumlah & daftar "Active Experiences" (data nyata,
    // menggantikan kartu mock di CSR).
    let tickets = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in { get_my_tickets().await.unwrap_or_default() } else { vec![] }
        },
    );

    // Store story global — dipakai untuk membuka <StoryViewer/> dari profil.
    let ctx = use_stories_store();

    // Story milik user sendiri sebagai satu grup (untuk thumbnail + viewer).
    let my_group = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in { get_my_story_group().await.ok().flatten() } else { None }
        },
    );

    // Sinkronkan grup "Story Saya" ke store agar bisa dibuka viewer (open_at).
    Effect::new(move |_| {
        if let Some(Some(group)) = my_group.get() {
            ctx.groups.set(vec![group]);
        }
    });

    // Refetch saat viewer ditutup (mis. sesudah hapus dari dalam viewer).
    Effect::new(move |prev: Option<bool>| {
        let open = ctx.active_group.get().is_some();
        if prev == Some(true) && !open {
            my_group.refetch();
        }
        open
    });

    // Aksi hapus story (dari thumbnail); setelah selesai → refetch daftar.
    let delete_story = Action::new(|id: &String| {
        let id = id.clone();
        async move { delete_my_story(id).await }
    });
    Effect::new(move |_| {
        // value() menjadi Some setiap kali aksi selesai → refresh daftar.
        if delete_story.value().get().is_some() {
            my_group.refetch();
        }
    });

    let on_logout = move |_: web_sys::MouseEvent| {
        leptos::task::spawn_local(async move {
            let _ = logout_action().await;
            #[cfg(feature = "hydrate")]
            {
                if let Some(win) = web_sys::window() {
                    let _ = win.location().replace("/login");
                }
            }
        });
    };

    view! {
        <div class="page profile-page">
            <div class="profile-glow" aria-hidden="true"></div>

            <header class="page-header">
                <A
                    href="/pulse"
                    attr:class="icon-btn"
                    attr:aria-label="Messages"
                >
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                        stroke="currentColor" stroke-width="2"
                        stroke-linecap="round" stroke-linejoin="round">
                        <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
                    </svg>
                </A>
                <span class="page-logo">"PULSE"</span>
                <div class="header-actions">
                    <ThemeToggle />
                    <A href="/notifications" attr:class="bell-btn">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                            stroke="currentColor" stroke-width="2"
                            stroke-linecap="round" stroke-linejoin="round">
                            <path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9" />
                            <path d="M13.73 21a2 2 0 01-3.46 0" />
                        </svg>
                        <span class="bell-dot"></span>
                    </A>
                    <div class="nav-avatar">
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                            stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
                            <circle cx="12" cy="7" r="4" />
                        </svg>
                    </div>
                </div>
            </header>

            <Suspense fallback=|| view! {
                <div class="avatar-section">
                    <div class="avatar-ring">
                        <div class="avatar-circle shim" style="border-radius:50%"></div>
                    </div>
                    <div class="shim" style="width:140px;height:20px;margin:12px auto 6px"></div>
                    <div class="shim" style="width:180px;height:14px;margin:0 auto 8px"></div>
                    <div class="shim" style="width:90px;height:22px;border-radius:99px;margin:0 auto"></div>
                </div>
                <div class="stats-row stats-row--mobile-only">
                    <div class="shim" style="flex:1;height:64px;border-radius:12px"></div>
                    <div class="shim" style="flex:1;height:64px;border-radius:12px"></div>
                </div>
                <div class="menu-section">
                    <div class="shim" style="width:120px;height:12px;margin-bottom:10px"></div>
                    <div class="menu-list">
                        {(0..5).map(|_| view! {
                            <div class="shim" style="height:52px;border-radius:12px;margin-bottom:6px"></div>
                        }).collect_view()}
                    </div>
                </div>
            }>
            {move || {
                auth.get()
                    .map(|res| {
                        match res.ok().flatten() {
                            // ── Belum login ───────────────────────────────────────
                            None => {
                                view! {
                                    <div style="display:flex;flex-direction:column;align-items:center;justify-content:center;padding:4rem 20px;text-align:center">
                                        <p style="color:var(--text-muted);margin-bottom:1.5rem;font-size:.9rem">
                                            "Kamu harus masuk untuk melihat profil."
                                        </p>
                                        <A href="/login" attr:class="tier-add-btn">
                                            "Masuk"
                                        </A>
                                    </div>
                                }
                                    .into_any()
                            }
                            // ── Sudah login ───────────────────────────────────────
                            Some(u) => {
                                let display_name = u.name.clone();
                                let display_email = u
                                    .email
                                    .clone()
                                    .unwrap_or_else(|| "-".into());
                                let avatar_initial = u
                                    .name
                                    .chars()
                                    .next()
                                    .unwrap_or('P')
                                    .to_uppercase()
                                    .to_string();
                                let is_merchant = u.role == "merchant";
                                let active_tickets = move || {
                                    tickets
                                        .get()
                                        .map(|list| {
                                            list.iter().filter(|t| t.status == "active").count()
                                        })
                                        .unwrap_or(0)
                                };
                                // Placeholder hingga backend menyediakan field `points`.
                                let points: i64 = 12450;
                                let points_display = if points >= 1000 {
                                    format!("{},{}", points / 1000, (points % 1000) / 10)
                                } else {
                                    points.to_string()
                                };
                                let pd1 = points_display.clone();
                                let pd2 = points_display.clone();

                                view! {
                                        // ── Avatar + identitas ─────────────────────
                                        <div class="avatar-section">
                                            <div class="avatar-ring">
                                                <div class="avatar-circle">
                                                    <span style="font-size:2rem;font-weight:800">
                                                        {avatar_initial}
                                                    </span>
                                                </div>
                                            </div>
                                            <h1 class="profile-name">{display_name}</h1>
                                            <p class="profile-email">{display_email}</p>
                                            <span class="profile-tier-badge">"VIP MEMBER"</span>
                                        </div>

                                        // ── Stat ringkas (mobile) ──────────────────
                                        <div class="stats-row stats-row--mobile-only">
                                            <div class="stat-card">
                                                <span class="stat-label">"ACTIVE TICKETS"</span>
                                                <span class="stat-value">{active_tickets}</span>
                                            </div>
                                            <div class="stat-card">
                                                <span class="stat-label">"POINTS"</span>
                                                <span class="stat-value stat-value--accent">
                                                    {pd1}
                                                </span>
                                            </div>
                                        </div>

                                        // ── Premium banner (saat bukan premium) ────
                                        <Suspense>
                                            {move || {
                                                let not_premium = !premium.get().unwrap_or(false);
                                                not_premium
                                                    .then(|| {
                                                        view! {
                                                            <A
                                                                href="/subscription"
                                                                attr:class="profile-premium-banner"
                                                            >
                                                                <span class="profile-premium-crown">
                                                                    "👑"
                                                                </span>
                                                                <div class="profile-premium-text">
                                                                    <span class="profile-premium-title">
                                                                        "Kinetic Premium"
                                                                    </span>
                                                                    <span class="profile-premium-sub">
                                                                        "Story tak terbatas · Prioritas tiket"
                                                                    </span>
                                                                </div>
                                                                <span class="profile-premium-cta">
                                                                    "Upgrade"
                                                                </span>
                                                            </A>
                                                        }
                                                    })
                                            }}
                                        </Suspense>

                                        // ── Story Saya (list + hapus) ──────────────
                                        <div class="menu-section">
                                            <span class="menu-section-label">"STORY SAYA"</span>
                                            <Suspense fallback=|| {
                                                view! {
                                                    <div class="my-stories-hint">"Memuat story…"</div>
                                                }
                                            }>
                                                {move || {
                                                    match my_group.get() {
                                                        Some(Some(group)) if !group.stories.is_empty() => {
                                                            let now = chrono::Utc::now();
                                                            Either::Right(
                                                                view! {
                                                                    <div class="my-stories-grid">
                                                                        {group
                                                                            .stories
                                                                            .iter()
                                                                            .enumerate()
                                                                            .map(|(i, s)| {
                                                                                let id = s.id.clone();
                                                                                let is_video = matches!(
                                                                                    s.media_type,
                                                                                    StoryMediaType::Video
                                                                                );
                                                                                let url = s.media_url.clone();
                                                                                let active = s.expires_at > now;
                                                                                view! {
                                                                                    <div
                                                                                        class="my-story-cell"
                                                                                        on:click=move |_| ctx.open_at(0, i)
                                                                                    >
                                                                                            {if is_video {
                                                                                                Either::Left(
                                                                                                    view! {
                                                                                                        <video
                                                                                                            class="my-story-thumb"
                                                                                                            src=url
                                                                                                            muted
                                                                                                            playsinline
                                                                                                            preload="metadata"
                                                                                                        ></video>
                                                                                                    },
                                                                                                )
                                                                                            } else {
                                                                                                Either::Right(
                                                                                                    view! {
                                                                                                        <img
                                                                                                            class="my-story-thumb"
                                                                                                            src=url
                                                                                                            alt=""
                                                                                                            decoding="async"
                                                                                                        />
                                                                                                    },
                                                                                                )
                                                                                            }}
                                                                                            <span class=if active {
                                                                                                "my-story-badge my-story-badge--active"
                                                                                            } else {
                                                                                                "my-story-badge"
                                                                                            }>
                                                                                                {if active { "Aktif" } else { "Arsip" }}
                                                                                            </span>
                                                                                            <button
                                                                                                class="my-story-del"
                                                                                                aria-label="Hapus story"
                                                                                                on:click=move |ev| {
                                                                                                    ev.stop_propagation();
                                                                                                    let ok = web_sys::window()
                                                                                                        .and_then(|w| {
                                                                                                            w.confirm_with_message("Hapus story ini?").ok()
                                                                                                        })
                                                                                                        .unwrap_or(false);
                                                                                                    if ok {
                                                                                                        delete_story.dispatch(id.clone());
                                                                                                    }
                                                                                                }
                                                                                            >
                                                                                                <svg
                                                                                                    width="16"
                                                                                                    height="16"
                                                                                                    viewBox="0 0 24 24"
                                                                                                    fill="none"
                                                                                                    stroke="currentColor"
                                                                                                    stroke-width="2"
                                                                                                    stroke-linecap="round"
                                                                                                >
                                                                                                    <polyline points="3 6 5 6 21 6" />
                                                                                                    <path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6m3 0V4a2 2 0 012-2h4a2 2 0 012 2v2" />
                                                                                                </svg>
                                                                                            </button>
                                                                                        </div>
                                                                                    }
                                                                                })
                                                                                .collect_view()}
                                                                        </div>
                                                                    },
                                                                )
                                                        }
                                                        Some(_) => Either::Left(
                                                            view! {
                                                                <div class="my-stories-hint">
                                                                    "Belum ada story. Story yang kamu buat akan muncul di sini."
                                                                </div>
                                                            },
                                                        ),
                                                        None => Either::Left(
                                                            view! {
                                                                <div class="my-stories-hint">"Memuat story…"</div>
                                                            },
                                                        ),
                                                    }
                                                }}
                                            </Suspense>
                                        </div>

                                        // ── Account control menu ───────────────────
                                        <div class="menu-section">
                                            <span class="menu-section-label">"ACCOUNT CONTROL"</span>
                                            <div class="menu-list">
                                                <A href="/profile/edit" attr:class="menu-item">
                                                    <div class="menu-item-icon">
                                                        <svg
                                                            width="18"
                                                            height="18"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="1.8"
                                                            stroke-linecap="round"
                                                        >
                                                            <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
                                                            <circle cx="12" cy="7" r="4" />
                                                        </svg>
                                                    </div>
                                                    <span class="menu-item-label">"Edit Profile"</span>
                                                    <svg
                                                        width="16"
                                                        height="16"
                                                        viewBox="0 0 24 24"
                                                        fill="none"
                                                        stroke="currentColor"
                                                        stroke-width="2"
                                                        stroke-linecap="round"
                                                    >
                                                        <polyline points="9 18 15 12 9 6" />
                                                    </svg>
                                                </A>
                                                <A href="/subscription" attr:class="menu-item menu-item--premium">
                                                    <div class="menu-item-icon menu-item-icon--premium">
                                                        <svg
                                                            width="18"
                                                            height="18"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="1.8"
                                                            stroke-linecap="round"
                                                        >
                                                            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
                                                        </svg>
                                                    </div>
                                                    <span class="menu-item-label">"PULSE Premium"</span>
                                                    <svg
                                                        width="16"
                                                        height="16"
                                                        viewBox="0 0 24 24"
                                                        fill="none"
                                                        stroke="currentColor"
                                                        stroke-width="2"
                                                        stroke-linecap="round"
                                                    >
                                                        <polyline points="9 18 15 12 9 6" />
                                                    </svg>
                                                </A>
                                                <Show when=move || is_merchant>
                                                    <A href="/merchant" attr:class="menu-item">
                                                        <div class="menu-item-icon">
                                                            <svg
                                                                width="18"
                                                                height="18"
                                                                viewBox="0 0 24 24"
                                                                fill="none"
                                                                stroke="currentColor"
                                                                stroke-width="1.8"
                                                                stroke-linecap="round"
                                                            >
                                                                <path d="M3 9l1-5h16l1 5M4 9v11a1 1 0 001 1h14a1 1 0 001-1V9M3 9h18" />
                                                            </svg>
                                                        </div>
                                                        <span class="menu-item-label">
                                                            "Merchant Hub"
                                                        </span>
                                                        <svg
                                                            width="16"
                                                            height="16"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="2"
                                                            stroke-linecap="round"
                                                        >
                                                            <polyline points="9 18 15 12 9 6" />
                                                        </svg>
                                                    </A>
                                                </Show>
                                                <A href="/notifications" attr:class="menu-item">
                                                    <div class="menu-item-icon">
                                                        <svg
                                                            width="18"
                                                            height="18"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="1.8"
                                                            stroke-linecap="round"
                                                            stroke-linejoin="round"
                                                        >
                                                            <path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9" />
                                                            <path d="M13.73 21a2 2 0 01-3.46 0" />
                                                        </svg>
                                                    </div>
                                                    <span class="menu-item-label">"Preferences"</span>
                                                    <svg
                                                        width="16"
                                                        height="16"
                                                        viewBox="0 0 24 24"
                                                        fill="none"
                                                        stroke="currentColor"
                                                        stroke-width="2"
                                                        stroke-linecap="round"
                                                    >
                                                        <polyline points="9 18 15 12 9 6" />
                                                    </svg>
                                                </A>
                                                <button
                                                    class="menu-item menu-item--danger"
                                                    on:click=on_logout
                                                >
                                                    <div class="menu-item-icon menu-item-icon--danger">
                                                        <svg
                                                            width="18"
                                                            height="18"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="1.8"
                                                            stroke-linecap="round"
                                                            stroke-linejoin="round"
                                                        >
                                                            <path d="M9 21H5a2 2 0 01-2-2V5a2 2 0 012-2h4" />
                                                            <polyline points="16 17 21 12 16 7" />
                                                            <line x1="21" y1="12" x2="9" y2="12" />
                                                        </svg>
                                                    </div>
                                                    <span class="menu-item-label">
                                                        "Sign Out of Account"
                                                    </span>
                                                </button>
                                            </div>
                                        </div>

                                        // ── Pulse points card ──────────────────────
                                        <div class="profile-points-card">
                                            <span class="profile-points-label">
                                                "KINETIC PULSE POINTS"
                                            </span>
                                            <div class="profile-points-value">
                                                <span class="profile-points-num">{pd2}</span>
                                                <span class="profile-points-unit">"PTS"</span>
                                            </div>
                                            <button class="profile-redeem-btn">
                                                "REDEEM REWARDS"
                                            </button>
                                        </div>

                                        // ── Active experiences (tiket nyata) ───────
                                        <div class="profile-experiences">
                                            <div class="profile-exp-header">
                                                <span class="profile-exp-title">
                                                    "ACTIVE EXPERIENCES"
                                                </span>
                                                <A
                                                    href="/tickets"
                                                    attr:class="profile-exp-viewall"
                                                >
                                                    "VIEW ALL HISTORY"
                                                </A>
                                            </div>
                                            <div class="profile-exp-list">
                                                <Suspense fallback=|| {
                                                    view! { <div class="profile-exp-skeleton" /> }
                                                }>
                                                    {move || {
                                                        let list = tickets.get().unwrap_or_default();
                                                        let upcoming: Vec<_> = list
                                                            .into_iter()
                                                            .filter(|t| t.status == "active")
                                                            .take(2)
                                                            .collect();
                                                        if upcoming.is_empty() {
                                                            view! {
                                                                <p style="color:var(--clr-muted);font-size:.85rem;padding:.5rem 0">
                                                                    "Belum ada tiket aktif. "
                                                                    <A
                                                                        href="/explore"
                                                                        attr:style="color:var(--clr-accent)"
                                                                    >
                                                                        "Jelajahi event →"
                                                                    </A>
                                                                </p>
                                                            }
                                                                .into_any()
                                                        } else {
                                                            upcoming
                                                                .into_iter()
                                                                .map(|t| {
                                                                    let id = t.id.clone();
                                                                    let name = t.event_name.to_uppercase();
                                                                    let venue = format!(
                                                                        "{} • {}",
                                                                        t.event_venue.clone().unwrap_or_default().to_uppercase(),
                                                                        t.event_city.clone().unwrap_or_default().to_uppercase(),
                                                                    );
                                                                    let date = format_date(&t.event_date);
                                                                    let price = format_price(t.unit_price);
                                                                    let cover = t.cover_url.clone().unwrap_or_default();
                                                                    view! {
                                                                        <div class="profile-exp-card">
                                                                            <div class="profile-exp-img-wrap">
                                                                                {if cover.is_empty() {
                                                                                    view! {
                                                                                        <div
                                                                                            class="profile-exp-img"
                                                                                            style="display:flex;align-items:center;justify-content:center;font-size:1.75rem;background:var(--clr-border)"
                                                                                        >
                                                                                            "🎪"
                                                                                        </div>
                                                                                    }
                                                                                        .into_any()
                                                                                } else {
                                                                                    view! {
                                                                                        <img
                                                                                            src=cover
                                                                                            alt=name.clone()
                                                                                            class="profile-exp-img"
                                                                                            loading="lazy"
                                                                                        />
                                                                                    }
                                                                                        .into_any()
                                                                                }}
                                                                            </div>
                                                                            <div class="profile-exp-info">
                                                                                <div class="profile-exp-status">
                                                                                    <span class="profile-exp-pill profile-exp-pill--upcoming">
                                                                                        "UPCOMING"
                                                                                    </span>
                                                                                    <span class="profile-exp-when">{date}</span>
                                                                                </div>
                                                                                <h3 class="profile-exp-name">{name}</h3>
                                                                                <p class="profile-exp-venue">{venue}</p>
                                                                                <div class="profile-exp-footer">
                                                                                    <A
                                                                                        href=format!("/tickets/{id}")
                                                                                        attr:class="profile-exp-view-btn"
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
                                                                                            <path d="M2 9a3 3 0 010-6h20a3 3 0 010 6H2zM2 15a3 3 0 000 6h20a3 3 0 000-6H2z" />
                                                                                        </svg>
                                                                                        "VIEW TICKET"
                                                                                    </A>
                                                                                    <span class="profile-exp-price">{price}</span>
                                                                                </div>
                                                                            </div>
                                                                        </div>
                                                                    }
                                                                })
                                                                .collect_view()
                                                                .into_any()
                                                        }
                                                    }}
                                                </Suspense>
                                            </div>
                                        </div>

                                }
                                    .into_any()
                            }
                        }
                    })
            }}
        </Suspense>
        <BottomNav active="profile" />
        // Viewer story — dibuka saat thumbnail "Story Saya" di-klik (open_at).
        <StoryViewer />
    </div>
    }
}
