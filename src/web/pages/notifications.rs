//! notifications.rs — Halaman Notifikasi (unified SSR + hydration).
//!
//! Port dari `csr/pages/notifications.rs`:
//!   - `use_notifications_store()` (spawn_local, client-only) → `get_notifications`
//!     server fn via `Resource` (data ter-render saat SSR).
//!   - Logika derivasi section/pill/icon dari `be_to_notif` direplikasi murni
//!     di sisi server pada `NotificationItem` → struct view `NotifView`.
//!   - Layout grup (TODAY / PROMOTIONS), pill, icon per-kind, dan empty state
//!     bertips dipertahankan identik dengan CSR.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::hooks::ThemeToggle;
use crate::web::api::get_notifications;
use crate::web::app::AuthResource;
use crate::web::components::BottomNav;
use crate::web::models::NotificationItem;

// ── Shimmer skeleton list (5 cards, 2 groups, staggered delay) ───────────────

#[component]
fn NotifListSkeleton() -> impl IntoView {
    view! {
        <div class="notif-list">
            // ── TODAY ──────────────────────────────────────────────────────
            <div class="notif-group">
                <span class="shimmer-section-label"
                    style="animation-delay:0s;display:block;"></span>
                // Card 1
                <div class="notif-card notif-card--shimmer">
                    <div class="shimmer-icon" style="animation-delay:0.04s;"></div>
                    <div class="notif-content">
                        <div class="notif-row-top">
                            <div class="shimmer-line" style="width:55%;animation-delay:0.06s;"></div>
                            <div class="shimmer-bg" style="width:38px;height:18px;border-radius:100px;animation-delay:0.08s;"></div>
                        </div>
                        <div class="shimmer-line shimmer-line--medium" style="animation-delay:0.1s;"></div>
                    </div>
                </div>
                // Card 2
                <div class="notif-card notif-card--shimmer">
                    <div class="shimmer-icon" style="animation-delay:0.12s;"></div>
                    <div class="notif-content">
                        <div class="notif-row-top">
                            <div class="shimmer-line" style="width:65%;animation-delay:0.14s;"></div>
                            <div class="shimmer-bg" style="width:44px;height:10px;border-radius:4px;animation-delay:0.16s;"></div>
                        </div>
                        <div class="shimmer-line shimmer-line--long" style="width:70%;animation-delay:0.18s;"></div>
                    </div>
                </div>
                // Card 3
                <div class="notif-card notif-card--shimmer">
                    <div class="shimmer-icon" style="animation-delay:0.2s;"></div>
                    <div class="notif-content">
                        <div class="notif-row-top">
                            <div class="shimmer-line" style="width:72%;animation-delay:0.22s;"></div>
                            <div class="shimmer-bg" style="width:38px;height:18px;border-radius:100px;animation-delay:0.24s;"></div>
                        </div>
                        <div class="shimmer-line shimmer-line--medium" style="width:90%;animation-delay:0.26s;"></div>
                    </div>
                </div>
            </div>

            // ── PROMOTIONS ─────────────────────────────────────────────────
            <div class="notif-group">
                <span class="shimmer-section-label"
                    style="width:80px;animation-delay:0.3s;display:block;"></span>
                // Card 4
                <div class="notif-card notif-card--shimmer">
                    <div class="shimmer-icon" style="animation-delay:0.33s;"></div>
                    <div class="notif-content">
                        <div class="notif-row-top">
                            <div class="shimmer-line" style="width:50%;animation-delay:0.35s;"></div>
                            <div class="shimmer-bg" style="width:46px;height:18px;border-radius:100px;animation-delay:0.37s;"></div>
                        </div>
                        <div class="shimmer-line shimmer-line--medium" style="width:75%;animation-delay:0.39s;"></div>
                    </div>
                </div>
                // Card 5
                <div class="notif-card notif-card--shimmer">
                    <div class="shimmer-icon" style="animation-delay:0.43s;"></div>
                    <div class="notif-content">
                        <div class="notif-row-top">
                            <div class="shimmer-line" style="width:60%;animation-delay:0.45s;"></div>
                            <div class="shimmer-bg" style="width:44px;height:10px;border-radius:4px;animation-delay:0.47s;"></div>
                        </div>
                        <div class="shimmer-line shimmer-line--long" style="width:85%;animation-delay:0.49s;"></div>
                    </div>
                </div>
            </div>
        </div>
    }
}

