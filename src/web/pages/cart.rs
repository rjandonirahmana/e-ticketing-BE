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

        </div>
    }
}
