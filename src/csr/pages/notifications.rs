use leptos::prelude::*;
use leptos_router::components::A;

use crate::csr::components::BottomNav;
use crate::csr::hooks::ThemeToggle;
use crate::csr::state::use_notifications_store;

fn icon_for(kind: &str) -> impl IntoView {
    match kind {
        "success" => view! {
            <svg
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="#c8ff5e"
                stroke-width="2.4"
                stroke-linecap="round"
                stroke-linejoin="round"
            >
                <path d="M22 11.08V12a10 10 0 11-5.93-9.14" />
                <polyline points="22 4 12 14.01 9 11.01" />
            </svg>
        }
        .into_any(),
        "promo" => view! {
            <svg
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="#b78cff"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
            >
                <path d="M2 9a3 3 0 010-6h20a3 3 0 010 6H2zM2 15a3 3 0 000 6h20a3 3 0 000-6H2z" />
            </svg>
        }
        .into_any(),
        "artist" => view! { <div class="notif-avatar"></div> }.into_any(),
        _ => view! {
            <svg
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="#7da7ff"
                stroke-width="2"
                stroke-linecap="round"
                stroke-linejoin="round"
            >
                <path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9" />
                <path d="M13.73 21a2 2 0 01-3.46 0" />
            </svg>
        }
        .into_any(),
    }
}

