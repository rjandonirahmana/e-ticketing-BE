//! Banner slider di home page — diisi dari event terbaru via server function.
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::web::api::get_events;
use crate::web::models::{format_date, Event};
use crate::web::utils::format_number;

#[derive(Clone, Debug, PartialEq)]
pub struct Banner {
    pub id:       String,
    pub title:    String,
    pub subtitle: String,
    pub badge:    String,
    pub date:     String,
    pub price:    String,
    pub cover:    String,
    pub href:     String,
}

fn event_to_banner(e: &Event) -> Banner {
    let price = if e.display_price <= 0.0 {
        "FREE".into()
    } else {
        format!("Rp{}", format_number(e.display_price as i64))
    };
    let dt = e.start_time.unwrap_or(e.event_date);
    Banner {
        id:       e.id.clone(),
        title:    e.name.clone(),
        subtitle: e.venue.clone().unwrap_or_default(),
        badge:    e.status.to_uppercase(),
        date:     format_date(&dt),
        price,
        cover:    e.cover_url.clone().unwrap_or_default(),
        href:     format!("/events/{}", e.slug),
    }
}

#[derive(Clone, Copy)]
pub struct BannersCtx {
    pub items:   RwSignal<Vec<Banner>>,
    pub loading: RwSignal<bool>,
}

impl BannersCtx {
    pub fn load(&self) {
        // SSR guard: spawn_local tidak tersedia di server
        if is_server() {
            return;
        }
        if self.loading.get_untracked() {
            return;
        }
        self.loading.set(true);
        let items   = self.items;
        let loading = self.loading;
        spawn_local(async move {
            if let Ok(res) = get_events(Some(1), None, None, None, Some(5)).await {
                let banners = res.data.iter().map(event_to_banner).collect();
                items.set(banners);
            }
            loading.set(false);
        });
    }
}

pub fn provide_banners_store() {
    let ctx = BannersCtx {
        items:   RwSignal::new(Vec::new()),
        loading: RwSignal::new(false),
    };
    ctx.load();
    provide_context(ctx);
}

pub fn use_banners_store() -> BannersCtx {
    use_context::<BannersCtx>().expect("BannersCtx not provided")
}
