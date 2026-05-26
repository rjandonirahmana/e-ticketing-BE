use leptos::prelude::*;

use crate::web::api::{get_categories, get_events};
use crate::web::components::EventCard;

#[component]
pub fn ExplorePage() -> impl IntoView {
    let search = RwSignal::new(String::new());
    let city = RwSignal::new(String::new());
    let category = RwSignal::new(String::new());
    let page = RwSignal::new(1i64);

    let categories = Resource::new(|| (), |_| get_categories());

    let events = Resource::new(
        move || (page.get(), city.get(), category.get(), search.get()),
        |(p, ci, ca, se)| {
            get_events(
                Some(p),
                Some(ci).filter(|s| !s.is_empty()),
                Some(ca).filter(|s| !s.is_empty()),
                Some(se).filter(|s| !s.is_empty()),
            )
        },
    );

    let total_pages = RwSignal::new(1i64);

    view! {
        <div class="page-header">
            <div class="container">
                <p class="page-header__eyebrow">"// temukan event seru"</p>
                <h1 class="page-header__title">"Jelajahi Event"</h1>
                <p class="page-header__sub">"Temukan ribuan event dari seluruh Indonesia"</p>
            </div>
        </div>

        <div class="container">
            // ── Filter Bar ──────────────────────────────────────────────────
            <div class="exp-searchbar-row">
                <input
                    type="search"
                    class="filter-bar__input"
                    placeholder="Cari event, artis, venue..."
                    prop:value=search
                    on:input=move |ev| {
                        search.set(event_target_value(&ev));
                        page.set(1);
                    }
                />

                <Suspense>
                    {move || {
                        categories
                            .get()
                            .map(|res| {
                                let cats = res.unwrap_or_default();
                                view! {
                                    <select
                                        class="filter-bar__select"
                                        on:change=move |ev| {
                                            category.set(event_target_value(&ev));
                                            page.set(1);
                                        }
                                    >
                                        <option value="">"Semua Kategori"</option>
                                        {cats
                                            .into_iter()
                                            .map(|c| {
                                                let c2 = c.clone();
                                                view! { <option value=c>{c2}</option> }
                                            })
                                            .collect_view()}
                                    </select>
                                }
                            })
                    }}
                </Suspense>

                <input
                    type="text"
                    class="filter-bar__input"
                    style="max-width:200px"
                    placeholder="Filter kota..."
                    prop:value=city
                    on:input=move |ev| {
                        city.set(event_target_value(&ev));
                        page.set(1);
                    }
                />
            </div>

            // ── Results ─────────────────────────────────────────────────────
            <Suspense fallback=|| {
                view! {
                    <div class="loading">
                        <div class="loading__spinner" />
                        <span>"Memuat event..."</span>
                    </div>
                }
            }>
                {move || {
                    events
                        .get()
                        .map(|res| {
                            match res {
                                Ok(pg) => {
                                    total_pages.set(pg.total_pages);
                                    let total = pg.total;
                                    let count = pg.data.len();
                                    if pg.data.is_empty() {
                                        view! {
                                            <div class="empty">
                                                <div class="empty__icon">"🔍"</div>
                                                <div class="empty__title">"Tidak ditemukan"</div>
                                                <div class="empty__sub">
                                                    "Coba kata kunci atau filter berbeda."
                                                </div>
                                            </div>
                                        }
                                            .into_any()
                                    } else {
                                        view! {
                                            <p style="color:var(--clr-muted);font-size:0.875rem;margin-bottom:1.25rem">
                                                {format!("Menampilkan {count} dari {total} event")}
                                            </p>
                                            <div class="events-grid">
                                                {pg
                                                    .data
                                                    .into_iter()
                                                    .map(|e| {
                                                        view! { <EventCard event=e /> }
                                                    })
                                                    .collect_view()}
                                            </div>
                                        }
                                            .into_any()
                                    }
                                }
                                Err(_) => {
                                    view! {
                                        <div class="alert alert--error">
                                            "Gagal memuat event. Coba lagi."
                                        </div>
                                    }
                                        .into_any()
                                }
                            }
                        })
                }}
            </Suspense>

            // ── Pagination ──────────────────────────────────────────────────
            <div style="display:flex;justify-content:center;gap:0.75rem;padding:2.5rem 0">
                <button
                    class="btn btn--ghost btn--sm"
                    on:click=move |_| {
                        if page.get() > 1 {
                            page.update(|p| *p -= 1);
                        }
                    }
                    disabled=move || page.get() <= 1
                >
                    "← Prev"
                </button>
                <span style="display:flex;align-items:center;color:var(--clr-muted);font-size:0.875rem">
                    {move || format!("Hal {} / {}", page.get(), total_pages.get())}
                </span>
                <button
                    class="btn btn--ghost btn--sm"
                    on:click=move |_| {
                        if page.get() < total_pages.get() {
                            page.update(|p| *p += 1);
                        }
                    }
                    disabled=move || page.get()
                >
                    = total_pages.get()
                    >
                    "Next →"
                </button>
            </div>
        </div>
    }
}
