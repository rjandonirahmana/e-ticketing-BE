//! web/pages/explore — Explore page (unified SSR + hydration).

mod search_overlay;

use search_overlay::SearchOverlay;

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::A;
use leptos_router::hooks::use_query_map;

use crate::web::components::story_bars::StoryBar;
use crate::web::components::story_viewer::StoryViewer;
use crate::web::components::{
    BannerSlider, BottomNav, EmptyState, ProductCardPub, ProductCardShimmer,
};
use crate::web::components::{CartButton, ThemeToggle};
use crate::web::state::use_products_store;

#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;
#[cfg(feature = "hydrate")]
use wasm_bindgen::JsCast;
#[cfg(feature = "hydrate")]
use send_wrapper::SendWrapper;
#[cfg(feature = "hydrate")]
use wasm_bindgen::prelude::*;


// ── Main Explore Page ─────────────────────────────────────────────────────────
#[component]
pub fn ExplorePage() -> impl IntoView {
    let params = use_query_map();
    let initial_q = params.with_untracked(|p| p.get("q").unwrap_or_default());
    let initial_cat = params.with_untracked(|p| p.get("cat").unwrap_or("All".into()));

    let query = RwSignal::new(initial_q);
    let active_cat = RwSignal::new(initial_cat);
    let show_overlay = RwSignal::new(false);
    let overlay_visible = RwSignal::new(false);

    let store = use_products_store();

    // ── SSR page-1: render kartu product LANGSUNG di HTML awal ────────────────────
    // AKAR "lambat saat pertama diakses": landing (/) dulu hanya mengirim shimmer;
    // product baru terisi SETELAH bundle WASM (besar) diunduh → hydrate → memicu
    // fetch. Resource blocking ini dieksekusi di SERVER (ikut di HTML awal) dan
    // nilainya di-serialize ke klien, sehingga:
    //   • kunjungan pertama langsung melihat kartu (bukan menunggu WASM),
    //   • hydration cocok byte-for-byte (tak ada refetch, tak ada flash).
    // Di-key sekali oleh kategori AWAL (dari URL); pergantian kategori setelah itu
    // ditangani store di klien, jadi resource ini tak refetch berulang.
    let initial_cat_val = active_cat.get_untracked();
    // Blocking di sini MURNI urusan SSR: flag `blocking` hanya dibaca leptos di
    // balik `#[cfg(feature = "ssr")]` (resource.rs:334), jadi di WASM ia identik
    // dengan `Resource::new`. Pemecahan `#[cfg]` yang dulu ada di sini karena itu
    // tak pernah mengubah perilaku klien sedikit pun — lihat catatan lengkap di
    // `pages/product_detail.rs`, dan akar masalah navigasinya di
    // `web/app/guards.rs`.
    //
    // Yang tetap dibutuhkan: page-1 harus sudah ada di HTML pertama, supaya
    // kunjungan dingin melihat kartu product tanpa menunggu bundel WASM turun.
    let ssr_first = Resource::new_blocking(
        || (),
        move |_| {
            let cat = initial_cat_val.clone();
            async move {
                let cat_opt = if cat == "All" || cat.is_empty() {
                    None
                } else {
                    Some(cat)
                };
                crate::web::api::get_products(
                    Some(1),
                    None,
                    cat_opt,
                    None,
                    Some(crate::web::state::products::PAGE_SIZE),
                )
                .await
            }
        },
    );

    Effect::new(move |prev: Option<String>| {
        let cat = active_cat.get();
        match prev {
            // Run pertama (pasca-hydration): page-1 sudah ada di HTML dari resource
            // SSR → seed store dari situ, JANGAN refetch (hindari round-trip &
            // flash shimmer). Fallback fetch hanya bila resource entah kenapa kosong.
            None => match ssr_first.get() {
                Some(Ok(res)) => store.seed_first(&res, cat.clone()),
                _ => store.load_cat(cat.clone()),
            },
            // Kategori benar-benar berganti → fetch page-1 kategori baru.
            Some(prev_cat) if prev_cat != cat => store.load_cat(cat.clone()),
            _ => {}
        }
        cat
    });


    // Rekomendasi "Untuk Kamu" (tanpa perlu "like"):
    //   1) Coba rekomendasi SERVER (user login) dari DB perilaku (user_affinity).
    //   2) Fallback: kategori favorit dari localStorage (perilaku di device ini).
    let rec_products = RwSignal::new(Vec::<crate::web::state::products::ExploreProduct>::new());
    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        spawn_local(async move {
            // (1) Server-side (persisten, lintas-sesi untuk user login).
            if let Ok(res) = crate::web::api::get_recommended_products().await {
                if !res.data.is_empty() {
                    rec_products.set(
                        res.data
                            .iter()
                            .map(crate::web::state::products::product_to_explore_pub)
                            .collect(),
                    );
                    return;
                }
            }
            // (2) Fallback client-side (localStorage) untuk anonim / belum ada data.
            let cats = crate::web::behavior::top_categories(1);
            if let Some(cat) = cats.into_iter().next() {
                if let Ok(res) =
                    crate::web::api::get_products(Some(1), None, Some(cat), None, Some(10)).await
                {
                    rec_products.set(
                        res.data
                            .iter()
                            .map(crate::web::state::products::product_to_explore_pub)
                            .collect(),
                    );
                }
            }
        });
    });

    // Infinite scroll: pasang listener "scroll" di window. Prefetch dimulai
    // ~2.5 layar sebelum ujung dokumen — cukup jauh agar fetch selesai SEBELUM
    // user tiba di bawah (shimmer nyaris tak pernah terlihat pada scroll
    // normal; dulu 700px fixed → flick cepat selalu menabrak shimmer selama
    // network round-trip). load_more() sudah punya guard
    // (loading/loading_more/has_more) → aman dipanggil berkali-kali tiap product
    // scroll tanpa fetch ganda.
    #[cfg(feature = "hydrate")]
    {
        let scroll_cb: StoredValue<Option<SendWrapper<Closure<dyn Fn()>>>> =
            StoredValue::new(None);
        Effect::new(move |_| {
            let cb = Closure::<dyn Fn()>::new(move || {
                let Some(win) = web_sys::window() else { return };
                let inner_h = win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
                let scroll_y = win.scroll_y().unwrap_or(0.0);
                let doc_h = win
                    .document()
                    .and_then(|d| d.document_element())
                    .map(|e| e.scroll_height() as f64)
                    .unwrap_or(0.0);
                // Ambang relatif viewport (min. 1200px utk layar pendek).
                let threshold = (inner_h * 2.5).max(1200.0);
                if doc_h - (scroll_y + inner_h) < threshold {
                    store.load_more();
                }
            });
            if let Some(win) = web_sys::window() {
                let _ = win.add_event_listener_with_callback(
                    "scroll",
                    cb.as_ref().unchecked_ref(),
                );
            }
            scroll_cb.set_value(Some(SendWrapper::new(cb)));
        });
        on_cleanup(move || {
            if let Some(Some(cb)) = scroll_cb.try_update_value(|o| o.take()) {
                if let Some(win) = web_sys::window() {
                    let _ = win.remove_event_listener_with_callback(
                        "scroll",
                        cb.as_ref().unchecked_ref(),
                    );
                }
                drop(cb);
            }
        });
    }

    let close_gen = RwSignal::new(0u32);

    let open_overlay = move || {
        close_gen.update(|n| *n = n.wrapping_add(1));
        show_overlay.set(true);
        overlay_visible.set(true);
    };

    let close_overlay = move || {
        overlay_visible.set(false);
        let gen = close_gen.get_untracked().wrapping_add(1);
        close_gen.set(gen);
        #[cfg(feature = "hydrate")]
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(380).await;
            if close_gen.get_untracked() == gen {
                show_overlay.set(false);
            }
        });
        #[cfg(not(feature = "hydrate"))]
        {
            show_overlay.set(false);
        }
    };

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

    // Sumber data feed: resource SSR sampai store aktif di klien, lalu store.
    // Semua reaktif — beralih otomatis begitu `seed_first`/`load_cat` menaikkan
    // fetch_gen. Query pencarian kosong saat SSR/paint pertama, jadi `filtered`
    // == item store → peralihan mulus tanpa perubahan tampilan.
    let feed_loading = Signal::derive(move || {
        if store.is_active() {
            store.loading.get()
        } else {
            ssr_first.get().is_none()
        }
    });
    let feed_error = Signal::derive(move || {
        if store.is_active() {
            store.error.get()
        } else {
            match ssr_first.get() {
                Some(Err(e)) => e.to_string(),
                _ => String::new(),
            }
        }
    });
    let feed_list = Signal::derive(move || {
        if store.is_active() {
            filtered.get()
        } else {
            match ssr_first.get() {
                Some(Ok(res)) => res
                    .data
                    .iter()
                    .map(crate::web::state::products::product_to_explore_pub)
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            }
        }
    });
    let feed_total = Signal::derive(move || {
        if store.is_active() {
            store.total.get()
        } else {
            match ssr_first.get() {
                Some(Ok(res)) => res.total,
                _ => 0,
            }
        }
    });

    let close_c = StoredValue::new(close_overlay);

    let placeholders = vec!["search product, artists...", "cari sepatu lari", "kaos polos"];
    let _ph_idx = RwSignal::new(0usize);
    let ph_text = RwSignal::new(placeholders[0].to_string());
    let ph_show = RwSignal::new(true);

    // Placeholder rotator — client only
    #[cfg(feature = "hydrate")]
    {
        let ph_timer: StoredValue<Option<leptos::prelude::IntervalHandle>> = StoredValue::new(None);
        let phs = placeholders.clone();
        ph_timer.set_value(
            set_interval_with_handle(
                move || {
                    ph_show.set(false);
                    let phs2 = phs.clone();
                    spawn_local(async move {
                        gloo_timers::future::TimeoutFuture::new(300).await;
                        let next = (_ph_idx.get_untracked() + 1) % phs2.len();
                        _ph_idx.set(next);
                        ph_text.set(phs2[next].to_string());
                        ph_show.set(true);
                    });
                },
                std::time::Duration::from_millis(2200),
            )
            .ok(),
        );
        on_cleanup(move || {
            if let Some(Some(h)) = ph_timer.try_update_value(|o| o.take()) {
                h.clear();
            }
        });
    }
    #[cfg(not(feature = "hydrate"))]
    {
        ph_text.set(placeholders[0].to_string());
        ph_show.set(true);
    }

    // Lock body scroll saat overlay — client only
    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        if let Some(body) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.body())
        {
            if show_overlay.get() {
                let _ = body.class_list().add_1("body-scroll-locked");
            } else {
                let _ = body.class_list().remove_1("body-scroll-locked");
            }
        }
    });

    // ⌘K / Escape keybind — client only
    #[cfg(feature = "hydrate")]
    {
        let kb_handler: StoredValue<Option<SendWrapper<Closure<dyn Fn(web_sys::KeyboardEvent)>>>> =
            StoredValue::new(None);
        Effect::new(move |_| {
            let handler = Closure::new({
                let open_overlay = open_overlay.clone();
                let close_overlay = close_overlay.clone();
                let show_overlay = show_overlay.clone();
                move |ev: web_sys::KeyboardEvent| {
                    if (ev.meta_key() || ev.ctrl_key()) && ev.key().eq_ignore_ascii_case("k") {
                        ev.prevent_default();
                        open_overlay();
                    } else if ev.key() == "Escape" && show_overlay.get_untracked() {
                        ev.prevent_default();
                        close_overlay();
                    }
                }
            });
            if let Some(win) = web_sys::window() {
                let _ = win.add_event_listener_with_callback(
                    "keydown",
                    handler.as_ref().unchecked_ref::<js_sys::Function>(),
                );
            }
            kb_handler.set_value(Some(SendWrapper::new(handler)));
        });
        on_cleanup(move || {
            if let Some(Some(old)) = kb_handler.try_update_value(|o| o.take()) {
                if let Some(win) = web_sys::window() {
                    let _ = win.remove_event_listener_with_callback(
                        "keydown",
                        old.as_ref().unchecked_ref::<js_sys::Function>(),
                    );
                }
                drop(old);
            }
        });
    }

    view! {
        <Title text="Jelajahi Product — PULSE" />
        <Meta
            name="description"
            content="Temukan product pilihan dari toko-toko di kotamu. Belanja sekarang di PULSE."
        />
        <div class="page explore-page exp-page">
            <header class="page-header exp-header">
                <A
                    href="/pulse-landing"
                    attr:class="exp-partner-btn"
                    attr:aria-label="Jadi Partner"
                >
                    <svg
                        width="12"
                        height="12"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    >
                        <path d="M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2" />
                        <circle cx="9" cy="7" r="4" />
                        <line x1="19" y1="8" x2="19" y2="14" />
                        <line x1="22" y1="11" x2="16" y2="11" />
                    </svg>
                    "Jadi Partner"
                </A>
                <span class="page-logo">"PULSE"</span>
                <div class="header-actions">
                    <CartButton />
                    <ThemeToggle />
                    <A href="/notifications" attr:class="bell-btn">
                        <svg
                            width="18"
                            height="18"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                        >
                            <path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9" />
                            <path d="M13.73 21a2 2 0 01-3.46 0" />
                        </svg>
                        <span class="bell-dot"></span>
                    </A>
                </div>
            </header>

            <div class="exp-searchbar-row">
                <button
                    class="exp-searchbar"
                    on:click=move |_| open_overlay()
                    aria-label="Cari product"
                >
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
                    <span class=move || {
                        format!(
                            "exp-searchbar-ph {}",
                            if ph_show.get() { "ph-in" } else { "ph-out" },
                        )
                    }>{move || ph_text.get()}</span>
                </button>
                <button class="exp-filter-btn" aria-label="Filter">
                    <svg
                        width="17"
                        height="17"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    >
                        <line x1="4" y1="6" x2="20" y2="6" />
                        <line x1="8" y1="12" x2="16" y2="12" />
                        <line x1="11" y1="18" x2="13" y2="18" />
                    </svg>
                </button>
            </div>

            // Banner slider (dari tabel `banners`); fallback kartu statis
            // "SPONSORED" bila belum ada banner aktif yang diunggah admin.
            // Markup + putar-otomatis + panah ada di `components/banner_slider.rs`,
            // dipakai bersama halaman /pulse.
            <div class="exp-promo-wrap">
                <BannerSlider fallback=|| {
                    view! {
                        <div class="exp-promo">
                            <span class="exp-promo-tag">"SPONSORED"</span>
                            <h2 class="exp-promo-heading">"UPGRADE TO VIP" <br /> "PULSE PASS"</h2>
                            <p class="exp-promo-desc">
                                "Akses lebih awal, diskon khusus, dan prioritas antrian untuk product pilihan."
                            </p>
                            <button class="exp-promo-cta">"Claim Offer"</button>
                        </div>
                    }
                } />
            </div>

            // ── Untuk Kamu (rekomendasi implisit dari perilaku) ──────────────
            {move || {
                let list = rec_products.get();
                (!list.is_empty())
                    .then(|| {
                        let cards = list
                            .into_iter()
                            .take(10)
                            .map(|ev| {
                                let href = format!("/products/{}", ev.slug);
                                view! {
                                    <a href=href class="exp-fy-card">
                                        <div class="exp-fy-img-wrap">
                                            <img
                                                src=ev.cover.clone()
                                                alt=ev.title.clone()
                                                class="exp-fy-img"
                                                loading="lazy"
                                            />
                                            {ev
                                                .is_live
                                                .then(|| view! { <span class="exp-fy-live">"LIVE"</span> })}
                                        </div>
                                        <div class="exp-fy-body">
                                            <div class="exp-fy-title">{ev.title.clone()}</div>
                                            <div class="exp-fy-price">{ev.price_str.clone()}</div>
                                        </div>
                                    </a>
                                }
                            })
                            .collect_view();
                        view! {
                            <div class="exp-fy-section">
                                <div class="exp-fy-head">
                                    <span class="exp-section-eyebrow">"REKOMENDASI"</span>
                                    <h2 class="exp-fy-title-h">"Untuk Kamu"</h2>
                                </div>
                                <div class="exp-fy-rail">{cards}</div>
                            </div>
                        }
                    })
            }}

            <div class="exp-section-hdr-row">
                <div class="exp-section-hdr-left">
                    <span class="exp-section-eyebrow">"TRENDING NOW"</span>
                    <h2 class="exp-section-title">"Story Product"</h2>
                </div>
                // Arsip publik semua story yang pernah ada (bukan list product).
                <A href="/stories" attr:class="exp-view-all">
                    "View All →"
                </A>
            </div>

            <StoryBar />

            <div class="exp-chips">
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

            <div class="exp-results-bar">
                <div class="exp-results-left">
                    <span class="exp-results-eyebrow">"Product Tersedia"</span>
                    <span class="exp-results-count">
                        // TOTAL dari COUNT server (semua halaman), bukan jumlah
                        // item yang baru termuat. Saat user mengetik pencarian
                        // (filter lokal), tampilkan jumlah hasil filter itu.
                        // Suspense: feed_total membaca resource SSR (ssr_first) —
                        // pembacaan resource WAJIB di dalam Suspense agar tak ada
                        // hydration mismatch (sama seperti grid feed di bawah).
                        <Suspense fallback=|| ()>
                            {move || {
                                if query.get().trim().is_empty() {
                                    feed_total.get().max(0)
                                } else {
                                    filtered.with(|f| f.len()) as i64
                                }
                            }}
                        </Suspense>
                        " product tersedia"
                    </span>
                </div>
                <div class="exp-results-right">
                    {move || {
                        (active_cat.get() != "All")
                            .then(|| {
                                view! {
                                    <button
                                        class="exp-clear-btn"
                                        on:click=move |_| active_cat.set("All".into())
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
                            })
                    }}
                </div>
            </div>

            <div class="exp-feed">
                <Suspense fallback=move || {
                    let shims = (0..6)
                        .map(|i| {
                            view! {
                                <div
                                    class="exp-shimmer-wrap"
                                    style=format!("animation-delay:{}ms", i * 60)
                                >
                                    <ProductCardShimmer />
                                </div>
                            }
                        })
                        .collect_view();
                    view! { <div class="exp-mkt-grid">{shims}</div> }
                }>
                {move || {
                    if feed_loading.get() {
                        let shims = (0..6)
                            .map(|i| {
                                view! {
                                    <div
                                        class="exp-shimmer-wrap"
                                        style=format!("animation-delay:{}ms", i * 60)
                                    >
                                        <ProductCardShimmer />
                                    </div>
                                }
                            })
                            .collect_view();
                        view! { <div class="exp-mkt-grid">{shims}</div> }.into_any()
                    } else if !feed_error.with(|e| e.is_empty()) {
                        view! {
                            <div class="exp-empty">
                                // Pesannya datang dari store, yang sudah
                                // membedakan sebabnya (sesi habis / perangkat
                                // luring / server bermasalah). Sebelumnya kalimat
                                // ini dipatri di sini dan SELALU berbunyi "tidak
                                // bisa terhubung ke server" — untuk galat apa pun,
                                // termasuk saat server terhubung dan menjawab
                                // dengan baik.
                                <EmptyState
                                    icon="⚠️"
                                    title="Gagal Memuat"
                                    // Closure pembungkusnya sudah dijalankan
                                    // ulang saat `feed_error` berubah, jadi nilai
                                    // biasa sudah cukup — `EmptyState` menerima
                                    // String, bukan sinyal.
                                    body=feed_error.get()
                                />
                                <button
                                    class="exp-reset-btn"
                                    on:click=move |_| store.load_cat(active_cat.get_untracked())
                                >
                                    "Coba Lagi"
                                </button>
                            </div>
                        }
                            .into_any()
                    } else {
                        let list = feed_list.get();
                        if list.is_empty() {
                            view! {
                                <div class="exp-empty">
                                    <EmptyState
                                        icon="🔍"
                                        title="Belum Ada Product"
                                        body="Coba pilih kategori lain atau ubah filter."
                                    />
                                    <button
                                        class="exp-reset-btn"
                                        on:click=move |_| active_cat.set("All".into())
                                    >
                                        "Atur Ulang Filter"
                                    </button>
                                </div>
                            }
                                .into_any()
                        } else {
                            let cards = list
                                .into_iter()
                                .enumerate()
                                .map(|(i, ev)| {
                                    view! { <ProductCardPub ev=ev index=i /> }
                                })
                                .collect_view();
                            let shims = store
                                .loading_more
                                .get()
                                .then(|| {
                                    (0..4)
                                        .map(|_| {
                                            // Shimmer load_more menyatu di grid yang SAMA
                                            // (bukan grid terpisah di bawah) — kartu shimmer
                                            // langsung tergantikan data di posisinya, sama
                                            // seperti "Produk Berkaitan" di detail product.
                                            view! {
                                                <div class="exp-shimmer-wrap">
                                                    <ProductCardShimmer />
                                                </div>
                                            }
                                        })
                                        .collect_view()
                                });
                            view! { <div class="exp-mkt-grid">{cards}{shims}</div> }.into_any()
                        }
                    }
                }}
                </Suspense>
            </div>

            <div class="exp-genre-section">
                <span class="exp-section-eyebrow">"EXPLORE BY GENRE"</span>
                <div class="exp-genre-chips">
                    {move || {
                        store
                            .categories
                            .with(|cats| {
                                cats.iter()
                                    .filter(|c| *c != "All")
                                    .map(|label| {
                                        let lc = label.clone();
                                        let lk = label.clone();
                                        view! {
                                            <button
                                                class=move || {
                                                    if active_cat.get() == lc {
                                                        "exp-genre-chip exp-genre-chip--on"
                                                    } else {
                                                        "exp-genre-chip"
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
            </div>

            <BottomNav active="explore" />

            {move || {
                show_overlay
                    .get()
                    .then(|| {
                        let cc = close_c.get_value();
                        view! {
                            <div
                                class=move || {
                                    if overlay_visible.get() {
                                        "exp-sovl-backdrop exp-sovl-backdrop--open"
                                    } else {
                                        "exp-sovl-backdrop"
                                    }
                                }
                                on:click=move |_| cc()
                            ></div>
                        }
                    })
            }}

            {move || {
                show_overlay
                    .get()
                    .then(|| {
                        let cc = close_c.get_value();
                        view! {
                            <div class=move || {
                                if overlay_visible.get() {
                                    "exp-sovl-wrap exp-sovl-wrap--open"
                                } else {
                                    "exp-sovl-wrap"
                                }
                            }>
                                <SearchOverlay
                                    query=query
                                    active_cat=active_cat
                                    on_close=cc
                                    store=store
                                    ph_text=ph_text
                                />
                            </div>
                        }
                    })
            }}

            <StoryViewer />
        </div>
    }
}
