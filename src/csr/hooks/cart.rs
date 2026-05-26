use leptos::prelude::*;
use gloo_storage::{LocalStorage, Storage};

use crate::csr::models::CartItem;
// Re-ekspor dari utils agar import lama (crate::hooks::format_idr) tetap valid
pub use crate::csr::utils::format_idr;

const CART_STORAGE_KEY: &str = "kinetic_cart";

#[derive(Clone, Copy)]
pub struct CartCtx {
    pub items: RwSignal<Vec<CartItem>>,
}

impl CartCtx {
    pub fn total_items(&self) -> i32 {
        self.items.with(|v| v.iter().map(|i| i.quantity).sum())
    }

    pub fn subtotal(&self) -> i64 {
        self.items
            .with(|v| v.iter().map(|i| i.unit_price * i.quantity as i64).sum())
    }

    pub fn get_qty(&self, tier_id: &str) -> i32 {
        self.items.with(|v| {
            v.iter()
                .find(|i| i.tier_id == tier_id)
                .map(|i| i.quantity)
                .unwrap_or(0)
        })
    }

    pub fn add_item(&self, item: CartItem) {
        self.items.update(|v| {
            if let Some(existing) = v.iter_mut().find(|i| i.tier_id == item.tier_id) {
                existing.quantity += item.quantity;
            } else {
                v.push(item);
            }
        });
        self.persist();
    }

    pub fn remove_item(&self, tier_id: &str) {
        self.items.update(|v| v.retain(|i| i.tier_id != tier_id));
        self.persist();
    }

    pub fn update_qty(&self, tier_id: &str, qty: i32) {
        if qty <= 0 {
            self.remove_item(tier_id);
            return;
        }
        self.items.update(|v| {
            if let Some(it) = v.iter_mut().find(|i| i.tier_id == tier_id) {
                it.quantity = qty;
            }
        });
        self.persist();
    }

    pub fn clear(&self) {
        self.items.set(Vec::new());
        LocalStorage::delete(CART_STORAGE_KEY);
    }

    fn persist(&self) {
        self.items.with(|v| {
            if v.is_empty() {
                LocalStorage::delete(CART_STORAGE_KEY);
            } else {
                let _ = LocalStorage::set(CART_STORAGE_KEY, v);
            }
        });
    }
}

pub fn provide_cart() {
    let initial: Vec<CartItem> = LocalStorage::get(CART_STORAGE_KEY).unwrap_or_default();
    provide_context(CartCtx {
        items: RwSignal::new(initial),
    });
}

pub fn use_cart() -> CartCtx {
    use_context::<CartCtx>().expect("CartCtx not provided — call provide_cart()")
}
