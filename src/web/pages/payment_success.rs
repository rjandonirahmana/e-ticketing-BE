/// Page yang ditampilkan setelah pembayaran berhasil.
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::web::app::PendingOrderCtx;
use crate::web::components::ThemeToggle;

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
        query.read().get("produk").unwrap_or_else(|| "Toko".to_string())
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

    // Toast sukses sekali saat halaman dibuka (mis. balik dari gateway pembayaran
    // yang TIDAK melewati toast checkout). Guard prev supaya hanya sekali.
    let toast = crate::web::components::use_toast();
    Effect::new(move |prev: Option<()>| {
        if prev.is_none() {
            let code = order_code();
            let msg = if code.is_empty() {
                "Pembayaran berhasil. Kode pengambilan sudah terbit.".to_string()
            } else {
                format!("Order #{code} lunas. Kode pengambilan sudah terbit.")
            };
            toast.notify(
                crate::web::components::ToastKind::Success,
                "Pembayaran berhasil".into(),
                Some(msg),
                Some("/tickets".into()),
            );
        }
    });

    view! {
        <div class="page ps-page">
            <header class="page-header">
                <div style="width:36px"></div>
                <span class="page-logo">"PULSE"</span>
                <div class="header-actions">
                    <ThemeToggle />
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
                <h1 class="ps-title">"Pembayaran Berhasil"</h1>
                <p class="ps-sub">"Kode pengambilanmu siap. Tunjukkan saat mengambil barang di toko."</p>
            </div>

            // ── Product summary card ───────────────────────────────────────
            <div class="ps-card">
                <div class="ps-card-top">
                    <div>
                        <div class="ps-card-label">"ACARA"</div>
                        <div class="ps-card-product">{event_name}</div>
                    </div>
                    <div class="ps-ticket-icon">
                        <svg width="56" height="56" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" opacity="0.25">
                            <path d="M15 5v2m0 4v2m0 4v2M5 5a2 2 0 00-2 2v3a2 2 0 110 4v3a2 2 0 002 2h14a2 2 0 002-2v-3a2 2 0 110-4V7a2 2 0 00-2-2H5z"/>
                        </svg>
                    </div>
                </div>
                <div class="ps-card-price-row">
                    <span class="ps-card-label">"TOTAL DIBAYAR"</span>
                    <span class="ps-card-price">{total_amount}</span>
                </div>
            </div>

            // ── Order meta ───────────────────────────────────────────────
            <div class="ps-meta-card">
                <div class="ps-meta-row">
                    <span class="ps-meta-label">"Kode Pesanan"</span>
                    <span class="ps-meta-val">{order_code}</span>
                </div>
                <div class="ps-meta-divider"></div>
                <div class="ps-meta-row">
                    <span class="ps-meta-label">"Status"</span>
                    <span class="ps-status">
                        <span class="ps-status-dot"></span>
                        "TERKONFIRMASI"
                    </span>
                </div>
            </div>

            // ── CTA buttons ──────────────────────────────────────────────
            <div class="ps-actions">
                <A href="/tickets" attr:class="ps-primary-btn">
                    "Lihat Kode Pengambilan"
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <line x1="5" y1="12" x2="19" y2="12"/>
                        <polyline points="12 5 19 12 12 19"/>
                    </svg>
                </A>
                <A href="/explore" attr:class="ps-secondary-btn">"Kembali ke Beranda"</A>
            </div>

        </div>
    }
}
