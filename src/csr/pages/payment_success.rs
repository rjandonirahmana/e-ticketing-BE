/// Page ditampilkan setelah pembayaran berhasil dikonfirmasi (POST /orders/:id/pay).
/// Menampilkan konfirmasi sukses dengan ringkasan order dan aksi ke tiket.
use leptos::prelude::*;
use chrono::{Datelike, Timelike};
use leptos_router::components::A;

use crate::csr::hooks::{format_idr, use_nav, ThemeToggle};

/// Simpan snapshot order setelah payment sukses (diisi dari checkout sebelum clear cart)
#[derive(Clone, Debug)]
pub struct SuccessOrderSnapshot {
    pub order_id: String,
    pub order_code: String,
    pub event_name: String,
    pub event_date: String,
    pub total_amount: i64,
    pub paid_at: String, // formatted
}

#[derive(Clone)]
pub struct PaymentSuccessCtx {
    pub snapshot: RwSignal<Option<SuccessOrderSnapshot>>,
}

pub fn provide_payment_success() {
    provide_context(PaymentSuccessCtx {
        snapshot: RwSignal::new(None),
    });
}

pub fn use_payment_success() -> PaymentSuccessCtx {
    use_context::<PaymentSuccessCtx>().expect("PaymentSuccessCtx not provided")
}

fn now_formatted() -> String {
    // chrono dengan feature "wasmbind" otomatis pakai JS Date di WASM
    let wib = chrono::FixedOffset::east_opt(7 * 3600).unwrap();
    let now = chrono::Utc::now().with_timezone(&wib);
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mon = months.get(now.month() as usize - 1).copied().unwrap_or("?");
    format!(
        "{} {}, {:02}:{:02}:{:02} WIB",
        now.day(), mon, now.hour(), now.minute(), now.second()
    )
}

#[component]
pub fn PaymentSuccessPage() -> impl IntoView {
    let navigate = use_nav();
    let success_ctx = use_payment_success();

    // Buat snapshot dummy jika tidak ada (navigasi langsung ke URL ini)
    let snap = success_ctx
        .snapshot
        .get()
        .unwrap_or_else(|| SuccessOrderSnapshot {
            order_id: "KS-0000-0000-XX".into(),
            order_code: "ORDER".into(),
            event_name: "Your Event".into(),
            event_date: "—".into(),
            total_amount: 0,
            paid_at: now_formatted(),
        });
    let snap = StoredValue::new(snap);

    view! {
        <div class="page ps-page">
            <header class="page-header">
                <div style="width:36px"></div>
                <span class="page-logo">"KINETIC"</span>
                <div class="header-actions">
                    <ThemeToggle />
                    <A href="/profile" attr:class="nav-avatar">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/><circle cx="12" cy="7" r="4"/>
                        </svg>
                    </A>
                </div>
            </header>

            // ── Success hero ─────────────────────────────────────────────
            <div class="ps-hero">
                <div class="ps-check-circle">
                    <div class="ps-check-inner">
                        <svg width="44" height="44" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                            <polyline points="20 6 9 17 4 12"/>
                        </svg>
                    </div>
                </div>
                <h1 class="ps-title">"Payment Successful"</h1>
                <p class="ps-sub">"Your setlist is ready. See you at the stage."</p>
            </div>

            // ── Event summary card ───────────────────────────────────────
            <div class="ps-card">
                <div class="ps-card-top">
                    <div>
                        <div class="ps-card-label">"EVENT"</div>
                        <div class="ps-card-event">{move || snap.get_value().event_name.clone()}</div>
                        <div class="ps-card-date-row">
                            <span class="ps-card-label">"DATE"</span>
                            <span class="ps-card-date">{move || snap.get_value().event_date.clone()}</span>
                        </div>
                    </div>
                    <div class="ps-ticket-icon">
                        <svg width="56" height="56" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" opacity="0.25">
                            <path d="M15 5v2m0 4v2m0 4v2M5 5a2 2 0 00-2 2v3a2 2 0 110 4v3a2 2 0 002 2h14a2 2 0 002-2v-3a2 2 0 110-4V7a2 2 0 00-2-2H5z"/>
                        </svg>
                    </div>
                </div>
                <div class="ps-card-price-row">
                    <span class="ps-card-label">"PRICE PAID"</span>
                    <span class="ps-card-price">{move || format_idr(snap.get_value().total_amount)}</span>
                </div>
            </div>

            // ── Order meta ───────────────────────────────────────────────
            <div class="ps-meta-card">
                <div class="ps-meta-row">
                    <span class="ps-meta-label">"Order ID"</span>
                    <span class="ps-meta-val">{move || snap.get_value().order_code.clone()}</span>
                </div>
                <div class="ps-meta-divider"></div>
                <div class="ps-meta-row">
                    <span class="ps-meta-label">"Status"</span>
                    <span class="ps-status">
                        <span class="ps-status-dot"></span>
                        "CONFIRMED"
                    </span>
                </div>
                <div class="ps-meta-divider"></div>
                <div class="ps-meta-row">
                    <span class="ps-meta-label">"Timestamp"</span>
                    <span class="ps-meta-val">{move || snap.get_value().paid_at.clone()}</span>
                </div>
            </div>

            // ── CTA buttons ──────────────────────────────────────────────
            <div class="ps-actions">
                <A href="/tickets" attr:class="ps-primary-btn">
                    "View My Tickets"
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <line x1="5" y1="12" x2="19" y2="12"/>
                        <polyline points="12 5 19 12 12 19"/>
                    </svg>
                </A>
                <button class="ps-secondary-btn" on:click=move |_| navigate("/explore", Default::default())>
                    "Back to Home"
                </button>
            </div>
        </div>
    }
}
