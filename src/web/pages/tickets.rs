//! tickets.rs — Halaman Tiket Saya dengan QR Code (SSR).

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::get_my_tickets;
use crate::web::app::AuthResource;
use crate::web::components::{BottomNav, EmptyState, ThemeToggle, TicketCardShimmer};
use crate::web::models::{format_date, format_price};

/// QR display component using SSR-compatible SVG generation.
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
                                    view! {
                                        <rect x=x y=y width="8" height="8" fill="#0d0d1a" />
                                    }
                                })
                        })
                        .collect_view()}
                </svg>
            </div>
            <span class="qr-ref">{code}</span>
        </div>
    }
}

#[component]
pub fn TicketsPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let tickets = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in { get_my_tickets().await } else { Ok(vec![]) }
        },
    );

    let filter = RwSignal::new("all".to_string());

    view! {
        <div class="page tickets-page">

            // ── Header ──────────────────────────────────────────────────────────
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

            // ── Title + filter tabs ──────────────────────────────────────────────
            <div class="tickets-hero-row">
                <div>
                    <h1 class="tickets-title">"MY"<br />"TICKETS"</h1>
                    <p class="tickets-sub">
                        "Your upcoming stage access and past memories stored in high-fidelity."
                    </p>
                </div>
                <div class="tickets-filter-btns">
                    {[("all", "Semua"), ("active", "Aktif"), ("used", "Digunakan")]
                        .iter()
                        .map(|(val, label)| {
                            let v = *val;
                            let cls = move || {
                                if filter.get() == v {
                                    "tk-filter-btn tk-filter-btn--active"
                                } else {
                                    "tk-filter-btn"
                                }
                            };
                            view! {
                                <button class=cls on:click=move |_| filter.set(v.into())>
                                    {*label}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </div>

            // ── Ticket list ──────────────────────────────────────────────────────
            <div class="tickets-list tickets-list--mobile-only">
                <Suspense fallback=move || {
                    view! {
                        <div>
                            <TicketCardShimmer />
                            <TicketCardShimmer />
                            <TicketCardShimmer />
                        </div>
                    }
                }>
                    {move || {
                        tickets
                            .get()
                            .map(|res| match res {
                                Err(_) => {
                                    view! {
                                        <EmptyState
                                            icon="⚠️"
                                            title="GAGAL MEMUAT"
                                            body="Gagal memuat tiket. Coba login ulang."
                                        />
                                    }
                                        .into_any()
                                }
                                Ok(mut list) => {
                                    let f = filter.get();
                                    if f != "all" {
                                        list.retain(|t| t.status == f);
                                    }
                                    if list.is_empty() {
                                        let (icon, title, body) = match f.as_str() {
                                            "active" => (
                                                "🎫",
                                                "BELUM ADA TIKET AKTIF",
                                                "Beli tiket event favoritmu dan mulai pengalamanmu!",
                                            ),
                                            "used" => (
                                                "🕐",
                                                "BELUM ADA RIWAYAT",
                                                "Tiket yang sudah dipakai akan muncul di sini.",
                                            ),
                                            _ => (
                                                "📭",
                                                "TIDAK ADA TIKET",
                                                "Tiket kamu akan muncul di sini setelah pembelian.",
                                            ),
                                        };
                                        return view! {
                                            <EmptyState
                                                icon=icon
                                                title=title
                                                body=body
                                                cta_label="JELAJAHI EVENT"
                                                cta_href="/explore"
                                            />
                                        }
                                            .into_any();
                                    }
                                    list.into_iter()
                                        .map(|t| {
                                            let code = t.ticket_code.clone();
                                            let event = t.event_name.clone();
                                            let var = t.variant_name.clone();
                                            let date = format_date(&t.event_date);
                                            let venue = t.event_venue.clone().unwrap_or_default();
                                            let price = format_price(t.unit_price);
                                            let status = t.status.clone();
                                            let cover = t.cover_url.clone();
                                            let ticket_id = t.id.clone();
                                            // Tiket TERPAKAI harus terbaca beda sejak
                                            // sekilas. Kedua cabang sebelumnya
                                            // mengembalikan kelas yang sama persis —
                                            // percabangan yang tak melakukan apa-apa,
                                            // jadi tiket yang sudah dipakai tampil
                                            // identik dengan yang masih sah. Di
                                            // aplikasi tiket itu bukan soal rapi:
                                            // pemegang tiket (dan petugas yang
                                            // melihat layarnya) tak punya petunjuk
                                            // apa pun selain membuka QR-nya satu per
                                            // satu. Label "USED" di dalam kartu sudah
                                            // ada sejak dulu — yang hilang justru
                                            // penanda pada kartunya sendiri.
                                            let card_class = if status == "used" {
                                                "ticket-card ticket-card--link ticket-card--used"
                                            } else {
                                                "ticket-card ticket-card--link"
                                            };
                                            let ticket_code_for_qr = code.clone();
                                            view! {
                                                <A href=format!("/tickets/{ticket_id}") attr:class=card_class>
                                                    {match cover {
                                                        Some(url) => {
                                                            view! {
                                                                <img
                                                                    src=url
                                                                    alt=event.clone()
                                                                    class="ticket-cover"
                                                                />
                                                            }
                                                                .into_any()
                                                        }
                                                        None => {
                                                            view! {
                                                                <div class="ticket-cover" style="display:flex;align-items:center;justify-content:center;font-size:2rem">
                                                                    "🎪"
                                                                </div>
                                                            }
                                                                .into_any()
                                                        }
                                                    }}
                                                    <div class="ticket-body">
                                                        <h3 class="ticket-event-title">{event}</h3>
                                                        <p class="ticket-event-venue-date">
                                                            {format!("{} • {}", date, venue.to_uppercase())}
                                                        </p>
                                                        <div class="ticket-meta-grid">
                                                            <div class="ticket-meta-item">
                                                                <span class="meta-label">"TIER"</span>
                                                                <span class="meta-val">{var}</span>
                                                            </div>
                                                            <div class="ticket-meta-item">
                                                                <span class="meta-label">"PRICE"</span>
                                                                <span class="meta-val">{price}</span>
                                                            </div>
                                                        </div>
                                                        {(status != "used")
                                                            .then(|| {
                                                                view! {
                                                                    <QrDisplay code=ticket_code_for_qr />
                                                                }
                                                            })}
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
                                                                {if status == "used" {
                                                                    "USED"
                                                                } else {
                                                                    "OPEN QR"
                                                                }}
                                                            </div>
                                                            <div class="ticket-price">{code}</div>
                                                        </div>
                                                    </div>
                                                </A>
                                            }
                                        })
                                        .collect_view()
                                        .into_any()
                                }
                            })
                            .unwrap_or_else(|| view! { <div /> }.into_any())
                    }}
                </Suspense>
            </div>

            // ── Past Experiences ─────────────────────────────────────────────────
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
