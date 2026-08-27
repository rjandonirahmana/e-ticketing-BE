//! orders.rs — Halaman Riwayat Order (unified SSR + hydration).
//!
//! Port parity dari `csr/pages/orders.rs`:
//!   - `use_orders_store()` + `Effect` → `Resource::new(.., get_my_orders)`.
//!   - Tab filter All / Waiting for Payment / Paid + pencarian (artist/venue/kode).
//!   - Desain `order-card` (thumbnail, status badge, divider, footer aksi)
//!     dipertahankan identik dengan CSR.
//!   - `OrderCardShimmer` saat loading via fallback Suspense.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::hooks::ThemeToggle;
use crate::web::api::get_my_orders;
use crate::web::app::AuthResource;
use crate::web::components::{BottomNav, CartButton, EmptyState, OrderCardShimmer};
use crate::web::models::{format_date, format_price, OrderListItem};

/// Klasifikasi status mentah backend → label tampilan + jenis.
/// kind: "pending" | "paid" | "cancelled".
fn classify(status: &str) -> (&'static str, &'static str) {
    let s = status.to_lowercase();
    if s == "paid" || s == "completed" {
        ("PAID", "paid")
    } else if s == "cancelled" || s == "canceled" || s == "expired" {
        ("CANCELLED", "cancelled")
    } else {
        ("WAITING FOR PAYMENT", "pending")
    }
}

#[component]
pub fn OrdersPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let orders = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in { get_my_orders().await } else { Ok(vec![]) }
        },
    );

    let filter = RwSignal::new("All".to_string());
    let query = RwSignal::new(String::new());

    let filtered_orders = Memo::new(move |_| {
        let q = query.get().to_lowercase();
        let f = filter.get();
        match orders.get() {
            Some(Ok(list)) => list
                .into_iter()
                .filter(|o| {
                    let (_, kind) = classify(&o.status);
                    let status_match = match f.as_str() {
                        "Waiting for Payment" => kind == "pending",
                        "Paid" => kind == "paid",
                        _ => true,
                    };
                    let ev = o.event_name.as_deref().unwrap_or("").to_lowercase();
                    let vn = o.venue.as_deref().unwrap_or("").to_lowercase();
                    let search_match = q.is_empty()
                        || ev.contains(&q)
                        || vn.contains(&q)
                        || o.order_code.to_lowercase().contains(&q);
                    status_match && search_match
                })
                .collect::<Vec<_>>(),
            _ => vec![],
        }
    });

    view! {
        <div class="page orders-page">
            <header class="page-header">
                <A href="/explore" attr:class="back-btn">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none"
                        stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </A>
                <span class="page-title">"Order History"</span>
                <div class="header-actions">
                    <CartButton />
                    <ThemeToggle />
                    <button class="icon-btn" aria-label="More">
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                            stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <circle cx="12" cy="5" r="1.4" />
                            <circle cx="12" cy="12" r="1.4" />
                            <circle cx="12" cy="19" r="1.4" />
                        </svg>
                    </button>
                </div>
            </header>
            <div class="orders-search">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                    stroke="currentColor" stroke-width="2" stroke-linecap="round">
                    <circle cx="11" cy="11" r="8" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                </svg>
                <input
                    type="search"
                    class="search-input"
                    placeholder="Cari produk atau toko..."
                    prop:value=move || query.get()
                    on:input=move |ev| query.set(event_target_value(&ev))
                />
            </div>
            <div class="filter-tabs">
                {["All", "Waiting for Payment", "Paid"].iter().map(|f| {
                    let label = *f;
                    let cls = move || {
                        if filter.get() == label { "filter-tab filter-tab--active" }
                        else { "filter-tab" }
                    };
                    view! {
                        <button class=cls on:click=move |_| filter.set(label.into())>
                            {label}
                        </button>
                    }
                }).collect_view()}
            </div>
            <Suspense fallback=|| view! {
                <div class="orders-list">
                    <OrderCardShimmer />
                    <OrderCardShimmer />
                    <OrderCardShimmer />
                </div>
            }>
                {move || {
                    orders.get().map(|res| {
                        let orders_content = if !is_logged_in() && auth.get().is_some() {
                            view! {
                                <EmptyState
                                    icon="🔒"
                                    title="HARUS MASUK"
                                    body="Kamu harus masuk untuk melihat riwayat order."
                                    cta_label="MASUK"
                                    cta_href="/login"
                                />
                            }.into_any()
                        } else {
                            match res {
                                Err(_) => view! {
                                    <EmptyState
                                        icon="⚠️"
                                        title="TERJADI KESALAHAN"
                                        body="Gagal memuat order. Coba login ulang."
                                    />
                                }.into_any(),
                                Ok(_) => view! {
                                    {move || {
                                        let f = filter.get();
                                        let filtered = filtered_orders.get();
                                        if filtered.is_empty() {
                                            let (icon, title, body) = match f.as_str() {
                                                "Waiting for Payment" => (
                                                    "🕐",
                                                    "TIDAK ADA PESANAN PENDING",
                                                    "Pesanan yang menunggu pembayaran akan muncul di sini.",
                                                ),
                                                "Paid" => (
                                                    "✅",
                                                    "BELUM ADA PESANAN SELESAI",
                                                    "Pesanan yang sudah dibayar akan muncul di sini.",
                                                ),
                                                _ => (
                                                    "🛒",
                                                    "BELUM ADA PESANAN",
                                                    "Pesanan Anda akan muncul di sini setelah melakukan pembelian.",
                                                ),
                                            };
                                            view! {
                                                <EmptyState
                                                    icon=icon
                                                    title=title
                                                    body=body
                                                    cta_label="JELAJAHI PRODUK"
                                                    cta_href="/explore"
                                                />
                                            }.into_any()
                                        } else {
                                            filtered.into_iter().map(order_card).collect_view().into_any()
                                        }
                                    }}
                                }.into_any(),
                            }
                        };

                        view! {
                            <div class="orders-list">
                                {orders_content}
                            </div>
                        }.into_any()
                    })
                }}
            </Suspense>
            <BottomNav active="orders" />
        </div>
    }
}

