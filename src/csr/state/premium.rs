// src/state/premium.rs
//
// Global state untuk Premium Subscription.
// Di-provide di root App, dapat diakses oleh semua komponen via use_premium_store().

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::csr::services::premium as premium_svc;

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct PremiumCtx {
    /// true jika user saat ini adalah active premium subscriber.
    pub is_premium: RwSignal<bool>,
    /// Loading state saat fetch status.
    pub loading: RwSignal<bool>,
    /// true jika status sudah di-load sekali (hindari re-fetch).
    pub loaded: RwSignal<bool>,
}

impl PremiumCtx {
    /// Load status premium dari backend.
    /// Dipanggil sekali di app start atau setelah login.
    pub fn load(&self) {
        let ctx = *self;
        if ctx.loaded.get_untracked() {
            return;
        }
        spawn_local(async move {
            ctx.loading.set(true);
            if let Ok(status) = premium_svc::fetch_premium_status().await {
                ctx.is_premium.set(status.is_premium);
                ctx.loaded.set(true);
            }
            ctx.loading.set(false);
        });
    }

    /// Refresh ulang status (misal setelah berhasil aktivasi premium).
    pub fn refresh(&self) {
        let ctx = *self;
        spawn_local(async move {
            ctx.loading.set(true);
            if let Ok(status) = premium_svc::fetch_premium_status().await {
                ctx.is_premium.set(status.is_premium);
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