// ── View model (hasil derivasi server-side, setara `be_to_notif` di CSR) ──────
#[derive(Clone)]
struct NotifView {
    id: String,
    icon_kind: &'static str, // success | promo | artist | bell
    title: String,
    body: String,
    time: String,
    pill: Option<&'static str>,
    pill_kind: &'static str, // live | promo | new
    section: &'static str,   // TODAY | PROMOTIONS
}

fn map_notif(n: &NotificationItem) -> NotifView {
    let (icon_kind, pill, pill_kind): (&'static str, Option<&'static str>, &'static str) =
        match n.kind.as_str() {
            "payment_success" | "order_paid" => ("success", Some("PAID"), "live"),
            "promo" => ("promo", Some("PROMO"), "promo"),
            "artist_update" => ("artist", Some("NEW"), "new"),
            "product_reminder" => ("bell", None, "new"),
            _ => ("bell", None, "new"),
        };
    let section = if n.kind == "promo" { "PROMOTIONS" } else { "TODAY" };
    let time = n
        .created_at
        .map(|dt| dt.format("%d %b %H:%M").to_string())
        .unwrap_or_else(|| "—".into());

    NotifView {
        id: n.id.clone(),
        icon_kind,
        title: n.title.to_uppercase(),
        body: n.body.clone(),
        time,
        pill,
        pill_kind,
        section,
    }
}

fn icon_for(kind: &str) -> impl IntoView {
    match kind {
        "success" => view! {
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#c8ff5e"
                stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
                <path d="M22 11.08V12a10 10 0 11-5.93-9.14" />
                <polyline points="22 4 12 14.01 9 11.01" />
            </svg>
        }
        .into_any(),
        "promo" => view! {
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#b78cff"
                stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M2 9a3 3 0 010-6h20a3 3 0 010 6H2zM2 15a3 3 0 000 6h20a3 3 0 000-6H2z" />
            </svg>
        }
        .into_any(),
        "artist" => view! { <div class="notif-avatar"></div> }.into_any(),
        _ => view! {
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#7da7ff"
                stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9" />
                <path d="M13.73 21a2 2 0 01-3.46 0" />
            </svg>
        }
        .into_any(),
    }
}

#[component]
pub fn NotificationsPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let notifs = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in { get_notifications().await } else { Ok(vec![]) }
        },
    );

    let groups = ["TODAY", "PROMOTIONS"];

    view! {
        <div class="page notif-page">
            <header class="page-header">
                <A href="/" attr:class="back-btn">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                        stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </A>
                <span class="page-logo">"NOTIFICATIONS"</span>
                <div class="header-actions">
                    <ThemeToggle />
                </div>
            </header>

            <Suspense fallback=|| view! { <NotifListSkeleton/> }>
                {move || {
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <div style="text-align:center;padding:4rem 0">
                                <p style="color:var(--clr-muted);margin-bottom:1.5rem">
                                    "Kamu harus masuk untuk melihat notifikasi."
                                </p>
                                <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                            </div>
                        }
                            .into_any();
                    }

                    notifs
                        .get()
                        .map(|res| {
                            let list: Vec<NotifView> = match res {
                                Ok(items) => items.iter().map(map_notif).collect(),
                                Err(_) => vec![],
                            };

                            if list.is_empty() {
                                empty_state().into_any()
                            } else {
                                view! {
                                    <div class="notif-list">
                                        {groups
                                            .iter()
                                            .map(|g| {
                                                let label = *g;
                                                let section_items: Vec<NotifView> = list
                                                    .iter()
                                                    .filter(|n| n.section == label)
                                                    .cloned()
                                                    .collect();
                                                if section_items.is_empty() {
                                                    return view! { <div></div> }.into_any();
                                                }
                                                view! {
                                                    <div class="notif-group">
                                                        <span class="notif-section-label">{label}</span>
                                                        {section_items
                                                            .into_iter()
                                                            .map(notif_card)
                                                            .collect_view()}
                                                    </div>
                                                }
                                                    .into_any()
                                            })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            }
                        })
                        .unwrap_or_else(|| view! { <div /> }.into_any())
                }}
            </Suspense>

            <BottomNav active="" />
        </div>
    }
}

