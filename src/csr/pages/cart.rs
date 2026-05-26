use crate::csr::hooks::use_nav;
use leptos::prelude::*;
use leptos_router::components::A;

use crate::csr::hooks::{format_idr, use_cart, ThemeToggle};

#[component]
pub fn CartPage() -> impl IntoView {
    let cart = use_cart();
    let navigate = use_nav();

    // FIX #7: satu Memo untuk semua data cart-derived.
    // Sebelumnya ada 2 closure terpisah yang masing-masing memanggil
    // cart.items.get(), artinya 2 reactive subscription.  Dengan Memo,
    // hanya ada 1 subscription; semua derived value (items + subtotal)
    // dihitung sekaligus dan hasilnya di-cache sampai cart berubah lagi.
    let cart_memo = Memo::new(move |_| {
        let items = cart.items.get();
        let subtotal = items
            .iter()
            .map(|i| i.unit_price * i.quantity as i64)
            .sum::<i64>();
        (items, subtotal)
    });

    let proceed = move |_| navigate("/checkout", Default::default());

    view! {
        <div class="page cart-page">
            <header class="page-header">
                <A href="/" attr:class="back-btn"><svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><polyline points="15 18 9 12 15 6"/></svg></A>
                <span class="page-logo">"KINETIC"</span>
                <div class="header-actions">
                    <ThemeToggle />
                    <A href="/profile" attr:class="nav-avatar"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/><circle cx="12" cy="7" r="4"/></svg></A>
                </div>
            </header>

            <div class="cart-title-wrap">
                <h1 class="cart-title">"YOUR"<br/>"PASSES"</h1>
                <p class="cart-sub">"Review your selection before checkout."</p>
            </div>

            {move || {
                let (items, subtotal) = cart_memo.get();
                if items.is_empty() {
                    view! {
                        <div class="empty-cart">
                            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"><path d="M2 9a3 3 0 010-6h20a3 3 0 010 6H2zM2 15a3 3 0 000 6h20a3 3 0 000-6H2z"/></svg>
                            <p>"No passes selected yet."</p>
                            <A href="/" attr:class="browse-btn">"BROWSE EVENTS"</A>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <>
                            <div class="cart-items">
                                {items.iter().map(|item| {
                                    let it = item.clone();
                                    let it_for_remove = it.clone();
                                    let it_for_minus = it.clone();
                                    let it_for_plus = it.clone();
                                    let img = if it.event_cover.is_empty() {
                                        "https://images.unsplash.com/photo-1470225620780-dba8ba36b745?w=200&q=80".to_string()
                                    } else { it.event_cover.clone() };
                                    let line_total = it.unit_price * it.quantity as i64;
                                    view! {
                                        <div class="cart-item">
                                            <img src=img alt=it.event_title.clone() class="item-img" />
                                            <div class="item-info">
                                                <div class="item-event">{it.event_title.clone()}</div>
                                                <div class="item-tier">{it.tier_name.clone()}</div>
                                                <div class="item-venue">{it.venue_name.clone()}</div>
                                                <div class="item-price">{format_idr(it.unit_price)}</div>
                                            </div>
                                            <div class="item-right">
                                                <button class="item-remove" on:click=move |_| cart.remove_item(&it_for_remove.tier_id)>
                                                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14H6L5 6"/><path d="M10 11v6M14 11v6"/><path d="M9 6V4h6v2"/></svg>
                                                </button>
                                                <div class="item-qty-ctrl">
                                                    <button class="iq-btn" on:click=move |_| cart.update_qty(&it_for_minus.tier_id, it_for_minus.quantity - 1)>"−"</button>
                                                    <span class="iq-val">{it.quantity}</span>
                                                    <button class="iq-btn iq-btn--add" on:click=move |_| cart.update_qty(&it_for_plus.tier_id, it_for_plus.quantity + 1)>"+"</button>
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
                                    <span class="summary-total-val">{format_idr(subtotal)}</span>
                                </div>
                            </div>
                        </>
                    }.into_any()
                }
            }}

            {move || {
                let (items, subtotal) = cart_memo.get();
                if items.is_empty() { return ().into_any(); }
                let summary = items.iter().map(|i| format!("{}x {}", i.quantity, i.tier_name)).collect::<Vec<_>>().join(", ");
                view! {
                    <div class="cart-footer">
                        <div class="footer-left">
                            <span class="footer-meta-label">"ITEMS SUMMARY"</span>
                            <span class="footer-meta-desc">{summary}</span>
                        </div>
                        <div class="footer-right">
                            <div>
                                <span class="footer-total-label">"TOTAL AMOUNT"</span>
                                <div class="footer-total">{format_idr(subtotal)}</div>
                            </div>
                            <button class="proceed-btn" on:click=proceed.clone()>
                                "PROCEED TO PAYMENT"
                                <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><line x1="5" y1="12" x2="19" y2="12"/><polyline points="12 5 19 12 12 19"/></svg>
                            </button>
                        </div>
                    </div>
                }.into_any()
            }}
        </div>
    }
}
