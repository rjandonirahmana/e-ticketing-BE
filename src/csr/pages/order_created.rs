use chrono::{Datelike, Timelike};
/// Page ditampilkan setelah POST /orders berhasil dan order dalam status "pending".
/// User bisa melihat ringkasan order dan diarahkan ke halaman pembayaran atau tiket.
use leptos::prelude::*;
use leptos_router::components::A;

use crate::csr::hooks::use_cart;
use crate::csr::hooks::{format_idr, use_nav, ThemeToggle};
use crate::csr::models::ConfirmPaymentRequest;
use crate::csr::models::OrderRef;
use crate::csr::pages::payment_success::{use_payment_success, SuccessOrderSnapshot};
use crate::csr::services::chat as chat_svc;
use crate::csr::services::payment as pay_svc;
use leptos::task::spawn_local;

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
        now.day(),
        mon,
        now.hour(),
        now.minute(),
        now.second()
    )
}

/// Shared context: setelah create_order di checkout, simpan OrderRef di sini
/// supaya order_created page bisa membacanya.
#[derive(Clone)]
pub struct PendingOrderCtx {
    pub order: RwSignal<Option<OrderRef>>,
}

pub fn provide_pending_order() {
    provide_context(PendingOrderCtx {
        order: RwSignal::new(None),
    });
}

pub fn use_pending_order() -> PendingOrderCtx {
    use_context::<PendingOrderCtx>().expect("PendingOrderCtx not provided")
}

