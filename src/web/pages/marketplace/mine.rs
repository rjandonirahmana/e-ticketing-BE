//! web/pages/marketplace/mine.rs — `/marketplace/mine`: "Postingan Saya".
//! Semua postingan MILIK user yang login, SEMUA status (aktif/terjual/
//! diarsipkan) — beda dari `MarketplaceFeed` yang publik dan cuma aktif.
//! Tanpa filter/pencarian (isinya sudah pasti milik sendiri, daftarnya kecil).

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::A;

use crate::web::components::{BottomNav, EmptyState, IconBack, PostCard, PostCardShimmer};

#[cfg(feature = "hydrate")]
use send_wrapper::SendWrapper;
#[cfg(feature = "hydrate")]
use wasm_bindgen::JsCast;

#[component]
pub fn MyPostsPage() -> impl IntoView {
    let items = RwSignal::new(Vec::<crate::web::models::Post>::new());
    let page = RwSignal::new(1i64);
    let total = RwSignal::new(0i64);
    let loading = RwSignal::new(true);
    let loading_more = RwSignal::new(false);
    let has_more = RwSignal::new(false);
    let error = RwSignal::new(String::new());

    let load_more = move || {
        if loading_more.get_untracked() || loading.get_untracked() || !has_more.get_untracked() {
            return;
        }
        loading_more.set(true);
        let next = page.get_untracked() + 1;
        leptos::task::spawn_local(async move {
            if let Ok(pg) = crate::web::api::list_my_posts(Some(next)).await {
                page.set(next);
                has_more.set(pg.page < pg.total_pages);
                items.update(|v| v.extend(pg.data));
            }
            loading_more.set(false);
        });
    };
    #[allow(unused_variables)]
    let load_more = load_more;

    Effect::new(move |_| {
        loading.set(true);
        error.set(String::new());
        leptos::task::spawn_local(async move {
            match crate::web::api::list_my_posts(Some(1)).await {
                Ok(pg) => {
                    has_more.set(pg.page < pg.total_pages);
                    total.set(pg.total);
                    items.set(pg.data);
                    page.set(1);
                }
                Err(e) => error.set(e.to_string()),
            }
            loading.set(false);
        });
    });

    // Infinite scroll — pola sama `marketplace/mod.rs`.
    #[cfg(feature = "hydrate")]
    {
        let scroll_cb: StoredValue<Option<SendWrapper<wasm_bindgen::closure::Closure<dyn Fn()>>>> =
            StoredValue::new(None);
        Effect::new(move |_| {
            let cb = wasm_bindgen::closure::Closure::<dyn Fn()>::new(move || {
                let Some(win) = web_sys::window() else { return };
                let inner_h = win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
                let scroll_y = win.scroll_y().unwrap_or(0.0);
                let doc_h = win
                    .document()
                    .and_then(|d| d.document_element())
                    .map(|e| e.scroll_height() as f64)
                    .unwrap_or(0.0);
                let threshold = (inner_h * 2.5).max(1200.0);
                if doc_h - (scroll_y + inner_h) < threshold {
                    load_more();
                }
            });
            if let Some(win) = web_sys::window() {
                let _ = win.add_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref());
            }
            scroll_cb.set_value(Some(SendWrapper::new(cb)));
        });
        on_cleanup(move || {
            if let Some(Some(cb)) = scroll_cb.try_update_value(|o| o.take()) {
                if let Some(win) = web_sys::window() {
                    let _ = win.remove_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref());
                }
                drop(cb);
            }
        });
    }

    view! {
        <Title text="Postingan Saya — Hub PULSE" />
        <div class="page pk-page">
            <header class="page-header pk-header">
                <A href="/marketplace" attr:class="pk-back-btn" attr:aria-label="Kembali">
                    <IconBack />
                </A>
                <span class="page-logo">"POSTINGAN SAYA"</span>
                <A href="/marketplace/new" attr:class="pk-post-btn">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
                        <line x1="12" y1="5" x2="12" y2="19" />
                        <line x1="5" y1="12" x2="19" y2="12" />
                    </svg>
                    "Posting"
                </A>
            </header>

            <div class="pk-results-bar">
                <span class="pk-results-count"><b>{move || total.get().max(0)}</b>" postingan"</span>
            </div>

            <div class="pk-feed">
                {move || {
                    if loading.get() {
                        let shims = (0..4).map(|_| view! { <PostCardShimmer /> }).collect_view();
                        view! { <div class="pk-grid">{shims}</div> }.into_any()
                    } else if !error.with(|e| e.is_empty()) {
                        view! {
                            <div class="pk-empty">
                                <EmptyState icon="⚠️" title="Gagal Memuat" body=error.get() />
                            </div>
                        }.into_any()
                    } else {
                        let list = items.get();
                        if list.is_empty() {
                            view! {
                                <div class="pk-empty">
                                    <EmptyState
                                        icon="🛍️"
                                        title="Belum Ada Postingan"
                                        body="Barang yang kamu posting di Hub akan muncul di sini."
                                        cta_label="Posting Sekarang"
                                        cta_href="/marketplace/new"
                                    />
                                </div>
                            }.into_any()
                        } else {
                            let cards = list
                                .into_iter()
                                .enumerate()
                                .map(|(i, p)| view! { <PostCard post=p index=i /> })
                                .collect_view();
                            let shims = loading_more
                                .get()
                                .then(|| (0..4).map(|_| view! { <PostCardShimmer /> }).collect_view());
                            view! { <div class="pk-grid">{cards}{shims}</div> }.into_any()
                        }
                    }
                }}
            </div>

            <BottomNav active="marketplace" />
        </div>
    }
}
