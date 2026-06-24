//! state/premium.rs — Global state Premium Subscription.
//!
//! Status di-fetch via server function `get_premium_status` (mengembalikan bool langsung).

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::web::api::get_premium_status;

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct PremiumCtx {
    pub is_premium: RwSignal<bool>,
    pub loading: RwSignal<bool>,
    pub loaded: RwSignal<bool>,
}

impl PremiumCtx {
    pub fn load(&self) {
        if is_server() {
            return;
        }
        let ctx = *self;
        if ctx.loaded.get_untracked() {
            return;
        }
        spawn_local(async move {
            ctx.loading.set(true);
            if let Ok(v) = get_premium_status().await {
                ctx.is_premium.set(v);
                ctx.loaded.set(true);
            }
            ctx.loading.set(false);
        });
    }

    pub fn refresh(&self) {
        if is_server() {
            return;
        }
        let ctx = *self;
        spawn_local(async move {
            ctx.loading.set(true);
            if let Ok(v) = get_premium_status().await {
                ctx.is_premium.set(v);
            }
            ctx.loading.set(false);
        });
    }
}

// ── Provider & hook ───────────────────────────────────────────────────────────

pub fn provide_premium_store() {
    let ctx = PremiumCtx {
        is_premium: RwSignal::new(false),
        loading: RwSignal::new(false),
        loaded: RwSignal::new(false),
    };
    provide_context(ctx);
}

pub fn use_premium_store() -> PremiumCtx {
    use_context::<PremiumCtx>()
        .expect("PremiumCtx not provided — pastikan provide_premium_store() dipanggil di App")
}
