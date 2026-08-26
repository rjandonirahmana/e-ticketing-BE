//! my_tickets.rs — Semua tiket milik user (layout ticket-* classes).

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::get_my_tickets;
use crate::web::app::AuthResource;
use crate::web::components::{BottomNav, EmptyState, ThemeToggle};
use crate::web::models::{format_date, format_price};

#[component]
pub fn MyTicketsPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let tickets = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in { get_my_tickets().await } else { Ok(vec![]) }
        },
    );

    view! {
        <div class="page ticket-page">
            <header class="page-header">
                <A href="/explore" attr:class="back-btn">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="page-logo">"MY TICKETS"</span>
                <div class="header-actions">
                    <ThemeToggle/>
                </div>
            </header>

            <Suspense fallback=|| view! {
                <div class="ticket-list">
                    {(0..3).map(|_| view! {
                        <div class="ticket-card">
                            <div class="ticket-accent shimmer-bg"></div>
                            <div class="ticket-body" style="display:flex;flex-direction:column;gap:10px;padding:16px">
                                <div class="shimmer-bg" style="width:70%;height:16px;border-radius:4px"></div>
                                <div class="shimmer-bg" style="width:50%;height:12px;border-radius:4px"></div>
                                <div class="shimmer-bg" style="width:40%;height:12px;border-radius:4px"></div>
                            </div>
                        </div>
                    }).collect_view()}
                </div>
            }>
                {move || {
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <EmptyState
                                icon="🔒"
                                title="HARUS MASUK"
                                body="Kamu harus masuk untuk melihat tiket."
                                cta_label="MASUK"
                                cta_href="/login"
                            />
                        }.into_any();
                    }

                    tickets.get().map(|res| match res {
                        Ok(list) if list.is_empty() => view! {
                            <EmptyState
                                icon="🎟"
                                title="BELUM ADA TIKET"
                                body="Beli tiket product favoritmu dan temukan pengalaman seru!"
                                cta_label="JELAJAHI EVENT"
                                cta_href="/explore"
                            />
                        }.into_any(),
                        Ok(list) => view! {
                            <div class="ticket-list">
                                {list.into_iter().map(|t| {
                                    let href = format!("/tickets/{}", t.id);
                                    let status = t.status.clone();
                                    let status_cls = if status == "used" || status == "checked_in" {
                                        "ticket-status-badge ticket-status-badge--used"
                                    } else {
                                        "ticket-status-badge ticket-status-badge--active"
                                    };
                                    let status_label = match status.as_str() {
                                        "used" | "checked_in" => "Sudah Digunakan",
                                        _ => "Aktif",
                                    };
                                    let date  = format_date(&t.event_date);
                                    let venue = t.event_venue.clone().unwrap_or_default();
                                    let price = format_price(t.unit_price);
                                    view! {
                                        <A href=href attr:class="ticket-card">
                                            <div class="ticket-accent"></div>
                                            <div class="ticket-body">
                                                <div class="ticket-product-name">{t.event_name.clone()}</div>
                                                <div class="ticket-variant">
                                                    {t.variant_name.clone()}
                                                    " · "
                                                    {price}
                                                </div>
                                                <div class="ticket-code">{t.ticket_code.clone()}</div>
                                                <div class="ticket-meta">
                                                    <svg width="11" height="11" viewBox="0 0 24 24" fill="none"
                                                         stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                                        <rect x="3" y="4" width="18" height="18" rx="2"/>
                                                        <line x1="16" y1="2" x2="16" y2="6"/>
                                                        <line x1="8" y1="2" x2="8" y2="6"/>
                                                        <line x1="3" y1="10" x2="21" y2="10"/>
                                                    </svg>
                                                    {date}
                                                    {(!venue.is_empty()).then(|| format!("  ·  📍 {venue}"))}
                                                </div>
                                                <span class=status_cls>{status_label}</span>
                                            </div>
                                        </A>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any(),
                        Err(_) => view! {
                            <EmptyState
                                icon="⚠️"
                                title="GAGAL MEMUAT"
                                body="Gagal memuat tiket. Coba login ulang."
                                cta_label="LOGIN"
                                cta_href="/login"
                            />
                        }.into_any(),
                    }).unwrap_or_else(|| view! { <div/> }.into_any())
                }}
            </Suspense>

            <BottomNav active="tickets"/>
        </div>
    }
}
