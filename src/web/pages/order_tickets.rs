//! order_tickets.rs — Tiket dalam satu order. Route: /orders/:id/tickets

use chrono::{DateTime, Utc};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::get_order_tickets;
use crate::web::app::AuthResource;
use crate::web::models::{format_price, TicketResponse};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fmt_product_date(dt: &DateTime<Utc>) -> String {
    use chrono::Datelike;
    let months = ["JAN","FEB","MAR","APR","MEI","JUN","JUL","AGU","SEP","OKT","NOV","DES"];
    format!("{} {} {}", dt.day(), months[dt.month0() as usize], dt.year())
}

fn status_dot_cls(s: &str) -> &'static str {
    match s.to_lowercase().as_str() {
        "active" | "issued" => "yt-ticket-dot yt-ticket-dot--active",
        "used" | "checked_in" => "yt-ticket-dot yt-ticket-dot--used",
        "refunded" => "yt-ticket-dot yt-ticket-dot--refunded",
        _ => "yt-ticket-dot",
    }
}

fn status_badge_label(s: &str) -> &'static str {
    match s.to_lowercase().as_str() {
        "active" | "issued" => "ACTIVE",
        "used" | "checked_in" => "USED",
        "refunded" => "REFUNDED",
        "expired" => "EXPIRED",
        _ => "—",
    }
}

// ── Ticket card ───────────────────────────────────────────────────────────────

fn ticket_card(t: TicketResponse, seq: usize) -> impl IntoView {
    let detail_href  = format!("/tickets/{}", t.id);
    let dot_cls      = status_dot_cls(&t.status);
    let badge_label  = status_badge_label(&t.status);
    let ticket_num   = format!("{:02}", seq);
    let code_short   = t.ticket_code.chars().take(4).collect::<String>().to_uppercase();
    let code_display = format!("NP-{}-{}", code_short, ticket_num);
    let section      = if t.variant_name.is_empty() { "GA".to_string() } else { t.variant_name.clone() };
    let row_seat     = format!("{} / {}", (seq / 5 + 1) * 2, seq + 10);
    let is_active    = matches!(t.status.to_lowercase().as_str(), "active" | "issued");

    view! {
        <div class="yt-ticket-card">
            <div class="yt-ticket-accent"></div>
            <div class="yt-ticket-body">
                <div class="yt-ticket-top-row">
                    <div class="yt-ticket-num-block">
                        <span class="yt-ticket-num-label">"TICKET #"</span>
                        <span class="yt-ticket-code">{code_display}</span>
                    </div>
                    <div class=if is_active { "yt-status-badge yt-status-badge--active" } else { "yt-status-badge yt-status-badge--used" }>
                        <span class=dot_cls></span>
                        {badge_label}
                    </div>
                </div>

                <div class="yt-ticket-divider"></div>

                <div class="yt-section-row">
                    <div class="yt-section-block">
                        <span class="yt-field-label">"SECTION"</span>
                        <span class="yt-field-val">{section}</span>
                    </div>
                    <div class="yt-section-block yt-section-block--right">
                        <span class="yt-field-label">"ROW / SEAT"</span>
                        <span class="yt-field-val">{row_seat}</span>
                    </div>
                </div>

                <div class="yt-attendee-block">
                    <span class="yt-field-label">"EVENT"</span>
                    <span class="yt-attendee-name">{t.event_name}</span>
                </div>

                <A href=detail_href attr:class="yt-view-btn">
                    "VIEW TICKET "
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <rect x="3" y="3" width="7" height="7" rx="1"/>
                        <rect x="14" y="3" width="7" height="7" rx="1"/>
                        <rect x="3" y="14" width="7" height="7" rx="1"/>
                        <path d="M14 17h7M17 14v7"/>
                    </svg>
                </A>
            </div>
        </div>
    }
}

// ── Main page ─────────────────────────────────────────────────────────────────

