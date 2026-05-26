//! Banner slider di home page — diisi dari event terbaru via GET /events
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::csr::models::ListEventsRequest;
use crate::csr::services::event as event_svc;
use crate::csr::utils::format_number;

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

#[derive(Clone, Copy)]
pub struct BannersCtx {
    pub items:   RwSignal<Vec<Banner>>,
    pub loading: RwSignal<bool>,
}

impl BannersCtx {
    pub fn load(&self) {
        // Guard: jangan fetch ulang kalau sudah/sedang loading
        if self.loading.get_untracked() {
            return;
        }
        self.loading.set(true);
        let items   = self.items;
        let loading = self.loading;
        spawn_local(async move {
            let req = ListEventsRequest {
                category:  String::new(),
                query:     String::new(),
                page:      1,
                page_size: 5,
            };
            if let Ok(res) = event_svc::list_events(&req).await {
                let banners = res.events.iter().map(|e| {
                    let price = e
                        .tiers
                        .first()
                        .map(|t| {
                            if t.price_idr == 0 {
                                "FREE".into()
                            } else {
                                format!("Rp{}", format_number(t.price_idr))
                            }
                        })
                        .unwrap_or_else(|| "TBA".into());
                    Banner {
                        id:       e.id.clone(),
                        title:    e.title.clone(),
                        subtitle: e.venue.name.clone(),
                        badge:    e.status.to_uppercase(),
                        date:     e.start_time.clone(),
                        price,
                        cover:    e.cover_url.clone(),
                        href:     format!("/events/{}", e.id),
                    }
                }).collect();
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
