use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::hooks::{use_auth, AuthCtx};
pub use crate::web::hooks::ThemeToggle;

fn is_merchant(auth: AuthCtx) -> bool {
    auth.user.with(|u| {
        u.as_ref()
            .map(|p| p.membership_tier == "MERCHANT")
            .unwrap_or(false)
    })
}

fn is_admin(auth: AuthCtx) -> bool {
    auth.user
        .with(|u| u.as_ref().map(|p| p.role == "admin").unwrap_or(false))
}

#[allow(dead_code)]
#[component]
pub fn TopNav(#[prop(optional)] back_href: Option<&'static str>) -> impl IntoView {
    view! {
        <header class="page-header">
            {match back_href {
                Some(href) => {
                    view! {
                        <A href=href attr:class="back-btn">
                            <svg
                                width="22"
                                height="22"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2.5"
                                stroke-linecap="round"
                            >
                                <polyline points="15 18 9 12 15 6" />
                            </svg>
                        </A>
                    }
                        .into_any()
                }
                None => {
                    view! {
                        <button class="icon-btn" aria-label="Menu">
                            <svg
                                width="20"
                                height="20"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                                stroke-linecap="round"
                            >
                                <line x1="3" y1="6" x2="21" y2="6" />
                                <line x1="3" y1="12" x2="21" y2="12" />
                                <line x1="3" y1="18" x2="21" y2="18" />
                            </svg>
                        </button>
                    }
                        .into_any()
                }
            }} <span class="page-logo">"PULSE"</span> <div class="header-actions">
                <ThemeToggle />
                <A href="/profile" attr:class="nav-avatar">
                    <svg
                        width="18"
                        height="18"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    >
                        <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
                        <circle cx="12" cy="7" r="4" />
                    </svg>
                </A>
            </div>
        </header>
    }
}

/// Bottom navigation bar.
/// Tabs: EXPLORE | TICKETS | ORDERS | PROFILE | (MERCHANT if merchant) | (ADMIN if admin)
#[component]
pub fn BottomNav(#[prop(default = "")] active: &'static str) -> impl IntoView {
    let auth_ctx = use_auth();
    let cls = move |key: &str| {
        if key == active {
            "bottom-item bottom-item--active"
        } else {
            "bottom-item"
        }
    };
    view! {
        <nav class="bottom-nav">

            // 1. EXPLORE
            <A href="/explore" attr:class=cls("explore")>
                <svg
                    width="22"
                    height="22"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <circle cx="11" cy="11" r="8" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                </svg>
                <span class="bottom-label">"EXPLORE"</span>
            </A>

            // 1b. LIVES — daftar merchant yang sedang siaran langsung
            <A href="/lives" attr:class=cls("lives")>
                <svg
                    width="22"
                    height="22"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <polygon points="23 7 16 12 23 17 23 7" />
                    <rect x="1" y="5" width="15" height="14" rx="2" ry="2" />
                </svg>
                <span class="bottom-label">"LIVE"</span>
            </A>


            // 0. PULSE CHAT
            <A href="/pulse" attr:class=cls("pulse")>
                <svg
                    width="22"
                    height="22"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
                </svg>
                <span class="bottom-label">"CHAT"</span>
            </A>

            // 2. TICKETS
            <A href="/tickets" attr:class=cls("tickets")>
                <svg
                    width="22"
                    height="22"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <path d="M2 9a3 3 0 010-6h20a3 3 0 010 6H2zM2 15a3 3 0 000 6h20a3 3 0 000-6H2z" />
                </svg>
                <span class="bottom-label">"TICKETS"</span>
            </A>

            // 3. ORDERS (new — between TICKETS and PROFILE)
            <A href="/orders" attr:class=cls("orders")>
                <svg
                    width="22"
                    height="22"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <path d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2" />
                    <rect x="9" y="3" width="6" height="4" rx="1" />
                    <path d="M9 12h6M9 16h4" />
                </svg>
                <span class="bottom-label">"ORDERS"</span>
            </A>

            // 4. PROFILE
            <A href="/profile" attr:class=cls("profile")>
                <svg
                    width="22"
                    height="22"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
                    <circle cx="12" cy="7" r="4" />
                </svg>
                <span class="bottom-label">"PROFILE"</span>
            </A>

            // 5. MERCHANT — only if user role is merchant
            {move || {
                is_merchant(auth_ctx)
                    .then(|| {
                        view! {
                            <A href="/merchant" attr:class=cls("merchant")>
                                <svg
                                    width="22"
                                    height="22"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="1.8"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                >
                                    <path d="M3 7l1.5-3h15L21 7" />
                                    <path d="M3 7v13a1 1 0 001 1h16a1 1 0 001-1V7" />
                                    <path d="M9 11h6" />
                                </svg>
                                <span class="bottom-label">"MERCHANT"</span>
                            </A>
                        }
                    })
            }}

            // ADMIN — only if user role is admin
            {move || {
                is_admin(auth_ctx)
                    .then(|| {
                        view! {
                            <A href="/admin" attr:class=cls("admin")>
                                <svg
                                    width="22"
                                    height="22"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="1.8"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                >
                                    <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
                                </svg>
                                <span class="bottom-label">"ADMIN"</span>
                            </A>
                        }
                    })
            }}
        </nav>
    }
}
