mod analytics_tab;
mod banners_tab;
mod events_tab;
mod review_tab;

use analytics_tab::{view_analytics_admin, view_finance_admin, view_settings_admin};
use banners_tab::view_banners;
use events_tab::view_all_events;
use review_tab::view_review;

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::{get_admin_events, get_admin_stats, get_banners};
use crate::web::app::AuthResource;
use crate::web::components::{BottomNav, MerchantEventCardShimmer, ThemeToggle};
use crate::web::models::{Event, PaginatedEvents};

#[derive(Clone, Copy, PartialEq, Eq)]
enum AdminTab {
    Review,
    Events,
    Banners,
    Analytics,
    Finance,
    Settings,
}

impl AdminTab {
    fn label(self) -> &'static str {
        match self {
            Self::Review    => "Review",
            Self::Events    => "Acara",
            Self::Banners   => "Spanduk",
            Self::Analytics => "Analitik",
            Self::Finance   => "Keuangan",
            Self::Settings  => "Pengaturan",
        }
    }
}

#[component]
pub fn AdminPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");

    let is_admin = move || {
        auth.get()
            .and_then(|r| r.ok())
            .flatten()
            .map(|u| u.role == "admin")
            .unwrap_or(false)
    };

    let all_events_res = Resource::new(
        move || is_admin(),
        |ok| async move {
            if ok {
                get_admin_events(Some(1), None).await
            } else {
                Ok(PaginatedEvents {
                    data: vec![], total: 0, page: 1, per_page: 50, total_pages: 0,
                })
            }
        },
    );

    let pending_res = Resource::new(
        move || is_admin(),
        |ok| async move {
            if ok {
                get_admin_events(Some(1), Some("edited".to_string())).await
            } else {
                Ok(PaginatedEvents {
                    data: vec![], total: 0, page: 1, per_page: 50, total_pages: 0,
                })
            }
        },
    );

    let stats_res = Resource::new(
        move || is_admin(),
        |ok| async move {
            if ok {
                get_admin_stats().await
            } else {
                Err(ServerFnError::ServerError("denied".into()))
            }
        },
    );

    let banners_res = Resource::new(
        move || is_admin(),
        |ok| async move { if ok { get_banners().await } else { Ok(vec![]) } },
    );

    let all_events: RwSignal<Vec<Event>>     = RwSignal::new(vec![]);
    let pending_events: RwSignal<Vec<Event>> = RwSignal::new(vec![]);
    let all_loaded     = RwSignal::new(false);
    let pending_loaded = RwSignal::new(false);

    Effect::new(move |_| {
        if !all_loaded.get() {
            if let Some(Ok(pg)) = all_events_res.get() {
                all_events.set(pg.data);
                all_loaded.set(true);
            }
        }
    });
    Effect::new(move |_| {
        if !pending_loaded.get() {
            if let Some(Ok(pg)) = pending_res.get() {
                pending_events.set(pg.data);
                pending_loaded.set(true);
            }
        }
    });

    let total_sold   = move || all_events.with(|v| v.iter().map(|e| e.total_sold).sum::<i32>());
    let total_quota  = move || all_events.with(|v| v.iter().map(|e| e.total_quota).sum::<i32>());
    let capacity_pct = move || {
        let q = total_quota();
        if q == 0 { 0u32 } else { ((total_sold() as f64 / q as f64) * 100.0).round() as u32 }
    };

    let active_page = RwSignal::new(AdminTab::Review);
    let toast: RwSignal<Option<(String, bool)>> = RwSignal::new(None);

    let tabs = [
        AdminTab::Review,
        AdminTab::Events,
        AdminTab::Banners,
        AdminTab::Analytics,
        AdminTab::Finance,
        AdminTab::Settings,
    ];

    view! {
        <div class="page merchant-page mhub-mobile">

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
                    <A href="/scan" attr:class="mhub-scan-btn" attr:aria-label="Scan Tiket">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <polyline points="4 7 4 4 7 4"/>
                            <polyline points="20 7 20 4 17 4"/>
                            <polyline points="4 17 4 20 7 20"/>
                            <polyline points="20 17 20 20 17 20"/>
                            <rect x="8" y="8" width="8" height="8" rx="1"/>
                        </svg>
                        <span class="mhub-btn-label">"SCAN"</span>
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

            <div class="mhub-mobile-tabs">
                {tabs.iter().map(|tab| {
                    let t = *tab;
                    view! {
                        <button
                            class=move || if active_page.get() == t {
                                "mhub-mtab mhub-mtab--active"
                            } else {
                                "mhub-mtab"
                            }
                            on:click=move |_| active_page.set(t)>
                            {t.label()}
                            {move || {
                                if t == AdminTab::Review {
                                    let cnt = pending_events.with(|v| v.len());
                                    if cnt > 0 {
                                        return view! {
                                            <span style="background:#ef4444;color:#fff;border-radius:99px;\
                                                         font-size:0.65rem;font-weight:700;padding:1px 5px;\
                                                         margin-left:4px;vertical-align:middle">
                                                {cnt}
                                            </span>
                                        }.into_any();
                                    }
                                }
                                view! { <span/> }.into_any()
                            }}
                        </button>
                    }
                }).collect_view()}
            </div>

            {move || toast.get().map(|(msg, is_err)| {
                let cls = if is_err { "admin-toast admin-toast--err" } else { "admin-toast admin-toast--ok" };
                view! {
                    <div class=cls>
                        <span>{msg}</span>
                        <button class="admin-toast-x" on:click=move |_| toast.set(None)>"✕"</button>
                    </div>
                }
            })}

            <Suspense fallback=move || {
                (0..3).map(|_| view! { <MerchantEventCardShimmer /> }).collect_view()
            }>
                {move || {
                    let evs_all = if all_loaded.get() {
                        all_events.get()
                    } else {
                        all_events_res.get()
                            .and_then(|r| r.ok())
                            .map(|pg| pg.data)
                            .unwrap_or_default()
                    };
                    let stats_opt = stats_res.get().and_then(|r| r.ok());
                    let banners_list = banners_res.get()
                        .and_then(|r| r.ok())
                        .unwrap_or_default();

                    match active_page.get() {
                        AdminTab::Review    => view_review(pending_events, all_events, toast).into_any(),
                        AdminTab::Events    => view_all_events(evs_all).into_any(),
                        AdminTab::Banners   => view_banners(banners_list).into_any(),
                        AdminTab::Analytics => view_analytics_admin(evs_all, stats_opt).into_any(),
                        AdminTab::Finance   => view_finance_admin(evs_all).into_any(),
                        AdminTab::Settings  => view_settings_admin().into_any(),
                    }
                }}
            </Suspense>

        </div>
        <BottomNav active="admin" />
    }
}