fn order_card(o: OrderListItem) -> impl IntoView {
    let (status_label, kind) = classify(&o.status);
    let is_pending = kind == "pending";
    let is_cancelled = kind == "cancelled";

    let pill_cls = match kind {
        "paid" => "order-status-badge order-status-badge--paid",
        "pending" => "order-status-badge order-status-badge--pending",
        _ => "order-status-badge order-status-badge--cancelled",
    };

    // pending   → /orders/{id}          (halaman verifikasi/pembayaran)
    // paid      → /orders/{id}/tickets
    // cancelled → tidak ada aksi
    let action_href = if is_pending {
        format!("/orders/{}", o.id)
    } else if !is_cancelled {
        format!("/orders/{}/tickets", o.id)
    } else {
        String::new()
    };

    let title = o.event_name.clone().unwrap_or_else(|| "Produk".into());
    let venue = o.venue.clone().unwrap_or_default();
    let date = o
        .event_date
        .as_ref()
        .map(format_date)
        .unwrap_or_default();
    let date_venue = if venue.is_empty() {
        date.clone()
    } else if date.is_empty() {
        venue.clone()
    } else {
        format!("{} • {}", date, venue)
    };
    let price = format_price(o.total_amount);
    let cover = o.cover_url.clone();

    view! {
        <div class="order-card">
            <div class="order-card-top">
                <div class="order-thumb">
                    {match cover {
                        Some(url) => {
                            view! { <img src=url alt="produk" class="order-thumb-img" /> }
                                .into_any()
                        }
                        None => {
                            view! {
                                <div class="order-thumb-placeholder">
                                    <svg width="26" height="26" viewBox="0 0 24 24" fill="none"
                                        stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                        <path d="M9 19V6l12-3v13M9 19c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zm12-3c0 1.105-1.343 2-3 2s-3-.895-3-2 1.343-2 3-2 3 .895 3 2zM9 10l12-3" />
                                    </svg>
                                </div>
                            }
                                .into_any()
                        }
                    }}
                </div>

                <div class="order-info">
                    <div class="order-name-row">
                        <h3 class="order-product-name">{title}</h3>
                        <span class=pill_cls>{status_label}</span>
                    </div>
                    <div class="order-date-venue">
                        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                            stroke-width="2" stroke-linecap="round">
                            <rect x="3" y="4" width="18" height="18" rx="2" />
                            <line x1="16" y1="2" x2="16" y2="6" />
                            <line x1="8" y1="2" x2="8" y2="6" />
                            <line x1="3" y1="10" x2="21" y2="10" />
                        </svg>
                        <span>{date_venue}</span>
                    </div>
                </div>
            </div>

            <div class="order-card-divider"></div>

            <div class="order-card-footer">
                <div class="order-total-block">
                    <span class="order-total-label">"TOTAL AMOUNT"</span>
                    <span class="order-total-price">{price}</span>
                </div>

                {if is_cancelled {
                    view! { <span></span> }.into_any()
                } else if is_pending {
                    view! {
                        <A href=action_href attr:class="order-action-btn order-action-btn--pay">
                            "Pay Now"
                        </A>
                    }
                        .into_any()
                } else {
                    view! {
                        <A href=action_href attr:class="order-action-btn order-action-btn--view">
                            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                                stroke-width="2.5" stroke-linecap="round">
                                <path d="M20 12V22H4V12" />
                                <path d="M22 7H2v5h20V7z" />
                                <path d="M12 22V7" />
                            </svg>
                            "Lihat Kode Ambil"
                        </A>
                    }
                        .into_any()
                }}
            </div>
        </div>
    }
}
