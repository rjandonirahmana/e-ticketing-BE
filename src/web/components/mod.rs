use leptos::prelude::*;
use leptos_router::components::A;

use crate::csr::hooks::use_auth;
// Re-export agar dapat diakses sebagai `crate::web::components::ThemeToggle`
pub use crate::csr::hooks::ThemeToggle;


fn is_merchant() -> bool {
    use_auth().user.with(|u| {
        u.as_ref()
            .map(|p| p.membership_tier == "MERCHANT")
            .unwrap_or(false)
    })
}

fn is_admin() -> bool {
    use_auth()
        .user
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
                is_merchant()
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
                is_admin()
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

#[component]
pub fn GridBackground() -> impl IntoView {
    view! {
        <div class="grid-bg" aria-hidden="true">
            <div class="grid-lines"></div>
            <div class="orb orb-1"></div>
            <div class="orb orb-2"></div>
        </div>
    }
}

#[component]
pub fn ErrorBanner(message: String) -> impl IntoView {
    view! {
        <div class="error-banner" role="alert">
            <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="#ff4f6b"
                stroke-width="2"
            >
                <circle cx="12" cy="12" r="10" />
                <line x1="12" y1="8" x2="12" y2="12" />
                <line x1="12" y1="16" x2="12.01" y2="16" />
            </svg>
            <span>{message}</span>
        </div>
    }
}

#[component]
pub fn KineticInput(
    id: &'static str,
    label: &'static str,
    #[prop(default = "text")] input_type: &'static str,
    value: RwSignal<String>,
    #[prop(optional)] placeholder: &'static str,
    #[prop(optional)] autocomplete: &'static str,
    #[prop(default = false)] disabled: bool,
) -> impl IntoView {
    let focused = RwSignal::new(false);
    let show_pass = RwSignal::new(false);
    let is_password = input_type == "password";

    let computed_type = move || {
        if is_password && show_pass.get() {
            "text".to_string()
        } else {
            input_type.to_string()
        }
    };

    let box_class = move || {
        if focused.get() {
            "field-box field-box--focused"
        } else {
            "field-box"
        }
    };

    view! {
        <div class="field-wrap">
            <label for=id class="field-label">
                {label}
            </label>
            <div class=box_class>
                <input
                    id=id
                    type=computed_type
                    placeholder=placeholder
                    autocomplete=autocomplete
                    disabled=disabled
                    class="field-input"
                    prop:value=move || value.get()
                    on:input=move |ev| value.set(event_target_value(&ev))
                    on:focus=move |_| focused.set(true)
                    on:blur=move |_| focused.set(false)
                />
                {if is_password {
                    view! {
                        <button
                            type="button"
                            class="pass-toggle"
                            tabindex="-1"
                            on:click=move |_| show_pass.update(|s| *s = !*s)
                        >
                            {move || {
                                if show_pass.get() {
                                    view! {
                                        <svg
                                            width="18"
                                            height="18"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2"
                                        >
                                            <path d="M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94M9.9 4.24A9.12 9.12 0 0112 4c7 0 11 8 11 8a18.5 18.5 0 01-2.16 3.19m-6.72-1.07a3 3 0 11-4.24-4.24" />
                                            <line x1="1" y1="1" x2="23" y2="23" />
                                        </svg>
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <svg
                                            width="18"
                                            height="18"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2"
                                        >
                                            <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                                            <circle cx="12" cy="12" r="3" />
                                        </svg>
                                    }
                                        .into_any()
                                }
                            }}
                        </button>
                    }
                        .into_any()
                } else {
                    ().into_any()
                }}
            </div>
        </div>
    }
}

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

// ═══════════════════════════════════════════════════════════════════════════
// SHIMMER SKELETONS
// ═══════════════════════════════════════════════════════════════════════════

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

#[component]
pub fn TicketCardShimmer() -> impl IntoView {
    view! {
        <div class="shim-ticket-card">
            <div class="shim shim-tkt-title"></div>
            <div class="shim shim-tkt-venue"></div>
            <div class="shim shim-tkt-date"></div>
            <div class="shim shim-tkt-badge"></div>
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

// ═══════════════════════════════════════════════════════════════════════════
// EMPTY STATES
// ═══════════════════════════════════════════════════════════════════════════

#[component]
pub fn EmptyState(
    icon: &'static str,
    title: &'static str,
    #[prop(into)] body: String,
    #[prop(optional)] cta_label: Option<&'static str>,
    #[prop(optional)] cta_href: Option<&'static str>,
) -> impl IntoView {
    view! {
        <div class="empty-state">
            <div class="empty-state-icon">{icon}</div>
            <div class="empty-state-title">{title}</div>
            <div class="empty-state-body">{body}</div>
            {cta_label
                .zip(cta_href)
                .map(|(label, href)| {
                    view! {
                        <a href=href class="empty-state-cta">
                            {label}
                        </a>
                    }
                })}
        </div>
    }
}

pub mod audio_pill;
pub mod detail_image_section;
pub mod draggable_overlay;
pub mod event_story_preview;
pub mod merchant_dashboard_event;
pub mod story_bars;
pub mod story_viewer;