/// Format ISO timestamp menjadi tampilan lokal sederhana: "08 May 2026, 10:45 WIB"
fn fmt_expiry(iso: &str) -> String {
    // Ambil date & time part saja: 2026-05-08T10:45:28.061225Z
    let dt = iso.split('.').next().unwrap_or(iso);
    let (date_part, time_part) = {
        let mut parts = dt.splitn(2, 'T');
        (parts.next().unwrap_or(""), parts.next().unwrap_or("00:00"))
    };
    let months = [
        "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut dp = date_part.split('-');
    let y = dp.next().unwrap_or("2026");
    let m: usize = dp.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let d = dp.next().unwrap_or("01");
    let time_hm = time_part.get(..5).unwrap_or(time_part);
    let mon = months.get(m).copied().unwrap_or("?");
    format!("{} {} {}, {} WIB", d, mon, y, time_hm)
}

#[component]
pub fn OrderCreatedPage() -> impl IntoView {
    let ctx = use_pending_order();
    let navigate = use_nav();
    let cart = use_cart();
    let paying = RwSignal::new(false);
    let pay_error = RwSignal::new(String::new());
    let success_ctx = use_payment_success();

    // Hoist inner signals so the closure stays FnMut (all RwSignal are Copy)
    let pending_order_sig = ctx.order;
    let success_snap_sig = success_ctx.snapshot;

    // Jika tidak ada pending order, redirect ke explore
    let order_opt = ctx.order.get();
    if order_opt.is_none() {
        navigate("/explore", Default::default());
        return view! { <div></div> }.into_any();
    }
    let order = order_opt.unwrap();
    let order = StoredValue::new(order);

    // FIX #4: guard agar spawn_local payment tidak menulis ke signal
    // setelah page di-unmount (user tekan Back saat payment in-flight).
    let mounted = StoredValue::new(true);
    on_cleanup(move || mounted.set_value(false));

    let on_pay = move |_| {
        let o = order.get_value();
        paying.set(true);
        pay_error.set(String::new());
        let nav = navigate.clone();
        let cart_ref = cart;
        let pend_sig = pending_order_sig;
        let succ_sig = success_snap_sig;
        let snap_order = o.clone();
        spawn_local(async move {
            let req = ConfirmPaymentRequest {
                order_id: snap_order.id.clone(),
                payment_token: "qris".into(),
            };
            match pay_svc::confirm_payment(&req).await {
                Ok(_) => {
                    // Guard: page mungkin sudah di-unmount sebelum async selesai
                    if !mounted.get_value() {
                        return;
                    }
                    // Simpan snapshot untuk halaman sukses
                    let event_name = snap_order
                        .items
                        .first()
                        .map(|i| i.event_name.clone())
                        .unwrap_or_default();
                    succ_sig.set(Some(SuccessOrderSnapshot {
                        order_id: snap_order.id.clone(),
                        order_code: snap_order.order_code.clone(),
                        event_name,
                        event_date: snap_order
                            .created_at
                            .as_deref()
                            .and_then(|s| s.get(..10))
                            .unwrap_or("—")
                            .to_string(),
                        total_amount: snap_order.total_amount,
                        paid_at: now_formatted(),
                    }));

                    cart_ref.clear();
                    pend_sig.set(None);

                    // Auto-join group event (fire-and-forget, tidak perlu cancel)
                    let event_id = cart_ref.items.with_untracked(|v| {
                        v.first().and_then(|i| {
                            if i.event_id.is_empty() {
                                None
                            } else {
                                Some(i.event_id.clone())
                            }
                        })
                    });
                    if let Some(eid) = event_id {
                        spawn_local(async move {
                            let _ = chat_svc::join_event_group(&eid).await;
                        });
                    }

                    paying.set(false);
                    nav("/payment-success", Default::default());
                }
                Err(e) => {
                    if !mounted.get_value() {
                        return;
                    }
                    paying.set(false);
                    pay_error.set(e.message);
                }
            }
        });
    };

    view! {
        <div class="page oc-page">
            <header class="page-header">
                <A href="/cart" attr:class="back-btn">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="page-logo">"ORDER SUMMARY"</span>
                <div class="header-actions">
                    <ThemeToggle />
                    <A href="/profile" attr:class="nav-avatar">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/><circle cx="12" cy="7" r="4"/>
                        </svg>
                    </A>
                </div>
            </header>

            // ── Hero banner ──────────────────────────────────────────────
            <div class="oc-hero">
                <div class="oc-hero-icon">
                    <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M9 12l2 2 4-4"/><path d="M21 12v7a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2h7"/>
                        <polyline points="16 5 19 2 22 5"/><line x1="19" y1="2" x2="19" y2="13"/>
                    </svg>
                </div>
                <h1 class="oc-hero-title">"ORDER CREATED"</h1>
                <p class="oc-hero-sub">
                    "Your order is confirmed. Complete payment before it expires."
                </p>
            </div>

            // ── Event / Order card ───────────────────────────────────────
            <div class="oc-card">
                <div class="oc-card-header">
                    <div class="oc-live-dot"></div>
                    <span class="oc-live-label">"LIVE NOW"</span>
                </div>

                <div class="oc-event-block">
                    {move || {
                        let o = order.get_value();
                        let first = o.items.first();
                        let event_name = first.map(|i| i.event_name.as_str()).unwrap_or("Your Event").to_string();
                        view! {
                            <h2 class="oc-event-name">{event_name}</h2>
                        }
                    }}
                </div>

                // Ticket details
                <div class="oc-ticket-section">
                    <div class="oc-section-row">
                        <span class="oc-section-head">"TICKET DETAILS"</span>
                        <span class="oc-section-badge">
                            {move || {
                                let o = order.get_value();
                                let total_qty: i32 = o.items.iter().map(|i| i.quantity).sum();
                                format!("{}× Tickets", total_qty)
                            }}
                        </span>
                    </div>
                    {move || {
                        let o = order.get_value();
                        o.items.iter().map(|item| {
                            let name = item.variant_name.clone();
                            let qty = item.quantity;
                            let sub = format_idr(item.subtotal);
                            view! {
                                <div class="oc-ticket-row">
                                    <div>
                                        <div class="oc-ticket-name">{name}</div>
                                        <div class="oc-ticket-qty">{format!("{}× ticket", qty)}</div>
                                    </div>
                                    <div class="oc-ticket-price">{sub}</div>
                                </div>
                            }
                        }).collect_view()
                    }}
                </div>

                // Price breakdown
                <div class="oc-price-section">
                    <div class="oc-section-head">"PRICE BREAKDOWN"</div>
                    <div class="oc-price-row">
                        <span>"Subtotal"</span>
                        <span>{move || format_idr(order.get_value().total_amount)}</span>
                    </div>
                    <div class="oc-price-total-row">
                        <span class="oc-price-total-label">"TOTAL PAYABLE"</span>
                        <div class="oc-price-total-right">
                            <span class="oc-price-total-amt">{move || format_idr(order.get_value().total_amount)}</span>
                            <span class="oc-secure-badge">
                                <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                    <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
                                </svg>
                                "SECURE PAYMENT"
                            </span>
                        </div>
                    </div>
                </div>

                // Order meta
                <div class="oc-meta-section">
                    <div class="oc-meta-row">
                        <span class="oc-meta-label">"Order Code"</span>
                        <span class="oc-meta-val">{move || order.get_value().order_code.clone()}</span>
                    </div>
                    <div class="oc-meta-row">
                        <span class="oc-meta-label">"Status"</span>
                        <span class="oc-status-badge">
                            <span class="oc-status-dot"></span>
                            {move || order.get_value().status.to_uppercase()}
                        </span>
                    </div>
                    {move || {
                        let o = order.get_value();
                        o.expired_at.as_deref().map(|exp| {
                            let display = fmt_expiry(exp);
                            view! {
                                <div class="oc-meta-row">
                                    <span class="oc-meta-label">"Expires At"</span>
                                    <span class="oc-meta-val oc-expiry">{display}</span>
                                </div>
                            }
                        })
                    }}
                </div>
            </div>

            // ── QR / pay section ─────────────────────────────────────────
            <div class="oc-pay-section">
                <div class="oc-pay-head">"SCAN TO PAY VIA QRIS"</div>
                // Dummy QR (sama seperti design referensi)
                <div class="oc-qr-wrap">
                    <svg viewBox="0 0 160 160" width="180" height="180" xmlns="http://www.w3.org/2000/svg">
                        <rect width="160" height="160" fill="white" rx="10"/>
                        // Top-left finder
                        <rect x="10" y="10" width="50" height="50" fill="#0d0d1a"/>
                        <rect x="16" y="16" width="38" height="38" fill="white"/>
                        <rect x="22" y="22" width="26" height="26" fill="#0d0d1a"/>
                        // Top-right finder
                        <rect x="100" y="10" width="50" height="50" fill="#0d0d1a"/>
                        <rect x="106" y="16" width="38" height="38" fill="white"/>
                        <rect x="112" y="22" width="26" height="26" fill="#0d0d1a"/>
                        // Bottom-left finder
                        <rect x="10" y="100" width="50" height="50" fill="#0d0d1a"/>
                        <rect x="16" y="106" width="38" height="38" fill="white"/>
                        <rect x="22" y="112" width="26" height="26" fill="#0d0d1a"/>
                        // Data modules (pseudo-random pattern)
                        <rect x="70" y="10" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="82" y="10" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="70" y="22" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="82" y="34" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="70" y="46" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="10" y="70" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="22" y="82" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="34" y="70" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="46" y="82" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="70" y="70" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="82" y="82" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="94" y="70" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="106" y="70" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="118" y="82" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="130" y="70" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="82" y="94" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="70" y="106" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="94" y="106" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="106" y="118" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="118" y="106" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="130" y="118" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="142" y="106" width="8" height="8" fill="#0d0d1a"/>
                        <rect x="142" y="130" width="8" height="8" fill="#0d0d1a"/>
                        // QRIS label
                        <rect x="62" y="72" width="36" height="16" rx="3" fill="#4f6bff"/>
                        <text x="80" y="84" text-anchor="middle" fill="white" font-size="8" font-family="monospace" font-weight="bold">"QRIS"</text>
                    </svg>
                </div>

                <button class="oc-copy-btn" on:click=move |_| {
                    let code = order.get_value().order_code.clone();
                    if let Some(win) = web_sys::window() {
                        let _ = win.navigator().clipboard().write_text(&code);
                    }
                }>
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/>
                    </svg>
                    "COPY PAYMENT LINK"
                </button>

                <div class="oc-or-divider"><span>"OR"</span></div>

                {move || (!pay_error.get().is_empty()).then(|| view! {
                    <div class="pay-error">{pay_error.get()}</div>
                })}

                <button
                    class="oc-bank-btn"
                    disabled=move || paying.get()
                    on:click=on_pay
                >
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <line x1="3" y1="22" x2="21" y2="22"/><line x1="6" y1="18" x2="6" y2="11"/>
                        <line x1="10" y1="18" x2="10" y2="11"/><line x1="14" y1="18" x2="14" y2="11"/>
                        <line x1="18" y1="18" x2="18" y2="11"/><polygon points="12 2 20 7 4 7"/>
                    </svg>
                    {move || if paying.get() { "PROCESSING..." } else { "PAY VIA BANK TRANSFER" }}
                </button>
            </div>

            <p class="oc-terms">
                "BY CLICKING PROCEED, YOU AGREE TO THE KINETIC STAGE TERMS OF SERVICE AND THE VENUE'S ENTRY POLICY. TICKETS ARE NON-REFUNDABLE ONCE PAYMENT IS FINALIZED."
            </p>
        </div>
    }.into_any()
}
