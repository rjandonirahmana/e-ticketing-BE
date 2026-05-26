/// Halaman keranjang belanja.
/// Menggunakan CartContext yang di-provide via App, tersedia setelah hydration.
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::web::app::CartContext;

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
pub fn CartPage() -> impl IntoView {
    let navigate = use_navigate();
    let cart_ctx = use_context::<CartContext>().expect("CartContext not provided");
    let items_sig = cart_ctx.items;

    let on_proceed = move |_| navigate("/checkout", Default::default());

    view! {
        <div class="page cart-page">
            <header class="page-header">
                <A href="/" attr:class="back-btn">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="page-logo">"PULSE"</span>
                <A href="/profile" attr:class="nav-avatar">
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/>
                        <circle cx="12" cy="7" r="4"/>
                    </svg>
                </A>
            </header>

            <div class="cart-title-wrap">
                <h1 class="cart-title">"YOUR"<br/>"PASSES"</h1>
                <p class="cart-sub">"Review your selection before checkout."</p>
            </div>

            {move || {
                let items = items_sig.get();
                if items.is_empty() {
                    return view! {
                        <div class="empty-cart">
                            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                <circle cx="9" cy="21" r="1"/>
                                <circle cx="20" cy="21" r="1"/>
                                <path d="M1 1h4l2.68 13.39a2 2 0 002 1.61h9.72a2 2 0 002-1.61L23 6H6"/>
                            </svg>
                            <p>"No passes selected yet."</p>
                            <A href="/explore" attr:class="browse-btn">"BROWSE EVENTS"</A>
                        </div>
                    }.into_any();
                }

                let sub = items.iter().map(|i| i.unit_price * i.quantity as i64).sum::<i64>();

                view! {
                    <div>
                        <div class="cart-items">
                            {items.iter().map(|item| {
                                let tier_id = item.tier_id.clone();
                                let tier_id_minus = tier_id.clone();
                                let tier_id_plus = tier_id.clone();
                                let tier_id_remove = tier_id.clone();
                                let qty = item.quantity;
                                let img = if item.event_cover.is_empty() {
                                    "https://images.unsplash.com/photo-1470225620780-dba8ba36b745?w=200&q=80".to_string()
                                } else {
                                    item.event_cover.clone()
                                };
                                let line_total = item.unit_price * item.quantity as i64;
                                let event_title = item.event_title.clone();
                                let tier_name = item.tier_name.clone();
                                let venue_name = item.venue_name.clone();
                                let unit_price = item.unit_price;

                                view! {
                                    <div class="cart-item">
                                        <img src=img alt=event_title.clone() class="item-img"/>
                                        <div class="item-info">
                                            <div class="item-event">{event_title}</div>
                                            <div class="item-tier">{tier_name}</div>
                                            <div class="item-venue">{venue_name}</div>
                                            <div class="item-price">{format_idr(unit_price)}</div>
                                        </div>
                                        <div class="item-right">
                                            <button
                                                class="item-remove"
                                                on:click=move |_| {
                                                    items_sig.update(|v| v.retain(|i| i.tier_id != tier_id_remove));
                                                }
                                            >
                                                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                                    <polyline points="3 6 5 6 21 6"/>
                                                    <path d="M19 6l-1 14H6L5 6"/>
                                                    <path d="M10 11v6M14 11v6"/>
                                                    <path d="M9 6V4h6v2"/>
                                                </svg>
                                            </button>
                                            <div class="item-qty-ctrl">
                                                <button class="iq-btn" on:click=move |_| {
                                                    let id = tier_id_minus.clone();
                                                    if qty > 1 {
                                                        items_sig.update(|v| {
                                                            if let Some(i) = v.iter_mut().find(|i| i.tier_id == id) {
                                                                i.quantity -= 1;
                                                            }
                                                        });
                                                    }
                                                }>"−"</button>
                                                <span class="iq-val">{qty}</span>
                                                <button class="iq-btn iq-btn--add" on:click=move |_| {
                                                    let id = tier_id_plus.clone();
                                                    items_sig.update(|v| {
                                                        if let Some(i) = v.iter_mut().find(|i| i.tier_id == id) {
                                                            i.quantity += 1;
                                                        }
                                                    });
                                                }>"+"</button>
                                            </div>
                                            <div class="item-subtotal">{format_idr(line_total)}</div>
                                        </div>
                                    </div>
                                }
                            }).collect_view()}
                        </div>

                        <div class="summary-card">
                            <div class="summary-title">"ITEMS SUMMARY"</div>
                            {items.iter().map(|item| view! {
                                <div class="summary-line">
                                    <span>{format!("{}× {}", item.quantity, item.tier_name)}</span>
                                    <span class="summary-line-val">{format_idr(item.unit_price * item.quantity as i64)}</span>
                                </div>
                            }).collect_view()}
                            <div class="summary-divider"></div>
                            <div class="summary-total-row">
                                <span class="summary-total-label">"TOTAL AMOUNT"</span>
                                <span class="summary-total-val">{format_idr(sub)}</span>
                            </div>
                        </div>

                        <div class="cart-footer">
                            <div class="footer-left">
                                <span class="footer-meta-label">"TOTAL AMOUNT"</span>
                                <div class="footer-total">{format_idr(sub)}</div>
                            </div>
                            <button class="proceed-btn" on:click=on_proceed.clone()>
                                "PROCEED TO PAYMENT"
                                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                    <line x1="5" y1="12" x2="19" y2="12"/>
                                    <polyline points="12 5 19 12 12 19"/>
                                </svg>
                            </button>
                        </div>
                    </div>
                }.into_any()
            }}

            <style>
                "
                .cart-page { min-height:100vh; display:flex; flex-direction:column; }
                .cart-title-wrap { padding:24px 20px 8px; }
                .cart-title { font-family:'Bebas Neue',sans-serif; font-size:40px; line-height:1; color:#fff; letter-spacing:2px; margin:0 0 8px; }
                .cart-sub { color:#6666aa; font-size:13px; }
                .empty-cart { flex:1; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:16px; padding:60px 24px; color:#6666aa; text-align:center; }
                .browse-btn { background:#c8ff5e; color:#0d0d1a; border-radius:12px; padding:12px 28px; font-weight:700; font-size:13px; letter-spacing:1px; text-decoration:none; }
                .cart-items { padding:0 16px; display:flex; flex-direction:column; gap:12px; margin-bottom:16px; }
                .cart-item { display:flex; gap:12px; background:rgba(255,255,255,0.04); border:1px solid rgba(255,255,255,0.07); border-radius:14px; padding:14px; }
                .item-img { width:70px; height:70px; border-radius:10px; object-fit:cover; flex-shrink:0; }
                .item-info { flex:1; min-width:0; }
                .item-event { font-size:13px; font-weight:600; color:#fff; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
                .item-tier { font-size:11px; color:#c8ff5e; margin-top:2px; }
                .item-venue { font-size:11px; color:#6666aa; margin-top:2px; }
                .item-price { font-size:13px; color:#aaa; margin-top:6px; }
                .item-right { display:flex; flex-direction:column; align-items:flex-end; gap:8px; }
                .item-remove { background:none; border:none; color:#6666aa; cursor:pointer; padding:4px; }
                .item-qty-ctrl { display:flex; align-items:center; gap:8px; background:rgba(255,255,255,0.06); border-radius:100px; padding:4px 8px; }
                .iq-btn { background:none; border:none; color:#c8ff5e; font-size:16px; cursor:pointer; padding:0 4px; line-height:1; }
                .iq-val { font-size:13px; color:#fff; min-width:16px; text-align:center; }
                .item-subtotal { font-size:13px; font-weight:700; color:#c8ff5e; }
                .summary-card { margin:0 16px 100px; background:rgba(255,255,255,0.03); border:1px solid rgba(255,255,255,0.06); border-radius:14px; padding:16px; }
                .summary-title { font-size:10px; letter-spacing:2px; color:#444466; font-weight:600; margin-bottom:12px; }
                .summary-line { display:flex; justify-content:space-between; align-items:center; padding:6px 0; color:#8888aa; font-size:13px; }
                .summary-line-val { color:#ccc; }
                .summary-divider { height:1px; background:rgba(255,255,255,0.06); margin:8px 0; }
                .summary-total-row { display:flex; justify-content:space-between; align-items:center; padding-top:4px; }
                .summary-total-label { font-size:11px; letter-spacing:1.5px; color:#8888aa; font-weight:600; }
                .summary-total-val { font-size:20px; font-weight:800; color:#c8ff5e; font-family:'Bebas Neue',sans-serif; }
                .cart-footer { position:fixed; bottom:0; left:0; right:0; background:rgba(5,8,20,0.95); border-top:1px solid rgba(255,255,255,0.08); padding:16px 20px; display:flex; align-items:center; justify-content:space-between; gap:16px; z-index:50; backdrop-filter:blur(12px); }
                .footer-left { display:flex; flex-direction:column; }
                .footer-meta-label { font-size:10px; letter-spacing:1.5px; color:#444466; font-weight:600; }
                .footer-total { font-size:20px; font-weight:800; color:#c8ff5e; font-family:'Bebas Neue',sans-serif; }
                .proceed-btn { display:flex; align-items:center; gap:8px; background:#c8ff5e; color:#0d0d1a; border:none; border-radius:14px; padding:14px 20px; font-weight:700; font-size:12px; letter-spacing:1px; cursor:pointer; white-space:nowrap; }
                "
            </style>
        </div>
    }
}
