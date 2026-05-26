use crate::csr::hooks::use_nav;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::csr::hooks::{format_idr, use_cart, ThemeToggle};
use crate::csr::models::{CreateOrderRequest, ValidatePromoRequest};
use crate::csr::pages::order_created::use_pending_order;
use crate::csr::pages::payment_success::{use_payment_success, SuccessOrderSnapshot};
use crate::csr::services::chat as chat_svc;
use crate::csr::services::payment as pay_svc;

const SERVICE_FEE: i64 = 125_000;
const PLATFORM_FEE: i64 = 25_000;

struct PayMethod {
    id: &'static str,
    label: &'static str,
    sub: &'static str,
}

const METHODS: &[PayMethod] = &[
    PayMethod {
        id: "CREDIT_CARD",
        label: "CREDIT CARD",
        sub: "Visa ending in **** 8821",
    },
    PayMethod {
        id: "GOPAY",
        label: "GOPAY / OVO",
        sub: "Direct Indonesian E-Wallet",
    },
    PayMethod {
        id: "BANK_TRANSFER",
        label: "BANK TRANSFER",
        sub: "Virtual Account BCA / Mandiri",
    },
];

#[component]
pub fn CheckoutPage() -> impl IntoView {
    let cart = use_cart();
    let navigate = use_nav();
    let pending_order_ctx = use_pending_order();
    let payment_success_ctx = use_payment_success();

    let method = RwSignal::new("CREDIT_CARD".to_string());
    let promo_code = RwSignal::new(String::new());
    let promo_applied = RwSignal::new(false);
    let discount = RwSignal::new(0_i64);
    let promo_loading = RwSignal::new(false);
    let promo_error = RwSignal::new(String::new());
    let paying = RwSignal::new(false);
    let pay_error = RwSignal::new(String::new());

    let total = Memo::new(move |_| cart.subtotal() + SERVICE_FEE + PLATFORM_FEE - discount.get());

    let on_apply_promo = move |_| {
        let code = promo_code.get();
        if code.trim().is_empty() {
            return;
        }
        promo_loading.set(true);
        promo_error.set(String::new());
        let sub = cart.subtotal();
        spawn_local(async move {
            match pay_svc::validate_promo(&ValidatePromoRequest {
                promo_code: code,
                subtotal: sub,
            })
            .await
            {
                Ok(res) => {
                    if res.valid {
                        discount.set(res.discount_idr);
                        promo_applied.set(true);
                    } else {
                        let m = if res.message.is_empty() {
                            "Invalid promo code.".into()
                        } else {
                            res.message
                        };
                        promo_error.set(m);
                    }
                }
                Err(_) => promo_error.set("Failed to validate promo.".into()),
            }
            promo_loading.set(false);
        });
    };

    // Hoist the inner RwSignals out of the context structs so the closure
    // only captures Copy values — avoids FnOnce when the context structs
    // themselves are moved.
    let pending_order_sig = pending_order_ctx.order;
    let success_snap_sig = payment_success_ctx.snapshot;

    let nav_pay = navigate.clone();
    let on_confirm = move |_| {
        paying.set(true);
        pay_error.set(String::new());
        let items = cart.items.get();
        let m = method.get();
        let promo = if promo_applied.get() {
            Some(promo_code.get())
        } else {
            None
        };
        let nav = nav_pay.clone();
        let pend_sig = pending_order_sig;
        let succ_sig = success_snap_sig;
        let cart_ref = cart;
        spawn_local(async move {
            let req = CreateOrderRequest {
                items: items.clone(),
                payment_method: m,
                promo_code: promo,
            };
            match pay_svc::create_order(&req).await {
                Ok(res) => {
                    if res.requires_redirect && !res.payment_url.is_empty() {
                        if let Some(win) = web_sys::window() {
                            let _ = win.location().set_href(&res.payment_url);
                        }
                        return;
                    }

                    // Cek apakah order sudah langsung paid (e.g. auto-confirm sandbox)
                    if res.order.status.eq_ignore_ascii_case("paid")
                        || res.order.status.eq_ignore_ascii_case("completed")
                    {
                        // Langsung ke success
                        let event_name = res
                            .order
                            .items
                            .first()
                            .map(|i| i.event_name.clone())
                            .unwrap_or_default();
                        succ_sig.set(Some(SuccessOrderSnapshot {
                            order_id: res.order.id.clone(),
                            order_code: res.order.order_code.clone(),
                            event_name,
                            event_date: res
                                .order
                                .created_at
                                .as_deref()
                                .and_then(|s| s.get(..10))
                                .unwrap_or("—")
                                .to_string(),
                            total_amount: res.order.total_amount,
                            paid_at: res.order.created_at.clone().unwrap_or_default(),
                        }));
                        cart_ref.clear();

                        let event_id_for_group = cart_ref.items.with_untracked(|v| {
                            v.first().and_then(|i| {
                                if i.event_id.is_empty() {
                                    None
                                } else {
                                    Some(i.event_id.clone())
                                }
                            })
                        });
                        if let Some(eid) = event_id_for_group {
                            leptos::task::spawn_local(async move {
                                let _ = chat_svc::join_event_group(&eid).await;
                            });
                        }
                        paying.set(false);
                        nav("/payment-success", Default::default());
                    } else {
                        // Order pending — simpan di context, navigasi ke order-created
                        pend_sig.set(Some(res.order));
                        paying.set(false);
                        nav("/order-created", Default::default());
                    }
                }
                Err(e) => {
                    paying.set(false);
                    pay_error.set(e.message);
                }
            }
        });
    };

    let on_confirm_clone = on_confirm.clone();

    view! {
        <div class="page">
            <header class="page-header">
                <A href="/cart" attr:class="back-btn">
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
                <span class="page-logo">"KINETIC"</span>
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
                    "Secure your spot at the Kinetic Stage. All prices are in IDR (Rupiah)."
                </p>
            </div>

            <section class="checkout-section">
                <div class="section-row">
                    <span class="section-head">"ORDER SUMMARY"</span>
                    <span class="section-badge">
                        {move || format!("{} ITEMS", cart.items.with(|v| v.len()))}
                    </span>
                </div>
                <div class="order-items">
                    {move || {
                        let items = cart.items.get();
                        if items.is_empty() {
                            view! {
                                <p class="empty-msg">
                                    "Your cart is empty. "<A href="/" attr:class="auth-prompt-link">
                                        "Browse events"
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

            <section class="checkout-section">
                <div class="promo-wrap">
                    <span class="promo-label">"HAVE A PROMO CODE?"</span>
                    <div class="promo-input-row">
                        <input
                            class="promo-input"
                            placeholder="Enter code"
                            disabled=move || promo_applied.get()
                            prop:value=move || promo_code.get()
                            on:input=move |ev| {
                                promo_code.set(event_target_value(&ev));
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
                                    "...".to_string()
                                } else if promo_applied.get() {
                                    "✓".to_string()
                                } else {
                                    "APPLY".to_string()
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

            <section class="checkout-section total-section">
                <div class="total-head">"TOTAL PAYABLE"</div>
                <div class="total-line">
                    <span>"Subtotal"</span>
                    <span>{move || format_idr(cart.subtotal())}</span>
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

            <div class="confirm-section">
                {move || {
                    (!pay_error.get().is_empty())
                        .then(|| view! { <div class="pay-error">{pay_error.get()}</div> })
                }}
                <button
                    class="confirm-btn"
                    disabled=move || paying.get() || cart.items.with(|v| v.is_empty())
                    on:click=on_confirm
                >
                    {move || {
                        if paying.get() {
                            view! {
                                <>
                                    <span class="spinner"></span>
                                    "PROCESSING..."
                                </>
                            }
                                .into_any()
                        } else {
                            view! { <>"CONFIRM PAYMENT"</> }.into_any()
                        }
                    }}
                </button>
                <p class="terms-note">
                    "BY CLICKING CONFIRM, YOU AGREE TO OUR TERMS OF SERVICE AND REFUND POLICIES"
                </p> <div class="trust-icons">
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

            <div class="pay-bar">
                <button
                    class="pay-bar-btn"
                    disabled=move || paying.get() || cart.items.with(|v| v.is_empty())
                    on:click=on_confirm_clone
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
