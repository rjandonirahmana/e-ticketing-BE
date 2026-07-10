//! merchant.rs — Halaman Merchant Hub.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::{
    get_merchant_events, get_merchant_public_events, get_merchant_public_profile,
    update_merchant_profile,
};
use crate::web::app::AuthResource;
use crate::web::components::{BottomNav, MerchantEventCardShimmer, ThemeToggle};
use crate::web::models::{format_date, format_price, Event, MerchantPublicProfile, PaginatedEvents};

use super::merchant_public::fmt_count;

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

            // ── Preview profil publik + editor (nama/deskripsi/logo/header) ────
            <MerchantProfileCard />

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

// ─── Profil merchant: preview publik + editor (nama/deskripsi/logo/header) ──────

/// Unggah gambar (logo/header) ke POST /upload/merchant-image → balas URL.
#[cfg(target_arch = "wasm32")]
async fn upload_merchant_image(file: &web_sys::File) -> Result<String, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let form = web_sys::FormData::new().map_err(|e| format!("{:?}", e))?;
    form.append_with_blob("file", file).map_err(|e| format!("{:?}", e))?;

    let opts = web_sys::RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&form);

    let req = web_sys::Request::new_with_str_and_init("/upload/merchant-image", &opts)
        .map_err(|e| format!("{:?}", e))?;
    let win = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_val = JsFuture::from(win.fetch_with_request(&req))
        .await
        .map_err(|e| format!("{:?}", e))?;
    let resp: web_sys::Response = resp_val.unchecked_into();
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let json = JsFuture::from(resp.json().map_err(|e| format!("{:?}", e))?)
        .await
        .map_err(|e| format!("{:?}", e))?;
    js_sys::Reflect::get(&json, &wasm_bindgen::JsValue::from_str("url"))
        .ok()
        .and_then(|v| v.as_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "URL kosong dari server".to_string())
}

