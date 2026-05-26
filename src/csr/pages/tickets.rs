use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::csr::components::{BottomNav, EmptyState, TicketCardShimmer};
use crate::csr::hooks::{format_idr, ThemeToggle};
use crate::csr::models::{IssuedTicket, ListMyTicketsRequest};
use crate::csr::services::ticket as ticket_svc;

#[component]
fn QrDisplay(code: String) -> impl IntoView {
    let bytes: Vec<u8> = code.bytes().collect();
    view! {
        <div class="qr-wrap">
            <div class="qr-box">
                <svg width="160" height="160" viewBox="0 0 160 160">
                    <rect width="160" height="160" fill="white" />
                    <rect x="10" y="10" width="50" height="50" fill="#0d0d1a" />
                    <rect x="16" y="16" width="38" height="38" fill="white" />
                    <rect x="22" y="22" width="26" height="26" fill="#0d0d1a" />
                    <rect x="100" y="10" width="50" height="50" fill="#0d0d1a" />
                    <rect x="106" y="16" width="38" height="38" fill="white" />
                    <rect x="112" y="22" width="26" height="26" fill="#0d0d1a" />
                    <rect x="10" y="100" width="50" height="50" fill="#0d0d1a" />
                    <rect x="16" y="106" width="38" height="38" fill="white" />
                    <rect x="22" y="112" width="26" height="26" fill="#0d0d1a" />
                    {(0i32..25)
                        .map(|i| {
                            let x = (70 + (i % 5) * 12).to_string();
                            let y = (70 + (i / 5) * 12).to_string();
                            let idx = (i as usize) % bytes.len().max(1);
                            let hash = (bytes.get(idx).copied().unwrap_or(0) as i32 + i) % 3;
                            (hash > 0)
                                .then(|| {
                                    view! { <rect x=x y=y width="8" height="8" fill="#0d0d1a" /> }
                                })
                        })
                        .collect_view()}
                </svg>
            </div>
            <span class="qr-ref">{code}</span>
        </div>
    }
}

fn parse_date_parts(date_str: &str) -> (i32, u32, u32, u32, u32) {
    let date_part = date_str.split('T').next().unwrap_or("");
    let time_part = date_str.split('T').nth(1).unwrap_or("00:00");
    let mut dp = date_part.split('-');
    let y: i32 = dp.next().and_then(|s| s.parse().ok()).unwrap_or(2026);
    let mo: u32 = dp.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let d: u32 = dp.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let mut tp = time_part.split(':');
    let h: u32 = tp.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let mi: u32 = tp.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (y, mo, d, h, mi)
}

fn format_event_date(date_str: &str) -> String {
    let (_, mo, d, h, mi) = parse_date_parts(date_str);
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let m = months
        .get((mo as usize).saturating_sub(1))
        .unwrap_or(&"Jan");
    format!("{} {} • {:02}:{:02} WIB", m, d, h, mi)
}

fn format_month_day(date_str: &str) -> String {
    let (_, mo, d, _, _) = parse_date_parts(date_str);
    let months = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    let m = months
        .get((mo as usize).saturating_sub(1))
        .unwrap_or(&"JAN");
    format!("{} {}", m, d)
}

#[component]
fn TicketCard(ticket: IssuedTicket) -> impl IntoView {
    let live = false;
    let show_qr = ticket.tier_type == "VIP" || ticket.tier_type == "BACKSTAGE" || live;
    let is_vip = ticket.tier_type == "VIP";

    let detail_href = format!("/tickets/{}", ticket.id);

    if is_vip && !live {
        let (_, mo, d, h, mi) = parse_date_parts(&ticket.event_time);
        let months = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let m = months
            .get((mo as usize).saturating_sub(1))
            .unwrap_or(&"Jan");
        let date_short = format!("{} {} • {:02}:{:02} WIB", d, m, h, mi);
        return view! {
            <A href=detail_href attr:class="ticket-card ticket-card--vip ticket-card--link">
                <div class="vip-top">
                    <div class="vip-icon-wrap">
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
                            <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
                        </svg>
                    </div>
                    <span class="vip-label">"VIP ACCESS"</span>
                </div>
                <h3 class="vip-title">{ticket.event_title.clone()}</h3>
                <div class="vip-meta-row">
                    <div>
                        <span class="meta-label">"SECTION"</span>
                        <span class="meta-val">{ticket.zone.clone()}</span>
                    </div>
                    <div>
                        <span class="meta-label">"ROW/SEAT"</span>
                        <span class="meta-val">{ticket.row_seat.clone()}</span>
                    </div>
                </div>
                <div class="vip-bottom">
                    <span class="vip-date">{date_short}</span>
                    <span class="vip-price">{format_idr(ticket.price_idr)}</span>
                </div>
            </A>
        }.into_any();
    }

    let card_class = if live {
        "ticket-card ticket-card--live ticket-card--link"
    } else {
        "ticket-card ticket-card--link"
    };
    let event_date = format_event_date(&ticket.event_time);
    let ticket_ref = ticket.ticket_ref.clone();
    let event_venue = format!(
        "{} • {}",
        format_month_day(&ticket.event_time),
        ticket.venue_name.to_uppercase()
    );

    view! {
        <A href=detail_href attr:class=card_class>
            {live.then(|| view! { <div class="live-badge-card">"LIVE TODAY"</div> })}
            <img
                src=ticket.event_cover.clone()
                alt=ticket.event_title.clone()
                class="ticket-cover"
            />
            <div class="ticket-body">
                <h3 class="ticket-event-title">{ticket.event_title.clone()}</h3>
                <p class="ticket-event-venue-date">{event_venue}</p>
                <div class="ticket-meta-grid">
                    <div class="ticket-meta-item">
                        <span class="meta-label">"VENUE"</span>
                        <span class="meta-val">{ticket.venue_name.clone()}</span>
                    </div>
                    <div class="ticket-meta-item">
                        <span class="meta-label">"DATE &amp; TIME"</span>
                        <span class="meta-val">{event_date}</span>
                    </div>
                </div>
                {show_qr.then(|| view! { <QrDisplay code=ticket_ref /> })}
                <div class="ticket-footer-row">
                    <div class="ticket-open-qr-btn">
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
                        "OPEN QR"
                    </div>
                    <div class="ticket-price">{format_idr(ticket.price_idr)}</div>
                </div>
            </div>
        </A>
    }.into_any()
}

