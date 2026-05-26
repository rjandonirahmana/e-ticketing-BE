//! Trending events — mengambil dari GET /events?page=1&per_page=6
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::csr::models::ListEventsRequest;
use crate::csr::services::event as event_svc;
use crate::csr::utils::format_number;

#[derive(Clone, Debug, PartialEq)]
pub struct TrendingItem {
    pub id: String,
    pub title: String,
    pub venue: String,
    pub price: String,
    pub cover: String,
    pub category: Vec<String>,
}

#[derive(Clone, Copy)]
pub struct TrendingCtx {
    pub items: RwSignal<Vec<TrendingItem>>,
    pub loading: RwSignal<bool>,
}

impl TrendingCtx {
    pub fn load(&self) {
        // Guard: jangan fetch ulang kalau sedang loading
        if self.loading.get_untracked() {
            return;
        }
        self.loading.set(true);
        let items = self.items;
        let loading = self.loading;
        spawn_local(async move {
            let req = ListEventsRequest {
                category: String::new(),
                query: String::new(),
                page: 1,
                page_size: 6,
            };
            if let Ok(res) = event_svc::list_events(&req).await {
                let trending = res
                    .events
                    .iter()
                    .map(|e| {
                        let price = if e.base_price_idr == 0 {
                            "FREE".into()
                        } else {
                            format!("Rp{}", format_number(e.base_price_idr))
                        };
                        TrendingItem {
                            id: e.id.clone(),
                            title: e.title.clone(),
                            venue: e.venue.name.clone(),
                            price,
                            cover: e.cover_url.clone(),
                            category: e.category.clone(),
                        }
                    })
                    .collect();
                items.set(trending);
            }
            loading.set(false);
        });
    }
}

pub fn provide_trending_store() {
    let ctx = TrendingCtx {
        items: RwSignal::new(Vec::new()),
        loading: RwSignal::new(false),
    };
    ctx.load();
    provide_context(ctx);
}

pub fn use_trending_store() -> TrendingCtx {
    use_context::<TrendingCtx>().expect("TrendingCtx not provided")
}
