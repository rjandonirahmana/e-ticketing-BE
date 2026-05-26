/// Halaman checkout: pilih metode pembayaran, apply promo, lalu create order.
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::web::api::server_fns::{create_order_cart, validate_promo};
use crate::web::app::{CartContext, PendingOrderCtx, SuccessSnapshot};

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

    let cart_ctx = use_context::<CartContext>().expect("CartContext not provided");
    let items_sig = cart_ctx.items;

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

    let total = move || subtotal() + SERVICE_FEE + PLATFORM_FEE - discount.get();

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

        // Serialize items ke JSON untuk server function
        let items_for_order: Vec<serde_json::Value> = items
            .iter()
            .map(|i| serde_json::json!({
                "ticket_variant_id": i.tier_id,
                "quantity": i.quantity,
            }))
            .collect();
        let items_json = serde_json::to_string(&items_for_order).unwrap_or_default();

        leptos::task::spawn_local(async move {
            match create_order_cart(items_json, m, promo).await {
                Ok(res) => {
                    // Jika redirect ke payment gateway
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
                        // Langsung sukses
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
                        // Pending — simpan dan ke order-created
                        pending_order_sig.set(Some(order));
                        items_sig.set(vec![]);
                        paying.set(false);
                        nav("/order-created", Default::default());
                    }
                }
                Err(e) => {
                    paying.set(false);
                    pay_error.set(e.to_string());
                }
            }
        });
    };

    view! {
        <div class="page checkout-page">
            <header class="page-header">
                <A href="/cart" attr:class="back-btn">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="page-logo">"CHECKOUT"</span>
                <div style="width:36px"></div>
            </header>

            // ── Cart summary ─────────────────────────────────────────────
            <div class="co-section">
                <div class="co-section-title">"YOUR ORDER"</div>
                {move || {
                    let items = items_sig.get();
                    items.iter().map(|item| {
                        let name = item.event_title.clone();
                        let tier = item.tier_name.clone();
                        let qty = item.quantity;
                        let line = item.unit_price * qty as i64;
                        view! {
                            <div class="co-order-row">
                                <div>
                                    <div class="co-order-name">{name}</div>
                                    <div class="co-order-tier">{format!("{}× {}", qty, tier)}</div>
                                </div>
                                <div class="co-order-price">{format_idr(line)}</div>
                            </div>
                        }
                    }).collect_view()
                }}
            </div>

            // ── Payment methods ──────────────────────────────────────────
            <div class="co-section">
                <div class="co-section-title">"PAYMENT METHOD"</div>
                {METHODS.iter().map(|m| {
                    let id = m.id;
                    let label = m.label;
                    let sub = m.sub;
                    view! {
                        <div
                            class="co-method-row"
                            class:co-method-active=move || method.get() == id
                            on:click=move |_| method.set(id.to_string())
                        >
                            <div class="co-method-radio">
                                <div class="co-method-radio-inner"></div>
                            </div>
                            <div>
                                <div class="co-method-label">{label}</div>
                                <div class="co-method-sub">{sub}</div>
                            </div>
                        </div>
                    }
                }).collect_view()}
            </div>

            // ── Promo code ───────────────────────────────────────────────
            <div class="co-section">
                <div class="co-section-title">"PROMO CODE"</div>
                <div class="co-promo-row">
                    <input
                        class="co-promo-input"
                        type="text"
                        placeholder="Enter promo code"
                        prop:value=move || promo_code.get()
                        on:input=move |e| promo_code.set(event_target_value(&e))
                        prop:disabled=move || promo_applied.get()
                    />
                    <button
                        class="co-promo-btn"
                        disabled=move || promo_loading.get() || promo_applied.get()
                        on:click=on_apply_promo
                    >
                        {move || if promo_applied.get() { "✓" } else if promo_loading.get() { "..." } else { "APPLY" }}
                    </button>
                </div>
                {move || (!promo_error.get().is_empty()).then(|| view! {
                    <p class="co-promo-error">{promo_error.get()}</p>
                })}
                {move || promo_applied.get().then(|| view! {
                    <p class="co-promo-ok">"Promo applied! Saving "{format_idr(discount.get())}</p>
                })}
            </div>

            // ── Price breakdown ──────────────────────────────────────────
            <div class="co-section">
                <div class="co-section-title">"PRICE BREAKDOWN"</div>
                <div class="co-price-row">
                    <span>"Subtotal"</span>
                    <span>{move || format_idr(subtotal())}</span>
                </div>
                <div class="co-price-row">
                    <span>"Service Fee"</span>
                    <span>{format_idr(SERVICE_FEE)}</span>
                </div>
                <div class="co-price-row">
                    <span>"Platform Fee"</span>
                    <span>{format_idr(PLATFORM_FEE)}</span>
                </div>
                {move || (discount.get() > 0).then(|| view! {
                    <div class="co-price-row co-price-discount">
                        <span>"Promo Discount"</span>
                        <span>"-"{format_idr(discount.get())}</span>
                    </div>
                })}
                <div class="co-price-total">
                    <span class="co-price-total-label">"TOTAL"</span>
                    <span class="co-price-total-val">{move || format_idr(total())}</span>
                </div>
            </div>

            {move || (!pay_error.get().is_empty()).then(|| view! {
                <div class="co-pay-error">{pay_error.get()}</div>
            })}

            // ── Confirm button ───────────────────────────────────────────
            <div class="co-footer">
                <div>
                    <div class="co-footer-label">"TOTAL PAYMENT"</div>
                    <div class="co-footer-total">{move || format_idr(total())}</div>
                </div>
                <button
                    class="co-confirm-btn"
                    disabled=move || paying.get()
                    on:click=on_confirm.clone()
                >
                    {move || if paying.get() { "PROCESSING..." } else { "CONFIRM & PAY" }}
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <line x1="5" y1="12" x2="19" y2="12"/>
                        <polyline points="12 5 19 12 12 19"/>
                    </svg>
                </button>
            </div>

            <style>
                "
                .checkout-page { min-height:100vh; display:flex; flex-direction:column; padding-bottom:100px; }
                .co-section { margin:0 16px 16px; background:rgba(255,255,255,0.04); border:1px solid rgba(255,255,255,0.07); border-radius:14px; padding:16px; }
                .co-section-title { font-size:10px; letter-spacing:2px; color:#444466; font-weight:600; margin-bottom:14px; }
                .co-order-row { display:flex; justify-content:space-between; align-items:center; padding:8px 0; border-bottom:1px solid rgba(255,255,255,0.05); }
                .co-order-row:last-child { border-bottom:none; }
                .co-order-name { font-size:13px; color:#ccc; font-weight:500; }
                .co-order-tier { font-size:11px; color:#6666aa; margin-top:2px; }
                .co-order-price { font-size:13px; color:#c8ff5e; font-weight:600; }
                .co-method-row { display:flex; align-items:center; gap:14px; padding:14px; background:rgba(255,255,255,0.03); border:1px solid rgba(255,255,255,0.07); border-radius:12px; cursor:pointer; margin-bottom:8px; transition:all 0.2s; }
                .co-method-row:last-child { margin-bottom:0; }
                .co-method-active { border-color:rgba(200,255,94,0.4); background:rgba(200,255,94,0.06); }
                .co-method-radio { width:18px; height:18px; border-radius:50%; border:2px solid rgba(255,255,255,0.2); display:flex; align-items:center; justify-content:center; flex-shrink:0; }
                .co-method-active .co-method-radio { border-color:#c8ff5e; }
                .co-method-active .co-method-radio-inner { width:8px; height:8px; border-radius:50%; background:#c8ff5e; }
                .co-method-label { font-size:13px; font-weight:600; color:#fff; }
                .co-method-sub { font-size:11px; color:#6666aa; margin-top:2px; }
                .co-promo-row { display:flex; gap:8px; }
                .co-promo-input { flex:1; background:rgba(255,255,255,0.06); border:1px solid rgba(255,255,255,0.1); border-radius:10px; padding:12px 14px; color:#fff; font-size:13px; outline:none; }
                .co-promo-btn { background:#c8ff5e; color:#0d0d1a; border:none; border-radius:10px; padding:12px 16px; font-weight:700; font-size:12px; cursor:pointer; white-space:nowrap; }
                .co-promo-btn:disabled { opacity:0.5; cursor:not-allowed; }
                .co-promo-error { color:#ff6b6b; font-size:12px; margin-top:8px; }
                .co-promo-ok { color:#c8ff5e; font-size:12px; margin-top:8px; }
                .co-price-row { display:flex; justify-content:space-between; align-items:center; padding:8px 0; color:#8888aa; font-size:13px; }
                .co-price-discount { color:#c8ff5e; }
                .co-price-total { display:flex; justify-content:space-between; align-items:center; padding-top:12px; margin-top:8px; border-top:1px solid rgba(255,255,255,0.08); }
                .co-price-total-label { font-size:11px; letter-spacing:1.5px; color:#aaa; font-weight:600; }
                .co-price-total-val { font-size:24px; font-weight:800; color:#c8ff5e; font-family:'Bebas Neue',sans-serif; }
                .co-pay-error { color:#ff6b6b; font-size:13px; text-align:center; padding:0 16px 8px; }
                .co-footer { position:fixed; bottom:0; left:0; right:0; background:rgba(5,8,20,0.95); border-top:1px solid rgba(255,255,255,0.08); padding:16px 20px; display:flex; align-items:center; justify-content:space-between; gap:16px; z-index:50; backdrop-filter:blur(12px); }
                .co-footer-label { font-size:10px; letter-spacing:1.5px; color:#444466; font-weight:600; }
                .co-footer-total { font-size:22px; font-weight:800; color:#c8ff5e; font-family:'Bebas Neue',sans-serif; }
                .co-confirm-btn { display:flex; align-items:center; gap:8px; background:#c8ff5e; color:#0d0d1a; border:none; border-radius:14px; padding:14px 20px; font-weight:700; font-size:12px; letter-spacing:1px; cursor:pointer; white-space:nowrap; }
                .co-confirm-btn:disabled { opacity:0.6; cursor:not-allowed; }
                "
            </style>
        </div>
    }
}