#[component]
pub fn TicketsPage() -> impl IntoView {
    let filter = RwSignal::new("Active".to_string());
    let tickets = RwSignal::new(Vec::<IssuedTicket>::new());
    let loading = RwSignal::new(true);

    Effect::new(move |_| {
        let f = filter.get().to_uppercase();
        loading.set(true);
        spawn_local(async move {
            let req = ListMyTicketsRequest {
                filter: f,
                page: 1,
                page_size: 20,
            };
            if let Ok(res) = ticket_svc::list_my_tickets(&req).await {
                if !res.tickets.is_empty() {
                    tickets.set(res.tickets);
                }
            }
            loading.set(false);
        });
    });

    let _upcoming_count =
        move || tickets.with(|v| v.iter().filter(|t| t.status == "ACTIVE").count());

    let _featured_ticket = move || tickets.with(|v| v.first().cloned());
    let _secondary_tickets = move || tickets.with(|v| v.iter().skip(1).cloned().collect::<Vec<_>>());

    view! {
        <div class="page tickets-page">
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

            // ── Page title + filter tabs ───────────────────────────────────
            <div class="tickets-hero-row">
                <div>
                    <h1 class="tickets-title">"MY"<br />"TICKETS"</h1>
                    <p class="tickets-sub">
                        "Your upcoming stage access and past memories<br/>stored in high-fidelity."
                    </p>
                </div>
                <div class="tickets-filter-btns">
                    {["Active", "Past Events"]
                        .iter()
                        .map(|f| {
                            let label = *f;
                            let filter_val = if label == "Active" { "Active" } else { "Past" };
                            let cls = move || {
                                if filter.get() == filter_val {
                                    "tk-filter-btn tk-filter-btn--active"
                                } else {
                                    "tk-filter-btn"
                                }
                            };
                            view! {
                                <button class=cls on:click=move |_| filter.set(filter_val.into())>
                                    {label}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </div>

            <div class="tickets-list tickets-list--mobile-only">
                {move || {
                    if loading.get() {
                        (0..3).map(|_| view! { <TicketCardShimmer /> }).collect_view().into_any()
                    } else {
                        let list = tickets.get();
                        if list.is_empty() {
                            let (icon, title, body) = match filter.get().as_str() {
                                "ACTIVE" => {
                                    (
                                        "🎫",
                                        "BELUM ADA TIKET AKTIF",
                                        "Beli tiket event favoritmu dan mulai pengalamanmu!",
                                    )
                                }
                                "PAST" => {
                                    (
                                        "🕐",
                                        "BELUM ADA RIWAYAT",
                                        "Tiket yang sudah dipakai akan muncul di sini.",
                                    )
                                }
                                _ => {
                                    (
                                        "📭",
                                        "TIDAK ADA TIKET",
                                        "Tiket kamu akan muncul di sini setelah pembelian.",
                                    )
                                }
                            };
                            view! {
                                <EmptyState
                                    icon=icon
                                    title=title
                                    body=body
                                    cta_label="JELAJAHI EVENT"
                                    cta_href="/"
                                />
                            }
                                .into_any()
                        } else {
                            list.into_iter()
                                .map(|t| view! { <TicketCard ticket=t /> })
                                .collect_view()
                                .into_any()
                        }
                    }
                }}
            </div>

            // ── Past Experiences ──────────────────────────────────────────
            <div class="past-section">
                <h3 class="past-section-title">"| PAST EXPERIENCES"</h3>
                <div class="past-list">
                    {[
                        (
                            "AUG 15, 2023",
                            "ECHOES OF SUMMER",
                            "Ancol Beach City • Festival B",
                            "Rp850.000",
                        ),
                        (
                            "JUL 02, 2023",
                            "URBAN BEAT FEST",
                            "GBK Sports Complex • VIP Area",
                            "Rp1.500.000",
                        ),
                    ]
                        .iter()
                        .map(|(date, title, venue, price)| {
                            view! {
                                <div class="past-item">
                                    <span class="past-item-date">{*date}</span>
                                    <div class="past-item-info">
                                        <span class="past-item-title">{*title}</span>
                                        <span class="past-item-venue">{*venue}</span>
                                    </div>
                                    <div class="past-item-right">
                                        <div class="past-item-price-col">
                                            <span class="past-item-price-label">"PRICE"</span>
                                            <span class="past-item-price">{*price}</span>
                                        </div>
                                        <button class="past-item-dl" aria-label="Download">
                                            <svg
                                                width="14"
                                                height="14"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2"
                                                stroke-linecap="round"
                                            >
                                                <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
                                                <polyline points="7 10 12 15 17 10" />
                                                <line x1="12" y1="15" x2="12" y2="3" />
                                            </svg>
                                        </button>
                                        <button class="past-item-star" aria-label="Favorite">
                                            <svg
                                                width="14"
                                                height="14"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2"
                                                stroke-linecap="round"
                                            >
                                                <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
                                            </svg>
                                        </button>
                                    </div>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            </div>

            <BottomNav active="tickets" />
        </div>
    }
}
