use chrono::{DateTime, FixedOffset, Utc};
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::csr::components::BottomNav;
use crate::csr::hooks::{format_idr, ThemeToggle};
use crate::csr::models::tickets::TicketResponse;
use crate::csr::services::ticket as ticket_svc;

#[component]
pub fn TicketDetailPage() -> impl IntoView {
    let params = use_params_map();
    let ticket = RwSignal::new(None::<TicketResponse>);
    let loading = RwSignal::new(true);

    // Fetch saat parameter id berubah
    Effect::new(move |_| {
        let id = params.with(|p| p.get("id").unwrap_or_default());
        if id.is_empty() {
            loading.set(false);
            return;
        }
        loading.set(true);
        spawn_local(async move {
            match ticket_svc::get_ticket(&id).await {
                Ok(data) => ticket.set(Some(data)),
                Err(_) => ticket.set(None),
            }
            loading.set(false);
        });
    });

    // Helper: UTC → WIB (UTC+7)
    let fmt_wib = |dt: &DateTime<Utc>| {
        let wib = dt.with_timezone(&FixedOffset::east_opt(7 * 3600).unwrap());
        let date = wib.format("%d %b %Y").to_string();
        let time = wib.format("%H:%M WIB").to_string();
        (date, time)
    };

    let shimmer = move || {
        view! {
            <div class="page td-page">

                // Hero shimmer
                <div class="shim" style="width:100%;height:260px;border-radius:0"></div>

                <div style="padding:16px;display:flex;flex-direction:column;gap:14px">
                    // Title + badge
                    <div class="shim" style="height:16px;width:90px;border-radius:100px"></div>
                    <div class="shim" style="height:28px;width:85%"></div>
                    <div class="shim" style="height:28px;width:60%"></div>

                    // Stub top
                    <div style="margin-top:6px;display:flex;flex-direction:column;gap:12px">
                        // Row 1: Ticket Ref & Price
                        <div style="display:flex;justify-content:space-between">
                            <div style="display:flex;flex-direction:column;gap:6px;width:45%">
                                <div
                                    class="shim"
                                    style="height:10px;width:60px;border-radius:100px"
                                ></div>
                                <div class="shim" style="height:16px;width:100%"></div>
                            </div>
                            <div style="display:flex;flex-direction:column;gap:6px;width:45%;align-items:flex-end">
                                <div
                                    class="shim"
                                    style="height:10px;width:60px;border-radius:100px"
                                ></div>
                                <div class="shim" style="height:16px;width:80%"></div>
                            </div>
                        </div>
                        // Row 2: Date & Time
                        <div style="display:flex;justify-content:space-between">
                            <div style="display:flex;flex-direction:column;gap:6px;width:45%">
                                <div
                                    class="shim"
                                    style="height:10px;width:40px;border-radius:100px"
                                ></div>
                                <div class="shim" style="height:16px;width:100%"></div>
                            </div>
                            <div style="display:flex;flex-direction:column;gap:6px;width:45%;align-items:flex-end">
                                <div
                                    class="shim"
                                    style="height:10px;width:40px;border-radius:100px"
                                ></div>
                                <div class="shim" style="height:16px;width:80%"></div>
                            </div>
                        </div>
                        // Row 3: Venue
                        <div style="display:flex;flex-direction:column;gap:6px">
                            <div
                                class="shim"
                                style="height:10px;width:50px;border-radius:100px"
                            ></div>
                            <div class="shim" style="height:16px;width:90%"></div>
                        </div>
                    </div>

                    // Divider (perf divider placeholder)
                    <div style="height:1px;background:var(--border-soft);margin:6px 0"></div>

                    // Stub bottom: QR + pills
                    <div style="display:flex;flex-direction:column;align-items:center;gap:14px;padding:8px 0">
                        <div class="shim" style="width:170px;height:170px;border-radius:12px"></div>
                        <div class="shim" style="height:12px;width:130px;border-radius:100px"></div>

                        <div style="display:flex;gap:10px;width:100%">
                            <div style="flex:1;display:flex;flex-direction:column;gap:6px;align-items:center">
                                <div
                                    class="shim"
                                    style="height:10px;width:50px;border-radius:100px"
                                ></div>
                                <div class="shim" style="height:18px;width:80%"></div>
                            </div>
                            <div style="flex:1;display:flex;flex-direction:column;gap:6px;align-items:center">
                                <div
                                    class="shim"
                                    style="height:10px;width:60px;border-radius:100px"
                                ></div>
                                <div class="shim" style="height:18px;width:80%"></div>
                            </div>
                        </div>
                    </div>

                    // Info card
                    <div style="display:flex;flex-direction:column;gap:10px;padding:12px 0">
                        <div style="display:flex;align-items:center;gap:10px">
                            <div
                                class="shim"
                                style="width:16px;height:16px;border-radius:50%"
                            ></div>
                            <div class="shim" style="height:14px;width:75%"></div>
                        </div>
                        <div style="display:flex;align-items:center;gap:10px">
                            <div
                                class="shim"
                                style="width:16px;height:16px;border-radius:50%"
                            ></div>
                            <div class="shim" style="height:14px;width:70%"></div>
                        </div>
                    </div>

                    // Actions
                    <div style="display:flex;gap:10px;margin-top:4px">
                        <div class="shim" style="height:44px;flex:1;border-radius:100px"></div>
                        <div class="shim" style="height:44px;flex:1;border-radius:100px"></div>
                    </div>

                    // Pulse strip
                    <div
                        style="margin-top:4px"
                        class="shim"
                        style="height:36px;width:100%;border-radius:100px"
                    ></div>
                </div>
            </div>
        }
    };

    view! {
        <div class="page td-page">
            // ── Header ──────────────────────────────────────────────────
            <header class="page-header">
                <A href="/tickets" attr:class="back-btn">
                    <svg
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                    >
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </A>
                <span class="td-page-title">"Ticket Details"</span>
                <div class="header-actions">
                    <ThemeToggle />
                    <button class="icon-btn" aria-label="Share">
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
                            <circle cx="18" cy="5" r="3" />
                            <circle cx="6" cy="12" r="3" />
                            <circle cx="18" cy="19" r="3" />
                            <line x1="8.59" y1="13.51" x2="15.42" y2="17.49" />
                            <line x1="15.41" y1="6.51" x2="8.59" y2="10.49" />
                        </svg>
                    </button>
                </div>
            </header>

            // ── Body (reactive) ───────────────────────────────────────
            {move || {
                if loading.get() {
                    return shimmer().into_any();
                }
                let Some(t) = ticket.get() else {
                    return view! {
                        <div class="td-mobile-layout">
                            <div class="td-empty-state">
                                <h2>"Tiket tidak ditemukan"</h2>
                                <p>"Tiket mungkin sudah dihapus atau ID tidak valid."</p>
                                <A href="/tickets" attr:class="td-btn td-btn--primary">
                                    "Kembali ke Tiket Saya"
                                </A>
                            </div>
                        </div>
                    }
                        .into_any();
                };
                let (date_str, time_str) = fmt_wib(&t.event_date);
                let venue = match (&t.event_venue, &t.event_city) {
                    (Some(v), Some(c)) => format!("{}, {}", v, c),
                    (Some(v), None) => v.clone(),
                    (None, Some(c)) => c.clone(),
                    (None, None) => "TBA".to_string(),
                };
                let cover = t
                    .cover_url
                    .unwrap_or_else(|| {
                        "https://images.unsplash.com/photo-1470225620780-dba8ba36b745?w=800&q=80"
                            .to_string()
                    });
                let status_badge = t.status.to_uppercase();
                let qr_ref = format!("TICKET#{}", t.ticket_code);

                view! {
                    <div class="td-mobile-layout">
                        // ── Hero ──────────────────────────────────────
                        <div class="td-hero">
                            <img src=cover alt=t.event_name.clone() class="td-hero-img" />
                            <div class="td-hero-gradient"></div>
                            <div class="td-hero-content">
                                <span class="td-confirmed">{status_badge}</span>
                                <h1 class="td-event-title">{t.event_name.clone()}</h1>
                            </div>
                        </div>

                        // ── Stub ──────────────────────────────────────
                        <div class="td-stub">
                            <div class="td-stub-top">
                                <div class="td-stub-row">
                                    <div class="td-stub-cell">
                                        <span class="td-label">"TICKET REF"</span>
                                        <span class="td-val">{t.ticket_code.clone()}</span>
                                    </div>
                                    <div class="td-stub-cell td-stub-cell--right">
                                        <span class="td-label">"PRICE PAID"</span>
                                        <span class="td-val td-val--accent">
                                            {format_idr(t.unit_price as i64)}
                                        </span>
                                    </div>
                                </div>
                                <div class="td-stub-row">
                                    <div class="td-stub-cell">
                                        <span class="td-label">"DATE"</span>
                                        <span class="td-val">{date_str}</span>
                                    </div>
                                    <div class="td-stub-cell td-stub-cell--right">
                                        <span class="td-label">"TIME"</span>
                                        <span class="td-val">{time_str}</span>
                                    </div>
                                </div>
                                <div class="td-stub-row">
                                    <div class="td-stub-cell td-stub-cell--full">
                                        <span class="td-label">"VENUE"</span>
                                        <span class="td-val">{venue}</span>
                                    </div>
                                </div>
                            </div>

                            <div class="td-perf-divider">
                                <span class="td-notch td-notch--left"></span>
                                <span class="td-dash"></span>
                                <span class="td-notch td-notch--right"></span>
                            </div>

                            <div class="td-stub-bottom">
                                <div class="td-qr-wrap">
                                    <div class="td-qr">{qr_svg(&t.ticket_code)}</div>
                                    <span class="td-qr-ref">{qr_ref}</span>
                                </div>
                                <div class="td-pill-row">
                                    <div class="td-pill">
                                        <span class="td-pill-label">"SECTION"</span>
                                        <span class="td-pill-val">{t.variant_name.clone()}</span>
                                    </div>
                                    // ROW/SEAT tidak tersedia di TicketResponse saat ini
                                    <div class="td-pill">
                                        <span class="td-pill-label">"ROW/SEAT"</span>
                                        <span class="td-pill-val">"-"</span>
                                    </div>
                                </div>
                                <div class="td-info-card">
                                    <div class="td-info-row">
                                        <svg
                                            width="16"
                                            height="16"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="#c8ff5e"
                                            stroke-width="2"
                                            stroke-linecap="round"
                                            stroke-linejoin="round"
                                        >
                                            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
                                        </svg>
                                        <span>"Show this QR at the gate for scanning"</span>
                                    </div>
                                    <div class="td-info-row">
                                        <svg
                                            width="16"
                                            height="16"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="#ffad2b"
                                            stroke-width="2"
                                            stroke-linecap="round"
                                            stroke-linejoin="round"
                                        >
                                            <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
                                            <line x1="12" y1="9" x2="12" y2="13" />
                                            <line x1="12" y1="17" x2="12.01" y2="17" />
                                        </svg>
                                        <span>
                                            "Don't share this code or screenshot with others"
                                        </span>
                                    </div>
                                </div>
                            </div>
                        </div>

                        // ── Actions ───────────────────────────────────
                        <div class="td-actions">
                            <button class="td-btn td-btn--primary">
                                <svg
                                    width="16"
                                    height="16"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                >
                                    <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
                                    <polyline points="7 10 12 15 17 10" />
                                    <line x1="12" y1="15" x2="12" y2="3" />
                                </svg>
                                <span>"Download PDF"</span>
                            </button>
                            <button class="td-btn td-btn--ghost">
                                <svg
                                    width="16"
                                    height="16"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                >
                                    <rect x="3" y="4" width="18" height="18" rx="2" ry="2" />
                                    <line x1="16" y1="2" x2="16" y2="6" />
                                    <line x1="8" y1="2" x2="8" y2="6" />
                                    <line x1="3" y1="10" x2="21" y2="10" />
                                </svg>
                                <span>"Add to Calendar"</span>
                            </button>
                        </div>

                        <div class="td-pulse-strip">
                            <span class="pulse-dot pulse-dot--green"></span>
                            <span>"PULSE ACTIVE: READY FOR ENTRY"</span>
                        </div>
                    </div>
                }
                    .into_any()
            }}

            <BottomNav active="tickets" />
        </div>
    }
}

