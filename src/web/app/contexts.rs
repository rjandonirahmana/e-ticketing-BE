//! web/app/contexts.rs — Tipe context global aplikasi (di-provide sekali di App).
//!
//! Dipisah dari router & providers agar tipe-tipe ini bisa di-import murni
//! tanpa menarik dependency view/router. Halaman lain meng-import lewat
//! re-export `crate::web::app::{AuthResource, CartContext, ...}`.

use leptos::prelude::*;

use crate::web::models::{CartItem, OrderRef, PendingSubOrder, UserResponse};

/// Resource auth global — di-provide di `provide_all_app_contexts()`.
pub type AuthResource = Resource<Result<Option<UserResponse>, ServerFnError>>;

#[derive(Clone, Debug, Default)]
pub struct SuccessSnapshot {
    pub order_code: String,
    pub event_name: String,
    pub total_amount: i64,
}

#[derive(Clone, Copy)]
pub struct CartContext {
    pub items: RwSignal<Vec<CartItem>>,
}

impl CartContext {
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

    pub fn update_qty(&self, tier_id: &str, qty: i32) {
        if qty <= 0 {
            let t = tier_id.to_string();
            self.items.update(|v| v.retain(|i| i.tier_id != t));
        } else {
            let t = tier_id.to_string();
            self.items.update(|v| {
                if let Some(it) = v.iter_mut().find(|i| i.tier_id == t) {
                    it.quantity = qty;
                }
            });
        }
        self.persist();
    }

    fn persist(&self) {
        #[cfg(target_arch = "wasm32")]
        self.items.with(|v| {
            if let Some(win) = web_sys::window() {
                if let Ok(Some(storage)) = win.local_storage() {
                    if v.is_empty() {
                        let _ = storage.remove_item("pulse_cart");
                    } else if let Ok(json) = serde_json::to_string(v) {
                        let _ = storage.set_item("pulse_cart", &json);
                    }
                }
            }
        });
    }
}

/// SSR-specific PendingOrderCtx (lebih lengkap dari CSR versi order_created.rs).
/// CSR order_created.rs punya PendingOrderCtx sendiri — keduanya di-provide karena
/// komponen berbeda menggunakan tipe berbeda.
#[derive(Clone, Copy)]
pub struct PendingOrderCtx {
    pub pending_order: RwSignal<Option<OrderRef>>,
    pub success_order: RwSignal<Option<SuccessSnapshot>>,
}

/// Context untuk subscription checkout — diisi subscription page, dibaca checkout page.
#[derive(Clone, Copy)]
pub struct PendingSubCtx {
    pub order: RwSignal<Option<PendingSubOrder>>,
}
