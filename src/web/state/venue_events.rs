use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::web::api::get_events;
use crate::web::models::Event;
use crate::web::utils::format_number;

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

fn event_to_venue(e: &Event) -> VenueEvent {
    let price = if e.display_price <= 0.0 {
        "GRATIS".into()
    } else {
        format!("Rp{}", format_number(e.display_price as i64))
    };
    let dt = e.start_time.unwrap_or(e.event_date);
    VenueEvent {
        slug:  e.slug.clone(),
        id:    e.id.clone(),
        title: e.name.clone(),
        date:  crate::web::models::format_date(&dt),
        price,
        cover: e.cover_url.clone().unwrap_or_default(),
        grad:  "linear-gradient(135deg, #1a3a8a 0%, #4f6bff 100%)".into(),
    }
}

#[derive(Clone, Copy)]
pub struct VenueEventsCtx {
    pub items:   RwSignal<Vec<VenueEvent>>,
    pub loading: RwSignal<bool>,
}

impl VenueEventsCtx {
    pub fn load(&self, _city: String) {
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
            if let Ok(res) = get_events(Some(1), None, None, None, Some(6)).await {
                let mapped = res.data.iter().map(event_to_venue).collect();
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
