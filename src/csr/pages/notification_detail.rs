use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::csr::hooks::ThemeToggle;

#[component]
pub fn NotificationDetailPage() -> impl IntoView {
    let params = use_params_map();
    let _id = params.with_untracked(|p| p.get("id").unwrap_or("n2".into()));

    view! {
        <div class="page">
            <header class="page-header">
                <A href="/notifications" attr:class="back-btn">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="page-logo">"NOTIFICATIONS"</span>
                <div class="header-actions">
                    <ThemeToggle />
                    <A href="/profile" attr:class="nav-avatar">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/><circle cx="12" cy="7" r="4"/>
                        </svg>
                    </A>
                </div>
            </header>

            <div class="nd-hero">
                <div class="nd-check-ring">
                    <div class="nd-check-circle">
                        <svg width="44" height="44" viewBox="0 0 24 24" fill="none" stroke="#0d0d1a" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                            <polyline points="20 6 9 17 4 12"/>
                        </svg>
                    </div>
                </div>
                <span class="nd-eyebrow">"PAYMENT CONFIRMED"</span>
                <h1 class="nd-title">"Midnight Jazz"</h1>
                <p class="nd-subtitle">
                    "Your payment of "<strong>"Rp850.000"</strong>" for Midnight Jazz was successful."
                </p>
            </div>

            <div class="nd-card">
                <div class="nd-row">
                    <div class="nd-cell">
                        <span class="nd-label">"ORDER ID"</span>
                        <span class="nd-val">"#PULSE-9923841"</span>
                    </div>
                    <div class="nd-cell nd-cell--right">
                        <span class="nd-label">"DATE"</span>
                        <span class="nd-val">"24 Oct 2023"</span>
                    </div>
                </div>
                <div class="nd-divider"></div>
                <div class="nd-row">
                    <div class="nd-pay">
                        <div class="nd-pay-icon">
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="#c8ff5e">
                                <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/>
                            </svg>
                        </div>
                        <div class="nd-cell">
                            <span class="nd-label">"PAYMENT METHOD"</span>
                            <span class="nd-val">"GOPAY"</span>
                        </div>
                    </div>
                    <div class="nd-cell nd-cell--right">
                        <span class="nd-label">"TOTAL"</span>
                        <span class="nd-total">"Rp850.000"</span>
                    </div>
                </div>
            </div>

            <div class="nd-venue-card">
                <div class="nd-venue-img"></div>
                <div class="nd-venue-info">
                    <span class="nd-eyebrow">"EVENT VENUE"</span>
                    <h3 class="nd-venue-name">"The Blue Note, Jakarta"</h3>
                </div>
            </div>

            <div class="nd-info-banner">
                <div class="nd-info-icon">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="#c8ff5e" stroke-width="2.4" stroke-linecap="round">
                        <circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/>
                    </svg>
                </div>
                <p>"Your e-ticket has been generated. You can find it in the "<strong>"Tickets"</strong>" tab or download the PDF version from your email."</p>
            </div>

            <div class="nd-cta-wrap">
                <A href="/tickets/tk1" attr:class="nd-cta-btn">
                    <span>"View Ticket"</span>
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M2 9a3 3 0 010-6h20a3 3 0 010 6H2zM2 15a3 3 0 000 6h20a3 3 0 000-6H2z"/>
                    </svg>
                </A>
            </div>
        </div>
    }
}
