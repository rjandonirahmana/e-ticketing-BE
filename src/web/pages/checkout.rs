/// Halaman checkout: pilih metode pembayaran, apply promo, lalu create order.
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;

use crate::web::api::server_fns::{create_order_cart, validate_promo};
use crate::web::app::{CartContext, PendingOrderCtx, SuccessSnapshot};
use crate::web::components::ThemeToggle;

const SERVICE_FEE: i64 = 125_000;
const PLATFORM_FEE: i64 = 25_000;

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

struct PayMethod {
    id: &'static str,
    label: &'static str,
    sub: &'static str,
}

const METHODS: &[PayMethod] = &[
    PayMethod { id: "CREDIT_CARD", label: "CREDIT CARD", sub: "Visa ending in **** 8821" },
    PayMethod { id: "GOPAY", label: "GOPAY / OVO", sub: "Direct Indonesian E-Wallet" },
    PayMethod { id: "BANK_TRANSFER", label: "BANK TRANSFER", sub: "Virtual Account BCA / Mandiri" },
];

#[component]
pub fn CheckoutPage() -> impl IntoView {
    let navigate = use_navigate();
    let nav_redirect = navigate.clone();

    let cart_ctx = use_context::<CartContext>().expect("CartContext not provided");
    let items_sig = cart_ctx.items;

    // Redirect to /cart when opened directly (empty cart / reload).
    // Use replace:true so the /checkout entry is removed from history,
    // preventing a back-button loop (back from /cart → /checkout → /cart …).
    Effect::new(move |_| {
        if items_sig.with(|v| v.is_empty()) {
            nav_redirect.clone()("/cart", NavigateOptions { replace: true, ..NavigateOptions::default() });
        }
    });

    let pending_ctx = use_context::<PendingOrderCtx>().expect("PendingOrderCtx not provided");
    let pending_order_sig = pending_ctx.pending_order;
    let success_sig = pending_ctx.success_order;

    let method = RwSignal::new("CREDIT_CARD".to_string());
    let promo_code = RwSignal::new(String::new());
    let promo_applied = RwSignal::new(false);
    let discount = RwSignal::new(0_i64);
    let promo_loading = RwSignal::new(false);
    let promo_error = RwSignal::new(String::new());
    let paying = RwSignal::new(false);
    let pay_error = RwSignal::new(String::new());

    let subtotal = move || {
        items_sig
            .get()
            .iter()
            .map(|i| i.unit_price * i.quantity as i64)
            .sum::<i64>()
    };

    let total = Memo::new(move |_| subtotal() + SERVICE_FEE + PLATFORM_FEE - discount.get());

    let on_apply_promo = move |_| {
        let code = promo_code.get();
        if code.trim().is_empty() { return; }
        promo_loading.set(true);
        promo_error.set(String::new());
        let sub = subtotal();
        leptos::task::spawn_local(async move {
            match validate_promo(code, sub).await {
                Ok(res) => {
                    if res.valid {
                        discount.set(res.discount_idr);
                        promo_applied.set(true);
                    } else {
                        promo_error.set(if res.message.is_empty() {
                            "Invalid promo code.".into()
                        } else {
                            res.message
                        });
                    }
                }
                Err(_) => promo_error.set("Failed to validate promo.".into()),
            }
            promo_loading.set(false);
        });
    };

    let on_confirm = move |_| {
        paying.set(true);
        pay_error.set(String::new());

        let items = items_sig.get();
        let m = method.get();
        let promo = if promo_applied.get() { Some(promo_code.get()) } else { None };
        let nav = navigate.clone();

        let items_for_order: Vec<serde_json::Value> = items
            .iter()
            .map(|i| serde_json::json!({
                "tier_id": i.tier_id,
                "quantity": i.quantity,
            }))
            .collect();
        let items_json = serde_json::to_string(&items_for_order).unwrap_or_default();

        leptos::task::spawn_local(async move {
            match create_order_cart(items_json, m, promo).await {
                Ok(res) => {
                    if res.requires_redirect && !res.payment_url.is_empty() {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(win) = web_sys::window() {
                            let _ = win.location().set_href(&res.payment_url);
                        }
                        paying.set(false);
                        return;
                    }

                    let order = res.order;

                    if order.status.eq_ignore_ascii_case("paid")
                        || order.status.eq_ignore_ascii_case("completed")
                    {
                        success_sig.set(Some(SuccessSnapshot {
                            order_code: order.order_code.clone(),
                            event_name: order
                                .items
                                .first()
                                .map(|i| i.event_name.clone())
                                .unwrap_or_default(),
                            total_amount: order.total_amount,
                        }));
                        items_sig.set(vec![]);
                        paying.set(false);
                        nav("/payment-success", Default::default());
                    } else {
                        // Order pending → langsung ke halaman ORDER DETAIL yang
                        // sudah siap bayar (QR QRIS + panduan + tombol konfirmasi),
                        // bukan halaman ringkasan generik /order-created.
                        let oid = order.id.clone();
                        pending_order_sig.set(Some(order));
                        items_sig.set(vec![]);
                        paying.set(false);
                        nav(&format!("/orders/{oid}"), Default::default());
                    }
                }
                Err(e) => {
                    paying.set(false);
                    pay_error.set(e.to_string());
                }
            }
        });
    };

    let on_confirm2 = on_confirm.clone();

    view! {
        <div class="page">
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
                <span class="page-logo">"PULSE"</span>
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

            <div class="checkout-hero">
                <h1 class="checkout-title">"CHECKOUT"</h1>
                <p class="checkout-sub">
                    "Secure your spot at the PULSE Stage. All prices are in IDR (Rupiah)."
                </p>
            </div>

            // ── Order summary ─────────────────────────────────────────────────────
            <section class="checkout-section">
                <div class="section-row">
                    <span class="section-head">"ORDER SUMMARY"</span>
                    <span class="section-badge">
                        {move || format!("{} ITEMS", items_sig.with(|v| v.len()))}
                    </span>
                </div>
                <div class="order-items">
                    {move || {
                        let items = items_sig.get();
                        if items.is_empty() {
                            view! {
                                <p class="empty-msg">
                                    "Keranjang kosong. "
                                    <A href="/explore" attr:class="auth-prompt-link">
                                        "Jelajahi event"
                                    </A>
                                </p>
                            }
                                .into_any()
                        } else {
                            items
                                .iter()
                                .map(|item| {
                                    let img = if item.event_cover.is_empty() {
                                        "https://images.unsplash.com/photo-1470225620780-dba8ba36b745?w=150&q=80"
                                            .to_string()
                                    } else {
                                        item.event_cover.clone()
                                    };
                                    view! {
                                        <div class="order-item">
                                            <img
                                                src=img
                                                alt=item.event_title.clone()
                                                class="order-item-img"
                                            />
                                            <div class="order-item-info">
                                                <div class="order-item-name">
                                                    {item.event_title.clone()}
                                                </div>
                                                <div class="order-item-meta">
                                                    {format!("{} • {}", item.tier_name, item.venue_name)}
                                                </div>
                                                {(item.quantity > 1)
                                                    .then(|| {
                                                        view! {
                                                            <div class="order-item-qty">
                                                                {format!("{}× ticket", item.quantity)}
                                                            </div>
                                                        }
                                                    })}
                                            </div>
                                            <div class="order-item-price">
                                                {format_idr(item.unit_price * item.quantity as i64)}
                                            </div>
                                        </div>
                                    }
                                })
                                .collect_view()
                                .into_any()
                        }
                    }}
                </div>
            </section>

            // ── Payment methods ───────────────────────────────────────────────────
            <section class="checkout-section">
                <span class="section-head">"PAYMENT METHOD"</span>
                <div class="method-list">
                    {METHODS
                        .iter()
                        .map(|opt| {
                            let id = opt.id;
                            let cls = move || {
                                if method.get() == id {
                                    "method-card method-card--active"
                                } else {
                                    "method-card"
                                }
                            };
                            view! {
                                <button
                                    class=cls
                                    type="button"
                                    on:click=move |_| method.set(id.to_string())
                                >
                                    <span class="method-icon">
                                        <svg
                                            width="20"
                                            height="20"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="1.8"
                                            stroke-linecap="round"
                                        >
                                            <rect x="2" y="5" width="20" height="14" rx="2" />
                                            <line x1="2" y1="10" x2="22" y2="10" />
                                        </svg>
                                    </span>
                                    <div class="method-info">
                                        <div class="method-label">{opt.label}</div>
                                        <div class="method-sub">{opt.sub}</div>
                                    </div>
                                    {move || {
                                        (method.get() == id)
                                            .then(|| {
                                                view! {
                                                    <div class="method-check">
                                                        <svg
                                                            width="14"
                                                            height="14"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="#080810"
                                                            stroke-width="3"
                                                            stroke-linecap="round"
                                                        >
                                                            <polyline points="20 6 9 17 4 12" />
                                                        </svg>
                                                    </div>
                                                }
                                            })
                                    }}
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </section>

            // ── Promo code ────────────────────────────────────────────────────────
            <section class="checkout-section">
                <div class="promo-wrap">
                    <span class="promo-label">"HAVE A PROMO CODE?"</span>
                    <div class="promo-input-row">
                        <input
                            class="promo-input"
                            type="text"
                            placeholder="Enter code"
                            prop:value=move || promo_code.get()
                            prop:disabled=move || promo_applied.get()
                            on:input=move |e| {
                                promo_code.set(event_target_value(&e));
                                promo_applied.set(false);
                                promo_error.set(String::new());
                            }
                        />
                        <button
                            class="promo-apply-btn"
                            disabled=move || promo_loading.get() || promo_applied.get()
                            on:click=on_apply_promo
                        >
                            {move || {
                                if promo_loading.get() {
                                    "..."
                                } else if promo_applied.get() {
                                    "✓"
                                } else {
                                    "APPLY"
                                }
                            }}
                        </button>
                    </div>
                </div>
                {move || {
                    (!promo_error.get().is_empty())
                        .then(|| view! { <p class="promo-error">{promo_error.get()}</p> })
                }}
                {move || {
                    promo_applied
                        .get()
                        .then(|| {
                            view! {
                                <p class="promo-success">
                                    {format!("Promo applied: −{}", format_idr(discount.get()))}
                                </p>
                            }
                        })
                }}
            </section>

            // ── Price breakdown ───────────────────────────────────────────────────
            <section class="checkout-section total-section">
                <div class="total-head">"TOTAL PAYABLE"</div>
                <div class="total-line">
                    <span>"Subtotal"</span>
                    <span>{move || format_idr(subtotal())}</span>
                </div>
                {move || {
                    promo_applied
                        .get()
                        .then(|| {
                            view! {
                                <div class="total-line total-line--discount">
                                    <span>{format!("Promo ({})", promo_code.get())}</span>
                                    <span>{format!("−{}", format_idr(discount.get()))}</span>
                                </div>
                            }
                        })
                }}
                <div class="total-line">
                    <span>"Service Fee"</span>
                    <span>{format_idr(SERVICE_FEE)}</span>
                </div>
                <div class="total-line">
                    <span>"Platform Fee"</span>
                    <span>{format_idr(PLATFORM_FEE)}</span>
                </div>
                <div class="total-final-row">
                    <span class="total-final-label">"TOTAL"</span>
                    <span class="total-final-amt">{move || format_idr(total.get())}</span>
                </div>
            </section>

            // ── Confirm section ───────────────────────────────────────────────────
            <div class="confirm-section">
                {move || {
                    (!pay_error.get().is_empty())
                        .then(|| view! { <div class="pay-error">{pay_error.get()}</div> })
                }}
                <button
                    class="confirm-btn"
                    disabled=move || paying.get() || items_sig.with(|v| v.is_empty())
                    on:click=on_confirm
                >
                    {move || {
                        if paying.get() { "PROCESSING..." } else { "CONFIRM PAYMENT" }
                    }}
                </button>
                <p class="terms-note">
                    "BY CLICKING CONFIRM, YOU AGREE TO OUR TERMS OF SERVICE AND REFUND POLICIES"
                </p>
                <div class="trust-icons">
                    <svg
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.5"
                        stroke-linecap="round"
                    >
                        <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
                    </svg>
                    <svg
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.5"
                        stroke-linecap="round"
                    >
                        <rect x="3" y="11" width="18" height="11" rx="2" />
                        <path d="M7 11V7a5 5 0 0110 0v4" />
                    </svg>
                    <svg
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.5"
                        stroke-linecap="round"
                    >
                        <rect x="2" y="5" width="20" height="14" rx="2" />
                        <line x1="2" y1="10" x2="22" y2="10" />
                    </svg>
                </div>
            </div>

            // ── Sticky pay bar ────────────────────────────────────────────────────
            <div class="pay-bar">
                <button
                    class="pay-bar-btn"
                    disabled=move || paying.get() || items_sig.with(|v| v.is_empty())
                    on:click=on_confirm2
                >
                    {move || {
                        if paying.get() {
                            "PROCESSING...".to_string()
                        } else {
                            format!("PAY {}", format_idr(total.get()))
                        }
                    }}
                </button>
            </div>
        </div>
    }
}
