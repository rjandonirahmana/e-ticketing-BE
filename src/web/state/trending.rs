//! Trending events — server function GetEvents (page 1, 6 item).
//!
//! Data diambil lewat server function `get_events` (jalur SSR yang benar,
//! menambahkan X-App-Token di server). Pola `RwSignal` + `spawn_local`
//! dipertahankan agar konsisten dengan store lain.
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::web::api::get_events;
use crate::web::models::Event;
use crate::web::utils::format_number;

#[derive(Clone, Debug, PartialEq)]
pub struct TrendingItem {
    pub id: String,
    pub title: String,
    pub venue: String,
    pub price: String,
    pub cover: String,
    pub category: Vec<String>,
}

fn event_to_trending(e: &Event) -> TrendingItem {
    let price = if e.display_price <= 0.0 {
        "FREE".into()
    } else {
        format!("Rp{}", format_number(e.display_price as i64))
    };
    TrendingItem {
        id: e.id.clone(),
        title: e.name.clone(),
        venue: e.venue.clone().unwrap_or_default(),
        price,
        cover: e.cover_url.clone().unwrap_or_default(),
        category: e.category.clone(),
    }
}

#[derive(Clone, Copy)]
pub struct TrendingCtx {
    pub items: RwSignal<Vec<TrendingItem>>,
    pub loading: RwSignal<bool>,
}

impl TrendingCtx {
    pub fn load(&self) {
        // SSR guard: spawn_local tidak tersedia di server
        if is_server() {
            return;
        }
        if self.loading.get_untracked() {
            return;
        }
        self.loading.set(true);
        let items = self.items;
        let loading = self.loading;
        spawn_local(async move {
            if let Ok(res) = get_events(Some(1), None, None, None, Some(6)).await {
                let trending = res.data.iter().map(event_to_trending).collect();
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
