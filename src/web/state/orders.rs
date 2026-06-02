use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Deserializer};

use crate::csr::services::client::get_private;
use crate::csr::utils::format_idr;

fn de_f64_or_str<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    use serde::de::{self, Visitor};
    struct F64OrStr;
    impl<'de> Visitor<'de> for F64OrStr {
        type Value = f64;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a number or numeric string")
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
            Ok(v)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
            Ok(v as f64)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
            v.parse::<f64>().map_err(de::Error::custom)
        }
    }
    d.deserialize_any(F64OrStr)
}

// ── BE response types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct BeOrder {
    pub id: String,
    #[serde(default)]
    pub order_code: String,
    #[serde(default)]
    pub status: String,
    #[serde(deserialize_with = "de_f64_or_str")]
    pub total_amount: f64,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub expired_at: Option<String>,
    #[serde(default)]
    pub event_name: Option<String>,
    #[serde(default)]
    pub event_date: Option<String>,
    #[serde(default)]
    pub venue: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
}

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

fn fmt_event_date(iso: &str) -> String {
    let date_part = iso.split('T').next().unwrap_or(iso);
    let mut parts = date_part.split('-');
    let y = parts.next().unwrap_or("2024");
    let m: usize = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let d: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let months = [
        "", "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mon = months.get(m).copied().unwrap_or("?");
    format!("{} {}, {}", mon, d, y)
}

fn be_to_order(b: BeOrder) -> Order {
    let title = b
        .event_name
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Event".into());

    let status = match b.status.to_lowercase().as_str() {
        "paid" | "completed" => "PAID",
        "pending" => "WAITING FOR PAYMENT",
        "cancelled" => "CANCELLED",
        _ => "PENDING",
    }
    .to_string();

    let event_date = b
        .event_date
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(fmt_event_date)
        .unwrap_or_else(|| {
            b.created_at
                .as_deref()
                .and_then(|s| s.get(..10))
                .unwrap_or("—")
                .to_string()
        });

    let venue = b
        .venue
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_default();

    Order {
        id: b.id.clone(),
        title,
        price: format_idr(b.total_amount as i64),
        event_date,
        venue,
        order_code: format!("#{}", b.order_code),
        status,
        cover_url: b.cover_url.filter(|s| !s.is_empty()),
        expired_at: b.expired_at,
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
            match get_private::<Vec<BeOrder>>("/orders").await {
                Ok(orders) => {
                    let mut a = Vec::new();
                    let mut w = Vec::new();
                    let mut p = Vec::new();
                    let mut c = Vec::new();
                    for o in orders {
                        let mapped = be_to_order(o);
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
                Err(e) => error.set(e.message),
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
