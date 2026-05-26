use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::csr::models::ListEventsRequest;
use crate::csr::services::event as event_svc;
use crate::csr::utils::format_number;

#[derive(Clone, Debug, PartialEq)]
pub struct VenueEvent {
    pub id:    String,
    pub slug:  String,
    pub title: String,
    pub date:  String,
    pub price: String,
    pub cover: String,
    pub grad:  String,
}

#[derive(Clone, Copy)]
pub struct VenueEventsCtx {
    pub items:   RwSignal<Vec<VenueEvent>>,
    pub loading: RwSignal<bool>,
}

impl VenueEventsCtx {
    pub fn load(&self, _city: String) {
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
                page_size: 6,
            };
            if let Ok(resp) = event_svc::list_events(&req).await {
                let mapped = resp
                    .events
                    .into_iter()
                    .map(|e| {
                        let price = if e.base_price_idr == 0 {
                            "GRATIS".into()
                        } else {
                            format!("Rp{}", format_number(e.base_price_idr))
                        };
                        VenueEvent {
                            slug:  e.slug,
                            id:    e.id,
                            title: e.title,
                            date:  e.start_time,
                            price,
                            cover: e.cover_url,
                            grad:  "linear-gradient(135deg, #1a3a8a 0%, #4f6bff 100%)".into(),
                        }
                    })
                    .collect();
                items.set(mapped);
            }
            loading.set(false);
        });
    }
}

pub fn provide_venue_events_store() {
    provide_context(VenueEventsCtx {
        items:   RwSignal::new(vec![]),
        loading: RwSignal::new(false),
    });
}

pub fn use_venue_events_store() -> VenueEventsCtx {
    use_context::<VenueEventsCtx>().expect("VenueEventsCtx not provided")
}
