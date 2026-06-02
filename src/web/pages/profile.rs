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

use leptos::prelude::*;
use leptos_router::components::A;

use crate::csr::hooks::ThemeToggle;
use crate::web::api::{get_my_tickets, get_premium_status, logout_action};
use crate::web::app::AuthResource;
use crate::web::components::BottomNav;
use crate::web::models::{format_date, format_price};

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
                get_premium_status()
                    .await
                    .ok()
                    .and_then(|v| v.get("is_premium").and_then(|b| b.as_bool()))
                    .unwrap_or(false)
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
        <Suspense fallback=|| {
            view! {
                <div class="loading">
                    <div class="loading__spinner" />
                    <span>"Memuat profil..."</span>
                </div>
            }
        }>
            {move || {
                auth.get()
                    .map(|res| {
                        match res.ok().flatten() {
                            // ── Belum login ───────────────────────────────────────
                            None => {
                                view! {
                                    <div
                                        class="container"
                                        style="padding:4rem 0;text-align:center"
                                    >
                                        <p style="color:var(--clr-muted);margin-bottom:1.5rem">
                                            "Kamu harus masuk untuk melihat profil."
                                        </p>
                                        <A href="/login" attr:class="btn btn--accent">
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
                                    <div class="page profile-page">
                                        <div class="profile-glow" aria-hidden="true"></div>

                                        // ── Mobile header ──────────────────────────
                                        <header class="page-header">
                                            <A
                                                href="/messages"
                                                attr:class="icon-btn"
                                                attr:aria-label="Messages"
                                            >
                                                <svg
                                                    width="20"
                                                    height="20"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2"
                                                    stroke-linecap="round"
                                                    stroke-linejoin="round"
                                                >
                                                    <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
                                                </svg>
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
                                                <div class="nav-avatar">
                                                    <svg
                                                        width="16"
                                                        height="16"
                                                        viewBox="0 0 24 24"
                                                        fill="none"
                                                        stroke="currentColor"
                                                        stroke-width="2"
                                                        stroke-linecap="round"
                                                    >
                                                        <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
                                                        <circle cx="12" cy="7" r="4" />
                                                    </svg>
                                                </div>
                                            </div>
                                        </header>

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

                                        <BottomNav active="profile" />
                                    </div>
                                }
                                    .into_any()
                            }
                        }
                    })
            }}
        </Suspense>
    }
}
