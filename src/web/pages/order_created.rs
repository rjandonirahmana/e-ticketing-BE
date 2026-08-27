/// Halaman yang ditampilkan setelah order dibuat dengan status "pending".
/// User bisa melihat ringkasan order dan melakukan pembayaran.
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::web::api::server_fns::confirm_order_payment;
use crate::web::app::PendingOrderCtx;
use crate::web::models::OrderRef;

fn format_idr(amount: i64) -> String {
    if amount == 0 {
        return "Gratis".to_string();
    }
    let s = amount.to_string();
    let chars: Vec<char> = s.chars().rev().collect();
    let grouped: String = chars
        .chunks(3)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(".")
        .chars()
        .rev()
        .collect();
    format!("Rp {}", grouped)
}

fn fmt_expiry(iso: &str) -> String {
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
    // StoredValue → .get_value() returns a clone setiap kali dipanggil,
    // sehingga reactive block {move || {}} tetap FnMut.
    let navigate = StoredValue::new(use_navigate());

    let ctx = use_context::<PendingOrderCtx>();

    // Order dari context (dikirim oleh checkout setelah create order)
    let pending_order: RwSignal<Option<OrderRef>> = match ctx {
        Some(ref c) => c.pending_order,
        None => RwSignal::new(None),
    };

    let paying = RwSignal::new(false);
    let pay_error = RwSignal::new(String::new());

    // Jika context success_order tersedia, simpan referensi
    let success_order_sig = ctx.as_ref().map(|c| c.success_order);

    view! {
        <div class="page oc-page">
            <header class="page-header">
                <A href="/cart" attr:class="back-btn">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="page-logo">"ORDER SUMMARY"</span>
            </header>

            {move || {
                let order = pending_order.get();
                if order.is_none() {
                    return view! {
                        <div class="oc-empty">
                            <p>"Tidak ada pesanan aktif."</p>
                            <A href="/orders" attr:class="btn btn--accent">"Lihat Riwayat Order"</A>
                        </div>
                    }.into_any();
                }
                let o = order.unwrap();
                let order_code = o.order_code.clone();
                let status = o.status.clone();
                let expired = o.expired_at.clone();
                let total = o.total_amount;
                let items = o.items.clone();
                let event_name = items.first().map(|i| i.event_name.clone()).unwrap_or_default();

                view! {
                    <div>
                        // Hero
                        <div class="oc-hero">
                            <div class="oc-hero-icon">
                                <svg width="36" height="36" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                    <path d="M9 12l2 2 4-4"/>
                                    <path d="M21 12v7a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2h7"/>
                                    <polyline points="16 5 19 2 22 5"/>
                                    <line x1="19" y1="2" x2="19" y2="13"/>
                                </svg>
                            </div>
                            <h1 class="oc-hero-title">"ORDER CREATED"</h1>
                            <p class="oc-hero-sub">"Complete payment before it expires."</p>
                        </div>

                        // Order card
                        <div class="oc-card">
                            <div class="oc-card-header">
                                <div class="oc-live-dot"></div>
                                <span class="oc-live-label">"LIVE NOW"</span>
                            </div>
                            <h2 class="oc-product-name">{event_name}</h2>

                            <div class="oc-ticket-section">
                                <div class="oc-section-row">
                                    <span class="oc-section-head">"RINCIAN PESANAN"</span>
                                    <span class="oc-section-badge">
                                        {format!("{}× barang", items.iter().map(|i| i.quantity).sum::<i32>())}
                                    </span>
                                </div>
                                {items.iter().map(|item| {
                                    let name = item.variant_name.clone();
                                    let qty = item.quantity;
                                    let sub = format_idr(item.subtotal);
                                    view! {
                                        <div class="oc-ticket-row">
                                            <div>
                                                <div class="oc-ticket-name">{name}</div>
                                                <div class="oc-ticket-qty">{format!("{}× barang", qty)}</div>
                                            </div>
                                            <div class="oc-ticket-price">{sub}</div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>

                            <div class="oc-price-section">
                                <div class="oc-price-total-row">
                                    <span class="oc-price-total-label">"TOTAL PAYABLE"</span>
                                    <span class="oc-price-total-amt">{format_idr(total)}</span>
                                </div>
                            </div>

                            <div class="oc-meta-section">
                                <div class="oc-meta-row">
                                    <span class="oc-meta-label">"Order Code"</span>
                                    <span class="oc-meta-val">{order_code.clone()}</span>
                                </div>
                                <div class="oc-meta-row">
                                    <span class="oc-meta-label">"Status"</span>
                                    <span class="oc-status-badge">
                                        <span class="oc-status-dot"></span>
                                        {status.to_uppercase()}
                                    </span>
                                </div>
                                {expired.into_iter().map(|exp| {
                                    let display = fmt_expiry(&exp);
                                    view! {
                                        <div class="oc-meta-row">
                                            <span class="oc-meta-label">"Expires At"</span>
                                            <span class="oc-meta-val oc-expiry">{display}</span>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        </div>

                        // Pay section
                        <div class="oc-pay-section">
                            <div class="oc-pay-head">"SCAN TO PAY VIA QRIS"</div>
                            <div class="oc-qr-wrap">
                                <svg viewBox="0 0 160 160" width="180" height="180" xmlns="http://www.w3.org/2000/svg">
                                    <rect width="160" height="160" fill="white" rx="10"/>
                                    <rect x="10" y="10" width="50" height="50" fill="#0d0d1a"/>
                                    <rect x="16" y="16" width="38" height="38" fill="white"/>
                                    <rect x="22" y="22" width="26" height="26" fill="#0d0d1a"/>
                                    <rect x="100" y="10" width="50" height="50" fill="#0d0d1a"/>
                                    <rect x="106" y="16" width="38" height="38" fill="white"/>
                                    <rect x="112" y="22" width="26" height="26" fill="#0d0d1a"/>
                                    <rect x="10" y="100" width="50" height="50" fill="#0d0d1a"/>
                                    <rect x="16" y="106" width="38" height="38" fill="white"/>
                                    <rect x="22" y="112" width="26" height="26" fill="#0d0d1a"/>
                                    <rect x="70" y="10" width="8" height="8" fill="#0d0d1a"/>
                                    <rect x="82" y="22" width="8" height="8" fill="#0d0d1a"/>
                                    <rect x="70" y="46" width="8" height="8" fill="#0d0d1a"/>
                                    <rect x="10" y="70" width="8" height="8" fill="#0d0d1a"/>
                                    <rect x="34" y="70" width="8" height="8" fill="#0d0d1a"/>
                                    <rect x="70" y="70" width="8" height="8" fill="#0d0d1a"/>
                                    <rect x="94" y="70" width="8" height="8" fill="#0d0d1a"/>
                                    <rect x="130" y="70" width="8" height="8" fill="#0d0d1a"/>
                                    <rect x="82" y="94" width="8" height="8" fill="#0d0d1a"/>
                                    <rect x="70" y="106" width="8" height="8" fill="#0d0d1a"/>
                                    <rect x="106" y="118" width="8" height="8" fill="#0d0d1a"/>
                                    <rect x="142" y="106" width="8" height="8" fill="#0d0d1a"/>
                                    <rect x="62" y="72" width="36" height="16" rx="3" fill="#4f6bff"/>
                                    <text x="80" y="84" text-anchor="middle" fill="white" font-size="8" font-family="monospace" font-weight="bold">"QRIS"</text>
                                </svg>
                            </div>

                            // Copy order code
                            <button class="oc-copy-btn" on:click={
                                #[allow(unused_variables)]
                                let code = order_code.clone();
                                move |_| {
                                    #[cfg(target_arch = "wasm32")]
                                    if let Some(win) = web_sys::window() {
                                        let _ = win.navigator().clipboard().write_text(&code);
                                    }
                                }
                            }>
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                    <rect x="9" y="9" width="13" height="13" rx="2"/>
                                    <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/>
                                </svg>
                                "COPY ORDER CODE"
                            </button>

                            <div class="oc-or-divider"><span>"OR"</span></div>

                            {move || (!pay_error.get().is_empty()).then(|| view! {
                                <div class="pay-error">{pay_error.get()}</div>
                            })}

                            <button
                                class="oc-bank-btn"
                                disabled=move || paying.get()
                                on:click=move |_| {
                                    let order = pending_order.get();
                                    let Some(o) = order else { return };
                                    paying.set(true);
                                    pay_error.set(String::new());
                                    let nav = navigate.get_value();
                                    let success_sig = success_order_sig;
                                    leptos::task::spawn_local(async move {
                                        match confirm_order_payment(o.id.clone()).await {
                                            Ok(updated) => {
                                                if let Some(sig) = success_sig {
                                                    sig.set(Some(crate::web::app::SuccessSnapshot {
                                                        order_code: updated.order_code.clone(),
                                                        event_name: updated.items.first()
                                                            .map(|i| i.event_name.clone())
                                                            .unwrap_or_default(),
                                                        total_amount: updated.total_amount,
                                                    }));
                                                }
                                                pending_order.set(None);
                                                paying.set(false);
                                                nav("/payment-success", Default::default());
                                            }
                                            Err(e) => {
                                                paying.set(false);
                                                pay_error.set(e.to_string());
                                            }
                                        }
                                    });
                                }
                            >
                                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                    <line x1="3" y1="22" x2="21" y2="22"/>
                                    <line x1="6" y1="18" x2="6" y2="11"/>
                                    <line x1="10" y1="18" x2="10" y2="11"/>
                                    <line x1="14" y1="18" x2="14" y2="11"/>
                                    <line x1="18" y1="18" x2="18" y2="11"/>
                                    <polygon points="12 2 20 7 4 7"/>
                                </svg>
                                {move || if paying.get() { "PROCESSING..." } else { "PAY VIA BANK TRANSFER" }}
                            </button>
                        </div>

                        <p class="oc-terms">
                            "DENGAN MELANJUTKAN, ANDA MENYETUJUI SYARAT LAYANAN PULSE DAN KEBIJAKAN TOKO. PESANAN TIDAK DAPAT DIBATALKAN SETELAH PEMBAYARAN SELESAI."
                        </p>
                    </div>
                }.into_any()
            }}

        </div>
    }
}
