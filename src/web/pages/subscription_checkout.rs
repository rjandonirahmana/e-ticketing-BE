//! Halaman pilih metode pembayaran untuk PULSE Premium.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::web::api::server_fns::confirm_subscription_payment;
use crate::web::app::{PendingOrderCtx, PendingSubCtx, SuccessSnapshot};
use crate::web::components::ThemeToggle;

struct PayMethod {
    id: &'static str,
    label: &'static str,
    sub: &'static str,
    icon: &'static str, // "wallet" | "bank" | "card"
}

const METHODS: &[PayMethod] = &[
    PayMethod { id: "GOPAY",         label: "GoPay / OVO",   sub: "Dompet digital langsung",       icon: "wallet" },
    PayMethod { id: "BANK_TRANSFER", label: "Bank Transfer",  sub: "Virtual Account BCA / Mandiri", icon: "bank"   },
    PayMethod { id: "CREDIT_CARD",   label: "Kartu Kredit",   sub: "Visa / Mastercard / JCB",       icon: "card"   },
];

fn format_idr(amount: i64) -> String {
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

fn plan_label(plan: &str) -> &'static str {
    match plan {
        "weekly"   => "Mingguan · 7 Hari",
        "yearly"   => "Tahunan · 365 Hari",
        "lifetime" => "Seumur Hidup",
        _          => "Bulanan · 30 Hari",
    }
}

fn method_icon(kind: &str) -> impl IntoView {
    match kind {
        "wallet" => view! {
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                 stroke="currentColor" stroke-width="1.8"
                 stroke-linecap="round" stroke-linejoin="round">
                <path d="M20 12V8H6a2 2 0 01-2-2c0-1.1.9-2 2-2h12v4"/>
                <path d="M4 6v12c0 1.1.9 2 2 2h14v-4"/>
                <path d="M18 12a2 2 0 000 4h4v-4z"/>
            </svg>
        }.into_any(),
        "bank" => view! {
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                 stroke="currentColor" stroke-width="1.8"
                 stroke-linecap="round" stroke-linejoin="round">
                <line x1="3" y1="22" x2="21" y2="22"/>
                <line x1="6" y1="18" x2="6" y2="11"/>
                <line x1="10" y1="18" x2="10" y2="11"/>
                <line x1="14" y1="18" x2="14" y2="11"/>
                <line x1="18" y1="18" x2="18" y2="11"/>
                <polygon points="12 2 20 7 4 7"/>
            </svg>
        }.into_any(),
        _ => view! {
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                 stroke="currentColor" stroke-width="1.8"
                 stroke-linecap="round" stroke-linejoin="round">
                <rect x="1" y="4" width="22" height="16" rx="2" ry="2"/>
                <line x1="1" y1="10" x2="23" y2="10"/>
            </svg>
        }.into_any(),
    }
}

