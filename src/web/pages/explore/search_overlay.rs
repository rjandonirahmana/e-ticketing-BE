use leptos::prelude::*;

use crate::web::components::{EmptyState, ProductCardShimmer};

#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;

#[component]
pub fn SearchOverlay(
    query: RwSignal<String>,
    active_cat: RwSignal<String>,
    ph_text: RwSignal<String>,
    on_close: impl Fn() + Clone + Send + Sync + 'static,
    store: crate::web::state::products::ProductsCtx,
) -> impl IntoView {
    let input_ref: NodeRef<leptos::html::Input> = NodeRef::new();

    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        if input_ref.get().is_some() {
            let input_ref = input_ref.clone();
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(80).await;
                if let Some(el) = input_ref.get() {
                    let _ = el.focus();
                }
            });
        }
    });

    let filtered = Memo::new(move |_| {
        let q = query.get().to_lowercase();
        store.items.with(|products| {
            products
                .iter()
                .filter(|e| {
                    q.is_empty()
                        || e.title.to_lowercase().contains(&q)
                        || e.city.to_lowercase().contains(&q)
                        || e.venue.to_lowercase().contains(&q)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
    });

    // ── Pencarian merchant (server-side, di-debounce 250 ms) ────────────────────
    // Product dicari client-side dari store; merchant tidak dimuat, jadi query ke
    // server. Debounce agar tak menembak tiap ketik. Effect hanya jalan di client.
    let merch_query = RwSignal::new(String::new());
    let debounce_gen = StoredValue::new(0u32);
    Effect::new(move |_| {
        let q = query.get();
        let gen = debounce_gen.get_value().wrapping_add(1);
        debounce_gen.set_value(gen);
        set_timeout(
            move || {
                if debounce_gen.get_value() != gen {
                    return;
                }
                merch_query.set(q.clone());

                // ── Rekam PENCARIAN sebagai sinyal minat ("Untuk Kamu") ────
                // Kategori diambil dari product yang cocok dengan kata kunci
                // (client-side, dari store yang sama dengan hasil tampil):
                //  - anonim  → localStorage (behavior::record_view)
                //  - login   → server (record_affinity, buffer batch)
                // Dijalankan di debounce settle (bukan tiap ketik).
                let ql = q.trim().to_lowercase();
                if ql.len() >= 2 {
                    let mut cats: Vec<String> = store.items.with_untracked(|products| {
                        products
                            .iter()
                            .filter(|e| {
                                e.title.to_lowercase().contains(&ql)
                                    || e.city.to_lowercase().contains(&ql)
                                    || e.venue.to_lowercase().contains(&ql)
                            })
                            .flat_map(|e| e.category.iter().cloned())
                            .collect()
                    });
                    cats.sort();
                    cats.dedup();
                    cats.truncate(3);
                    if !cats.is_empty() {
                        crate::web::behavior::record_view(&cats);
                        leptos::task::spawn_local(async move {
                            // Sinyal "search", BUKAN bawaan (yang jatuh ke
                            // "view", bobot 1). Blok ini memang merekam
                            // PENCARIAN — komentarnya sudah menyebut begitu —
                            // tetapi bobotnya selama ini disamakan dengan
                            // sekadar tergulir melewati satu produk.
                            //
                            // Mengetik kata pencarian adalah niat yang
                            // DINYATAKAN pengguna; membuka satu produk bisa
                            // terjadi karena salah ketuk. Bobot 2 menempatkannya
                            // di antara keduanya.
                            let _ = crate::web::api::record_affinity(
                                cats,
                                Some("search".to_string()),
                            )
                            .await;
                        });
                    }
                }
            },
            std::time::Duration::from_millis(250),
        );
    });
    // q kosong → merchant ACAK (discovery sebelum mengetik); ada q → cari nama.
    let merchants = Resource::new(
        move || merch_query.get(),
        |q| async move {
            if q.trim().len() < 2 {
                return crate::web::api::get_random_merchants(8)
                    .await
                    .unwrap_or_default();
            }
            crate::web::api::search_merchants(q).await.unwrap_or_default()
        },
    );

    let close_c = on_close.clone();

    view! {
        <div class="exp-sovl">
            <div class="exp-sovl-header">
                <button
                    class="exp-sovl-back"
                    on:click=move |_| {
                        query.set(String::new());
                        on_close();
                    }
                    aria-label="Tutup Pencarian"
                >
                    <svg
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                    >
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </button>
                <div class="exp-sovl-input-wrap">
                    <svg
                        width="15"
                        height="15"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.2"
                        stroke-linecap="round"
                    >
                        <circle cx="11" cy="11" r="8" />
                        <line x1="21" y1="21" x2="16.65" y2="16.65" />
                    </svg>
                    <input
                        node_ref=input_ref
                        type="search"
                        class="exp-sovl-input"
                        placeholder=move || ph_text.get()
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                        autocomplete="off"
                        autocapitalize="none"
                        spellcheck="false"
                    />
                    {move || {
                        (!query.get().is_empty())
                            .then(|| {
                                view! {
                                    <button
                                        class="exp-sovl-clear"
                                        on:click=move |_| query.set(String::new())
                                    >
                                        <svg
                                            width="13"
                                            height="13"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2.5"
                                        >
                                            <line x1="18" y1="6" x2="6" y2="18" />
                                            <line x1="6" y1="6" x2="18" y2="18" />
                                        </svg>
                                    </button>
                                }
                            })
                    }}
                </div>
            </div>

            <div class="exp-sovl-chips">
                {move || {
                    store
                        .categories
                        .with(|cats| {
                            cats.iter()
                                .map(|label| {
                                    let lc = label.clone();
                                    let lk = label.clone();
                                    view! {
                                        <button
                                            class=move || {
                                                if active_cat.get() == lc {
                                                    "exp-chip exp-chip--on"
                                                } else {
                                                    "exp-chip"
                                                }
                                            }
                                            on:click=move |_| active_cat.set(lk.clone())
                                        >
                                            {label.to_uppercase()}
                                        </button>
                                    }
                                })
                                .collect_view()
                        })
                }}
            </div>

            // ── Penyelenggara (merchant) yang cocok ──────────────────────────
            {move || {
                let list = merchants.get().unwrap_or_default();
                (!list.is_empty())
                    .then(|| {
                        view! {
                            <div class="exp-sovl-merchants">
                                <span class="exp-results-eyebrow">
                                    {move || {
                                        if query.get().trim().len() < 2 {
                                            "Toko untukmu"
                                        } else {
                                            "Toko"
                                        }
                                    }}
                                </span>
                                <div class="exp-merch-row">
                                    {list
                                        .into_iter()
                                        .map(|m| {
                                            let initial: String = m
                                                .store_name
                                                .chars()
                                                .next()
                                                .unwrap_or('P')
                                                .to_uppercase()
                                                .to_string();
                                            let logo = m.logo_url.clone().unwrap_or_default();
                                            let href = format!("/m/{}", m.merchant_id);
                                            view! {
                                                <a class="exp-merch-item" href=href>
                                                    {if logo.is_empty() {
                                                        view! {
                                                            <span class="exp-merch-avatar exp-merch-avatar--fb">
                                                                {initial}
                                                            </span>
                                                        }
                                                            .into_any()
                                                    } else {
                                                        view! {
                                                            <img
                                                                class="exp-merch-avatar"
                                                                src=logo
                                                                alt=""
                                                                loading="lazy"
                                                            />
                                                        }
                                                            .into_any()
                                                    }}
                                                    <span class="exp-merch-name">
                                                        {m.store_name.clone()}
                                                        {m
                                                            .verified
                                                            .then(|| {
                                                                view! {
                                                                    <span
                                                                        class="exp-merch-verified"
                                                                        title="Terverifikasi"
                                                                    >
                                                                        "✓"
                                                                    </span>
                                                                }
                                                            })}
                                                    </span>
                                                </a>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            </div>
                        }
                    })
            }}

            <div class="exp-sovl-count-bar">
                <span class="exp-results-eyebrow">"Hasil Pencarian"</span>
                <span class="exp-sovl-count">
                    {move || filtered.with(|f| f.len())} " product ditemukan"
                </span>
                {move || {
                    (!query.get().is_empty() || active_cat.get() != "All")
                        .then({
                            let cc = close_c.clone();
                            move || {
                                view! {
                                    <button
                                        class="exp-clear-btn"
                                        style="margin-left:auto"
                                        on:click={
                                            let cc2 = cc.clone();
                                            move |_| {
                                                query.set(String::new());
                                                active_cat.set("All".into());
                                                cc2();
                                            }
                                        }
                                    >
                                        <svg
                                            width="11"
                                            height="11"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2.5"
                                        >
                                            <line x1="18" y1="6" x2="6" y2="18" />
                                            <line x1="6" y1="6" x2="18" y2="18" />
                                        </svg>
                                        "Atur Ulang"
                                    </button>
                                }
                            }
                        })
                }}
            </div>

            <div class="exp-sovl-results">
                {move || {
                    if store.loading.get() {
                        return (0..4)
                            .map(|i| {
                                view! {
                                    <div style=format!("animation-delay:{}ms", i * 55)>
                                        <ProductCardShimmer />
                                    </div>
                                }
                            })
                            .collect_view()
                            .into_any();
                    }
                    let list = filtered.get();
                    if list.is_empty() {
                        view! {
                            <div class="exp-empty">
                                <EmptyState
                                    icon="🔍"
                                    title="Belum Ada Product"
                                    body="Coba kata kunci lain atau ubah filter."
                                />
                            </div>
                        }
                            .into_any()
                    } else {
                        list.into_iter()
                            .enumerate()
                            .map(|(i, ev)| {
                                let href = format!("/products/{}", ev.slug);
                                let venue_str = if ev.city.is_empty() {
                                    ev.venue.to_uppercase()
                                } else {
                                    format!(
                                        "{} • {}",
                                        ev.venue.to_uppercase(),
                                        ev.city.to_uppercase(),
                                    )
                                };
                                let is_hot = i % 2 == 0;
                                let badge_class = if is_hot {
                                    "exp-card-v2__badge exp-card-v2__badge--hot"
                                } else {
                                    "exp-card-v2__badge exp-card-v2__badge--limited"
                                };
                                let badge_text = if is_hot {
                                    "⚡ SELLING FAST"
                                } else {
                                    "LIMITED SEATS"
                                };
                                let cat = ev
                                    .category
                                    .first()
                                    .cloned()
                                    .unwrap_or_default()
                                    .to_uppercase();
                                view! {
                                    <div class="exp-cascade" style=format!("--i:{}", i)>
                                        <a href=href class="exp-card-v2">
                                            <div class="exp-card-v2__eyebrow">{cat}</div>
                                            <div class="exp-card-v2__img-wrap">
                                                <img
                                                    src=ev.cover.clone()
                                                    alt=ev.title.clone()
                                                    class="exp-card-v2__img"
                                                    loading="lazy"
                                                />
                                                <span class=badge_class>{badge_text}</span>
                                            </div>
                                            <div class="exp-card-v2__body">
                                                <h2 class="exp-card-v2__title">{ev.title.clone()}</h2>
                                                <div class="exp-card-v2__meta">
                                                    <div class="exp-card-v2__venue-date">
                                                        <span class="exp-card-v2__venue">{venue_str}</span>
                                                        <span class="exp-card-v2__date">{ev.date.clone()}</span>
                                                    </div>
                                                    <div class="exp-card-v2__price-wrap">
                                                        <span class="exp-card-v2__price-label">"STARTS FROM"</span>
                                                        <span class="exp-card-v2__price">
                                                            {ev.price_str.clone()}
                                                        </span>
                                                    </div>
                                                </div>
                                            </div>
                                        </a>
                                    </div>
                                }
                            })
                            .collect_view()
                            .into_any()
                    }
                }}
            </div>
        </div>
    }
}
