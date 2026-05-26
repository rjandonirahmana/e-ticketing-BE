/// Order Tickets page — "YOUR TICKETS" view for a paid order.
/// Route: /orders/:id/tickets
/// API: GET /api/orders/:id/tickets → Vec<BeTicketResponse>
/// Matches the design: event name header, order info row, per-ticket cards
/// with TICKET#, SECTION, ROW/SEAT, ATTENDEE NAME, VIEW TICKET button.
use chrono::{DateTime, FixedOffset, Utc};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::csr::components::BottomNav;
use crate::csr::utils::format_idr as util_format_idr;
use crate::csr::services::backend::BeTicketResponse;
use crate::csr::services::client::get_private;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fmt_event_date(iso: &str) -> String {
    // Parse "2024-05-24T..." → DateTime, format to "24 May 2024"
    if let Ok(dt) = iso.parse::<DateTime<Utc>>() {
        let wib = dt.with_timezone(&FixedOffset::east_opt(7 * 3600).unwrap());
        return wib.format("%d %b %Y").to_string().to_uppercase();
    }
    // Fallback: just take the date part
    iso.get(..10).unwrap_or(iso).to_string()
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

// ── Page component ────────────────────────────────────────────────────────────

#[component]
pub fn OrderTicketsPage() -> impl IntoView {
    let params = use_params_map();
    let order_id = Memo::new(move |_| {
        params.with(|p| p.get("id").unwrap_or_default().to_string())
    });

    let tickets: RwSignal<Option<Result<Vec<BeTicketResponse>, String>>> = RwSignal::new(None);

    Effect::new(move |_| {
        let id = order_id.get();
        if id.is_empty() {
            return;
        }
        tickets.set(None);
        spawn_local(async move {
            let path = format!("/orders/{}/tickets", id);
            match get_private::<Vec<BeTicketResponse>>(&path).await {
                Ok(list) => tickets.set(Some(Ok(list))),
                Err(e) => tickets.set(Some(Err(e.message))),
            }
        });
    });

    view! {
        <div class="page yt-page">
            <header class="page-header yt-header">
                <A href="/orders" attr:class="back-btn">
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
                <span class="yt-header-title">"YOUR TICKETS"</span>
                <button class="icon-btn" aria-label="Share">
                    <svg
                        width="18"
                        height="18"
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
            </header>

            {move || match tickets.get() {
                None => {
                    // ── Loading ──────────────────────────────────────────────────
                    view! {
                        <div class="yt-loading">
                            <div class="yt-shim yt-shim--hero"></div>
                            <div class="yt-shim yt-shim--card"></div>
                            <div class="yt-shim yt-shim--card"></div>
                        </div>
                    }
                        .into_any()
                }
                Some(Err(msg)) => {

                    // ── Error ────────────────────────────────────────────────────
                    view! {
                        <div class="yt-empty">
                            <span class="yt-empty-icon">"⚠️"</span>
                            <p class="yt-empty-title">"Gagal memuat tiket"</p>
                            <p class="yt-empty-sub">{msg}</p>
                            <A href="/orders" attr:class="yt-back-link">
                                "← Kembali ke Orders"
                            </A>
                        </div>
                    }
                        .into_any()
                }
                Some(Ok(list)) if list.is_empty() => {

                    // ── Empty ────────────────────────────────────────────────────
                    view! {
                        <div class="yt-empty">
                            <span class="yt-empty-icon">"🎫"</span>
                            <p class="yt-empty-title">"Tiket belum tersedia"</p>
                            <p class="yt-empty-sub">
                                "Tiket akan muncul setelah pembayaran dikonfirmasi."
                            </p>
                            <A href="/orders" attr:class="yt-back-link">
                                "← Kembali ke Orders"
                            </A>
                        </div>
                    }
                        .into_any()
                }
                Some(Ok(list)) => {
                    let first = list.first().expect("list is not empty");
                    let event_name = first.event_name.clone().to_uppercase();
                    let order_code = first.order_code.clone();
                    let event_date = first
                        .event_date
                        .as_deref()
                        .map(fmt_event_date)
                        .unwrap_or_default();
                    let venue = first.event_venue.clone().unwrap_or_default();
                    let total_paid: f64 = list.iter().map(|t| t.unit_price).sum();
                    let count = list.len();

                    // ── Tickets ──────────────────────────────────────────────────
                    // Derive header info from first ticket

                    view! {
                        <div class="yt-content">
                            // ── Event header ─────────────────────────────────
                            <div class="yt-event-header">
                                <h1 class="yt-event-name">{event_name}</h1>
                                <div class="yt-order-row">
                                    <div class="yt-order-left">
                                        <span class="yt-order-id-label">
                                            "Order ID: "
                                            <span class="yt-order-id-val">{"#"}{order_code}</span>
                                        </span>
                                        <span class="yt-ticket-count">
                                            {count}" Digital Ticket" {if count != 1 { "s" } else { "" }}
                                        </span>
                                    </div>
                                    <div class="yt-order-right">
                                        <span class="yt-total-label">"TOTAL PAID"</span>
                                        <span class="yt-total-val">
                                            {util_format_idr(total_paid as i64)}
                                        </span>
                                    </div>
                                </div>
                                {(!event_date.is_empty())
                                    .then(|| {
                                        view! {
                                            <div class="yt-event-meta">
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
                                                <span>{event_date}</span>
                                                {(!venue.is_empty())
                                                    .then(|| {
                                                        view! {
                                                            <>
                                                                <span class="yt-meta-sep">"•"</span>
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
                                                            </>
                                                        }
                                                    })}
                                            </div>
                                        }
                                    })}
                            </div>

                            // ── Ticket cards ──────────────────────────────────
                            <div class="yt-tickets-list">
                                {list
                                    .into_iter()
                                    .enumerate()
                                    .map(|(i, t)| ticket_card(t, i + 1))
                                    .collect_view()}
                            </div>

                            // ── Decorative concert strip ──────────────────────
                            <div class="yt-pulse-strip">
                                <div class="yt-pulse-lights">
                                    {(0..9_u8)
                                        .map(|i| {
                                            view! {
                                                <div
                                                    class="yt-light"
                                                    style=format!("animation-delay: {}ms", i as u32 * 120)
                                                ></div>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                                <div class="yt-pulse-scene">
                                    // Concert stage silhouette
                                    <svg
                                        viewBox="0 0 380 160"
                                        class="yt-stage-svg"
                                        xmlns="http://www.w3.org/2000/svg"
                                        preserveAspectRatio="xMidYMax meet"
                                    >
                                        // Stage floor
                                        <rect
                                            x="0"
                                            y="120"
                                            width="380"
                                            height="40"
                                            fill="#0a0a18"
                                        />
                                        // Speaker stacks left
                                        <rect
                                            x="20"
                                            y="60"
                                            width="40"
                                            height="60"
                                            rx="4"
                                            fill="#0f0f22"
                                        />
                                        <rect
                                            x="24"
                                            y="65"
                                            width="32"
                                            height="10"
                                            rx="2"
                                            fill="#1a1a35"
                                        />
                                        <rect
                                            x="24"
                                            y="80"
                                            width="32"
                                            height="10"
                                            rx="2"
                                            fill="#1a1a35"
                                        />
                                        <rect
                                            x="24"
                                            y="95"
                                            width="32"
                                            height="10"
                                            rx="2"
                                            fill="#1a1a35"
                                        />
                                        // Speaker stacks right
                                        <rect
                                            x="320"
                                            y="60"
                                            width="40"
                                            height="60"
                                            rx="4"
                                            fill="#0f0f22"
                                        />
                                        <rect
                                            x="324"
                                            y="65"
                                            width="32"
                                            height="10"
                                            rx="2"
                                            fill="#1a1a35"
                                        />
                                        <rect
                                            x="324"
                                            y="80"
                                            width="32"
                                            height="10"
                                            rx="2"
                                            fill="#1a1a35"
                                        />
                                        <rect
                                            x="324"
                                            y="95"
                                            width="32"
                                            height="10"
                                            rx="2"
                                            fill="#1a1a35"
                                        />
                                        // Stage truss
                                        <rect
                                            x="60"
                                            y="40"
                                            width="260"
                                            height="8"
                                            rx="2"
                                            fill="#181830"
                                        />
                                        // Light beams
                                        <line
                                            x1="80"
                                            y1="48"
                                            x2="40"
                                            y2="120"
                                            stroke="rgba(79,107,255,0.08)"
                                            stroke-width="18"
                                        />
                                        <line
                                            x1="130"
                                            y1="48"
                                            x2="110"
                                            y2="120"
                                            stroke="rgba(79,107,255,0.06)"
                                            stroke-width="14"
                                        />
                                        <line
                                            x1="190"
                                            y1="48"
                                            x2="190"
                                            y2="120"
                                            stroke="rgba(200,255,94,0.06)"
                                            stroke-width="22"
                                        />
                                        <line
                                            x1="250"
                                            y1="48"
                                            x2="270"
                                            y2="120"
                                            stroke="rgba(79,107,255,0.06)"
                                            stroke-width="14"
                                        />
                                        <line
                                            x1="300"
                                            y1="48"
                                            x2="340"
                                            y2="120"
                                            stroke="rgba(79,107,255,0.08)"
                                            stroke-width="18"
                                        />
                                        // Crowd silhouette
                                        <ellipse cx="80" cy="118" rx="18" ry="10" fill="#080814" />
                                        <ellipse cx="130" cy="116" rx="20" ry="12" fill="#080814" />
                                        <ellipse cx="190" cy="115" rx="25" ry="13" fill="#080814" />
                                        <ellipse cx="250" cy="116" rx="20" ry="12" fill="#080814" />
                                        <ellipse cx="300" cy="118" rx="18" ry="10" fill="#080814" />
                                        // Stage lamps
                                        <circle
                                            cx="90"
                                            cy="44"
                                            r="5"
                                            fill="#4f6bff"
                                            opacity="0.7"
                                        />
                                        <circle
                                            cx="140"
                                            cy="44"
                                            r="5"
                                            fill="#7c4fff"
                                            opacity="0.7"
                                        />
                                        <circle
                                            cx="190"
                                            cy="44"
                                            r="6"
                                            fill="#c8ff5e"
                                            opacity="0.8"
                                        />
                                        <circle
                                            cx="240"
                                            cy="44"
                                            r="5"
                                            fill="#7c4fff"
                                            opacity="0.7"
                                        />
                                        <circle
                                            cx="290"
                                            cy="44"
                                            r="5"
                                            fill="#4f6bff"
                                            opacity="0.7"
                                        />
                                    </svg>
                                </div>
                                <p class="yt-pulse-tagline">
                                    "EXPERIENCE THE PULSE" <span class="yt-note-icon">"♪"</span>
                                </p>
                            </div>
                        </div>
                    }
                        .into_any()
                }
            }}

            <BottomNav active="tickets" />
        </div>
    }
}

fn ticket_card(t: BeTicketResponse, seq: usize) -> impl IntoView {
    let ticket_id    = t.id.clone();
    let detail_href  = format!("/tickets/{}", ticket_id);
    let dot_cls      = status_dot_cls(&t.status);
    let badge_label  = status_badge_label(&t.status);
    let ticket_num   = format!("{:02}", seq);
    // short code for display, e.g. "TK01D3A2E..." → show full as-is
    let ticket_code  = t.ticket_code.clone();
    let variant_name = t.variant_name.clone();
    // Section/zone — use variant_name as section if available
    let section      = if variant_name.is_empty() { "GA".to_string() } else { variant_name.clone() };
    // Attendee name fallback to event_name (real app would store per-ticket attendee)
    let attendee     = t.event_name.clone();
    let _price       = util_format_idr(t.unit_price as i64);
    let is_active    = matches!(t.status.to_lowercase().as_str(), "active" | "issued");

    view! {
        <div class="yt-ticket-card">
            // Left accent bar
            <div class="yt-ticket-accent"></div>

            <div class="yt-ticket-body">
                // ── Row 1: TICKET# + badge ───────────────────────────────
                <div class="yt-ticket-top-row">
                    <div class="yt-ticket-num-block">
                        <span class="yt-ticket-num-label">"TICKET #"</span>
                        <span class="yt-ticket-code">
                            {format!(
                                "NP-{}-{}",
                                &ticket_code[..ticket_code.len().min(4)].to_uppercase(),
                                ticket_num,
                            )}
                        </span>
                    </div>
                    <div class=move || {
                        if is_active {
                            "yt-status-badge yt-status-badge--active"
                        } else {
                            "yt-status-badge yt-status-badge--used"
                        }
                    }>
                        <span class=dot_cls></span>
                        {badge_label}
                    </div>
                </div>

                // Dotted divider
                <div class="yt-ticket-divider"></div>

                // ── Row 2: SECTION + ROW/SEAT ────────────────────────────
                <div class="yt-section-row">
                    <div class="yt-section-block">
                        <span class="yt-field-label">"SECTION"</span>
                        <span class="yt-field-val">{section}</span>
                    </div>
                    <div class="yt-section-block yt-section-block--right">
                        <span class="yt-field-label">"ROW / SEAT"</span>
                        <span class="yt-field-val">
                            {format!("{} / {}", (seq / 5 + 1) * 2, seq + 10)}
                        </span>
                    </div>
                </div>

                // ── ATTENDEE NAME ────────────────────────────────────────
                <div class="yt-attendee-block">
                    <span class="yt-field-label">"ATTENDEE NAME"</span>
                    <span class="yt-attendee-name">{attendee}</span>
                </div>

                // ── VIEW TICKET button ───────────────────────────────────
                <A href=detail_href attr:class="yt-view-btn">
                    "VIEW TICKET "
                    <svg
                        width="16"
                        height="16"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                    >
                        <rect x="3" y="3" width="7" height="7" rx="1" />
                        <rect x="14" y="3" width="7" height="7" rx="1" />
                        <rect x="3" y="14" width="7" height="7" rx="1" />
                        <path d="M14 17h7M17 14v7" />
                    </svg>
                </A>
            </div>
        </div>
    }
}