#[component]
pub fn OrderTicketsPage() -> impl IntoView {
    let params = use_params_map();
    let order_id = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let tickets = Resource::new(
        move || (order_id(), is_logged_in()),
        |(id, logged_in)| async move {
            if logged_in && !id.is_empty() { get_order_tickets(id).await } else { Ok(vec![]) }
        },
    );

    view! {
        <div class="page yt-page">
            <header class="page-header yt-header">
                <A href="/orders" attr:class="back-btn">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="yt-header-title">"YOUR TICKETS"</span>
                <button class="icon-btn" attr:aria-label="Share">
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <circle cx="18" cy="5" r="3"/>
                        <circle cx="6" cy="12" r="3"/>
                        <circle cx="18" cy="19" r="3"/>
                        <line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/>
                        <line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/>
                    </svg>
                </button>
            </header>

            <Suspense fallback=|| view! {
                <div class="yt-loading">
                    <div class="yt-shim yt-shim--hero"></div>
                    <div class="yt-shim yt-shim--card"></div>
                    <div class="yt-shim yt-shim--card"></div>
                </div>
            }>
                {move || tickets.get().map(|res| match res {
                    Err(msg) => view! {
                        <div class="yt-empty">
                            <span class="yt-empty-icon">"⚠️"</span>
                            <p class="yt-empty-title">"Gagal memuat tiket"</p>
                            <p class="yt-empty-sub">{msg.to_string()}</p>
                            <A href="/orders" attr:class="yt-back-link">"← Kembali ke Orders"</A>
                        </div>
                    }.into_any(),

                    Ok(list) if list.is_empty() => view! {
                        <div class="yt-empty">
                            <span class="yt-empty-icon">"🎫"</span>
                            <p class="yt-empty-title">"Tiket belum tersedia"</p>
                            <p class="yt-empty-sub">
                                "Tiket akan muncul setelah pembayaran dikonfirmasi."
                            </p>
                            <A href="/orders" attr:class="yt-back-link">"← Kembali ke Orders"</A>
                        </div>
                    }.into_any(),

                    Ok(list) => {
                        let first    = list.first().expect("list is not empty");
                        let ev_name  = first.event_name.clone().to_uppercase();
                        let order_code = first.order_code.clone();
                        let ev_date  = fmt_product_date(&first.event_date);
                        let venue    = first.event_venue.clone().unwrap_or_default();
                        let total: f64 = list.iter().map(|t| t.unit_price).sum();
                        let count    = list.len();

                        view! {
                            <div class="yt-content">
                                <div class="yt-product-header">
                                    <h1 class="yt-product-name">{ev_name}</h1>
                                    <div class="yt-order-row">
                                        <div class="yt-order-left">
                                            <span class="yt-order-id-label">
                                                "Order ID: "
                                                <span class="yt-order-id-val">{"#"}{order_code}</span>
                                            </span>
                                            <span class="yt-ticket-count">
                                                {count}" Digital Ticket"{if count != 1 { "s" } else { "" }}
                                            </span>
                                        </div>
                                        <div class="yt-order-right">
                                            <span class="yt-total-label">"TOTAL PAID"</span>
                                            <span class="yt-total-val">{format_price(total)}</span>
                                        </div>
                                    </div>
                                    {(!ev_date.is_empty()).then(|| view! {
                                        <div class="yt-product-meta">
                                            <svg width="11" height="11" viewBox="0 0 24 24" fill="none"
                                                 stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                                <rect x="3" y="4" width="18" height="18" rx="2"/>
                                                <line x1="16" y1="2" x2="16" y2="6"/>
                                                <line x1="8" y1="2" x2="8" y2="6"/>
                                                <line x1="3" y1="10" x2="21" y2="10"/>
                                            </svg>
                                            <span>{ev_date}</span>
                                            {(!venue.is_empty()).then(|| view! {
                                                <>
                                                    <span class="yt-meta-sep">"•"</span>
                                                    <svg width="11" height="11" viewBox="0 0 24 24" fill="none"
                                                         stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                                        <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0118 0z"/>
                                                        <circle cx="12" cy="10" r="3"/>
                                                    </svg>
                                                    <span>{venue}</span>
                                                </>
                                            })}
                                        </div>
                                    })}
                                </div>

                                <div class="yt-tickets-list">
                                    {list.into_iter().enumerate()
                                        .map(|(i, t)| ticket_card(t, i + 1))
                                        .collect_view()}
                                </div>

                                <div class="yt-pulse-strip">
                                    <div class="yt-pulse-lights">
                                        {(0..9u8).map(|i| view! {
                                            <div class="yt-light"
                                                 style=format!("animation-delay:{}ms", i as u32 * 120)>
                                            </div>
                                        }).collect_view()}
                                    </div>
                                    <div class="yt-pulse-scene">
                                        <svg viewBox="0 0 380 160" class="yt-stage-svg"
                                             xmlns="http://www.w3.org/2000/svg"
                                             preserveAspectRatio="xMidYMax meet">
                                            <rect x="0" y="120" width="380" height="40" fill="#0a0a18"/>
                                            <rect x="20" y="60" width="40" height="60" rx="4" fill="#0f0f22"/>
                                            <rect x="24" y="65" width="32" height="10" rx="2" fill="#1a1a35"/>
                                            <rect x="24" y="80" width="32" height="10" rx="2" fill="#1a1a35"/>
                                            <rect x="24" y="95" width="32" height="10" rx="2" fill="#1a1a35"/>
                                            <rect x="320" y="60" width="40" height="60" rx="4" fill="#0f0f22"/>
                                            <rect x="324" y="65" width="32" height="10" rx="2" fill="#1a1a35"/>
                                            <rect x="324" y="80" width="32" height="10" rx="2" fill="#1a1a35"/>
                                            <rect x="324" y="95" width="32" height="10" rx="2" fill="#1a1a35"/>
                                            <rect x="60" y="40" width="260" height="8" rx="2" fill="#181830"/>
                                            <line x1="80" y1="48" x2="40" y2="120" stroke="rgba(79,107,255,0.08)" stroke-width="18"/>
                                            <line x1="190" y1="48" x2="190" y2="120" stroke="rgba(200,255,94,0.06)" stroke-width="22"/>
                                            <line x1="300" y1="48" x2="340" y2="120" stroke="rgba(79,107,255,0.08)" stroke-width="18"/>
                                            <ellipse cx="80" cy="118" rx="18" ry="10" fill="#080814"/>
                                            <ellipse cx="190" cy="115" rx="25" ry="13" fill="#080814"/>
                                            <ellipse cx="300" cy="118" rx="18" ry="10" fill="#080814"/>
                                            <circle cx="90" cy="44" r="5" fill="#4f6bff" opacity="0.7"/>
                                            <circle cx="190" cy="44" r="6" fill="#c8ff5e" opacity="0.8"/>
                                            <circle cx="290" cy="44" r="5" fill="#4f6bff" opacity="0.7"/>
                                        </svg>
                                    </div>
                                    <p class="yt-pulse-tagline">
                                        "EXPERIENCE THE PULSE " <span class="yt-note-icon">"♪"</span>
                                    </p>
                                </div>
                            </div>
                        }.into_any()
                    }
                }).unwrap_or_else(|| view! { <div/> }.into_any())}
            </Suspense>
        </div>
    }
}