// ── QR code nyata menggunakan qrcodegen ────────────────────────────────────
// Cargo.toml: qrcodegen = "1.8"
fn qr_svg(code: &str) -> impl IntoView {
    use qrcodegen::{QrCode, QrCodeEcc};
    use std::fmt::Write as FmtWrite;

    // Encode sebagai QR — Medium error correction (tolerate ~15% damage)
    let Ok(qr) = QrCode::encode_text(code, QrCodeEcc::Medium) else {
        // Fallback: kotak error yang terlihat jelas, bukan crash
        return view! {
            <div style="width:170px;height:170px;background:white;border-radius:10px;\
            display:flex;align-items:center;justify-content:center;flex-direction:column;gap:6px">
                <svg
                    width="28"
                    height="28"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="#ff6b6b"
                    stroke-width="2"
                    stroke-linecap="round"
                >
                    <circle cx="12" cy="12" r="10" />
                    <line x1="12" y1="8" x2="12" y2="12" />
                    <line x1="12" y1="16" x2="12.01" y2="16" />
                </svg>
                <span style="font-size:9px;color:#999;letter-spacing:0.1em">"QR UNAVAILABLE"</span>
            </div>
        }.into_any();
    };

    // Ukuran modul dan quiet-zone (spec: min 4 modul; 3 cukup untuk layar)
    let modules = qr.size() as usize; // e.g. 25 untuk version 2
    let quiet   = 3usize;             // border dalam modul
    let px      = 5usize;             // piksel per modul — 5px × 25 modul = 125px + border
    let total   = (modules + 2 * quiet) * px;

    // Build string rect satu kali — lebih cepat dari Vec<view!>
    let mut rects = String::with_capacity(modules * modules * 55);
    for y in 0..modules {
        for x in 0..modules {
            if qr.get_module(x as i32, y as i32) {
                let rx = (x + quiet) * px;
                let ry = (y + quiet) * px;
                let _ = write!(
                    rects,
                    "<rect x=\"{rx}\" y=\"{ry}\" width=\"{px}\" height=\"{px}\" fill=\"#0d0d1a\"/>"
                );
            }
        }
    }

    // SVG lengkap: background putih + semua modul gelap
    let svg = format!(
        "<svg width=\"{t}\" height=\"{t}\" viewBox=\"0 0 {t} {t}\" \
              xmlns=\"http://www.w3.org/2000/svg\" shape-rendering=\"crispEdges\">\
           <rect width=\"{t}\" height=\"{t}\" fill=\"white\"/>{rects}</svg>",
        t = total,
    );

    // Leptos: inject SVG string langsung — tidak ada overhead per-rect reactive
    view! { <div inner_html=svg /> }.into_any()
}