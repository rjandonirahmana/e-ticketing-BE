//! merchant.rs — Halaman Merchant Hub.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::get_merchant_events;
use crate::web::app::AuthResource;
use crate::web::components::{BottomNav, MerchantEventCardShimmer, ThemeToggle};
use crate::web::models::{format_date, format_price, Event, PaginatedEvents};

// ─── Status badge ─────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum EventStatus {
    OnSale,
    SoldOut,
    Presale,
}

impl EventStatus {
    fn from_event(e: &Event) -> Self {
        if e.total_quota > 0 && e.total_sold >= e.total_quota {
            Self::SoldOut
        } else if e.status == "active" {
            Self::OnSale
        } else {
            Self::Presale
        }
    }
    fn css_mod(&self) -> &'static str {
        match self {
            Self::OnSale  => "mhub-event-status mhub-event-status--sale",
            Self::SoldOut => "mhub-event-status mhub-event-status--sold",
            Self::Presale => "mhub-event-status mhub-event-status--presale",
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Self::OnSale  => "On Sale",
            Self::SoldOut => "Sold Out",
            Self::Presale => "Presale",
        }
    }
}

// ─── Component ────────────────────────────────────────────────────────────────

#[component]
pub fn MerchantPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");

    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let events = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in {
                get_merchant_events(Some(1)).await
            } else {
                Ok(PaginatedEvents {
                    data: vec![],
                    total: 0,
                    page: 1,
                    per_page: 20,
                    total_pages: 0,
                })
            }
        },
    );

    let active_page = RwSignal::new("tickets");

    let evs_list = move || {
        events
            .get()
            .and_then(|r| r.ok())
            .map(|pg| pg.data)
            .unwrap_or_default()
    };

    let total_sold  = move || evs_list().iter().map(|e| e.total_sold).sum::<i32>();
    let total_quota = move || evs_list().iter().map(|e| e.total_quota).sum::<i32>();
    let capacity_pct = move || {
        let q = total_quota();
        if q == 0 { 0u32 } else { ((total_sold() as f64 / q as f64) * 100.0).round() as u32 }
    };

    view! {
        <div class="page merchant-page mhub-mobile">

            // ── Header ────────────────────────────────────────────────────────
            <header class="mhub-header">
                <div class="mhub-header-left">
                    <div class="mhub-header-avatar">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/>
                            <circle cx="12" cy="7" r="4"/>
                        </svg>
                    </div>
                    <span class="mhub-header-title">"Merchant Hub"</span>
                </div>
                <div class="mhub-header-right">
                    <A href="/merchant/live" attr:class="mhub-live-btn" attr:aria-label="Go Live">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <polygon points="5 3 19 12 5 21 5 3"/>
                        </svg>
                        "LIVE"
                    </A>
                    <A href="/meet/host" attr:class="mhub-meet-btn" attr:aria-label="Mulai Meet">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <polygon points="23 7 16 12 23 17 23 7"/>
                            <rect x="1" y="5" width="15" height="14" rx="2" ry="2"/>
                        </svg>
                        "MEET"
                    </A>
                    <A href="/scan" attr:class="mhub-scan-btn" attr:aria-label="Scan Tiket">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <polyline points="4 7 4 4 7 4"/>
                            <polyline points="20 7 20 4 17 4"/>
                            <polyline points="4 17 4 20 7 20"/>
                            <polyline points="20 17 20 20 17 20"/>
                            <rect x="8" y="8" width="8" height="8" rx="1"/>
                        </svg>
                        "SCAN"
                    </A>
                    <ThemeToggle />
                    <A href="/notifications" attr:class="mhub-bell-btn" attr:aria-label="Notifikasi">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M18 8a6 6 0 10-12 0c0 7-3 9-3 9h18s-3-2-3-9"/>
                            <path d="M13.73 21a2 2 0 01-3.46 0"/>
                        </svg>
                        <span class="mhub-bell-badge"></span>
                    </A>
                </div>
            </header>

            // ── Stats strip ───────────────────────────────────────────────────
            <div class="mhub-stats-strip">
                <div class="mhub-stat-cell">
                    <span class="mhub-stat-label">"TOTAL TERJUAL"</span>
                    <span class="mhub-stat-value">
                        {move || {
                            let s = total_sold();
                            if s == 0 { "—".to_string() } else { format!("{s}") }
                        }}
                    </span>
                    <div class="mhub-stat-capacity-bar">
                        <div class="mhub-stat-capacity-fill"
                             style=move || format!("width:{}%", capacity_pct())>
                        </div>
                    </div>
                    <span class="mhub-stat-label">
                        {move || {
                            let q = total_quota();
                            let pct = capacity_pct();
                            if q == 0 { "—".to_string() } else { format!("{pct}% kapasitas") }
                        }}
                    </span>
                </div>
                <div class="mhub-stat-divider"></div>
                <div class="mhub-stat-cell">
                    <span class="mhub-stat-label">"SISA TIKET"</span>
                    <span class="mhub-stat-value">
                        {move || format!("{}", (total_quota() - total_sold()).max(0))}
                    </span>
                    <span class="mhub-stat-label">
                        {move || {
                            let q = total_quota();
                            if q == 0 { "—".to_string() } else { format!("dari {q} kuota") }
                        }}
                    </span>
                </div>
            </div>

            // ── Tab bar ───────────────────────────────────────────────────────
            <div class="mhub-mobile-tabs">
                {[
                    ("tickets",  "Tiket"),
                    ("analytics","Analitik"),
                    ("finance",  "Keuangan"),
                    ("settings", "Pengaturan"),
                ]
                .iter()
                .map(|(id, label)| {
                    let id = *id;
                    view! {
                        <button
                            class=move || if active_page.get() == id {
                                "mhub-mtab mhub-mtab--active"
                            } else {
                                "mhub-mtab"
                            }
                            on:click=move |_| active_page.set(id)>
                            {*label}
                        </button>
                    }
                })
                .collect_view()}
            </div>

            // ── Content ───────────────────────────────────────────────────────
            <Suspense fallback=move || {
                (0..3).map(|_| view! { <MerchantEventCardShimmer /> }).collect_view()
            }>
                {move || {
                    let evs = evs_list();
                    match active_page.get() {
                        "analytics" => view_analytics(evs).into_any(),
                        "finance"   => view_finance().into_any(),
                        "settings"  => view_settings().into_any(),
                        _           => view_tickets(evs).into_any(),
                    }
                }}
            </Suspense>

        </div>
        <BottomNav active="merchant" />

        // ── FAB ───────────────────────────────────────────────────────────────
        <A href="/merchant/events/create" attr:class="mhub-fab" attr:aria-label="Event baru">
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none"
                 stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                <line x1="12" y1="5" x2="12" y2="19"/>
                <line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
        </A>
    }
}

