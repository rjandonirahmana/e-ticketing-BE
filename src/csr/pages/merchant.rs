use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::csr::components::{BottomNav, GridBackground, MerchantRowShimmer};
use crate::csr::hooks::{format_idr, use_auth, use_nav, ThemeToggle};
use crate::csr::models::ListEventsRequest;
use crate::csr::services::event::{self as event_svc};
use crate::csr::state::events::ExploreEvent;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn fmt_date(iso: &str) -> String {
    let months = [
        "Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des",
    ];
    if iso.len() >= 10 {
        let parts: Vec<&str> = iso[..10].split('-').collect();
        if parts.len() == 3 {
            let y = parts[0];
            let m: usize = parts[1].parse().unwrap_or(1);
            let d: u32 = parts[2].parse().unwrap_or(1);
            let mon = months.get(m.saturating_sub(1)).unwrap_or(&"");
            return format!("{d} {mon} {y}");
        }
    }
    iso.to_string()
}

// ─── Status badge ─────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum EventStatus {
    OnSale,
    SoldOut,
    Presale,
}

impl EventStatus {
    fn from_ev(ev: &ExploreEvent) -> Self {
        if ev.total_quota > 0 && ev.total_sold >= ev.total_quota {
            Self::SoldOut
        } else if ev.is_live {
            Self::OnSale
        } else {
            Self::Presale
        }
    }
    fn css_mod(&self) -> &'static str {
        match self {
            Self::OnSale => "mhub-event-status mhub-event-status--sale",
            Self::SoldOut => "mhub-event-status mhub-event-status--sold",
            Self::Presale => "mhub-event-status mhub-event-status--presale",
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Self::OnSale => "On Sale",
            Self::SoldOut => "Sold Out",
            Self::Presale => "Presale",
        }
    }
}

// ─── Main component ───────────────────────────────────────────────────────────

