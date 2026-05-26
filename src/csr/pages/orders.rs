use leptos::prelude::*;
use leptos_router::components::A;

use crate::csr::components::{BottomNav, EmptyState, OrderCardShimmer};
use crate::csr::hooks::ThemeToggle;
use crate::csr::state::{use_orders_store, Order};

#[component]
pub fn OrdersPage() -> impl IntoView {
    let filter = RwSignal::new("All".to_string());
    let query = RwSignal::new(String::new());

    let store = use_orders_store();
    Effect::new(move |_| {
        store.load();
    });

    view! {
        <div class="page orders-page">
            <header class="page-header">
                <A href="/explore" attr:class="back-btn">
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
                <span class="page-title">"Order History"</span>
                <div class="header-actions">
                    <ThemeToggle />
                    <button class="icon-btn" aria-label="More">
                        <svg
                            width="20"
                            height="20"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                        >
                            <circle cx="12" cy="5" r="1.4" />
                            <circle cx="12" cy="12" r="1.4" />
                            <circle cx="12" cy="19" r="1.4" />
                        </svg>
                    </button>
                </div>
            </header>

            // Search bar
            <div class="orders-search">
                <svg
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                >
                    <circle cx="11" cy="11" r="8" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                </svg>
                <input
                    type="search"
                    class="search-input"
                    placeholder="Search by artist or venue..."
                    prop:value=move || query.get()
                    on:input=move |ev| query.set(event_target_value(&ev))
                />
            </div>

            // Filter tabs: All | Waiting for Payment | Paid
            <div class="filter-tabs">
                {["All", "Waiting for Payment", "Paid"]
                    .iter()
                    .map(|f| {
                        let label = *f;
                        let cls = move || {
                            if filter.get() == label {
                                "filter-tab filter-tab--active"
                            } else {
                                "filter-tab"
                            }
                        };
                        view! {
                            <button class=cls on:click=move |_| filter.set(label.into())>
                                {label}
                            </button>
                        }
                    })
                    .collect_view()}
            </div>

            // Loading shimmer
            {move || {
                store
                    .loading
                    .get()
                    .then(|| {
                        view! {
                            <div class="orders-list">
                                <OrderCardShimmer />
                                <OrderCardShimmer />
                                <OrderCardShimmer />
                            </div>
                        }
                    })
            }}

            // Error state
            {move || {
                (!store.error.get().is_empty())
                    .then(|| {
                        let err = store.error.get();
                        view! { <EmptyState icon="⚠️" title="TERJADI KESALAHAN" body=err /> }
                    })
            }}

            // Order list
            <div class="orders-list">
                {move || {
                    if store.loading.get() {
                        return view! { <span></span> }.into_any();
                    }
                    let q = query.get().to_lowercase();
                    let f = filter.get();
                    let list = match f.as_str() {
                        "Waiting for Payment" => store.waiting.with(|l| l.clone()),
                        "Paid" => store.paid.with(|l| l.clone()),
                        _ => store.all.with(|l| l.clone()),
                    };
                    let list: Vec<Order> = if q.is_empty() {
                        list
                    } else {
                        list.into_iter()
                            .filter(|o| {
                                o.title.to_lowercase().contains(&q)
                                    || o.venue.to_lowercase().contains(&q)
                            })
                            .collect()
                    };
                    if list.is_empty() {
                        let (icon, title, body) = match f.as_str() {
                            "Waiting for Payment" => {
                                (
                                    "🕐",
                                    "TIDAK ADA PESANAN PENDING",
                                    "Pesanan yang menunggu pembayaran akan muncul di sini.",
                                )
                            }
                            "Paid" => {
                                (
                                    "✅",
                                    "BELUM ADA PESANAN SELESAI",
                                    "Pesanan yang sudah dibayar akan muncul di sini.",
                                )
                            }
                            _ => {
                                (
                                    "🛒",
                                    "BELUM ADA PESANAN",
                                    "Pesanan Anda akan muncul di sini setelah melakukan pembelian.",
                                )
                            }
                        };
                        view! {
                            <EmptyState
                                icon=icon
                                title=title
                                body=body
                                cta_label="JELAJAHI EVENT"
                                cta_href="/explore"
                            />
                        }
                            .into_any()
                    } else {
                        list.into_iter().map(order_card).collect_view().into_any()
                    }
                }}
            </div>

            <BottomNav active="orders" />
        </div>
    }
}

fn order_card(o: Order) -> impl IntoView {
    let is_pending = o.status == "WAITING FOR PAYMENT";
    let is_cancelled = o.status == "CANCELLED";

    let pill_cls = if o.status == "PAID" {
        "order-status-badge order-status-badge--paid"
    } else if is_pending {
        "order-status-badge order-status-badge--pending"
    } else {
        "order-status-badge order-status-badge--cancelled"
    };

    // pending   → /orders/{id}          (Verification/payment page)
    // paid      → /orders/{id}/tickets  (GET /api/orders/:id/tickets)
    // cancelled → tidak ada aksi
    let action_href = if is_pending {
        format!("/orders/{}", o.id)
    } else if !is_cancelled {
        format!("/orders/{}/tickets", o.id) // ← FIX: bukan /tickets/{order_id}
    } else {
        String::new()
    };

    let date_venue = if o.venue.is_empty() {
        o.event_date.clone()
    } else {
        format!("{} • {}", o.event_date, o.venue)
    };

    let status_label = o.status.clone();
    let cover = o.cover_url.clone();

    view! {
        <div class="order-card">
            // Top row: thumbnail + info + badge
            <div class="order-card-top">
                <div class="order-thumb">
                    {match cover {
                        Some(url) => {
                            view! { <img src=url alt="event" class="order-thumb-img" /> }.into_any()
                        }
                        None => {
                            view! {
                                <div class="order-thumb-placeholder">
                                    <svg
                                        width="26"
                                        height="26"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="1.5"
                                        stroke-linecap="round"
                                    >
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
                        <h3 class="order-event-name">{o.title}</h3>
                        <span class=pill_cls>{status_label}</span>
                    </div>
                    <div class="order-date-venue">
                        <svg
                            width="11"
                            height="11"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                        >
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

            // Footer: total + action
            <div class="order-card-footer">
                <div class="order-total-block">
                    <span class="order-total-label">"TOTAL AMOUNT"</span>
                    <span class="order-total-price">{o.price}</span>
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
                            <svg
                                width="13"
                                height="13"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2.5"
                                stroke-linecap="round"
                            >
                                <path d="M20 12V22H4V12" />
                                <path d="M22 7H2v5h20V7z" />
                                <path d="M12 22V7" />
                            </svg>
                            "View Ticket"
                        </A>
                    }
                        .into_any()
                }}
            </div>
        </div>
    }
}