// ─── Tickets tab ──────────────────────────────────────────────────────────────

fn view_tickets(evs: Vec<Event>) -> impl IntoView {
    view! {
        <section class="mhub-events-section">
            <div class="mhub-events-header">
                <h3 class="mhub-events-title">"Event Saya"</h3>
                <span class="mhub-live-badge">
                    <span class="mhub-live-dot"></span>
                    "Live"
                </span>
            </div>
            {if evs.is_empty() {
                view! {
                    <div class="mhub-empty">
                        <div class="mhub-empty-icon-wrap">
                            <svg width="38" height="38" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                <rect x="1" y="4" width="22" height="16" rx="2" ry="2"/>
                                <line x1="1" y1="10" x2="23" y2="10"/>
                            </svg>
                        </div>
                        <p class="mhub-empty-title">"Belum Ada Event"</p>
                        <p class="mhub-empty-body">
                            "Buat event pertamamu dan mulai jual tiket ke audiensmu."
                        </p>
                        <A href="/merchant/events/create" attr:class="mhub-empty-cta">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                <line x1="12" y1="5" x2="12" y2="19"/>
                                <line x1="5" y1="12" x2="19" y2="12"/>
                            </svg>
                            "BUAT EVENT PERTAMA"
                        </A>
                    </div>
                }
                .into_any()
            } else {
                evs.into_iter()
                    .map(|ev| {
                        let cover = ev
                            .cover_url
                            .as_deref()
                            .filter(|s| !s.is_empty())
                            .unwrap_or(
                                "https://images.unsplash.com/photo-1514525253161-7a46d19cd819?w=600&q=80",
                            )
                            .to_string();
                        let status = EventStatus::from_event(&ev);
                        let title  = ev.name.clone();
                        let date   = format_date(&ev.event_date);
                        let venue_str = match (ev.venue.as_deref(), ev.city.as_deref()) {
                            (Some(v), Some(c)) if !c.is_empty() => format!("{v} • {c}"),
                            (Some(v), _) => v.to_string(),
                            _ => String::new(),
                        };
                        let sold  = ev.total_sold;
                        let quota = ev.total_quota;
                        let avail = (quota - sold).max(0);
                        let pct   = if quota > 0 {
                            ((sold as f64 / quota as f64) * 100.0).round() as u32
                        } else { 0 };
                        let fill_style = format!("width:{pct}%");
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
                        let remaining_text =
                            if quota == 0 { String::new() } else { format!("{avail} sisa") };
                        let fill_cls = match &status {
                            EventStatus::SoldOut => {
                                "mhub-event-progress-fill mhub-event-progress-fill--sold"
                            }
                            EventStatus::Presale => {
                                "mhub-event-progress-fill mhub-event-progress-fill--lime"
                            }
                            _ => "mhub-event-progress-fill",
                        };
                        let price     = format_price(ev.display_price);
                        let slug      = ev.slug.clone();
                        let status_css = status.css_mod();
                        let status_lbl = status.label();

                        view! {
                            <div class="mhub-event-card">
                                <div class="mhub-event-card-img-wrap">
                                    <img src=cover alt=title.clone() class="mhub-event-card-img"/>
                                    <span class=status_css>{status_lbl}</span>
                                </div>
                                <div class="mhub-event-card-body">
                                    <div class="mhub-event-card-top-row">
                                        <p class="mhub-event-card-title">{title}</p>
                                        <div class="mhub-event-card-price-block">
                                            <span class="mhub-event-price-label">"Mulai dari"</span>
                                            <span class="mhub-event-price-value">{price}</span>
                                        </div>
                                    </div>
                                    <p class="mhub-event-card-meta">{date}" • "{venue_str}</p>

                                    <div class="mhub-event-progress-section">
                                        <div class="mhub-event-progress-row">
                                            <span class="mhub-event-progress-key">"Penjualan"</span>
                                            <span class=val_cls>{val_text}</span>
                                        </div>
                                        <div class="mhub-event-progress-bar">
                                            <div class=fill_cls style=fill_style></div>
                                        </div>
                                        {(!remaining_text.is_empty()).then(|| {
                                            view! {
                                                <div class="mhub-event-remaining-row">
                                                    <span class="mhub-event-remaining-badge">
                                                        <svg width="10" height="10" viewBox="0 0 24 24"
                                                             fill="none" stroke="currentColor" stroke-width="2.5">
                                                            <circle cx="12" cy="12" r="10"/>
                                                            <line x1="12" y1="8" x2="12" y2="12"/>
                                                            <line x1="12" y1="16" x2="12.01" y2="16"/>
                                                        </svg>
                                                        {remaining_text}
                                                    </span>
                                                </div>
                                            }
                                        })}
                                    </div>

                                    <div class="mhub-event-card-actions">
                                        <A
                                            href=format!("/merchant/events/{slug}/edit")
                                            attr:class="mhub-event-manage-btn">
                                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                                 stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                                <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/>
                                                <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/>
                                            </svg>
                                            "Edit Event"
                                        </A>
                                    </div>
                                </div>
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </section>
    }
}

// ─── Analytics tab ────────────────────────────────────────────────────────────

fn view_analytics(evs: Vec<Event>) -> impl IntoView {
    let total        = evs.len();
    let active_count = evs.iter().filter(|e| e.status == "active").count();
    let top          = evs.iter().max_by_key(|e| e.total_sold).cloned();

    view! {
        <section class="merchant-stats">
            <div class="merchant-card merchant-velocity" style="margin-bottom:12px">
                <h3 class="merchant-section-title">"Event Terlaris"</h3>
                {if let Some(t) = top {
                    let pct = if t.total_quota > 0 {
                        ((t.total_sold as f64 / t.total_quota as f64) * 100.0).round() as u32
                    } else { 0 };
                    let title = t.name.clone();
                    let sold  = t.total_sold;
                    let quota = t.total_quota;
                    view! {
                        <div style="margin-top:10px">
                            <p style="font-size:13px;font-weight:600;margin-bottom:6px">{title}</p>
                            <div style="display:flex;justify-content:space-between;margin-bottom:4px">
                                <span class="merchant-label">{format!("{sold} terjual")}</span>
                                <span class="merchant-label">{format!("{pct}%")}</span>
                            </div>
                            <div style="background:var(--bg-elevated);height:6px;border-radius:3px;overflow:hidden">
                                <div style=format!(
                                    "width:{pct}%;background:var(--accent-lime);height:6px;border-radius:3px"
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
                        <p class="merchant-label" style="margin-top:8px">"Belum ada data."</p>
                    }
                    .into_any()
                }}
            </div>
            <div class="merchant-tile-row">
                <div class="merchant-tile">
                    <span class="merchant-label">"TOTAL EVENT"</span>
                    <span class="merchant-tile-value">{total}</span>
                </div>
                <div class="merchant-tile merchant-tile--accent">
                    <span class="merchant-label">"EVENT AKTIF"</span>
                    <span class="merchant-tile-value">{active_count}</span>
                </div>
            </div>
        </section>
    }
}

// ─── Finance tab ──────────────────────────────────────────────────────────────

fn view_finance() -> impl IntoView {
    view! {
        <section class="merchant-stats">
            <div class="merchant-card merchant-card--earnings">
                <span class="merchant-label">"SALDO TERSEDIA"</span>
                <h2 class="merchant-amount">"Rp 2.485.900.000"</h2>
                <div class="merchant-trend-row">
                    <span class="merchant-trend-meta">"Pending: Rp 340.200.000"</span>
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
                <input type="number" class="mhub-form-input" placeholder="cth. 1000000"/>
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

// ─── Settings tab ─────────────────────────────────────────────────────────────

fn view_settings() -> impl IntoView {
    view! {
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Profil Bisnis"</h3>
            <div class="mhub-form-row" style="margin-top:12px">
                <label class="mhub-form-label">"NAMA BISNIS"</label>
                <input type="text" class="mhub-form-input" value="Stellar Events Indonesia"/>
            </div>
            <div class="mhub-form-row" style="margin-top:10px">
                <label class="mhub-form-label">"EMAIL KONTAK"</label>
                <input type="email" class="mhub-form-input" value="contact@stellar.id"/>
            </div>
            <button class="mhub-modal-submit" style="width:100%;margin-top:14px">
                "Simpan Profil"
            </button>
        </section>
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Notifikasi"</h3>
            {[
                ("Penjualan Baru", true),
                ("Konfirmasi Payout", true),
                ("Laporan Mingguan", false),
            ]
            .iter()
            .map(|(l, c)| {
                view! {
                    <div class="mhub-toggle-row">
                        <span class="mhub-toggle-label">{*l}</span>
                        <label class="mhub-toggle-switch">
                            <input type="checkbox" prop:checked=*c/>
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