#[component]
fn MerchantProfileCard() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let my_id = move || {
        auth.get()
            .and_then(|r| r.ok())
            .flatten()
            .map(|u| u.id)
            .unwrap_or_default()
    };

    let profile = Resource::new(my_id, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("not_ready".into()));
        }
        // Dashboard ini adalah profil merchant SENDIRI. Merchant yang baru
        // di-upgrade tapi belum membuat toko belum punya row `merchant_details`,
        // sehingga `get_merchant_public_profile` balas NotFound → dulu render
        // sebagai 500 dan kartu profil mati (tak bisa isi & simpan toko). Untuk
        // pemilik sendiri, perlakukan "belum ada" sebagai profil KOSONG yang bisa
        // diedit — bukan error. (Halaman publik /m/{id} tetap menampilkan
        // "merchant tidak ditemukan" karena tak melewati jalur ini.)
        // `Ok::<_, ServerFnError>` mem-pin tipe error agar inferensi Resource
        // tetap konkret (tanpa ini `profile` kehilangan `Copy` → cascade error).
        get_merchant_public_profile(id.clone())
            .await
            .or_else(|_| {
                Ok::<_, ServerFnError>(MerchantPublicProfile {
                    merchant_id: id,
                    store_name: String::new(),
                    description: None,
                    logo_url: None,
                    header_url: None,
                    verified: false,
                    followers: 0,
                    events_count: 0,
                    rating_avg: 0.0,
                    rating_count: 0,
                    is_following: false,
                })
            })
    });
    let events = Resource::new(my_id, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("not_ready".into()));
        }
        get_merchant_public_events(id, Some(1)).await
    });

    // Editor state (di-seed sekali dari profile).
    let editing = RwSignal::new(false);
    let f_name = RwSignal::new(String::new());
    let f_desc = RwSignal::new(String::new());
    let logo_url = RwSignal::new(String::new());
    let header_url = RwSignal::new(String::new());
    let logo_prev = RwSignal::new(String::new());
    let header_prev = RwSignal::new(String::new());
    let uploading = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let msg = RwSignal::new(String::new());
    let seeded = RwSignal::new(false);

    Effect::new(move |_| {
        if let Some(Ok(p)) = profile.get() {
            if !seeded.get_untracked() {
                f_name.set(p.store_name.clone());
                f_desc.set(p.description.clone().unwrap_or_default());
                logo_url.set(p.logo_url.clone().unwrap_or_default());
                header_url.set(p.header_url.clone().unwrap_or_default());
                seeded.set(true);
            }
        }
    });

    let event_cover = move || {
        events
            .get()
            .and_then(|r| r.ok())
            .and_then(|pe| pe.data.first().and_then(|e| e.cover_url.clone()))
            .filter(|c| !c.is_empty())
    };
    let city = move || {
        events
            .get()
            .and_then(|r| r.ok())
            .and_then(|pe| pe.data.first().and_then(|e| e.city.clone()))
            .filter(|c| !c.is_empty())
    };
    let hero_src = move || {
        let h = if !header_prev.get().is_empty() {
            header_prev.get()
        } else {
            header_url.get()
        };
        if !h.is_empty() {
            Some(h)
        } else {
            event_cover()
        }
    };
    let avatar_src = move || {
        let l = if !logo_prev.get().is_empty() {
            logo_prev.get()
        } else {
            logo_url.get()
        };
        (!l.is_empty()).then_some(l)
    };

    // Pilih file → preview instan + unggah → simpan URL server ke `url_sig`.
    let make_pick = move |url_sig: RwSignal<String>, prev_sig: RwSignal<String>| {
        move |ev: leptos::ev::Event| {
            #[cfg(target_arch = "wasm32")]
            {
                use wasm_bindgen::JsCast;
                let Some(input) = ev
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                else {
                    return;
                };
                let Some(file) = input.files().and_then(|f| f.get(0)) else {
                    return;
                };
                if let Ok(obj) = web_sys::Url::create_object_url_with_blob(&file) {
                    prev_sig.set(obj);
                }
                uploading.set(true);
                msg.set(String::new());
                leptos::task::spawn_local(async move {
                    match upload_merchant_image(&file).await {
                        Ok(u) => url_sig.set(u),
                        Err(e) => msg.set(format!("Upload gagal: {e}")),
                    }
                    uploading.set(false);
                });
            }
            let _ = ev;
            let _ = (url_sig, prev_sig);
        }
    };
    let on_pick_header = make_pick(header_url, header_prev);
    let on_pick_logo = make_pick(logo_url, logo_prev);

    let do_save = move |_| {
        if saving.get_untracked() {
            return;
        }
        let name = f_name.get_untracked().trim().to_string();
        if name.len() < 2 {
            msg.set("Nama bisnis minimal 2 karakter.".into());
            return;
        }
        let desc = f_desc.get_untracked();
        let logo = logo_url.get_untracked();
        let header = header_url.get_untracked();
        saving.set(true);
        msg.set(String::new());
        leptos::task::spawn_local(async move {
            match update_merchant_profile(name, desc, logo, header).await {
                Ok(()) => {
                    msg.set("Profil tersimpan!".into());
                    profile.refetch();
                    editing.set(false);
                }
                Err(e) => msg.set(format!("Gagal menyimpan: {e}")),
            }
            saving.set(false);
        });
    };

    view! {
        <section class="mhub-profile-card">
            <div class="mp-hero mhub-phero">
                {move || { hero_src().map(|src| view! { <img src=src alt="" loading="lazy" /> }) }}
                <div class="mp-hero-grad"></div>
                <button
                    class="mhub-edit-toggle"
                    on:click=move |_| editing.update(|e| *e = !*e)
                >
                    {move || if editing.get() { "Tutup" } else { "Edit Profil" }}
                </button>
            </div>

            <div class="mp-head">
                <div class="mp-avatar-wrap">
                    {move || match avatar_src() {
                        Some(src) => view! { <img class="mp-avatar" src=src alt="Logo" /> }.into_any(),
                        None => {
                            let initial = f_name
                                .get()
                                .chars()
                                .next()
                                .unwrap_or('P')
                                .to_uppercase()
                                .to_string();
                            view! { <div class="mp-avatar mp-avatar-fallback">{initial}</div> }
                                .into_any()
                        }
                    }}
                    {move || {
                        profile
                            .get()
                            .and_then(|r| r.ok())
                            .map(|p| p.verified)
                            .unwrap_or(false)
                            .then(|| {
                                view! {
                                    <span class="mp-avatar-badge" title="Terverifikasi">
                                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                            stroke="currentColor" stroke-width="3" stroke-linecap="round"
                                            stroke-linejoin="round">
                                            <polyline points="20 6 9 17 4 12" />
                                        </svg>
                                    </span>
                                }
                            })
                    }}
                </div>
            </div>

            <div class="mp-container">
                <h1 class="mp-name">{move || f_name.get()}</h1>
                {move || {
                    city()
                        .map(|c| {
                            view! {
                                <p class="mp-loc">
                                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                        stroke="currentColor" stroke-width="2" stroke-linecap="round"
                                        stroke-linejoin="round">
                                        <path d="M21 10c0 7-9 12-9 12s-9-5-9-12a9 9 0 0 1 18 0z" />
                                        <circle cx="12" cy="10" r="3" />
                                    </svg>
                                    {c}
                                </p>
                            }
                        })
                }}

                <div class="mp-stats">
                    {move || {
                        let p = profile.get().and_then(|r| r.ok());
                        let (f, e, r) = p
                            .map(|p| (p.followers, p.events_count, p.rating_avg))
                            .unwrap_or((0, 0, 0.0));
                        let followers_href = format!("/m/{}/followers", my_id());
                        view! {
                            <a class="mp-stat mp-stat-link" href=followers_href>
                                <span class="mp-stat-num">{fmt_count(f)}</span>
                                <span class="mp-stat-label">"FOLLOWERS"</span>
                            </a>
                            <div class="mp-stat">
                                <span class="mp-stat-num">{fmt_count(e)}</span>
                                <span class="mp-stat-label">"EVENTS"</span>
                            </div>
                            <div class="mp-stat">
                                <span class="mp-stat-num">
                                    {format!("{:.1}", r)}<span class="mp-stat-star">"★"</span>
                                </span>
                                <span class="mp-stat-label">"RATING"</span>
                            </div>
                        }
                    }}
                </div>

                <Show when=move || editing.get()>
                    <div class="mhub-edit-form">
                        <label class="mhub-form-label">"HEADER"</label>
                        <label class="mhub-upload-tile">
                            <input type="file" accept="image/*" on:change=on_pick_header />
                            {move || {
                                let src = if !header_prev.get().is_empty() {
                                    header_prev.get()
                                } else {
                                    header_url.get()
                                };
                                if src.is_empty() {
                                    view! { <span class="mhub-upload-hint">"+ Unggah header"</span> }
                                        .into_any()
                                } else {
                                    view! { <img class="mhub-upload-prev" src=src alt="" /> }.into_any()
                                }
                            }}
                        </label>

                        <label class="mhub-form-label" style="margin-top:12px">"LOGO"</label>
                        <label class="mhub-upload-tile mhub-upload-tile--logo">
                            <input type="file" accept="image/*" on:change=on_pick_logo />
                            {move || {
                                let src = if !logo_prev.get().is_empty() {
                                    logo_prev.get()
                                } else {
                                    logo_url.get()
                                };
                                if src.is_empty() {
                                    view! { <span class="mhub-upload-hint">"+ Logo"</span> }.into_any()
                                } else {
                                    view! { <img class="mhub-upload-prev" src=src alt="" /> }.into_any()
                                }
                            }}
                        </label>

                        <label class="mhub-form-label" style="margin-top:12px">"NAMA BISNIS"</label>
                        <input
                            class="mhub-form-input"
                            type="text"
                            prop:value=move || f_name.get()
                            on:input=move |e| f_name.set(event_target_value(&e))
                        />

                        <label class="mhub-form-label" style="margin-top:10px">"DESKRIPSI"</label>
                        <textarea
                            class="mhub-form-input"
                            rows="4"
                            prop:value=move || f_desc.get()
                            on:input=move |e| f_desc.set(event_target_value(&e))
                        ></textarea>

                        {move || {
                            (!msg.get().is_empty())
                                .then(|| view! { <p class="mhub-form-msg">{msg.get()}</p> })
                        }}

                        <button
                            class="mhub-modal-submit"
                            style="width:100%;margin-top:12px"
                            disabled=move || saving.get() || uploading.get()
                            on:click=do_save
                        >
                            {move || {
                                if saving.get() {
                                    "Menyimpan…"
                                } else if uploading.get() {
                                    "Mengunggah…"
                                } else {
                                    "Simpan Profil"
                                }
                            }}
                        </button>
                    </div>
                </Show>
            </div>
        </section>
    }
}
