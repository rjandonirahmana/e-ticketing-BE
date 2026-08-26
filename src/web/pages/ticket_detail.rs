//! ticket_detail.rs — Halaman Detail Tiket (unified SSR + hydration).
//!
//! Port parity dari `csr/pages/ticket_detail.rs`:
//!   - `spawn_local` + `ticket_svc::get_ticket` → `Resource::new(.., get_ticket_detail)`
//!     sehingga detail + QR ter-render saat SSR (bisa di-screenshot tanpa nunggu JS).
//!   - **QR code asli via `qrcodegen`** (sama persis dengan CSR), bukan QR dekoratif.
//!   - Layout `td-*` (hero, stub, perf-divider, QR, pills, info card, actions,
//!     pulse strip) dipertahankan identik.

use chrono::{DateTime, FixedOffset, Utc};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::web::hooks::ThemeToggle;
use crate::web::utils::format_idr;
use crate::web::api::get_ticket_detail;
use crate::web::app::AuthResource;
use crate::web::components::BottomNav;

// ── Mask ticket code untuk story: tampilkan 4 char depan + 4 char belakang,
// ── sisanya disensor. ID tiket asli tidak pernah dikirim ke flow story.
fn mask_ticket_code(code: &str) -> String {
    let chars: Vec<char> = code.chars().collect();
    let n = chars.len();
    if n <= 8 {
        return "•".repeat(n.max(4));
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[n - 4..].iter().collect();
    format!("{head}••••{tail}")
}

// ── Shimmer skeleton (mirrors td-* layout, staggered delay top→bottom) ────────

#[component]
fn TicketDetailSkeleton() -> impl IntoView {
    view! {
        <div class="td-mobile-layout">
            // Hero — full-width cover placeholder
            <div class="td-hero">
                <div class="shimmer-bg"
                    style="position:absolute;inset:0;border-radius:0;animation-delay:0s;"></div>
                // Overlaid content at bottom of hero
                <div class="td-hero-content">
                    <div class="shimmer-bg"
                        style="width:72px;height:20px;border-radius:100px;animation-delay:0.06s;"></div>
                    <div class="shimmer-bg"
                        style="width:70%;height:30px;border-radius:8px;margin-top:8px;animation-delay:0.1s;"></div>
                </div>
            </div>

            // Stub card
            <div class="td-stub">
                <div class="td-stub-top">
                    // Row 1 — TICKET REF / PRICE PAID
                    <div class="td-stub-row">
                        <div class="td-stub-cell">
                            <div class="shimmer-bg" style="width:58px;height:8px;border-radius:3px;animation-delay:0.14s;"></div>
                            <div class="shimmer-bg" style="width:95px;height:15px;border-radius:4px;margin-top:5px;animation-delay:0.16s;"></div>
                        </div>
                        <div class="td-stub-cell td-stub-cell--right">
                            <div class="shimmer-bg" style="width:52px;height:8px;border-radius:3px;animation-delay:0.18s;"></div>
                            <div class="shimmer-bg" style="width:80px;height:15px;border-radius:4px;margin-top:5px;animation-delay:0.2s;"></div>
                        </div>
                    </div>
                    // Row 2 — DATE / TIME
                    <div class="td-stub-row">
                        <div class="td-stub-cell">
                            <div class="shimmer-bg" style="width:36px;height:8px;border-radius:3px;animation-delay:0.22s;"></div>
                            <div class="shimmer-bg" style="width:78px;height:15px;border-radius:4px;margin-top:5px;animation-delay:0.24s;"></div>
                        </div>
                        <div class="td-stub-cell td-stub-cell--right">
                            <div class="shimmer-bg" style="width:38px;height:8px;border-radius:3px;animation-delay:0.26s;"></div>
                            <div class="shimmer-bg" style="width:76px;height:15px;border-radius:4px;margin-top:5px;animation-delay:0.28s;"></div>
                        </div>
                    </div>
                    // Row 3 — VENUE (full width)
                    <div class="td-stub-row">
                        <div class="td-stub-cell td-stub-cell--full">
                            <div class="shimmer-bg" style="width:42px;height:8px;border-radius:3px;animation-delay:0.3s;"></div>
                            <div class="shimmer-bg" style="width:100%;height:15px;border-radius:4px;margin-top:5px;animation-delay:0.32s;"></div>
                        </div>
                    </div>
                </div>

                // Perforated divider (keep real chrome — it has no data)
                <div class="td-perf-divider">
                    <span class="td-notch td-notch--left"></span>
                    <span class="td-dash"></span>
                    <span class="td-notch td-notch--right"></span>
                </div>

                // Stub bottom — QR + pills + info
                <div class="td-stub-bottom">
                    <div class="td-qr-wrap">
                        <div class="shimmer-bg"
                            style="width:170px;height:170px;border-radius:8px;animation-delay:0.36s;"></div>
                        <div class="shimmer-bg"
                            style="width:115px;height:9px;border-radius:4px;margin-top:10px;animation-delay:0.38s;"></div>
                    </div>
                    <div class="td-pill-row">
                        <div class="td-pill">
                            <div class="shimmer-bg" style="width:52px;height:8px;border-radius:3px;animation-delay:0.41s;"></div>
                            <div class="shimmer-bg" style="width:68px;height:14px;border-radius:4px;margin-top:4px;animation-delay:0.43s;"></div>
                        </div>
                        <div class="td-pill">
                            <div class="shimmer-bg" style="width:58px;height:8px;border-radius:3px;animation-delay:0.45s;"></div>
                            <div class="shimmer-bg" style="width:28px;height:14px;border-radius:4px;margin-top:4px;animation-delay:0.47s;"></div>
                        </div>
                    </div>
                    <div class="td-info-card">
                        <div class="shimmer-bg" style="width:100%;height:34px;border-radius:8px;animation-delay:0.5s;"></div>
                        <div class="shimmer-bg" style="width:100%;height:34px;border-radius:8px;margin-top:8px;animation-delay:0.53s;"></div>
                    </div>
                </div>
            </div>

            // Action buttons
            <div class="td-actions">
                <div class="shimmer-bg" style="flex:1;height:46px;border-radius:100px;animation-delay:0.57s;"></div>
                <div class="shimmer-bg" style="flex:1;height:46px;border-radius:100px;animation-delay:0.61s;"></div>
            </div>

            // Pulse strip
            <div class="td-pulse-strip">
                <div class="shimmer-bg" style="width:62%;height:12px;border-radius:4px;animation-delay:0.66s;"></div>
            </div>
        </div>
    }
}

// ── Helper: UTC → WIB (UTC+7) ─────────────────────────────────────────────────
fn fmt_wib(dt: &DateTime<Utc>) -> (String, String) {
    let wib = dt.with_timezone(&FixedOffset::east_opt(7 * 3600).unwrap());
    (
        wib.format("%d %b %Y").to_string(),
        wib.format("%H:%M WIB").to_string(),
    )
}

#[component]
pub fn TicketDetailPage() -> impl IntoView {
    let params = use_params_map();
    let ticket_id = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();
    let navigate = use_navigate();

    // Penggerbang `is_logged_in()` DIBUANG dari sumber resource.
    //
    // Menebak status login di klien sebelum memanggil server tak menjaga apa
    // pun — server function-nya sudah menuntut sesi sendiri — tapi ia menambah
    // satu cara untuk gagal: selama `AuthResource` belum terbaca, fetcher
    // menjawab `not_ready`, dan `not_ready` dirender sebagai skeleton yang sama
    // persis dengan "sedang memuat". Halaman lalu berkedip tanpa akhir, tanpa
    // pesan, tanpa percobaan ulang — dan dari sisi pengguna itu terbaca sebagai
    // "klik tombol, halaman tidak pindah". Muat ulang menolong hanya karena
    // jalur SSR membaca sesi dari cookie di server, jauh dari masalah ini.
    //
    // Sekarang satu-satunya syarat adalah id-nya ada.
    let ticket = Resource::new(
        ticket_id,
        |id| async move {
            if id.is_empty() {
                return Err(ServerFnError::ServerError("not_ready".into()));
            }
            get_ticket_detail(id).await
        },
    );

    view! {
        <div class="page td-page">
            <header class="page-header">
                <A href="/tickets" attr:class="back-btn">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                        stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </A>
                <span class="td-page-title">"Ticket Details"</span>
                <div class="header-actions">
                    <ThemeToggle />
                </div>
            </header>

            <Suspense fallback=|| view! { <TicketDetailSkeleton/> }>
                {move || {
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <div class="td-mobile-layout">
                                <div class="td-empty-state">
                                    <h2>"Harus masuk"</h2>
                                    <p>"Kamu harus masuk untuk melihat tiket."</p>
                                    <A href="/login" attr:class="td-btn td-btn--primary">"Masuk"</A>
                                </div>
                            </div>
                        }
                            .into_any();
                    }

                    ticket
                        .get()
                        .map(|res| {
                            match res {
                                Err(e) if e.to_string().contains("not_ready") => {
                                    view! { <div /> }.into_any()
                                }
                                Err(_) => {
                                    view! {
                                        <div class="td-mobile-layout">
                                            <div class="td-empty-state">
                                                <h2>"Tiket tidak ditemukan"</h2>
                                                <p>
                                                    "Tiket mungkin sudah dihapus atau ID tidak valid."
                                                </p>
                                                <A
                                                    href="/tickets"
                                                    attr:class="td-btn td-btn--primary"
                                                >
                                                    "Kembali ke Tiket Saya"
                                                </A>
                                            </div>
                                        </div>
                                    }
                                        .into_any()
                                }
                                Ok(t) => {
                                    let (date_str, time_str) = fmt_wib(&t.event_date);
                                    let venue = match (&t.event_venue, &t.event_city) {
                                        (Some(v), Some(c)) => format!("{}, {}", v, c),
                                        (Some(v), None) => v.clone(),
                                        (None, Some(c)) => c.clone(),
                                        (None, None) => "TBA".to_string(),
                                    };
                                    let cover = t
                                        .cover_url
                                        .clone()
                                        .unwrap_or_else(|| {
                                            "https://images.unsplash.com/photo-1470225620780-dba8ba36b745?w=800&q=80"
                                                .to_string()
                                        });
                                    let status_badge = t.status.to_uppercase();
                                    let qr_ref = format!("TICKET#{}", t.ticket_code);

                                    // ── Share to story: ticket_code ASLI tidak pernah ikut —
                                    // hanya versi tersensor yang dikirim via query param.
                                    let _masked_code    = mask_ticket_code(&t.ticket_code);
                                    let _share_title    = t.event_name.clone();
                                    let _share_cover    = cover.clone();
                                    let _share_product_id = t.event_id.clone();
                                    let _share_slug     = t.event_slug.clone();
                                    let _share_date     = date_str.clone();
                                    let _share_venue    = venue.clone();
                                    let _share_tier     = t.variant_name.clone();
                                    let _share_price_str = if t.unit_price == 0.0 {
                                        "Gratis".to_string()
                                    } else {
                                        format_idr(t.unit_price as i64)
                                    };
                                    let _nav_share = navigate.clone();
                                    let share_to_story = move |_: web_sys::MouseEvent| {
                                        #[cfg(target_arch = "wasm32")]
                                        {
                                            let params = web_sys::UrlSearchParams::new()
                                                .expect("UrlSearchParams");
                                            params.append("event_id",    &_share_product_id);
                                            // Kirim slug agar story yang dipublish dari tiket tetap
                                            // tertaut ke halaman product (viewer bisa tap-through).
                                            params.append("event_slug",  &_share_slug);
                                            params.append("event_title", &_share_title);
                                            params.append("event_cover", &_share_cover);
                                            params.append("event_date",  &_share_date);
                                            params.append("event_venue", &_share_venue);
                                            params.append("product_price", &format!("{} · {}", _share_tier, _share_price_str));
                                            params.append("is_ticket",   "1");
                                            params.append("ticket_ref",  &_masked_code);
                                            if let Some(win) = web_sys::window() {
                                                if let Ok(Some(storage)) = win.session_storage() {
                                                    let _ = storage.set_item("story_hero_transition", "product");
                                                    let _ = storage.set_item("story_hero_cover", &_share_cover);
                                                }
                                            }
                                            let qs = params.to_string();
                                            _nav_share(&format!("/story?{}", qs), Default::default());
                                        }
                                    };

                                    view! {
                                        <div class="td-mobile-layout">
                                            // ── Hero ──────────────────────────
                                            <div class="td-hero">
                                                <img
                                                    src=cover
                                                    alt=t.event_name.clone()
                                                    class="td-hero-img"
                                                />
                                                <div class="td-hero-gradient"></div>
                                                <div class="td-hero-content">
                                                    <span class="td-confirmed">{status_badge}</span>
                                                    <h1 class="td-product-title">
                                                        {t.event_name.clone()}
                                                    </h1>
                                                </div>
                                            </div>

                                            // ── Stub ──────────────────────────
                                            <div class="td-stub">
                                                <div class="td-stub-top">
                                                    <div class="td-stub-row">
                                                        <div class="td-stub-cell">
                                                            <span class="td-label">"TICKET REF"</span>
                                                            <span class="td-val">
                                                                {t.ticket_code.clone()}
                                                            </span>
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
                                                            <span class="td-pill-val">
                                                                {t.variant_name.clone()}
                                                            </span>
                                                        </div>
                                                        <div class="td-pill">
                                                            <span class="td-pill-label">"ROW/SEAT"</span>
                                                            <span class="td-pill-val">"-"</span>
                                                        </div>
                                                    </div>
                                                    <div class="td-info-card">
                                                        <div class="td-info-row">
                                                            <svg width="16" height="16" viewBox="0 0 24 24"
                                                                fill="none" stroke="#c8ff5e" stroke-width="2"
                                                                stroke-linecap="round" stroke-linejoin="round">
                                                                <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
                                                            </svg>
                                                            <span>"Show this QR at the gate for scanning"</span>
                                                        </div>
                                                        <div class="td-info-row">
                                                            <svg width="16" height="16" viewBox="0 0 24 24"
                                                                fill="none" stroke="#ffad2b" stroke-width="2"
                                                                stroke-linecap="round" stroke-linejoin="round">
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

                                            // ── Actions ───────────────────────
                                            <div class="td-actions">
                                                <button class="td-btn td-btn--primary">
                                                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                                        stroke="currentColor" stroke-width="2"
                                                        stroke-linecap="round" stroke-linejoin="round">
                                                        <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
                                                        <polyline points="7 10 12 15 17 10" />
                                                        <line x1="12" y1="15" x2="12" y2="3" />
                                                    </svg>
                                                    <span>"Download PDF"</span>
                                                </button>
                                                <button class="td-btn td-btn--ghost">
                                                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                                        stroke="currentColor" stroke-width="2"
                                                        stroke-linecap="round" stroke-linejoin="round">
                                                        <rect x="3" y="4" width="18" height="18" rx="2" ry="2" />
                                                        <line x1="16" y1="2" x2="16" y2="6" />
                                                        <line x1="8" y1="2" x2="8" y2="6" />
                                                        <line x1="3" y1="10" x2="21" y2="10" />
                                                    </svg>
                                                    <span>"Add to Calendar"</span>
                                                </button>
                                                <button class="td-btn td-btn--ghost" on:click=share_to_story>
                                                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                                        stroke="currentColor" stroke-width="2"
                                                        stroke-linecap="round" stroke-linejoin="round">
                                                        <circle cx="18" cy="5" r="3" />
                                                        <circle cx="6" cy="12" r="3" />
                                                        <circle cx="18" cy="19" r="3" />
                                                        <line x1="8.59" y1="13.51" x2="15.42" y2="17.49" />
                                                        <line x1="15.41" y1="6.51" x2="8.59" y2="10.49" />
                                                    </svg>
                                                    <span>"Share to Story"</span>
                                                </button>
                                            </div>

                                            <div class="td-pulse-strip">
                                                <span class="pulse-dot pulse-dot--green"></span>
                                                <span>"PULSE ACTIVE: READY FOR ENTRY"</span>
                                            </div>
                                        </div>
                                    }
                                        .into_any()
                                }
                            }
                        })
                        .unwrap_or_else(|| view! { <div /> }.into_any())
                }}
            </Suspense>

            <BottomNav active="tickets" />
        </div>
    }
}