fn notif_card(n: NotifView) -> impl IntoView {
    let icon_class = format!("notif-icon notif-icon--{}", n.icon_kind);
    let href = format!("/notifications/{}", n.id);
    let pill_cls = format!("notif-pill notif-pill--{}", n.pill_kind);
    let is_live = n.pill_kind == "live";
    let has_pill = n.pill.is_some();
    let pill_text = n.pill.unwrap_or_default();
    let icon_kind = n.icon_kind;

    view! {
        <A href=href attr:class="notif-card">
            <div class=icon_class>{icon_for(icon_kind)}</div>
            <div class="notif-content">
                <div class="notif-row-top">
                    <h4 class="notif-title">{n.title}</h4>
                    {has_pill
                        .then(|| {
                            view! {
                                <span class=pill_cls>
                                    {is_live.then(|| view! { <span class="pulse-dot"></span> })}
                                    {pill_text}
                                </span>
                            }
                        })}
                    {(!has_pill)
                        .then(|| view! { <span class="notif-time">{n.time}</span> })}
                </div>
                <p class="notif-body">{n.body}</p>
            </div>
        </A>
    }
}

fn empty_state() -> impl IntoView {
    view! {
        <div class="notif-empty-wrap">
            <div class="notif-empty-icon-wrap">
                <div class="notif-empty-bell-circle">
                    <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                        stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9" />
                        <path d="M13.73 21a2 2 0 01-3.46 0" />
                    </svg>
                </div>
                <div class="notif-empty-bell-dot"></div>
            </div>

            <h2 class="notif-empty-title">"SEMUA TENANG"</h2>
            <p class="notif-empty-body">
                "Belum ada notifikasi untukmu. Notifikasi tentang tiket, promo, dan product akan muncul di sini."
            </p>

            <A href="/explore" attr:class="notif-empty-cta">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                    stroke-width="2" stroke-linecap="round">
                    <circle cx="11" cy="11" r="8" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                </svg>
                "JELAJAHI EVENT"
            </A>

            <div class="notif-empty-tips">
                <div class="notif-empty-tip">
                    <div class="notif-empty-tip-icon">
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                            stroke-width="2" stroke-linecap="round">
                            <path d="M22 11.08V12a10 10 0 11-5.93-9.14" />
                            <polyline points="22 4 12 14.01 9 11.01" />
                        </svg>
                    </div>
                    <div class="notif-empty-tip-body">
                        <span class="notif-empty-tip-title">"Konfirmasi tiket"</span>
                        <span class="notif-empty-tip-sub">
                            "Notifikasi muncul setelah pembelian berhasil."
                        </span>
                    </div>
                </div>
                <div class="notif-empty-tip">
                    <div class="notif-empty-tip-icon">
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                            stroke-width="2" stroke-linecap="round">
                            <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
                        </svg>
                    </div>
                    <div class="notif-empty-tip-body">
                        <span class="notif-empty-tip-title">"Promo & flash sale"</span>
                        <span class="notif-empty-tip-sub">
                            "Dapatkan penawaran terbaik sebelum kehabisan."
                        </span>
                    </div>
                </div>
                <div class="notif-empty-tip">
                    <div class="notif-empty-tip-icon">
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                            stroke-width="2" stroke-linecap="round">
                            <path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 00-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0020 4.77 5.07 5.07 0 0019.91 1S18.73.65 16 2.48a13.38 13.38 0 00-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 005 4.77a5.44 5.44 0 00-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 009 18.13V22" />
                        </svg>
                    </div>
                    <div class="notif-empty-tip-body">
                        <span class="notif-empty-tip-title">"Update dari artist"</span>
                        <span class="notif-empty-tip-sub">
                            "Ikuti artis favoritmu untuk notifikasi jadwal baru."
                        </span>
                    </div>
                </div>
            </div>
        </div>
    }
}
