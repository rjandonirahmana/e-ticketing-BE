use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::web::api::get_my_orders;
use crate::web::models::OrderListItem;
use crate::web::utils::format_idr;

// ── Frontend Order model ──────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct Order {
    pub id: String,
    pub title: String,
    pub price: String,
    pub event_date: String,
    pub venue: String,
    pub order_code: String,
    pub status: String,
    pub cover_url: Option<String>,
    pub expired_at: Option<String>,
}

fn item_to_order(b: OrderListItem) -> Order {
    let title = b
        .event_name
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Product".into());

    let status = match b.status.to_lowercase().as_str() {
        "paid" | "completed" => "PAID",
        "pending" => "WAITING FOR PAYMENT",
        "cancelled" => "CANCELLED",
        _ => "PENDING",
    }
    .to_string();

    let event_date = b
        .event_date
        .map(|dt| crate::web::models::format_date(&dt))
        .unwrap_or_else(|| crate::web::models::format_date(&b.created_at));

    let venue = b.venue.clone().filter(|s| !s.is_empty()).unwrap_or_default();

    Order {
        id: b.id.clone(),
        title,
        price: format_idr(b.total_amount as i64),
        event_date,
        venue,
        order_code: format!("#{}", b.order_code),
        status,
        cover_url: b.cover_url.filter(|s| !s.is_empty()),
        expired_at: b.expired_at.map(|dt| dt.to_rfc3339()),
    }
}

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct OrdersCtx {
    pub all: RwSignal<Vec<Order>>,
    pub waiting: RwSignal<Vec<Order>>,
    pub paid: RwSignal<Vec<Order>>,
    pub cancelled: RwSignal<Vec<Order>>,
    pub loading: RwSignal<bool>,
    pub error: RwSignal<String>,
}

impl OrdersCtx {
    pub fn load(&self) {
        // SSR guard: spawn_local tidak tersedia di server
        if is_server() {
            return;
        }
        if self.loading.get_untracked() {
            return;
        }
        self.loading.set(true);
        let all = self.all;
        let waiting = self.waiting;
        let paid = self.paid;
        let cancelled = self.cancelled;
        let loading = self.loading;
        let error = self.error;

        spawn_local(async move {
            match get_my_orders().await {
                Ok(orders) => {
                    let mut a = Vec::new();
                    let mut w = Vec::new();
                    let mut p = Vec::new();
                    let mut c = Vec::new();
                    for o in orders {
                        let mapped = item_to_order(o);
                        match mapped.status.as_str() {
                            "PAID" => {
                                a.push(mapped.clone());
                                p.push(mapped);
                            }
                            "CANCELLED" => {
                                a.push(mapped.clone());
                                c.push(mapped);
                            }
                            _ => {
                                a.push(mapped.clone());
                                w.push(mapped);
                            }
                        }
                    }
                    all.set(a);
                    waiting.set(w);
                    paid.set(p);
                    cancelled.set(c);
                }
                Err(e) => error.set(e.to_string()),
            }
            loading.set(false);
        });
    }
}

pub fn provide_orders_store() {
    provide_context(OrdersCtx {
        all: RwSignal::new(Vec::new()),
        waiting: RwSignal::new(Vec::new()),
        paid: RwSignal::new(Vec::new()),
        cancelled: RwSignal::new(Vec::new()),
        loading: RwSignal::new(false),
        error: RwSignal::new(String::new()),
    });
}

pub fn use_orders_store() -> OrdersCtx {
    use_context::<OrdersCtx>().expect("OrdersCtx not provided")
}
