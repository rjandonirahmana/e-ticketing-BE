use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::csr::components::{BottomNav, GridBackground, MerchantRowShimmer};
use crate::csr::hooks::{format_idr, use_auth, use_nav, ThemeToggle};
use crate::csr::services::client::{
    delete_private, get_public, post_multipart_private, put_multipart_private,
};
use crate::csr::services::event::{self as event_svc};
use crate::csr::state::events::ExploreEvent;

use serde::{Deserialize, Serialize};

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

#[derive(Serialize)]
struct BannerFormData {
    image_url: Option<String>,
    click_url: Option<String>,
    start_date: String,
    end_date: Option<String>,
    event_id: Option<String>,
}

// ─── Banner model (wire-level) ────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeBanner {
    pub id: i64,
    pub image_url: String,
    #[serde(default)]
    pub click_url: Option<String>,
    pub start_date: String,
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub event_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ─── Status badge — copy-paste persis dari merchant.rs ────────────────────────

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
            Self::OnSale => "Dijual",
            Self::SoldOut => "Habis Terjual",
            Self::Presale => "Pre-order",
        }
    }
}

// ─── Banner API helpers ───────────────────────────────────────────────────────

async fn api_list_banners() -> Result<Vec<BeBanner>, crate::csr::services::ApiError> {
    let list: Vec<BeBanner> = get_public("/banners").await?;
    Ok(list)
}

async fn api_delete_banner(id: i64) -> Result<(), crate::csr::services::ApiError> {
    #[derive(Deserialize)]
    struct Msg {
        #[allow(dead_code)]
        message: String,
    }
    let _: Msg = delete_private(&format!("/admin/banners/{}", id)).await?;
    Ok(())
}

async fn api_create_banner(
    click_url: String,
    start_date: String,
    end_date: String,
    event_id: String,
    image_url: String,
    image_file: Option<web_sys::File>,
) -> Result<BeBanner, crate::csr::services::ApiError> {
    use web_sys::FormData;

    let body = BannerFormData {
        image_url: if image_url.is_empty() {
            None
        } else {
            Some(image_url)
        },
        click_url: if click_url.is_empty() {
            None
        } else {
            Some(click_url)
        },
        start_date,
        end_date: if end_date.is_empty() {
            None
        } else {
            Some(end_date)
        },
        event_id: if event_id.is_empty() {
            None
        } else {
            Some(event_id)
        },
    };

    let form = FormData::new().map_err(|_| crate::csr::services::ApiError::network("FormData init"))?;
    form.append_with_str("data", &serde_json::to_string(&body).unwrap_or_default())
        .map_err(|_| crate::csr::services::ApiError::network("append data"))?;
    if let Some(f) = image_file {
        let name = f.name();
        form.append_with_blob_and_filename("image", &f, &name)
            .map_err(|_| crate::csr::services::ApiError::network("append image"))?;
    }
    post_multipart_private("/admin/banners", form).await
}

async fn api_update_banner(
    id: i64,
    click_url: String,
    start_date: String,
    end_date: String,
    event_id: String,
    image_url: String,
    image_file: Option<web_sys::File>,
) -> Result<BeBanner, crate::csr::services::ApiError> {
    use web_sys::FormData;

    let body = BannerFormData {
        image_url: if image_url.is_empty() {
            None
        } else {
            Some(image_url)
        },
        click_url: if click_url.is_empty() {
            None
        } else {
            Some(click_url)
        },
        start_date,
        end_date: if end_date.is_empty() {
            None
        } else {
            Some(end_date)
        },
        event_id: if event_id.is_empty() {
            None
        } else {
            Some(event_id)
        },
    };

    let form = FormData::new().map_err(|_| crate::csr::services::ApiError::network("FormData init"))?;
    form.append_with_str("data", &serde_json::to_string(&body).unwrap_or_default())
        .map_err(|_| crate::csr::services::ApiError::network("append data"))?;
    if let Some(f) = image_file {
        let name = f.name();
        form.append_with_blob_and_filename("image", &f, &name)
            .map_err(|_| crate::csr::services::ApiError::network("append image"))?;
    }
    put_multipart_private(&format!("/admin/banners/{}", id), form).await
}

// ─── AdminPage ────────────────────────────────────────────────────────────────