#[component]
pub fn MerchantPage() -> impl IntoView {
    let auth = use_auth();
    let navigate = use_nav();

    let merchant_events: RwSignal<Vec<ExploreEvent>> = RwSignal::new(vec![]);
    let events_loading = RwSignal::new(true);

    spawn_local(async move {
        let req = ListEventsRequest {
            category: String::new(),
            query: String::new(),
            page: 1,
            page_size: 50,
        };
        if let Ok(resp) = event_svc::list_mine(&req).await {
            merchant_events.set(
                resp.events
                    .iter()
                    .map(|e| crate::csr::state::events::event_to_explore_pub(e))
                    .collect(),
            );
        }
        events_loading.set(false);
    });

    {
        let nav = navigate.clone();
        Effect::new(move |_| {
            if auth.is_loading.get() {
                return;
            }
            if !auth.is_authenticated() {
                nav("/login", Default::default());
                return;
            }
            let ok = auth.user.with(|u| {
                u.as_ref()
                    .map(|p| p.membership_tier == "MERCHANT")
                    .unwrap_or(false)
            });
            if !ok {
                nav("/", Default::default());
            }
        });
    }

    let active_page = RwSignal::new("tickets");

    // Aggregate stats dari real data
    let total_sold_all =
        move || merchant_events.with(|evs| evs.iter().map(|e| e.total_sold).sum::<i32>());
    let total_quota_all =
        move || merchant_events.with(|evs| evs.iter().map(|e| e.total_quota).sum::<i32>());
    let capacity_pct = move || {
        let q = total_quota_all();
        if q == 0 {
            0u32
        } else {
            ((total_sold_all() as f64 / q as f64) * 100.0).round() as u32
        }
    };

    view! {
        <BottomNav active="merchant" />
        <GridBackground />
        <main class="page merchant-page mhub-mobile">

            // ── Header ────────────────────────────────────────────────────────
            <header class="mhub-header">
                <div class="mhub-header-left">
                    <div class="mhub-header-avatar">
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
                    </div>
                    <span class="mhub-header-title">"Merchant Hub"</span>
                </div>
                <div class="mhub-header-right">
                    <A
                        href="/merchant/scan"
                        attr:class="mhub-scan-btn"
                        attr:aria-label="Scan Tiket"
                    >
                        <svg
                            width="18"
                            height="18"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                        >
                            <polyline points="4 7 4 4 7 4" />
                            <polyline points="20 7 20 4 17 4" />
                            <polyline points="4 17 4 20 7 20" />
                            <polyline points="20 17 20 20 17 20" />
                            <rect x="8" y="8" width="8" height="8" rx="1" />
                        </svg>
                        "SCAN"
                    </A>
                    <ThemeToggle />
                    <A
                        href="/notifications"
                        attr:class="mhub-bell-btn"
                        attr:aria-label="Notifikasi"
                    >
                        <svg
                            width="18"
                            height="18"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                        >
                            <path d="M18 8a6 6 0 10-12 0c0 7-3 9-3 9h18s-3-2-3-9" />
                            <path d="M13.73 21a2 2 0 01-3.46 0" />
                        </svg>
                        <span class="mhub-bell-badge"></span>
                    </A>
                </div>
            </header>

            // ── Stats strip — real data ───────────────────────────────────────
            <div class="mhub-stats-strip">
                <div class="mhub-stat-cell">
                    <span class="mhub-stat-label">"TOTAL TERJUAL"</span>
                    <span class="mhub-stat-value">
                        {move || {
                            let s = total_sold_all();
                            if s == 0 { "—".to_string() } else { format!("{s}") }
                        }}
                    </span>
                    <div class="mhub-stat-capacity-bar">
                        <div
                            class="mhub-stat-capacity-fill"
                            style=move || format!("width:{}%", capacity_pct())
                        ></div>
                    </div>
                    <span class="mhub-stat-label">
                        {move || {
                            let q = total_quota_all();
                            let pct = capacity_pct();
                            if q == 0 { "—".to_string() } else { format!("{pct}% kapasitas") }
                        }}
                    </span>
                </div>
                <div class="mhub-stat-divider"></div>
                <div class="mhub-stat-cell">
                    <span class="mhub-stat-label">"SISA TIKET"</span>
                    <span class="mhub-stat-value">
                        {move || {
                            let r = (total_quota_all() - total_sold_all()).max(0);
                            format!("{r}")
                        }}
                    </span>
                    <span class="mhub-stat-label">
                        {move || {
                            let q = total_quota_all();
                            if q == 0 { "—".to_string() } else { format!("dari {q} kuota") }
                        }}
                    </span>
                </div>
            </div>

            // ── Tab bar ──────────────────────────────────────────────────────
            <div class="mhub-mobile-tabs">
                {[
                    ("tickets", "Tiket"),
                    ("analytics", "Analitik"),
                    ("finance", "Keuangan"),
                    ("settings", "Pengaturan"),
                ]
                    .iter()
                    .map(|(id, label)| {
                        let id = *id;
                        view! {
                            <button
                                class=move || {
                                    if active_page.get() == id {
                                        "mhub-mtab mhub-mtab--active"
                                    } else {
                                        "mhub-mtab"
                                    }
                                }
                                on:click=move |_| active_page.set(id)
                            >
                                {*label}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>

            // ── Content ──────────────────────────────────────────────────────
            {
                let nav_tickets = navigate.clone();
                move || match active_page.get() {
                    "analytics" => view_mobile_analytics(merchant_events).into_any(),
                    "finance" => view_mobile_finance().into_any(),
                    "settings" => view_mobile_settings().into_any(),
                    _ => {
                        view_tickets(merchant_events, events_loading, nav_tickets.clone())
                            .into_any()
                    }
                }
            }

        </main>

        // ── FAB ──────────────────────────────────────────────────────────────
        <button
            class="mhub-fab"
            aria-label="Event baru"
            on:click={
                let nav = navigate.clone();
                move |_| nav("/merchant/events/new", Default::default())
            }
        >
            <svg
                width="22"
                height="22"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="2.5"
                stroke-linecap="round"
            >
                <line x1="12" y1="5" x2="12" y2="19" />
                <line x1="5" y1="12" x2="19" y2="12" />
            </svg>
        </button>
    }
}

// ─── Tickets tab ──────────────────────────────────────────────────────────────

fn view_tickets(
    merchant_events: RwSignal<Vec<ExploreEvent>>,
    events_loading: RwSignal<bool>,
    navigate: impl Fn(&str, leptos_router::NavigateOptions) + Clone + Send + 'static,
) -> impl IntoView {
    view! {
        <section class="mhub-events-section">
            <div class="mhub-events-header">
                <h3 class="mhub-events-title">"Event Saya"</h3>
                <span class="mhub-live-badge">
                    <span class="mhub-live-dot"></span>
                    "Live"
                </span>
            </div>

            {move || {
                if events_loading.get() {
                    return (0..3)
                        .map(|_| view! { <MerchantRowShimmer /> })
                        .collect_view()
                        .into_any();
                }
                let evs = merchant_events.get();
                if evs.is_empty() {
                    view! {
                        <div class="mhub-empty">
                            <div class="mhub-empty-icon-wrap">
                                <svg
                                    width="38"
                                    height="38"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="1.5"
                                    stroke-linecap="round"
                                >
                                    <rect x="1" y="4" width="22" height="16" rx="2" ry="2" />
                                    <line x1="1" y1="10" x2="23" y2="10" />
                                </svg>
                            </div>
                            <p class="mhub-empty-title">"Belum Ada Event"</p>
                            <p class="mhub-empty-body">
                                "Buat event pertamamu dan mulai jual tiket ke audiensmu."
                            </p>
                            <button
                                class="mhub-empty-cta"
                                on:click={
                                    let nav = navigate.clone();
                                    move |_| nav("/merchant/events/new", Default::default())
                                }
                            >
                                <svg
                                    width="16"
                                    height="16"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2.5"
                                    stroke-linecap="round"
                                >
                                    <line x1="12" y1="5" x2="12" y2="19" />
                                    <line x1="5" y1="12" x2="19" y2="12" />
                                </svg>
                                "BUAT EVENT PERTAMA"
                            </button>
                        </div>
                    }
                        .into_any()
                } else {
                    evs.into_iter()
                        .map(|ev| {
                            let cover = if ev.cover.is_empty() {
                                "https://images.unsplash.com/photo-1514525253161-7a46d19cd819?w=600&q=80"
                                    .into()
                            } else {
                                ev.cover.clone()
                            };
                            let status = EventStatus::from_ev(&ev);
                            let title = ev.title.clone();
                            let date = fmt_date(&ev.date);
                            let venue = if ev.city.is_empty() {
                                ev.venue.clone()
                            } else {
                                format!("{} • {}", ev.venue, ev.city)
                            };
                            let price = ev.price;
                            let sold = ev.total_sold;
                            let quota = ev.total_quota;
                            let available = (quota - sold).max(0);
                            let pct = if quota > 0 {
                                ((sold as f64 / quota as f64) * 100.0).round() as u32
                            } else {
                                0
                            };
                            let fill_style = format!("width:{}%", pct);
                            let (val_text, val_cls) = if status == EventStatus::SoldOut {
                                (
                                    "100% Sold Out".to_string(),
                                    "mhub-event-progress-val mhub-event-progress-val--sold",
                                )
                            } else if quota == 0 {
                                ("—".to_string(), "mhub-event-progress-val")
                            } else {
                                (format!("{sold}/{quota} Terjual"), "mhub-event-progress-val")
                            };
                            let remaining_text = if quota == 0 {
                                String::new()
                            } else {
                                format!("{available} sisa")
                            };
                            let fill_cls = match &status {
                                EventStatus::SoldOut => {
                                    "mhub-event-progress-fill mhub-event-progress-fill--sold"
                                }
                                EventStatus::Presale => {
                                    "mhub-event-progress-fill mhub-event-progress-fill--lime"
                                }
                                _ => "mhub-event-progress-fill",
                            };
                            let ev_slug_for_nav = ev.slug.clone();

                            view! {
                                <div class="mhub-event-card">
                                    <div class="mhub-event-card-img-wrap">
                                        <img
                                            src=cover
                                            alt=title.clone()
                                            class="mhub-event-card-img"
                                        />
                                        <span class=status.css_mod()>{status.label()}</span>
                                    </div>
                                    <div class="mhub-event-card-body">
                                        <div class="mhub-event-card-top-row">
                                            <p class="mhub-event-card-title">{title}</p>
                                            <div class="mhub-event-card-price-block">
                                                <span class="mhub-event-price-label">"Mulai dari"</span>
                                                <span class="mhub-event-price-value">
                                                    {format_idr(price)}
                                                </span>
                                            </div>
                                        </div>
                                        <p class="mhub-event-card-meta">{date}" • "{venue}</p>

                                        // ── Sold / sisa progress ──────────────────
                                        <div class="mhub-event-progress-section">
                                            <div class="mhub-event-progress-row">
                                                <span class="mhub-event-progress-key">"Penjualan"</span>
                                                <span class=val_cls>{val_text}</span>
                                            </div>
                                            <div class="mhub-event-progress-bar">
                                                <div class=fill_cls style=fill_style></div>
                                            </div>
                                            {(!remaining_text.is_empty())
                                                .then(|| {
                                                    view! {
                                                        <div class="mhub-event-remaining-row">
                                                            <span class="mhub-event-remaining-badge">
                                                                <svg
                                                                    width="10"
                                                                    height="10"
                                                                    viewBox="0 0 24 24"
                                                                    fill="none"
                                                                    stroke="currentColor"
                                                                    stroke-width="2.5"
                                                                >
                                                                    <circle cx="12" cy="12" r="10" />
                                                                    <line x1="12" y1="8" x2="12" y2="12" />
                                                                    <line x1="12" y1="16" x2="12.01" y2="16" />
                                                                </svg>
                                                                {remaining_text}
                                                            </span>
                                                        </div>
                                                    }
                                                })}
                                        </div>

                                        <div class="mhub-event-card-actions">
                                            <button
                                                class="mhub-event-manage-btn"
                                                on:click={
                                                    let nav = navigate.clone();
                                                    let slug = ev_slug_for_nav.clone();
                                                    move |_| nav(
                                                        &format!("/merchant/events/{}/edit", slug),
                                                        Default::default(),
                                                    )
                                                }
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
                                                    <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" />
                                                    <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
                                                </svg>
                                                "Edit Event"
                                            </button>
                                        </div>
                                    </div>
                                </div>
                            }
                        })
                        .collect_view()
                        .into_any()
                }
            }}
        </section>
    }
}
// ─── Analytics ────────────────────────────────────────────────────────────────

fn view_mobile_analytics(merchant_events: RwSignal<Vec<ExploreEvent>>) -> impl IntoView {
    view! {
        <section class="merchant-stats">
            <div class="merchant-card merchant-velocity" style="margin-bottom:12px">
                <h3 class="merchant-section-title">"Event Terlaris"</h3>
                {move || {
                    let mut evs = merchant_events.get();
                    evs.sort_by(|a, b| b.total_sold.cmp(&a.total_sold));
                    if let Some(top) = evs.first() {
                        let pct = if top.total_quota > 0 {
                            ((top.total_sold as f64 / top.total_quota as f64) * 100.0).round()
                                as u32
                        } else {
                            0
                        };
                        let title = top.title.clone();
                        let sold = top.total_sold;
                        let quota = top.total_quota;
                        view! {
                            <div style="margin-top:10px">
                                <p style="font-size:13px;font-weight:600;margin-bottom:6px">
                                    {title}
                                </p>
                                <div style="display:flex;justify-content:space-between;margin-bottom:4px">
                                    <span class="merchant-label">{format!("{sold} terjual")}</span>
                                    <span class="merchant-label">{format!("{pct}%")}</span>
                                </div>
                                <div style="background:var(--bg-elevated);height:6px;border-radius:3px;overflow:hidden">
                                    <div style=format!(
                                        "width:{}%;background:var(--accent-lime);height:6px;border-radius:3px",
                                        pct,
                                    )></div>
                                </div>
                                <span class="merchant-label" style="margin-top:4px;display:block">
                                    {format!("{quota} total kuota")}
                                </span>
                            </div>
                        }
                            .into_any()
                    } else {
                        view! {
                            <p class="merchant-label" style="margin-top:8px">
                                "Belum ada data."
                            </p>
                        }
                            .into_any()
                    }
                }}
            </div>
            <div class="merchant-tile-row">
                <div class="merchant-tile">
                    <span class="merchant-label">"TOTAL EVENT"</span>
                    <span class="merchant-tile-value">
                        {move || merchant_events.with(|evs| evs.len())}
                    </span>
                </div>
                <div class="merchant-tile merchant-tile--accent">
                    <span class="merchant-label">"EVENT AKTIF"</span>
                    <span class="merchant-tile-value">
                        {move || {
                            merchant_events.with(|evs| evs.iter().filter(|e| e.is_live).count())
                        }}
                    </span>
                </div>
            </div>
        </section>
    }
}

// ─── Finance ──────────────────────────────────────────────────────────────────

fn view_mobile_finance() -> impl IntoView {
    view! {
        <section class="merchant-stats">
            <div class="merchant-card merchant-card--earnings">
                <span class="merchant-label">"SALDO TERSEDIA"</span>
                <h2 class="merchant-amount">{format_idr(2_485_900_000)}</h2>
                <div class="merchant-trend-row">
                    <span class="merchant-trend-meta">"Pending: "{format_idr(340_200_000)}</span>
                    <span class="merchant-trend-meta merchant-trend-meta--right">
                        "Settlement: 15 Nov"
                    </span>
                </div>
            </div>
        </section>
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Tarik Dana"</h3>
            <div class="mhub-form-row" style="margin-top:12px">
                <label class="mhub-form-label">"JUMLAH (RP)"</label>
                <input type="number" class="mhub-form-input" placeholder="cth. 1000000" />
            </div>
            <div class="mhub-form-row" style="margin-top:10px">
                <label class="mhub-form-label">"REKENING"</label>
                <select class="mhub-form-select">
                    <option>"BCA — ****2847"</option>
                </select>
            </div>
            <button class="mhub-modal-submit" style="width:100%;margin-top:14px">
                "AJUKAN PENARIKAN"
            </button>
        </section>
    }
}

// ─── Settings ─────────────────────────────────────────────────────────────────

fn view_mobile_settings() -> impl IntoView {
    view! {
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Profil Bisnis"</h3>
            <div class="mhub-form-row" style="margin-top:12px">
                <label class="mhub-form-label">"NAMA BISNIS"</label>
                <input type="text" class="mhub-form-input" value="Stellar Events Indonesia" />
            </div>
            <div class="mhub-form-row" style="margin-top:10px">
                <label class="mhub-form-label">"EMAIL KONTAK"</label>
                <input type="email" class="mhub-form-input" value="contact@stellar.id" />
            </div>
            <button class="mhub-modal-submit" style="width:100%;margin-top:14px">
                "Simpan Profil"
            </button>
        </section>
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Notifikasi"</h3>
            {[("Penjualan Baru", true), ("Konfirmasi Payout", true), ("Laporan Mingguan", false)]
                .iter()
                .map(|(l, c)| {
                    view! {
                        <div class="mhub-toggle-row">
                            <span class="mhub-toggle-label">{*l}</span>
                            <label class="mhub-toggle-switch">
                                <input type="checkbox" prop:checked=*c />
                                <span class="mhub-toggle-track"></span>
                            </label>
                        </div>
                    }
                })
                .collect_view()}
        </section>
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Keamanan"</h3>
            <div class="mhub-security-actions">
                <button class="mhub-security-btn">"🔒  Ganti Password"</button>
                <button class="mhub-security-btn">
                    "📱  Aktifkan 2FA  "
                    <span class="mhub-security-badge mhub-security-badge--off">"OFF"</span>
                </button>
            </div>
        </section>
    }
}