#[component]
pub fn NotificationsPage() -> impl IntoView {
    let store = use_notifications_store();
    store.load();
    let groups = ["TODAY", "PROMOTIONS"];

    view! {
        <div class="page notif-page">
            <header class="page-header">
                <A href="/" attr:class="back-btn">
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
                <span class="page-logo">"NOTIFICATIONS"</span>
                <div class="header-actions">
                    <ThemeToggle />
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

            {move || {
                store
                    .items
                    .with(|notifs| {
                        let total_items: usize = groups
                            .iter()
                            .map(|g| notifs.iter().filter(|n| n.section == *g).count())
                            .sum();
                        if total_items == 0 {
                            // Cek apakah semua group kosong

                            // ── EMPTY STATE ──────────────────────────────────────────
                            view! {
                                <div class="notif-empty-wrap">
                                    <div class="notif-empty-icon-wrap">
                                        <div class="notif-empty-bell-circle">
                                            <svg
                                                width="32"
                                                height="32"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="1.5"
                                                stroke-linecap="round"
                                                stroke-linejoin="round"
                                            >
                                                <path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9" />
                                                <path d="M13.73 21a2 2 0 01-3.46 0" />
                                            </svg>
                                        </div>
                                        <div class="notif-empty-bell-dot"></div>
                                    </div>

                                    <h2 class="notif-empty-title">"SEMUA TENANG"</h2>
                                    <p class="notif-empty-body">
                                        "Belum ada notifikasi untukmu. Notifikasi tentang tiket, promo, dan event akan muncul di sini."
                                    </p>

                                    <A href="/explore" attr:class="notif-empty-cta">
                                        <svg
                                            width="14"
                                            height="14"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2"
                                            stroke-linecap="round"
                                        >
                                            <circle cx="11" cy="11" r="8" />
                                            <line x1="21" y1="21" x2="16.65" y2="16.65" />
                                        </svg>
                                        "JELAJAHI EVENT"
                                    </A>

                                    <div class="notif-empty-tips">
                                        <div class="notif-empty-tip">
                                            <div class="notif-empty-tip-icon">
                                                <svg
                                                    width="16"
                                                    height="16"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2"
                                                    stroke-linecap="round"
                                                >
                                                    <path d="M22 11.08V12a10 10 0 11-5.93-9.14" />
                                                    <polyline points="22 4 12 14.01 9 11.01" />
                                                </svg>
                                            </div>
                                            <div class="notif-empty-tip-body">
                                                <span class="notif-empty-tip-title">
                                                    "Konfirmasi tiket"
                                                </span>
                                                <span class="notif-empty-tip-sub">
                                                    "Notifikasi muncul setelah pembelian berhasil."
                                                </span>
                                            </div>
                                        </div>
                                        <div class="notif-empty-tip">
                                            <div class="notif-empty-tip-icon">
                                                <svg
                                                    width="16"
                                                    height="16"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2"
                                                    stroke-linecap="round"
                                                >
                                                    <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
                                                </svg>
                                            </div>
                                            <div class="notif-empty-tip-body">
                                                <span class="notif-empty-tip-title">
                                                    "Promo & flash sale"
                                                </span>
                                                <span class="notif-empty-tip-sub">
                                                    "Dapatkan penawaran terbaik sebelum kehabisan."
                                                </span>
                                            </div>
                                        </div>
                                        <div class="notif-empty-tip">
                                            <div class="notif-empty-tip-icon">
                                                <svg
                                                    width="16"
                                                    height="16"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2"
                                                    stroke-linecap="round"
                                                >
                                                    <path d="M9 19c-5 1.5-5-2.5-7-3m14 6v-3.87a3.37 3.37 0 00-.94-2.61c3.14-.35 6.44-1.54 6.44-7A5.44 5.44 0 0020 4.77 5.07 5.07 0 0019.91 1S18.73.65 16 2.48a13.38 13.38 0 00-7 0C6.27.65 5.09 1 5.09 1A5.07 5.07 0 005 4.77a5.44 5.44 0 00-1.5 3.78c0 5.42 3.3 6.61 6.44 7A3.37 3.37 0 009 18.13V22" />
                                                </svg>
                                            </div>
                                            <div class="notif-empty-tip-body">
                                                <span class="notif-empty-tip-title">
                                                    "Update dari artist"
                                                </span>
                                                <span class="notif-empty-tip-sub">
                                                    "Ikuti artis favoritmu untuk notifikasi jadwal baru."
                                                </span>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            }
                                .into_any()
                        } else {
                            // ── NORMAL LIST ──────────────────────────────────────────
                            view! {
                                <div class="notif-list">
                                    {groups
                                        .iter()
                                        .map(|g| {
                                            let label = *g;
                                            let section_items: Vec<_> = notifs
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
                                                        .map(|n| {
                                                            let icon_class = format!(
                                                                "notif-icon notif-icon--{}",
                                                                n.kind,
                                                            );
                                                            let href = format!("/notifications/{}", n.id);
                                                            let has_pill = n.pill.is_some();
                                                            let pill_text = n.pill.clone().unwrap_or_default();
                                                            let pill_cls = format!(
                                                                "notif-pill notif-pill--{}",
                                                                n.pill_kind,
                                                            );
                                                            let is_live = n.pill_kind == "live";
                                                            let body_view = if n.kind == "success" {
                                                                view! {
                                                                    <p class="notif-body">
                                                                        "Transaction for "<strong>"Midnight Jazz"</strong>
                                                                        " confirmed. Total: "
                                                                        <span class="notif-amount">"Rp1.250.000"</span>
                                                                    </p>
                                                                }
                                                                    .into_any()
                                                            } else if n.kind == "artist" {
                                                                view! {
                                                                    <p class="notif-body">
                                                                        <span class="notif-highlight">"Void Echoes"</span>
                                                                        " just added a new date to their Tokyo residency. Get notified when tickets drop."
                                                                    </p>
                                                                }
                                                                    .into_any()
                                                            } else {
                                                                view! { <p class="notif-body">{n.body.clone()}</p> }
                                                                    .into_any()
                                                            };
                                                            let kind_for_icon = n.kind.clone();
                                                            let kind_for_alert = n.kind.clone();
                                                            let time_for_top = n.time.clone();
                                                            let time_for_meta = n.time.clone();
                                                            let title_for_top = n.title.clone();
                                                            let cta = n.cta.clone();
                                                            view! {
                                                                <A href=href attr:class="notif-card">
                                                                    <div class=icon_class>{icon_for(&kind_for_icon)}</div>
                                                                    <div class="notif-content">
                                                                        <div class="notif-row-top">
                                                                            <h4 class="notif-title">{title_for_top}</h4>
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
                                                                                .then(|| {
                                                                                    view! { <span class="notif-time">{time_for_top}</span> }
                                                                                })}
                                                                        </div>
                                                                        {body_view}
                                                                        {(kind_for_alert == "alert")
                                                                            .then(|| {
                                                                                view! { <span class="notif-meta">{time_for_meta}</span> }
                                                                            })}
                                                                        {cta
                                                                            .map(|c| view! { <button class="notif-cta">{c}</button> })}
                                                                    </div>
                                                                </A>
                                                            }
                                                        })
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
            }}

            <BottomNav active="" />
        </div>
    }
}
