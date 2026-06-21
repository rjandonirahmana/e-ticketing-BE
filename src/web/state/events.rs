use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::web::api::{get_categories, get_events};
use crate::web::models::{format_date, Event};
use crate::web::utils::format_number;

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
    let price_raw = e.display_price as i64;
    let price_str = if price_raw <= 0 {
        "FREE".into()
    } else {
        format!("Rp{}", format_number(price_raw))
    };
    let dt = e.start_time.unwrap_or(e.event_date);

    ExploreEvent {
        id: e.id.clone(),
        slug: e.slug.clone(),
        title: e.name.clone(),
        category: e.category.clone(),
        date: format_date(&dt),
        venue: e.venue.clone().unwrap_or_default(),
        city: e.city.clone().unwrap_or_default(),
        price: price_raw,
        price_str,
        cover: e.cover_url.clone().unwrap_or_default(),
        is_live: e.status.eq_ignore_ascii_case("live"),
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
    // Cancels stale fetches when category changes rapidly.
    fetch_gen: RwSignal<u32>,
}

impl EventsCtx {
    pub fn load(&self) {
        self.load_cat(String::new());
    }

    pub fn load_cat(&self, category: String) {
        if is_server() {
            return;
        }
        leptos::logging::log!("[EventsStore] load_cat: category={:?}", category);
        self.loading.set(true);

        // Increment generation so any in-flight fetch from the previous
        // category becomes a no-op when it completes.
        let gen = self.fetch_gen.get_untracked().wrapping_add(1);
        self.fetch_gen.set(gen);

        let fetch_gen = self.fetch_gen;
        let items = self.items;
        let loading = self.loading;
        let error = self.error;

        spawn_local(async move {
            let cat = if category == "All" { String::new() } else { category };
            let cat_opt = if cat.is_empty() { None } else { Some(cat) };

            leptos::logging::log!("[EventsStore] get_events fetch starting...");

            // Race the server function against a 12-second safety timeout.
            // Prevents infinite shimmer if the DB query hangs or network drops.
            let fetch = get_events(Some(1), None, cat_opt, None, Some(40));
            let timeout = gloo_timers::future::TimeoutFuture::new(12_000);

            let result = futures::future::select(Box::pin(fetch), Box::pin(timeout)).await;

            match result {
                futures::future::Either::Left((srv_result, _)) => {
                    leptos::logging::log!("[EventsStore] get_events ok={}", srv_result.is_ok());
                    if fetch_gen.get_untracked() == gen {
                        match srv_result {
                            Ok(res) => items.set(res.data.iter().map(event_to_explore).collect()),
                            Err(e) => error.set(e.to_string()),
                        }
                    }
                }
                futures::future::Either::Right(_) => {
                    leptos::logging::log!("[EventsStore] get_events TIMED OUT after 12s");
                    if fetch_gen.get_untracked() == gen {
                        error.set("Koneksi ke server habis waktu. Coba refresh.".to_string());
                    }
                }
            }
            loading.set(false);
        });
    }
}

pub fn provide_events_store() {
    let ctx = EventsCtx {
        items: RwSignal::new(Vec::new()),
        categories: RwSignal::new(vec!["All".to_string()]),
        // Start as loading=true so SSR renders the shimmer. The client
        // hydrates with the same initial state → no hydration mismatch.
        // ExplorePage's Effect triggers the actual fetch post-hydration.
        loading: RwSignal::new(true),
        error: RwSignal::new(String::new()),
        fetch_gen: RwSignal::new(0),
    };

    // Load categories from BE — client only.
    if !is_server() {
        let cats_signal = ctx.categories;
        spawn_local(async move {
            if let Ok(mut cats) = get_categories().await {
                cats.retain(|c| !c.is_empty());
                let mut full = vec!["All".to_string()];
                full.extend(cats);
                cats_signal.set(full);
            }
        });
        // NOTE: Do NOT call ctx.load() here. On direct navigation to /explore,
        // calling it sets loading=true before hydration, but SSR already rendered
        // with loading=true (shimmer). The mismatch previously caused hydration
        // failure. Now ExplorePage's own Effect owns the initial fetch.
    }

    provide_context(ctx);
}

pub fn use_events_store() -> EventsCtx {
    use_context::<EventsCtx>().expect("EventsCtx not provided")
}
