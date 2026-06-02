use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::csr::models::{Event, ListEventsRequest};
use crate::csr::services::event as event_svc;
use crate::csr::utils::format_number;

// ── Frontend event model ──────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct ExploreEvent {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub category: Vec<String>,
    pub date: String,
    pub venue: String,
    pub city: String,
    pub price: i64,
    pub price_str: String,
    pub cover: String,
    pub is_live: bool,
    pub status: String,
    pub total_sold: i32,
    pub total_quota: i32,
}

pub fn event_to_explore_pub(e: &Event) -> ExploreEvent {
    event_to_explore(e)
}

pub(super) fn event_to_explore(e: &Event) -> ExploreEvent {
    let price_raw = e
        .tiers
        .first()
        .map(|t| t.price_idr)
        .unwrap_or(e.base_price_idr);
    let price_str = if price_raw == 0 {
        "FREE".into()
    } else {
        format!("Rp{}", format_number(price_raw))
    };

    ExploreEvent {
        id: e.id.clone(),
        slug: e.slug.clone(),
        title: e.title.clone(),
        category: e.category.clone(),
        date: e.start_time.clone(),
        venue: e.venue.name.clone(),
        city: e.venue.city.clone(),
        price: price_raw,
        price_str,
        cover: e.cover_url.clone(),
        is_live: e.status.to_lowercase() == "live",
        status: e.status.clone(),
        total_sold: e.total_sold,
        total_quota: e.total_quota,
    }
}

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct EventsCtx {
    pub items: RwSignal<Vec<ExploreEvent>>,
    pub categories: RwSignal<Vec<String>>,
    pub loading: RwSignal<bool>,
    pub error: RwSignal<String>,
}

impl EventsCtx {
    /// Load semua event (tanpa filter kategori)
    pub fn load(&self) {
        self.load_cat(String::new());
    }

    pub fn load_cat(&self, category: String) {
        // SSR guard: spawn_local tidak tersedia di server
        if is_server() {
            return;
        }
        // Guard: jangan kirim request baru kalau sedang loading
        if self.loading.get_untracked() {
            return;
        }
        self.loading.set(true);
        let items = self.items;
        let loading = self.loading;
        let error = self.error;

        spawn_local(async move {
            let req = ListEventsRequest {
                category: if category == "All" {
                    String::new()
                } else {
                    category
                },
                query: String::new(),
                page: 1,
                page_size: 40,
            };
            match event_svc::list_events(&req).await {
                Ok(res) => items.set(res.events.iter().map(event_to_explore).collect()),
                Err(e) => error.set(e.message),
            }
            loading.set(false);
        });
    }
}

pub fn provide_events_store() {
    let ctx = EventsCtx {
        items: RwSignal::new(Vec::new()),
        // "All" always first; BE categories appended setelah fetch
        categories: RwSignal::new(vec!["All".to_string()]),
        loading: RwSignal::new(false),
        error: RwSignal::new(String::new()),
    };

    // Load categories dari BE — hanya di client
    if !is_server() {
        let cats_signal = ctx.categories;
        spawn_local(async move {
            if let Ok(mut cats) = event_svc::get_categories().await {
                cats.retain(|c| !c.is_empty());
                let mut full = vec!["All".to_string()];
                full.extend(cats);
                cats_signal.set(full);
            }
        });

        // Load awal event — guard di dalam load_cat mencegah double fetch
        ctx.load();
    }

    provide_context(ctx);
}

pub fn use_events_store() -> EventsCtx {
    use_context::<EventsCtx>().expect("EventsCtx not provided")
}
