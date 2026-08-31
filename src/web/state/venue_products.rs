use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::web::api::get_products;
use crate::web::models::Product;
use crate::web::utils::format_number;

#[derive(Clone, Debug, PartialEq)]
pub struct VenueProduct {
    pub id:    String,
    pub slug:  String,
    pub title: String,
    pub date:  String,
    pub price: String,
    pub cover: String,
    pub grad:  String,
}

fn product_to_venue(e: &Product) -> VenueProduct {
    let price = if e.display_price <= 0.0 {
        "GRATIS".into()
    } else {
        format!("Rp{}", format_number(e.display_price as i64))
    };
    let dt = e.start_time.unwrap_or(e.event_date);
    VenueProduct {
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
pub struct VenueProductsCtx {
    pub items:   RwSignal<Vec<VenueProduct>>,
    pub loading: RwSignal<bool>,
}

impl VenueProductsCtx {
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
            if let Ok(res) = get_products(Some(1), None, None, None, Some(6), None).await {
                let mapped = res.data.iter().map(product_to_venue).collect();
                items.set(mapped);
            }
            loading.set(false);
        });
    }
}

pub fn provide_venue_products_store() {
    provide_context(VenueProductsCtx {
        items:   RwSignal::new(vec![]),
        loading: RwSignal::new(false),
    });
}

pub fn use_venue_products_store() -> VenueProductsCtx {
    use_context::<VenueProductsCtx>().expect("VenueProductsCtx not provided")
}
