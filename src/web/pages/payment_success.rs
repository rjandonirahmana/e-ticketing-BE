/// Page yang ditampilkan setelah pembayaran berhasil.
/// Membaca order_id dari query param (?order_id=...) atau context signal.
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::web::app::PendingOrderCtx;

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

#[component]
pub fn PaymentSuccessPage() -> impl IntoView {
    let query = use_query_map();

    // Baca snapshot dari context jika tersedia (set oleh checkout setelah order sukses)
    let order_ctx = use_context::<PendingOrderCtx>();

    let order_code = move || {
        // Prioritaskan context, fallback ke query param
        if let Some(ctx) = &order_ctx {
            if let Some(ord) = ctx.success_order.get() {
                return ord.order_code;
            }
        }
        query.read().get("order_code").unwrap_or_default()
    };

    let event_name = move || {
        if let Some(ctx) = &order_ctx {
            if let Some(ord) = ctx.success_order.get() {
                return ord.event_name;
            }
        }
        query.read().get("event").unwrap_or_else(|| "Your Event".to_string())
    };

    let total_amount = move || {
        if let Some(ctx) = &order_ctx {
            if let Some(ord) = ctx.success_order.get() {
                return format_idr(ord.total_amount);
            }
        }
        query
            .read()
            .get("amount")
            .and_then(|s| s.parse::<i64>().ok())
            .map(format_idr)
            .unwrap_or_else(|| "—".to_string())
    };

    view! {
        <div class="page ps-page">
            <header class="page-header">
                <div style="width:36px"></div>
                <span class="page-logo">"PULSE"</span>
                <A href="/profile" attr:class="nav-avatar">
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/>
                        <circle cx="12" cy="7" r="4"/>
                    </svg>
                </A>
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
                        <div class="ps-card-event">{event_name}</div>
                    </div>
                    <div class="ps-ticket-icon">
                        <svg width="56" height="56" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" opacity="0.25">
                            <path d="M15 5v2m0 4v2m0 4v2M5 5a2 2 0 00-2 2v3a2 2 0 110 4v3a2 2 0 002 2h14a2 2 0 002-2v-3a2 2 0 110-4V7a2 2 0 00-2-2H5z"/>
                        </svg>
                    </div>
                </div>
                <div class="ps-card-price-row">
                    <span class="ps-card-label">"PRICE PAID"</span>
                    <span class="ps-card-price">{total_amount}</span>
                </div>
            </div>

            // ── Order meta ───────────────────────────────────────────────
            <div class="ps-meta-card">
                <div class="ps-meta-row">
                    <span class="ps-meta-label">"Order Code"</span>
                    <span class="ps-meta-val">{order_code}</span>
                </div>
                <div class="ps-meta-divider"></div>
                <div class="ps-meta-row">
                    <span class="ps-meta-label">"Status"</span>
                    <span class="ps-status">
                        <span class="ps-status-dot"></span>
                        "CONFIRMED"
                    </span>
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
                <A href="/explore" attr:class="ps-secondary-btn">"Back to Home"</A>
            </div>

            <style>
                "
                .ps-page { min-height:100vh; display:flex; flex-direction:column; }
                .ps-hero { display:flex; flex-direction:column; align-items:center; padding:40px 24px 32px; text-align:center; }
                .ps-check-circle { width:90px; height:90px; border-radius:50%; background:rgba(200,255,94,0.1); border:1px solid rgba(200,255,94,0.3); display:flex; align-items:center; justify-content:center; margin-bottom:20px; animation:ps-pop 0.4s cubic-bezier(0.34,1.56,0.64,1); }
                .ps-check-inner { color:#c8ff5e; }
                @keyframes ps-pop { from{transform:scale(0.5);opacity:0} to{transform:scale(1);opacity:1} }
                .ps-title { font-family:'Bebas Neue',sans-serif; font-size:28px; letter-spacing:2px; color:#fff; margin:0 0 8px; }
                .ps-sub { color:#6666aa; font-size:13px; }
                .ps-card { margin:0 16px 12px; background:rgba(255,255,255,0.04); border:1px solid rgba(255,255,255,0.08); border-radius:16px; padding:20px; }
                .ps-card-top { display:flex; justify-content:space-between; align-items:flex-start; margin-bottom:16px; }
                .ps-card-label { font-size:10px; letter-spacing:1.5px; color:#444466; font-weight:600; margin-bottom:4px; }
                .ps-card-event { font-size:16px; font-weight:700; color:#fff; }
                .ps-card-price-row { display:flex; justify-content:space-between; align-items:center; }
                .ps-card-price { font-size:20px; font-weight:800; color:#c8ff5e; font-family:'Bebas Neue',sans-serif; letter-spacing:1px; }
                .ps-meta-card { margin:0 16px 12px; background:rgba(255,255,255,0.03); border:1px solid rgba(255,255,255,0.06); border-radius:14px; padding:16px 20px; }
                .ps-meta-row { display:flex; justify-content:space-between; align-items:center; padding:10px 0; }
                .ps-meta-label { font-size:12px; color:#6666aa; }
                .ps-meta-val { font-size:13px; color:#ccc; font-weight:500; }
                .ps-meta-divider { height:1px; background:rgba(255,255,255,0.05); }
                .ps-status { display:flex; align-items:center; gap:6px; }
                .ps-status-dot { width:8px; height:8px; background:#c8ff5e; border-radius:50%; animation:ps-blink 1.5s infinite; }
                @keyframes ps-blink { 0%,100%{opacity:1} 50%{opacity:0.3} }
                .ps-actions { margin:16px; display:flex; flex-direction:column; gap:12px; }
                .ps-primary-btn { display:flex; align-items:center; justify-content:center; gap:8px; background:#c8ff5e; color:#0d0d1a; border-radius:14px; padding:16px 24px; font-weight:700; font-size:13px; letter-spacing:1.5px; text-decoration:none; }
                .ps-secondary-btn { display:flex; align-items:center; justify-content:center; background:rgba(255,255,255,0.05); border:1px solid rgba(255,255,255,0.1); color:#aaa; border-radius:14px; padding:15px 24px; font-size:13px; text-decoration:none; }
                "
            </style>
        </div>
    }
}