#[component]
pub fn AdminPage() -> impl IntoView {
    let auth = use_auth();
    let navigate = use_nav();

    // ── Load semua event platform via admin endpoint ──────────────────────────
    let all_events: RwSignal<Vec<ExploreEvent>> = RwSignal::new(vec![]);
    let events_loading = RwSignal::new(true);
    // Event dengan status "edited" yang menunggu review admin
    let pending_events: RwSignal<Vec<crate::csr::models::Event>> = RwSignal::new(vec![]);
    let pending_loading = RwSignal::new(true);

    spawn_local(async move {
        // Load semua event untuk analytics
        if let Ok(resp) = event_svc::admin_list_all_events(1, 100, None).await {
            all_events.set(
                resp.events
                    .iter()
                    .map(|e| crate::csr::state::events::event_to_explore_pub(e))
                    .collect(),
            );
        }
        events_loading.set(false);
    });

    spawn_local(async move {
        // Load event yang menunggu review (status = "edited")
        if let Ok(resp) = event_svc::admin_list_all_events(1, 50, Some("edited")).await {
            pending_events.set(resp.events);
        }
        pending_loading.set(false);
    });

    // ── Guard: hanya role "admin" ─────────────────────────────────────────────
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
            let ok = auth
                .user
                .with(|u| u.as_ref().map(|p| p.role == "admin").unwrap_or(false));
            if !ok {
                nav("/", Default::default());
            }
        });
    }

    let active_page = RwSignal::new("review");

    // ── Aggregate stats (sum lintas merchant) ─────────────────────────────────
    let total_sold_all =
        Memo::new(move |_| all_events.with(|evs| evs.iter().map(|e| e.total_sold).sum::<i32>()));
    let total_quota_all =
        Memo::new(move |_| all_events.with(|evs| evs.iter().map(|e| e.total_quota).sum::<i32>()));
    let capacity_pct = Memo::new(move |_| {
        let q = total_quota_all.get();
        if q == 0 {
            0u32
        } else {
            ((total_sold_all.get() as f64 / q as f64) * 100.0).round() as u32
        }
    });

    view! {
        <BottomNav active="admin" />
        <GridBackground />
        <main class="page merchant-page mhub-mobile">

            // ── Header — identik merchant.rs, icon perisai, judul "Admin Hub" ─
            <header class="mhub-header">
                <div class="mhub-header-left">
                    <div class="mhub-header-avatar">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
                        </svg>
                    </div>
                    <span class="mhub-header-title">"Pusat Admin"</span>
                </div>
                <div class="mhub-header-right">
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

            // ── Stats strip — data platform, identik merchant.rs ──────────────
            <div class="mhub-stats-strip">
                <div class="mhub-stat-cell">
                    <span class="mhub-stat-label">"TOTAL TERJUAL"</span>
                    <span class="mhub-stat-value">{move || {
                        let s = total_sold_all.get();
                        if s == 0 { "—".to_string() } else { format!("{s}") }
                    }}</span>
                    <div class="mhub-stat-capacity-bar">
                        <div class="mhub-stat-capacity-fill"
                             style=move || format!("width:{}%", capacity_pct.get())>
                        </div>
                    </div>
                    <span class="mhub-stat-label">{move || {
                        let q = total_quota_all.get(); let pct = capacity_pct.get();
                        if q == 0 { "—".to_string() } else { format!("{pct}% kapasitas") }
                    }}</span>
                </div>
                <div class="mhub-stat-divider"></div>
                <div class="mhub-stat-cell">
                    <span class="mhub-stat-label">"SISA TIKET"</span>
                    <span class="mhub-stat-value">{move || {
                        format!("{}", (total_quota_all.get() - total_sold_all.get()).max(0))
                    }}</span>
                    <span class="mhub-stat-label">{move || {
                        let q = total_quota_all.get();
                        if q == 0 { "—".to_string() } else { format!("dari {q} kuota") }
                    }}</span>
                </div>
            </div>

            // ── Tab bar — identik merchant + tab "Review" + "Banner" ─────────
            <div class="mhub-mobile-tabs">
                {[
                    ("review",    "Review"),
                    ("events",    "Acara"),
                    ("banners",   "Spanduk"),
                    ("analytics", "Analitik"),
                    ("finance",   "Keuangan"),
                    ("settings",  "Pengaturan"),
                ].iter().map(|(id, label)| {
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
                            // Badge merah untuk jumlah event menunggu review
                            {move || if id == "review" {
                                let cnt = pending_events.with(|v| v.len());
                                if cnt > 0 {
                                    view! {
                                        <span style="background:#ef4444;color:#fff;border-radius:99px;\
                                                     font-size:0.65rem;font-weight:700;padding:1px 5px;\
                                                     margin-left:4px;vertical-align:middle">
                                            {cnt}
                                        </span>
                                    }.into_any()
                                } else { view!{<span/>}.into_any() }
                            } else { view!{<span/>}.into_any() }}
                        </button>
                    }
                }).collect_view()}
            </div>

            // ── Content ───────────────────────────────────────────────────────
            {
                let nav_ev = navigate.clone();
                move || match active_page.get() {
                    "review"    => view_event_review(pending_events, all_events).into_any(),
                    "banners"   => view_banners_tab().into_any(),
                    "analytics" => view_admin_analytics(all_events).into_any(),
                    "finance"   => view_admin_finance(all_events).into_any(),
                    "settings"  => view_admin_settings().into_any(),
                    _           => view_all_events(all_events, events_loading, nav_ev.clone()).into_any(),
                }
            }

        </main>
        // ── Tidak ada FAB — admin tidak membuat event sendiri ─────────────────
    }
}

// ─── Tab: Review Event (pending approval) ────────────────────────────────────