#[component]
pub fn SubscriptionCheckoutPage() -> impl IntoView {
    let navigate          = StoredValue::new(use_navigate());
    let sub_ctx           = use_context::<PendingSubCtx>().expect("PendingSubCtx missing");
    let pending_order_ctx = use_context::<PendingOrderCtx>();

    let method    = RwSignal::new("GOPAY".to_string());
    let paying    = RwSignal::new(false);
    let pay_error = RwSignal::new(String::new());

    let on_pay = move |_| {
        let Some(order) = sub_ctx.order.get() else { return };
        paying.set(true);
        pay_error.set(String::new());
        let nav        = navigate.get_value();
        let plan       = order.plan.clone();
        let order_id   = order.order_id.clone();
        let order_code = order.order_code.clone();
        let amount     = order.amount_idr;

        leptos::task::spawn_local(async move {
            match confirm_subscription_payment(order_id, plan).await {
                Ok(_) => {
                    if let Some(ctx) = pending_order_ctx {
                        ctx.success_order.set(Some(SuccessSnapshot {
                            order_code,
                            event_name: "PULSE Premium".to_string(),
                            total_amount: amount,
                        }));
                    }
                    sub_ctx.order.set(None);
                    paying.set(false);
                    nav("/payment-success", Default::default());
                }
                Err(e) => {
                    paying.set(false);
                    pay_error.set(e.to_string());
                }
            }
        });
    };

    view! {
        <div class="page sub-pay-page">
            <header class="page-header">
                <button class="back-btn" aria-label="Kembali"
                    on:click=move |_| {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(win) = web_sys::window() {
                            let _ = win.history().ok().map(|h| h.back());
                        }
                    }>
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </button>
                <span class="page-title page-title--premium">"PEMBAYARAN PREMIUM"</span>
                <div class="header-actions"><ThemeToggle /></div>
            </header>

            {move || {
                let order = sub_ctx.order.get();

                // ── No active order (direct URL / refresh) ───────────────────
                if order.is_none() {
                    return view! {
                        <div class="sub-pay-empty">
                            <div class="sub-pay-empty-crown">"👑"</div>
                            <p class="sub-pay-empty-title">"Tidak Ada Pesanan Aktif"</p>
                            <p class="sub-pay-empty-desc">
                                "Pilih paket terlebih dahulu untuk melanjutkan pembayaran."
                            </p>
                            <A href="/subscription" attr:class="sub-pay-back-link">
                                "Lihat Paket Premium"
                            </A>
                        </div>
                    }.into_any();
                }

                let o          = order.unwrap();
                let plan_name  = plan_label(&o.plan).to_string();
                let amount_str = format_idr(o.amount_idr);

                view! {
                    <div class="sub-pay-body">

                        // ── Premium hero banner ───────────────────────────────
                        <div class="sub-pay-hero-banner">
                            <div class="sub-pay-hero-glow" aria-hidden="true"/>
                            <div class="sub-pay-hero-left">
                                <span class="sub-pay-hero-crown">"👑"</span>
                                <div>
                                    <p class="sub-pay-hero-tag">"PULSE PREMIUM"</p>
                                    <p class="sub-pay-hero-plan">{plan_name}</p>
                                </div>
                            </div>
                            <p class="sub-pay-hero-amount">{amount_str}</p>
                        </div>

                        // ── Order code pill ───────────────────────────────────
                        <div class="sub-pay-code-pill">
                            <svg width="11" height="11" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5">
                                <rect x="3" y="3" width="18" height="18" rx="2"/>
                                <line x1="9" y1="9" x2="15" y2="9"/>
                                <line x1="9" y1="12" x2="15" y2="12"/>
                                <line x1="9" y1="15" x2="12" y2="15"/>
                            </svg>
                            <span>"Kode Pesanan"</span>
                            <code class="sub-pay-code">{o.order_code.clone()}</code>
                        </div>

                        // ── Pilih metode pembayaran ───────────────────────────
                        <section class="sub-pay-methods">
                            <h2 class="sub-pay-section-title">"Pilih Metode Pembayaran"</h2>
                            <div class="sub-pay-method-list">
                                {METHODS.iter().map(|m| {
                                    let mid  = m.id;
                                    let icon = m.icon;
                                    let is_selected = move || method.get() == mid;
                                    view! {
                                        <button
                                            class="sub-pay-method"
                                            class:sub-pay-method--selected=is_selected
                                            on:click=move |_| method.set(mid.to_string())
                                            aria-pressed=move || is_selected().to_string()
                                        >
                                            <div class="sub-pay-method-icon"
                                                 class:sub-pay-method-icon--on=is_selected>
                                                {method_icon(icon)}
                                            </div>
                                            <div class="sub-pay-method-text">
                                                <span class="sub-pay-method-label">{m.label}</span>
                                                <span class="sub-pay-method-sub">{m.sub}</span>
                                            </div>
                                            <div class="sub-pay-method-radio">
                                                <div class="sub-pay-method-radio-dot"
                                                     class:sub-pay-method-radio-dot--on=is_selected/>
                                            </div>
                                        </button>
                                    }
                                }).collect_view()}
                            </div>
                        </section>

                        // ── Error ─────────────────────────────────────────────
                        <Show when=move || !pay_error.get().is_empty()>
                            <div class="sub-pay-error">
                                <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="2.5">
                                    <circle cx="12" cy="12" r="10"/>
                                    <line x1="12" y1="8" x2="12" y2="12"/>
                                    <line x1="12" y1="16" x2="12.01" y2="16"/>
                                </svg>
                                {move || pay_error.get()}
                            </div>
                        </Show>

                        // ── Secure note ───────────────────────────────────────
                        <div class="sub-pay-secure">
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2">
                                <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
                            </svg>
                            <span>"Transaksi dienkripsi & aman"</span>
                        </div>

                    </div>
                }.into_any()
            }}

            // ── Sticky CTA ───────────────────────────────────────────────────────
            <div class="sub-pay-sticky">
                <button
                    class="sub-cta-btn"
                    class:sub-cta-btn--loading=paying
                    on:click=on_pay
                    disabled=move || paying.get() || sub_ctx.order.get().is_none()
                >
                    {move || if paying.get() {
                        view! {
                            <span class="sub-cta-spinner" aria-hidden="true"/>
                            " Memproses..."
                        }.into_any()
                    } else {
                        view! {
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5">
                                <rect x="1" y="4" width="22" height="16" rx="2" ry="2"/>
                                <line x1="1" y1="10" x2="23" y2="10"/>
                            </svg>
                            " Bayar Sekarang"
                        }.into_any()
                    }}
                </button>
                <p class="sub-cta-terms">
                    "Dengan membayar, kamu menyetujui "
                    <a href="/terms" class="sub-cta-link">"Syarat & Ketentuan"</a>
                    " PULSE Premium."
                </p>
            </div>
        </div>
    }
}
