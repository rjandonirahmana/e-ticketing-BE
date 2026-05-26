use crate::csr::components::BottomNav;
use crate::csr::hooks::use_nav;
use crate::csr::hooks::{use_auth, ThemeToggle};
use crate::csr::state::premium::use_premium_store;
use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn ProfilePage() -> impl IntoView {
    let auth = use_auth();
    let navigate = use_nav();

    let premium = use_premium_store();
    Effect::new(move |_| {
        premium.load();
    });

    {
        let nav = navigate.clone();
        Effect::new(move |_| {
            if !auth.is_loading.get() && !auth.is_authenticated() {
                nav("/login", Default::default());
            }
        });
    }

    let nav_logout = navigate.clone();
    let on_logout = move |_| {
        auth.logout();
        nav_logout("/login", Default::default());
    };

    let display_name = move || {
        auth.user.with(|u| {
            u.as_ref()
                .map(|p| p.full_name.clone())
                .unwrap_or_else(|| "Alex Vanderbilt".into())
        })
    };
    let display_email = move || {
        auth.user.with(|u| {
            u.as_ref()
                .map(|p| p.email.clone())
                .unwrap_or_else(|| "alex.v@kinetic.music".into())
        })
    };
    let avatar_url = move || {
        auth.user
            .with(|u| u.as_ref().map(|p| p.avatar_url.clone()).unwrap_or_default())
    };
    let active_tickets = move || {
        auth.user
            .with(|u| u.as_ref().map(|p| p.active_tickets).unwrap_or(12))
    };
    let points = move || {
        auth.user
            .with(|u| u.as_ref().map(|p| p.points).unwrap_or(12450))
    };
    let points_display = move || {
        let p = points();
        if p >= 1000 {
            format!("{},{}", p / 1000, (p % 1000) / 10)
        } else {
            p.to_string()
        }
    };

    view! {
        <div class="page profile-page">
            <div class="profile-glow" aria-hidden="true"></div>

            // ── Mobile header ──────────────────────────────────────────────
            <header class="page-header">
                <A href="/pulse" attr:class="icon-btn" attr:aria-label="Messages">
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
                        {move || {
                            let url = avatar_url();
                            if url.is_empty() {
                                view! {
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
                                }
                                    .into_any()
                            } else {
                                view! { <img src=url alt="" /> }.into_any()
                            }
                        }}
                    </div>
                </div>
            </header>

            // ── Left column: avatar + identity + menu ────────────────
            <div class="avatar-section">
                <div class="avatar-ring">
                    <div class="avatar-circle">
                        {move || {
                            let url = avatar_url();
                            if url.is_empty() {
                                view! {
                                    <svg
                                        width="60"
                                        height="60"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="1.5"
                                        stroke-linecap="round"
                                    >
                                        <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
                                        <circle cx="12" cy="7" r="4" />
                                    </svg>
                                }
                                    .into_any()
                            } else {
                                view! { <img src=url class="avatar-img" /> }.into_any()
                            }
                        }}
                    </div>
                    <button class="avatar-edit" aria-label="Edit photo">
                        <svg
                            width="14"
                            height="14"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2.5"
                            stroke-linecap="round"
                        >
                            <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" />
                            <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
                        </svg>
                    </button>
                </div>
                <h1 class="profile-name">{display_name}</h1>
                <p class="profile-email">{display_email}</p>
                <span class="profile-tier-badge">"VIP MEMBER"</span>
            </div>

            <div class="stats-row stats-row--mobile-only">
                <div class="stat-card">
                    <span class="stat-label">"ACTIVE TICKETS"</span>
                    <span class="stat-value">{active_tickets}</span>
                </div>
                <div class="stat-card">
                    <span class="stat-label">"POINTS"</span>
                    <span class="stat-value stat-value--accent">{points_display}</span>
                </div>
            </div>

            <Show when=move || !premium.is_premium.get()>
                <A href="/subscription" attr:class="profile-premium-banner">
                    <span class="profile-premium-crown">"👑"</span>
                    <div class="profile-premium-text">
                        <span class="profile-premium-title">"Kinetic Premium"</span>
                        <span class="profile-premium-sub">
                            "Story tak terbatas · Prioritas tiket"
                        </span>
                    </div>
                    <span class="profile-premium-cta">"Upgrade"</span>
                </A>
            </Show>

            // Account Control menu
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
                    <A href="/payment-methods" attr:class="menu-item">
                        <div class="menu-item-icon">
                            <svg
                                width="18"
                                height="18"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="1.8"
                            >
                                <rect x="2" y="5" width="20" height="14" rx="2" />
                                <line x1="2" y1="10" x2="22" y2="10" />
                            </svg>
                        </div>
                        <span class="menu-item-label">"Payment Methods"</span>
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
                    <A href="/profile/security" attr:class="menu-item">
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
                                <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
                            </svg>
                        </div>
                        <span class="menu-item-label">"Security &amp; Privacy"</span>
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
                    <button class="menu-item menu-item--danger" on:click=on_logout>
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
                        <span class="menu-item-label">"Sign Out of Account"</span>
                    </button>
                </div>
            </div>

            // ── Right column: points + active experiences ──────────────
            // Kinetic Pulse Points card
            <div class="profile-points-card">
                <span class="profile-points-label">"KINETIC PULSE POINTS"</span>
                <div class="profile-points-value">
                    <span class="profile-points-num">{points_display}</span>
                    <span class="profile-points-unit">"PTS"</span>
                </div>
                <button class="profile-redeem-btn">"REDEEM REWARDS"</button>
            </div>

            // Active Experiences (tickets)
            <div class="profile-experiences">
                <div class="profile-exp-header">
                    <span class="profile-exp-title">"ACTIVE EXPERIENCES"</span>
                    <A href="/tickets" attr:class="profile-exp-viewall">
                        "VIEW ALL HISTORY"
                    </A>
                </div>
                <div class="profile-exp-list">
                    <div class="profile-exp-card">
                        <div class="profile-exp-img-wrap">
                            <img
                                src="https://images.unsplash.com/photo-1470225620780-dba8ba36b745?w=400&q=80"
                                alt="Neon Rhythm 2024"
                                class="profile-exp-img"
                            />
                        </div>
                        <div class="profile-exp-info">
                            <div class="profile-exp-status">
                                <span class="profile-exp-pill profile-exp-pill--upcoming">
                                    "UPCOMING"
                                </span>
                                <span class="profile-exp-when">"TOMORROW"</span>
                            </div>
                            <h3 class="profile-exp-name">"NEON RHYTHM 2024"</h3>
                            <p class="profile-exp-venue">"STARLIGHT ARENA • JAKARTA, ID"</p>
                            <div class="profile-exp-footer">
                                <A href="/tickets/tk1" attr:class="profile-exp-view-btn">
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
                                <button class="profile-exp-share-btn" aria-label="Share">
                                    <svg
                                        width="16"
                                        height="16"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                    >
                                        <circle cx="18" cy="5" r="3" />
                                        <circle cx="6" cy="12" r="3" />
                                        <circle cx="18" cy="19" r="3" />
                                        <line x1="8.59" y1="13.51" x2="15.42" y2="17.49" />
                                        <line x1="15.41" y1="6.51" x2="8.59" y2="10.49" />
                                    </svg>
                                </button>
                                <span class="profile-exp-price">"Rp1.250.000"</span>
                            </div>
                        </div>
                    </div>
                    <div class="profile-exp-card">
                        <div class="profile-exp-img-wrap">
                            <img
                                src="https://images.unsplash.com/photo-1514320291840-2e0a9bf2a9ae?w=400&q=80"
                                alt="Echoes of Midnight"
                                class="profile-exp-img"
                            />
                        </div>
                        <div class="profile-exp-info">
                            <div class="profile-exp-status">
                                <span class="profile-exp-date-badge">"12 MAY 2024"</span>
                            </div>
                            <h3 class="profile-exp-name">
                                "ECHOES OF "
                                <span class="profile-exp-name--accent">"MIDNIGHT"</span>
                            </h3>
                            <p class="profile-exp-venue">"UNDERGROUND LOUNGE • BANDUNG, ID"</p>
                            <div class="profile-exp-footer">
                                <A href="/tickets/tk2" attr:class="profile-exp-view-btn">
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
                                <button class="profile-exp-share-btn" aria-label="Share">
                                    <svg
                                        width="16"
                                        height="16"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                    >
                                        <circle cx="18" cy="5" r="3" />
                                        <circle cx="6" cy="12" r="3" />
                                        <circle cx="18" cy="19" r="3" />
                                        <line x1="8.59" y1="13.51" x2="15.42" y2="17.49" />
                                        <line x1="15.41" y1="6.51" x2="8.59" y2="10.49" />
                                    </svg>
                                </button>
                                <span class="profile-exp-price">"Rp850.000"</span>
                            </div>
                        </div>
                    </div>
                </div>
            </div>

            <BottomNav active="profile" />
        </div>
    }
}