fn view_event_review(
    pending_events: RwSignal<Vec<crate::csr::models::Event>>,
    all_events: RwSignal<Vec<ExploreEvent>>,
) -> impl IntoView {
    let toast: RwSignal<Option<(String, bool)>> = RwSignal::new(None);
    let processing: RwSignal<Option<String>> = RwSignal::new(None);

    let do_update_status = move |event_id: String, status: &'static str| {
        let id = event_id.clone();
        processing.set(Some(id.clone()));
        spawn_local(async move {
            match event_svc::admin_update_event_status(&id, status).await {
                Ok(updated) => {
                    // Hapus dari pending list jika sudah di-approve/reject
                    pending_events.update(|v| v.retain(|e| e.id != id));
                    // Update di all_events juga
                    all_events.update(|evs| {
                        if let Some(ex) = evs.iter_mut().find(|e| e.id == id) {
                            ex.status = updated.status.clone();
                        }
                    });
                    let msg = match status {
                        "active" => "✅ Event diaktifkan — kini terlihat oleh pengguna.",
                        "cancelled" => "❌ Event dibatalkan.",
                        _ => "Status event diperbarui.",
                    };
                    toast.set(Some((msg.to_string(), false)));
                }
                Err(e) => toast.set(Some((format!("Gagal: {}", e.message), true))),
            }
            processing.set(None);
        });
    };

    view! {
        <section class="mhub-events-section">
            <div class="mhub-events-header">
                <h3 class="mhub-events-title">"Review Event"</h3>
                <span class="mhub-live-badge">
                    <span class="mhub-live-dot"></span>
                    "Menunggu"
                </span>
            </div>

            // Toast
            {move || toast.get().map(|(msg, is_err)| {
                let cls = if is_err { "admin-toast admin-toast--err" } else { "admin-toast admin-toast--ok" };
                view! {
                    <div class=cls>
                        <span>{msg}</span>
                        <button class="admin-toast-x" on:click=move |_| toast.set(None)>"✕"</button>
                    </div>
                }
            })}

            {move || {
                let evs = pending_events.get();
                if evs.is_empty() {
                    return view! {
                        <div class="mhub-empty">
                            <div class="mhub-empty-icon-wrap">
                                <svg width="38" height="38" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                    <polyline points="20 6 9 17 4 12"/>
                                </svg>
                            </div>
                            <p class="mhub-empty-title">"Semua Bersih ✅"</p>
                            <p class="mhub-empty-body">"Tidak ada event yang menunggu review."</p>
                        </div>
                    }.into_any();
                }

                evs.into_iter().map(|ev| {
                    let id = ev.id.clone();
                    let id_approve = id.clone();
                    let id_reject  = id.clone();
                    let cover = if ev.cover_url.is_empty() {
                        "https://images.unsplash.com/photo-1514525253161-7a46d19cd819?w=600&q=80".to_string()
                    } else { ev.cover_url.clone() };
                    let title  = ev.title.clone();
                    let venue  = ev.venue.name.clone();
                    let city   = ev.venue.city.clone();
                    let date   = ev.start_time.get(..10).unwrap_or("").to_string();
                    let status = ev.status.clone();
                    let is_processing = Signal::derive(move || {
                        processing.with(|p| p.as_deref() == Some(&id))
                    });

                    view! {
                        <div class="mhub-event-card" style="border:2px solid var(--accent-lime)">
                            <div class="mhub-event-card-img-wrap">
                                <img src=cover alt=title.clone() class="mhub-event-card-img"/>
                                <span class="mhub-event-status mhub-event-status--presale">
                                    "⏳ Menunggu Review"
                                </span>
                            </div>
                            <div class="mhub-event-card-body">
                                <div class="mhub-event-card-top-row">
                                    <p class="mhub-event-card-title">{title}</p>
                                    <span style="font-size:0.7rem;color:var(--text-muted);padding:3px 7px;\
                                                 background:var(--surface-2);border-radius:6px;font-weight:600">
                                        {status}
                                    </span>
                                </div>
                                <p class="mhub-event-card-meta">
                                    {date}" • "{venue}
                                    {if city.is_empty() { String::new() } else { format!(" • {}", city) }}
                                </p>

                                <div style="display:flex;gap:8px;margin-top:10px">
                                    // APPROVE → active
                                    <button
                                        class="mhub-event-manage-btn"
                                        style="flex:1;background:var(--accent-lime);color:#000;font-weight:700"
                                        disabled=move || is_processing.get()
                                        on:click={
                                            let id_a = id_approve.clone();
                                            move |_| do_update_status(id_a.clone(), "active")
                                        }>
                                        {move || if is_processing.get() { "Memproses..." } else { "✅ Aktifkan" }}
                                    </button>
                                    // REJECT → cancelled
                                    <button
                                        class="mhub-event-manage-btn"
                                        style="flex:1;background:var(--surface-2);color:var(--text-muted)"
                                        disabled=move || is_processing.get()
                                        on:click={
                                            let id_r = id_reject.clone();
                                            move |_| do_update_status(id_r.clone(), "cancelled")
                                        }>
                                        {move || if is_processing.get() { "Memproses..." } else { "❌ Batalkan" }}
                                    </button>
                                </div>
                            </div>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </section>
    }
}

// ─── Tab: Semua Event (identik view_tickets di merchant.rs) ───────────────────

fn view_all_events(
    all_events: RwSignal<Vec<ExploreEvent>>,
    events_loading: RwSignal<bool>,
    navigate: impl Fn(&str, leptos_router::NavigateOptions) + Clone + Send + 'static,
) -> impl IntoView {
    view! {
        <section class="mhub-events-section">
            <div class="mhub-events-header">
                <h3 class="mhub-events-title">"Semua Acara Platform"</h3>
                <span class="mhub-live-badge">
                    <span class="mhub-live-dot"></span>
                    "Langsung"
                </span>
            </div>

            {move || {
                if events_loading.get() {
                    return (0..3).map(|_| view! { <MerchantRowShimmer/> })
                        .collect_view().into_any();
                }
                let evs = all_events.get();
                if evs.is_empty() {
                    return view! {
                        <div class="mhub-empty">
                            <div class="mhub-empty-icon-wrap">
                                <svg width="38" height="38" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                    <rect x="1" y="4" width="22" height="16" rx="2" ry="2"/>
                                    <line x1="1" y1="10" x2="23" y2="10"/>
                                </svg>
                            </div>
                            <p class="mhub-empty-title">"Belum Ada Acara"</p>
                            <p class="mhub-empty-body">"Belum ada acara yang terdaftar di platform."</p>
                        </div>
                    }.into_any();
                }
                evs.into_iter().map(|ev| {
                    let cover = if ev.cover.is_empty() {
                        "https://images.unsplash.com/photo-1514525253161-7a46d19cd819?w=600&q=80".into()
                    } else { ev.cover.clone() };

                    let status     = EventStatus::from_ev(&ev);
                    let title      = ev.title.clone();
                    let date       = fmt_date(&ev.date);
                    let venue      = if ev.city.is_empty() { ev.venue.clone() }
                                     else { format!("{} • {}", ev.venue, ev.city) };
                    let price      = ev.price;
                    let sold       = ev.total_sold;
                    let quota      = ev.total_quota;
                    let available  = (quota - sold).max(0);
                    let pct        = if quota > 0 {
                        ((sold as f64 / quota as f64) * 100.0).round() as u32
                    } else { 0 };
                    let fill_style = format!("width:{}%", pct);

                    let (val_text, val_cls) = if status == EventStatus::SoldOut {
                        ("100% Habis Terjual".to_string(), "mhub-event-progress-val mhub-event-progress-val--sold")
                    } else if quota == 0 {
                        ("—".to_string(), "mhub-event-progress-val")
                    } else {
                        (format!("{sold}/{quota} Terjual"), "mhub-event-progress-val")
                    };

                    let remaining_text = if quota == 0 { String::new() }
                                         else { format!("{available} sisa") };
                    let fill_cls = match &status {
                        EventStatus::SoldOut => "mhub-event-progress-fill mhub-event-progress-fill--sold",
                        EventStatus::Presale => "mhub-event-progress-fill mhub-event-progress-fill--lime",
                        _                    => "mhub-event-progress-fill",
                    };
                    let slug = ev.slug.clone();

                    view! {
                        <div class="mhub-event-card">
                            <div class="mhub-event-card-img-wrap">
                                <img src=cover alt=title.clone() class="mhub-event-card-img"/>
                                <span class=status.css_mod()>{status.label()}</span>
                            </div>
                            <div class="mhub-event-card-body">
                                <div class="mhub-event-card-top-row">
                                    <p class="mhub-event-card-title">{title}</p>
                                    <div class="mhub-event-card-price-block">
                                        <span class="mhub-event-price-label">"Mulai dari"</span>
                                        <span class="mhub-event-price-value">{format_idr(price)}</span>
                                    </div>
                                </div>
                                <p class="mhub-event-card-meta">{date}" • "{venue}</p>

                                <div class="mhub-event-progress-section">
                                    <div class="mhub-event-progress-row">
                                        <span class="mhub-event-progress-key">"Penjualan"</span>
                                        <span class=val_cls>{val_text}</span>
                                    </div>
                                    <div class="mhub-event-progress-bar">
                                        <div class=fill_cls style=fill_style></div>
                                    </div>
                                    {(!remaining_text.is_empty()).then(|| view! {
                                        <div class="mhub-event-remaining-row">
                                            <span class="mhub-event-remaining-badge">
                                                <svg width="10" height="10" viewBox="0 0 24 24" fill="none"
                                                     stroke="currentColor" stroke-width="2.5">
                                                    <circle cx="12" cy="12" r="10"/>
                                                    <line x1="12" y1="8" x2="12" y2="12"/>
                                                    <line x1="12" y1="16" x2="12.01" y2="16"/>
                                                </svg>
                                                {remaining_text}
                                            </span>
                                        </div>
                                    })}
                                </div>

                                <div class="mhub-event-card-actions">
                                    <button class="mhub-event-manage-btn"
                                        on:click={
                                            let nav = navigate.clone();
                                            let s   = slug.clone();
                                            move |_| nav(
                                                &format!("/merchant/events/{}/edit", s),
                                                Default::default(),
                                            )
                                        }>
                                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                            <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/>
                                            <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/>
                                        </svg>
                                        "Sunting Acara"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </section>
    }
}

// ─── Tab: Banner CRUD ─────────────────────────────────────────────────────────

fn view_banners_tab() -> impl IntoView {
    let banners: RwSignal<Vec<BeBanner>> = RwSignal::new(vec![]);
    let loading = RwSignal::new(true);
    let toast: RwSignal<Option<(String, bool)>> = RwSignal::new(None);
    let deleting: RwSignal<Option<i64>> = RwSignal::new(None);
    // None=tutup | Some(None)=tambah | Some(Some(b))=edit b
    let modal: RwSignal<Option<Option<BeBanner>>> = RwSignal::new(None);

    spawn_local(async move {
        match api_list_banners().await {
            Ok(data) => banners.set(data),
            Err(e) => toast.set(Some((format!("Gagal memuat: {}", e.message), true))),
        }
        loading.set(false);
    });

    view! {
        <section class="mhub-events-section">

            // baris header — identik merchant "Acara Saya" + badge
            <div class="mhub-events-header">
                <h3 class="mhub-events-title">"Pengelolaan Spanduk"</h3>
                <button class="admin-add-btn"
                    on:click=move |_| modal.set(Some(None))>
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <line x1="12" y1="5" x2="12" y2="19"/>
                        <line x1="5" y1="12" x2="19" y2="12"/>
                    </svg>
                    "Tambah"
                </button>
            </div>

            // toast
            {move || match toast.get() {
                None => view!{ <span/> }.into_any(),
                Some((msg, is_err)) => view! {
                    <div class=if is_err {"admin-toast admin-toast--err"} else {"admin-toast admin-toast--ok"}>
                        {if is_err {"⚠️ "} else {"✅ "}}
                        {msg}
                        <button class="admin-toast-x"
                            on:click=move |_| toast.set(None)>"×"</button>
                    </div>
                }.into_any(),
            }}

            // daftar banner
            {move || {
                if loading.get() {
                    return (0..3).map(|_| view!{ <MerchantRowShimmer/> })
                        .collect_view().into_any();
                }
                let items = banners.get();
                if items.is_empty() {
                    return view! {
                        <div class="mhub-empty">
                            <div class="mhub-empty-icon-wrap">
                                <svg width="38" height="38" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                    <rect x="3" y="3" width="18" height="18" rx="2"/>
                                    <path d="M3 9h18M9 21V9"/>
                                </svg>
                            </div>
                            <p class="mhub-empty-title">"Belum Ada Spanduk"</p>
                            <p class="mhub-empty-body">"Ketuk Tambah untuk membuat spanduk pertama."</p>
                        </div>
                    }.into_any();
                }
                items.into_iter().map(|b| {
                    let bid     = b.id;
                    let bid_del = b.id;
                    let b_edit  = b.clone();

                    // Signal::derive adalah Copy — aman dipakai di disabled=
                    let is_busy = Signal::derive(move || {
                        deleting.get() == Some(bid)
                    });

                    view! {
                        <div class="admin-banner-row">
                            // thumbnail
                            <div class="admin-banner-thumb">
                                {if b.image_url.is_empty() {
                                    view!{ <div class="admin-banner-no-img">"🖼"</div> }.into_any()
                                } else {
                                    view!{ <img src=b.image_url.clone()
                                               alt=format!("Spanduk #{}", b.id)
                                               class="admin-banner-img"/> }.into_any()
                                }}
                                {b.deleted_at.as_ref().map(|_| view!{
                                    <span class="admin-banner-badge admin-banner-badge--off">"Dihapus"</span>
                                })}
                            </div>

                            // info
                            <div class="admin-banner-info">
                                <p class="admin-banner-title">"Spanduk #"{b.id}</p>
                                {b.click_url.as_ref().map(|l| view!{
                                    <p class="admin-banner-link">
                                        <svg width="9" height="9" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="2">
                                            <path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71"/>
                                            <path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71"/>
                                        </svg>
                                        {l.clone()}
                                    </p>
                                })}
                                <p class="admin-banner-order">"Mulai: "{fmt_date(&b.start_date)}</p>
                                {b.end_date.as_ref().map(|e| view!{
                                    <p class="admin-banner-order">"Selesai: "{fmt_date(e)}</p>
                                })}
                            </div>

                            // aksi
                            <div class="admin-banner-actions">
                                <button class="mhub-event-manage-btn"
                                    disabled=move || is_busy.get()
                                    on:click=move |_| modal.set(Some(Some(b_edit.clone())))>
                                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                        <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/>
                                        <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/>
                                    </svg>
                                    "Sunting"
                                </button>
                                <button class="admin-del-btn"
                                    disabled=move || is_busy.get()
                                    on:click=move |_| {
                                        deleting.set(Some(bid_del));
                                        spawn_local(async move {
                                            match api_delete_banner(bid_del).await {
                                                Ok(_) => {
                                                    banners.update(|v| v.retain(|b| b.id != bid_del));
                                                    toast.set(Some((
                                                        format!("Spanduk #{} dihapus", bid_del), false,
                                                    )));
                                                }
                                                Err(e) => toast.set(Some((
                                                    format!("Gagal hapus: {}", e.message), true,
                                                ))),
                                            }
                                            deleting.set(None);
                                        });
                                    }>
                                    {move || if is_busy.get() {"..."} else {"🗑"}}
                                </button>
                            </div>
                        </div>
                    }
                }).collect_view().into_any()
            }}

            // modal
            {move || match modal.get() {
                None => view!{ <span/> }.into_any(),
                Some(maybe) => view! {
                    <BannerFormModal
                        initial=maybe
                        on_close=move || modal.set(None)
                        on_saved=move |saved: BeBanner| {
                            banners.update(|list| {
                                if let Some(i) = list.iter().position(|b| b.id == saved.id) {
                                    list[i] = saved;
                                } else {
                                    list.push(saved);
                                }
                            });
                            modal.set(None);
                            toast.set(Some(("Spanduk berhasil disimpan ✅".into(), false)));
                        }
                    />
                }.into_any(),
            }}
        </section>
    }
}

// ─── Banner Form Modal ────────────────────────────────────────────────────────

#[component]
fn BannerFormModal(
    initial: Option<BeBanner>,
    on_close: impl Fn() + Clone + Send + Sync + 'static,
    on_saved: impl Fn(BeBanner) + Clone + Send + Sync + 'static,
) -> impl IntoView {
    let is_edit = initial.is_some();

    let f_img_url = RwSignal::new(
        initial
            .as_ref()
            .map(|b| b.image_url.clone())
            .unwrap_or_default(),
    );
    let f_click_url = RwSignal::new(
        initial
            .as_ref()
            .and_then(|b| b.click_url.clone())
            .unwrap_or_default(),
    );
    // start_date: potong ke "YYYY-MM-DDTHH:MM" supaya cocok dengan input datetime-local
    let f_start_date = RwSignal::new(
        initial
            .as_ref()
            .map(|b| b.start_date.get(..16).unwrap_or("").to_string())
            .unwrap_or_default(),
    );
    let f_end_date = RwSignal::new(
        initial
            .as_ref()
            .and_then(|b| {
                b.end_date
                    .as_deref()
                    .map(|s| s.get(..16).unwrap_or("").to_string())
            })
            .unwrap_or_default(),
    );
    let f_event_id = RwSignal::new(
        initial
            .as_ref()
            .and_then(|b| b.event_id.clone())
            .unwrap_or_default(),
    );
    let f_file: RwSignal<Option<web_sys::File>> = RwSignal::new(None);
    let f_preview = RwSignal::new(
        initial
            .as_ref()
            .map(|b| b.image_url.clone())
            .unwrap_or_default(),
    );

    let banner_id = initial.as_ref().map(|b| b.id).unwrap_or(0i64);
    let saving = RwSignal::new(false);
    let err = RwSignal::new(String::new());

    let do_submit = {
        let cb = on_saved.clone();
        move |_| {
            let img_url = f_img_url.get_untracked().trim().to_string();
            let img_file = f_file.get_untracked();
            if img_url.is_empty() && img_file.is_none() {
                err.set("Unggah gambar atau isi URL gambar.".into());
                return;
            }
            let start = f_start_date.get_untracked().trim().to_string();
            if start.is_empty() {
                err.set("Tanggal mulai wajib diisi.".into());
                return;
            }
            err.set(String::new());
            saving.set(true);

            let bid = banner_id;
            let click_url = f_click_url.get_untracked().trim().to_string();
            let end_date = f_end_date.get_untracked().trim().to_string();
            let event_id = f_event_id.get_untracked().trim().to_string();
            // Tambah ":00Z" agar jadi ISO 8601 lengkap yang diharapkan backend
            let start_iso = if start.len() == 16 {
                format!("{}:00Z", start)
            } else {
                start
            };
            let end_iso = if end_date.len() == 16 {
                format!("{}:00Z", end_date)
            } else {
                end_date
            };
            let cb2 = cb.clone();

            spawn_local(async move {
                let res = if is_edit {
                    api_update_banner(
                        bid, click_url, start_iso, end_iso, event_id, img_url, img_file,
                    )
                    .await
                } else {
                    api_create_banner(click_url, start_iso, end_iso, event_id, img_url, img_file)
                        .await
                };
                saving.set(false);
                match res {
                    Ok(saved) => cb2(saved),
                    Err(e) => err.set(format!("Gagal: {}", e.message)),
                }
            });
        }
    };

    let close_hdr = on_close.clone();

    view! {
        // backdrop — klik di luar tutup
        <div class="admin-backdrop"
             on:click=move |_| on_close()>

            // panel — klik di dalam tidak menutup
            <div class="admin-modal-panel"
                 on:click=|e| e.stop_propagation()>

                // header modal
                <div class="admin-modal-hdr">
                    <h2 class="admin-modal-ttl">
                        {if is_edit {"Sunting Spanduk"} else {"Tambah Spanduk"}}
                    </h2>
                    <button class="admin-modal-close"
                        on:click=move |_| close_hdr()>
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                            <line x1="18" y1="6" x2="6" y2="18"/>
                            <line x1="6" y1="6" x2="18" y2="18"/>
                        </svg>
                    </button>
                </div>

                // preview gambar
                <div class="admin-preview-box">
                    {move || {
                        let p = f_preview.get();
                        if p.is_empty() {
                            view!{
                                <div class="admin-preview-ph">
                                    <svg width="28" height="28" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="1.5">
                                        <rect x="3" y="3" width="18" height="18" rx="2"/>
                                        <circle cx="8.5" cy="8.5" r="1.5"/>
                                        <polyline points="21 15 16 10 5 21"/>
                                    </svg>
                                    <span>"Pratinjau gambar"</span>
                                </div>
                            }.into_any()
                        } else {
                            view!{ <img src=p alt="pratinjau" class="admin-preview-img"/> }.into_any()
                        }
                    }}
                    <label class="admin-upload-lbl">
                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/>
                            <polyline points="17 8 12 3 7 8"/>
                            <line x1="12" y1="3" x2="12" y2="15"/>
                        </svg>
                        "Unggah"
                        <input type="file" accept="image/*" style="display:none"
                            on:change=move |ev| {
                                use leptos::wasm_bindgen::JsCast;
                                let input = ev.target()
                                    .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok());
                                if let Some(inp) = input {
                                    if let Some(files) = inp.files() {
                                        if let Some(file) = files.get(0) {
                                            if let Ok(url) =
                                                web_sys::Url::create_object_url_with_blob(&file)
                                            {
                                                f_preview.set(url);
                                            }
                                            f_file.set(Some(file));
                                        }
                                    }
                                }
                            }
                        />
                    </label>
                </div>

                // form
                <div class="admin-modal-form">

                    <div class="mhub-form-row">
                        <label class="mhub-form-label">"URL GAMBAR (alternatif unggah)"</label>
                        <input class="mhub-form-input" type="url" placeholder="https://..."
                               prop:value=move || f_img_url.get()
                               on:input=move |e| {
                                   let v = event_target_value(&e);
                                   f_preview.set(v.clone());
                                   f_img_url.set(v);
                               }/>
                    </div>

                    <div class="mhub-form-row">
                        <label class="mhub-form-label">"URL KLIK (opsional)"</label>
                        <input class="mhub-form-input" type="url" placeholder="https://..."
                               prop:value=move || f_click_url.get()
                               on:input=move |e| f_click_url.set(event_target_value(&e))/>
                    </div>

                    <div class="admin-form-row2">
                        <div class="mhub-form-row" style="flex:1">
                            <label class="mhub-form-label">"TANGGAL MULAI *"</label>
                            <input class="mhub-form-input" type="datetime-local"
                                   prop:value=move || f_start_date.get()
                                   on:input=move |e| f_start_date.set(event_target_value(&e))/>
                        </div>
                        <div class="mhub-form-row" style="flex:1">
                            <label class="mhub-form-label">"TANGGAL SELESAI"</label>
                            <input class="mhub-form-input" type="datetime-local"
                                   prop:value=move || f_end_date.get()
                                   on:input=move |e| f_end_date.set(event_target_value(&e))/>
                        </div>
                    </div>

                    <div class="mhub-form-row">
                        <label class="mhub-form-label">"ID ACARA (opsional)"</label>
                        <input class="mhub-form-input" type="text" placeholder="ULID acara terkait"
                               prop:value=move || f_event_id.get()
                               on:input=move |e| f_event_id.set(event_target_value(&e))/>
                    </div>

                    {move || {
                        let e = err.get();
                        if e.is_empty() { view!{ <span/> }.into_any() }
                        else { view!{ <p class="admin-form-err">"⚠️ "{e}</p> }.into_any() }
                    }}

                    <button class="mhub-modal-submit"
                            style="width:100%;margin-top:14px"
                            disabled=move || saving.get()
                            on:click=do_submit>
                        {move || if saving.get()  { "Menyimpan..." }
                                 else if is_edit  { "Simpan Perubahan" }
                                 else             { "Buat Spanduk" }}
                    </button>
                </div>
            </div>
        </div>
    }
}

// ─── Tab: Analitik — identik view_mobile_analytics ───────────────────────────

fn view_admin_analytics(all_events: RwSignal<Vec<ExploreEvent>>) -> impl IntoView {
    view! {
        <section class="merchant-stats">
            <div class="merchant-card merchant-velocity" style="margin-bottom:12px">
                <h3 class="merchant-section-title">"Acara Terlaris (Platform)"</h3>
                {move || {
                    let mut evs = all_events.get();
                    evs.sort_by(|a, b| b.total_sold.cmp(&a.total_sold));
                    if let Some(top) = evs.first() {
                        let pct = if top.total_quota > 0 {
                            ((top.total_sold as f64 / top.total_quota as f64) * 100.0).round() as u32
                        } else { 0 };
                        let title = top.title.clone();
                        let sold  = top.total_sold;
                        let quota = top.total_quota;
                        view! {
                            <div style="margin-top:10px">
                                <p style="font-size:13px;font-weight:600;margin-bottom:6px">{title}</p>
                                <div style="display:flex;justify-content:space-between;margin-bottom:4px">
                                    <span class="merchant-label">{format!("{sold} terjual")}</span>
                                    <span class="merchant-label">{format!("{pct}%")}</span>
                                </div>
                                <div style="background:var(--bg-elevated);height:6px;border-radius:3px;overflow:hidden">
                                    <div style=format!("width:{}%;background:var(--accent-lime);height:6px;border-radius:3px", pct)></div>
                                </div>
                                <span class="merchant-label" style="margin-top:4px;display:block">
                                    {format!("{quota} total kuota")}
                                </span>
                            </div>
                        }.into_any()
                    } else {
                        view!{ <p class="merchant-label" style="margin-top:8px">"Belum ada data."</p> }.into_any()
                    }
                }}
            </div>
            <div class="merchant-tile-row" style="padding:0 16px">
                <div class="merchant-tile">
                    <span class="merchant-label">"TOTAL ACARA"</span>
                    <span class="merchant-tile-value">{move || all_events.with(|evs| evs.len())}</span>
                </div>
                <div class="merchant-tile merchant-tile--accent">
                    <span class="merchant-label">"ACARA AKTIF"</span>
                    <span class="merchant-tile-value">
                        {move || all_events.with(|evs| evs.iter().filter(|e| e.is_live).count())}
                    </span>
                </div>
            </div>
        </section>
    }
}

// ─── Tab: Keuangan — identik view_mobile_finance + data real ──────────────────

fn view_admin_finance(all_events: RwSignal<Vec<ExploreEvent>>) -> impl IntoView {
    let total_sold =
        Memo::new(move |_| all_events.with(|evs| evs.iter().map(|e| e.total_sold).sum::<i32>()));
    let total_quota =
        Memo::new(move |_| all_events.with(|evs| evs.iter().map(|e| e.total_quota).sum::<i32>()));

    view! {
        <section class="merchant-stats">
            <div class="merchant-card merchant-card--earnings">
                <span class="merchant-label">"TIKET TERJUAL (PLATFORM)"</span>
                <h2 class="merchant-amount">{move || format!("{}", total_sold.get())}</h2>
                <div class="merchant-trend-row">
                    <span class="merchant-trend-meta">
                        "Sisa: "{move || format!("{}", (total_quota.get() - total_sold.get()).max(0))}
                    </span>
                    <span class="merchant-trend-meta merchant-trend-meta--right">
                        "Kuota: "{move || format!("{}", total_quota.get())}
                    </span>
                </div>
            </div>
        </section>
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Ringkasan Platform"</h3>
            <div class="merchant-tile-row" style="margin-top:12px">
                <div class="merchant-tile">
                    <span class="merchant-label">"TOTAL ACARA"</span>
                    <span class="merchant-tile-value">{move || all_events.with(|evs| evs.len())}</span>
                </div>
                <div class="merchant-tile merchant-tile--accent">
                    <span class="merchant-label">"LANGSUNG"</span>
                    <span class="merchant-tile-value">
                        {move || all_events.with(|evs| evs.iter().filter(|e| e.is_live).count())}
                    </span>
                </div>
            </div>
        </section>
    }
}

// ─── Tab: Pengaturan — identik view_mobile_settings ──────────────────────────

fn view_admin_settings() -> impl IntoView {
    view! {
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Keamanan Akun"</h3>
            <div class="mhub-security-actions">
                <button class="mhub-security-btn">"🔒  Ganti Kata Sandi"</button>
                <button class="mhub-security-btn">
                    "📱  Aktifkan 2FA  "
                    <span class="mhub-security-badge mhub-security-badge--off">"MATI"</span>
                </button>
            </div>
        </section>
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Notifikasi Admin"</h3>
            {[("Acara Baru Didaftarkan",true),("Transaksi Platform",true),("Laporan Harian",false)]
                .iter().map(|(l, c)| view! {
                    <div class="mhub-toggle-row">
                        <span class="mhub-toggle-label">{*l}</span>
                        <label class="mhub-toggle-switch">
                            <input type="checkbox" prop:checked=*c/>
                            <span class="mhub-toggle-track"></span>
                        </label>
                    </div>
                }).collect_view()}
        </section>
    }
}