// ── QR code asli via qrcodegen (identik dengan CSR) ───────────────────────────
// Cargo.toml: qrcodegen = "1.8"
fn qr_svg(code: &str) -> impl IntoView {
    use qrcodegen::{QrCode, QrCodeEcc};
    use std::fmt::Write as FmtWrite;

    let Ok(qr) = QrCode::encode_text(code, QrCodeEcc::Medium) else {
        return view! {
            <div style="width:170px;height:170px;background:white;border-radius:10px;display:flex;align-items:center;justify-content:center;flex-direction:column;gap:6px">
                <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="#ff6b6b"
                    stroke-width="2" stroke-linecap="round">
                    <circle cx="12" cy="12" r="10" />
                    <line x1="12" y1="8" x2="12" y2="12" />
                    <line x1="12" y1="16" x2="12.01" y2="16" />
                </svg>
                <span style="font-size:9px;color:#999;letter-spacing:0.1em">"QR UNAVAILABLE"</span>
            </div>
        }
            .into_any();
    };

    let modules = qr.size() as usize;
    let quiet = 3usize;
    let px = 5usize;
    let total = (modules + 2 * quiet) * px;

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

    let svg = format!(
        "<svg width=\"{t}\" height=\"{t}\" viewBox=\"0 0 {t} {t}\" \
              xmlns=\"http://www.w3.org/2000/svg\" shape-rendering=\"crispEdges\">\
           <rect width=\"{t}\" height=\"{t}\" fill=\"white\"/>{rects}</svg>",
        t = total,
    );

    view! { <div inner_html=svg /> }.into_any()
}
