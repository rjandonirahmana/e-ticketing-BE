//! web/app/guards.rs — Route guards berbasis role (client-side UX).
//!
//! CATATAN KEAMANAN: guard ini hanya menyembunyikan UI & redirect — bukan
//! batas keamanan. Otorisasi sebenarnya ditegakkan di server function
//! (`require_role` / `require_roles` di web/api/server_fns).

use leptos::prelude::*;

use super::contexts::AuthResource;

/// Skeleton fallback saat AuthResource masih loading.
fn guard_skeleton() -> impl IntoView {
    view! {
        <div class="page">
            <div style="display:flex;align-items:center;justify-content:space-between;
             padding:14px 16px;border-bottom:1px solid var(--border-soft);
             background:var(--bg-page);position:sticky;top:0;z-index:40">
                <div class="shim" style="width:36px;height:36px;border-radius:50%"></div>
                <div class="shim" style="width:72px;height:18px;border-radius:4px"></div>
                <div class="shim" style="width:36px;height:36px;border-radius:50%"></div>
            </div>
            <div style="padding:20px 16px;display:flex;flex-direction:column;gap:16px;flex:1">
                {(0..6u32)
                    .map(|_| {
                        view! {
                            <div style="display:flex;align-items:center;gap:12px">
                                <div
                                    class="shim"
                                    style="width:56px;height:56px;border-radius:12px;flex-shrink:0"
                                ></div>
                                <div style="flex:1;display:flex;flex-direction:column;gap:8px">
                                    <div
                                        class="shim"
                                        style="height:15px;border-radius:6px;width:75%"
                                    ></div>
                                    <div
                                        class="shim"
                                        style="height:12px;border-radius:6px;width:50%"
                                    ></div>
                                </div>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}

/// Guard: user harus login.
#[component]
pub(crate) fn AuthGuard(children: ChildrenFn) -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource not provided");
    let children = StoredValue::new(children);
    view! {
        <Suspense fallback=guard_skeleton>
            {move || {
                auth
                    .get()
                    .map(|result| match result {
                        Ok(Some(_)) => children.with_value(|c| c()).into_any(),
                        _ => {
                            view! { <leptos_router::components::Redirect path="/login" /> }
                                .into_any()
                        }
                    })
            }}
        </Suspense>
    }
}

/// Guard: user harus punya role "admin".
#[component]
pub(crate) fn AdminGuard(children: ChildrenFn) -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource not provided");
    let children = StoredValue::new(children);
    view! {
        <Suspense fallback=guard_skeleton>
            {move || {
                auth
                    .get()
                    .map(|result| match result {
                        Ok(Some(user)) if user.role == "admin" => {
                            children.with_value(|c| c()).into_any()
                        }
                        Ok(Some(_)) => {
                            view! { <leptos_router::components::Redirect path="/explore" /> }
                                .into_any()
                        }
                        _ => {
                            view! { <leptos_router::components::Redirect path="/login" /> }
                                .into_any()
                        }
                    })
            }}
        </Suspense>
    }
}

/// Guard: user harus punya role "merchant" atau "admin".
#[component]
pub(crate) fn MerchantGuard(children: ChildrenFn) -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource not provided");
    let children = StoredValue::new(children);
    view! {
        <Suspense fallback=guard_skeleton>
            {move || {
                auth
                    .get()
                    .map(|result| match result {
                        Ok(Some(user)) if user.role == "merchant" || user.role == "admin" => {
                            children.with_value(|c| c()).into_any()
                        }
                        Ok(Some(_)) => {
                            view! { <leptos_router::components::Redirect path="/explore" /> }
                                .into_any()
                        }
                        _ => {
                            view! { <leptos_router::components::Redirect path="/login" /> }
                                .into_any()
                        }
                    })
            }}
        </Suspense>
    }
}
