//! event_detail.rs — Halaman Detail Event (SSR + hydration).
//!
//! Cart-based ticket selection matching CSR design:
//! Add button → qty ctrl, footer subtotal → Secure Tickets → /cart.

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};
use crate::web::api::{get_event_detail, get_events};
use crate::web::app::{AuthResource, CartContext};
use crate::web::components::story_viewer::StoryViewer;
use crate::web::components::{EventCardPub, LiveStreamViewer, MerchantLivePip};
use crate::web::hooks::ThemeToggle;
use crate::web::models::{CartItem, format_date, format_price};
use crate::web::seo::SeoMeta;

#[component]
pub fn EventDetailPage() -> impl IntoView {
    let params    = use_params_map();
    let slug      = Memo::new(move |_| params.read().get("slug").unwrap_or_default());
    let event_res = Resource::new_blocking(move || slug.get(), |s| get_event_detail(s));
    // Kategori event ini → dipakai untuk (a) mencari event BERKAITAN, dan
    // (b) mencatat minat user untuk rekomendasi "Untuk Kamu".
    let rel_cat = Memo::new(move |_| {
        event_res
            .get()
            .and_then(|r| r.ok())
            .and_then(|ev| ev.category.first().cloned())
    });
    // ── Event Berkaitan: daftar inkremental (LIMIT/OFFSET) ────────────────────
    // Dulu fetch 24 item sekaligus. Sekarang chunk kecil per halaman
    // (page/per_page get_events = LIMIT/OFFSET di DB); chunk berikutnya diminta
    // otomatis saat user scroll mendekati ujung halaman.
    const REL_PAGE_SIZE: i64 = 12;
    let rel_items: RwSignal<Vec<crate::web::models::Event>> = RwSignal::new(Vec::new());
    let rel_page = RwSignal::new(1i64);
    let rel_has_more = RwSignal::new(false);
    let rel_loading = RwSignal::new(false);

    // Muat satu halaman berikutnya & APPEND. Guard rangkap (loading/has_more)
    // agar aman dipanggil bertubi-tubi dari event scroll tanpa fetch ganda.
    let load_rel = move || {
        if rel_loading.get_untracked() {
            return;
        }
        let next = rel_page.get_untracked();
        if next > 1 && !rel_has_more.get_untracked() {
            return;
        }
        rel_loading.set(true);
        let cat = rel_cat.get_untracked();
        leptos::task::spawn_local(async move {
            if let Ok(res) = get_events(Some(next), None, cat, None, Some(REL_PAGE_SIZE)).await {
                rel_has_more.set(res.page < res.total_pages);
                rel_page.set(next + 1);
                rel_items.update(|v| v.extend(res.data));
            }
            rel_loading.set(false);
        });
    };

    // Reset + muat halaman pertama begitu detail event termuat / slug berganti.
    Effect::new(move |_| {
        if event_res.get().map(|r| r.is_ok()) != Some(true) {
            return;
        }
        let _ = rel_cat.get(); // ikut re-run bila kategori (slug) berubah
        rel_items.set(Vec::new());
        rel_page.set(1);
        rel_has_more.set(false);
        load_rel();
    });

    // Footer beli disembunyikan begitu user scroll melewati kartu venue
    // ("Get Directions" = bagian detail terakhir) → area "Event Berkaitan"
    // bersih tanpa tombol beli menutupi konten.
    let past_dirs = RwSignal::new(false);

    // Peta venue asli (OpenStreetMap) langsung dirender di detail event bila
    // koordinat tersedia — init post-hydration (pola sama dgn venue_location).
    Effect::new(move |_| {
        if let Some(Ok(ev)) = event_res.get() {
            if let (Some(la), Some(lo)) = (ev.latitude, ev.longitude) {
                let label = ev.venue.clone().unwrap_or_else(|| ev.name.clone());
                crate::web::utils::map_viewer("ed-venue-map", la, lo, &label);
            }
        }
    });
    on_cleanup(|| crate::web::utils::map_destroy("ed-venue-map"));

    // Infinite scroll: mulai prefetch ~2.5 layar sebelum ujung dokumen supaya
    // data tiba sebelum user sampai di bawah (sama seperti Explore).
    #[cfg(feature = "hydrate")]
    {
        use send_wrapper::SendWrapper;
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

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
                let threshold = (inner_h * 2.5).max(1200.0);
                if doc_h - (scroll_y + inner_h) < threshold {
                    load_rel();
                }
                // Deteksi "sudah melewati kartu venue": bottom kartu di atas
                // viewport → sembunyikan footer beli (set hanya saat berubah).
                let past = win
                    .document()
                    .and_then(|d| d.get_element_by_id("ed-venue-card"))
                    .map(|el| el.get_bounding_client_rect().bottom() < 0.0)
                    .unwrap_or(false);
                if past_dirs.get_untracked() != past {
                    past_dirs.set(past);
                }
            });
            if let Some(win) = web_sys::window() {
                let _ =
                    win.add_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref());
            }
            scroll_cb.set_value(Some(SendWrapper::new(cb)));
        });
        on_cleanup(move || {
            if let Some(Some(cb)) = scroll_cb.try_update_value(|o| o.take()) {
                if let Some(win) = web_sys::window() {
                    let _ = win
                        .remove_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref());
                }
                drop(cb);
            }
        });
    }
    // Rekomendasi implisit (tanpa "like"): catat kategori event yang dibuka user
    // ke localStorage. Effect hanya jalan di client saat detail sudah termuat.
    Effect::new(move |_| {
        if let Some(Ok(ev)) = event_res.get() {
            let cats = ev.category.clone();
            // (a) Client: localStorage (jalan utk semua user, termasuk anonim).
            crate::web::behavior::record_view(&cats);
            // (b) Server: persist ke DB (user login) untuk rekomendasi lintas-sesi.
            //     No-op diam-diam bila belum login.
            leptos::task::spawn_local(async move {
                let _ = crate::web::api::record_affinity(cats, None).await;
            });
        }
    });

    let navigate  = use_navigate();
    let auth     = use_context::<AuthResource>().expect("AuthResource missing");
    let cart_ctx = use_context::<CartContext>().expect("CartContext not provided");

    // ── Bottom sheets ─────────────────────────────────────────────────────────
    // Info merchant (klik penyelenggara → sheet dulu, BUKAN langsung /m/{id})
    // dan pilihan tiket (varian). Panel selalu dirender agar transisi CSS
    // slide-up/fade berjalan (lihat styles/parts/39-event-sheets.css).
    let merchant_sheet = RwSignal::new(false);
    let tickets_sheet = RwSignal::new(false);

    // ── Story penyelenggara (lingkaran ala story-bar explore) ────────────────
    // Fetch ringan saat detail termuat; lingkaran hanya tampil bila merchant
    // punya story aktif (termasuk story ulasan). Klik → buka StoryViewer yang
    // di-mount halaman ini (pola sama dengan /profile & /m/{id}).
    let mch_stories: RwSignal<Vec<crate::web::state::stories::StoryGroup>> =
        RwSignal::new(Vec::new());
    let stories_ctx = crate::web::state::stories::use_stories_store();
    #[cfg(feature = "hydrate")]
    Effect::new(move |_| {
        let Some(Ok(ev)) = event_res.get() else { return };
        let mid = ev.merchant_id.clone();
        leptos::task::spawn_local(async move {
            if let Ok(groups) = crate::web::api::get_merchant_stories(mid).await {
                mch_stories.set(groups);
            }
        });
    });
    let open_merchant_stories = move |_| {
        let logged_in = auth
            .get_untracked()
            .and_then(|r| r.ok())
            .flatten()
            .is_some();
        if !logged_in {
            if let Some(win) = web_sys::window() {
                let _ = win.location().assign("/login");
            }
            return;
        }
        let list = mch_stories.get_untracked();
        if !list.is_empty() {
            stories_ctx.groups.set(list);
            stories_ctx.open_at(0, 0);
        }
    };
    // Info merchant TIDAK di-fetch terpisah: sudah ikut payload detail event
    // (`ev.merchant`, JOIN + agregat satu query di server) — sheet render
    // langsung dari data yang ada, tanpa fetch kedua & tanpa loading state.

    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let shimmer = move || view! {
        // No .page wrapper here — .page is hoisted outside Suspense below
        <div class="shim" style="width:100%;height:280px;border-radius:0"></div>
        <div style="padding:20px 16px;display:flex;flex-direction:column;gap:14px">
            <div class="shim" style="height:18px;width:80px;border-radius:100px"></div>
            <div class="shim" style="height:32px;width:85%"></div>
            <div class="shim" style="height:32px;width:60%"></div>
            <div style="display:flex;gap:10px;margin-top:4px">
                <div class="shim" style="height:13px;width:130px"></div>
                <div class="shim" style="height:13px;width:100px"></div>
            </div>
            <div style="height:1px;background:var(--border-soft);margin:8px 0"></div>
            <div class="shim" style="height:14px;width:120px"></div>
            {(0..2i32)
                .map(|_| {
                    view! {
                        <div style="display:flex;justify-content:space-between;align-items:center;
                        padding:14px 0;border-bottom:1px solid var(--border-soft)">
                            <div style="display:flex;flex-direction:column;gap:8px">
                                <div class="shim" style="height:16px;width:130px"></div>
                                <div class="shim" style="height:12px;width:80px"></div>
                            </div>
                            <div
                                class="shim"
                                style="height:36px;width:80px;border-radius:100px"
                            ></div>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    };

    view! {
        // .page is always rendered — Suspense only replaces inner content
        <div class="page ed-page">
            <Suspense fallback=shimmer>
                {move || {
                    let navigate = navigate.clone();
                    Suspend::new(async move {
                        match event_res.await {
                            Err(_) => {
                                view! {
                                    <header class="page-header ed-header">
                                        <A
                                            href="/explore"
                                            attr:class="back-btn"
                                            attr:aria-label="Back"
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
                                        </A>
                                        <span class="page-logo">"KINETIC"</span>
                                        <div style="width:36px"></div>
                                    </header>
                                    <div style="display:flex;flex-direction:column;align-items:center;
                                    justify-content:center;min-height:60vh;padding:20px;text-align:center">
                                        <div style="font-size:3rem;margin-bottom:12px">"🔍"</div>
                                        <h2 style="font-size:1.1rem;font-weight:700;text-transform:uppercase;
                                        letter-spacing:.06em;margin-bottom:8px">
                                            "EVENT TIDAK DITEMUKAN"
                                        </h2>
                                        <p style="color:var(--text-muted);font-size:.85rem;margin-bottom:20px">
                                            "Event ini mungkin sudah selesai atau telah dihapus."
                                        </p>
                                        <A href="/explore" attr:class="tier-add-btn">
                                            "JELAJAHI EVENT"
                                        </A>
                                    </div>
                                }
                                    .into_any()
                            }
                            Ok(ev) => {
                                let title = ev.name.clone();
                                let cover = ev.cover_url.clone().unwrap_or_default();
                                let date_str = format_date(&ev.event_date);
                                let venue_str = match (&ev.venue, &ev.city) {
                                    (Some(v), Some(c)) => format!("{}, {}", v, c),
                                    (Some(v), None) => v.clone(),
                                    (None, Some(c)) => c.clone(),
                                    (None, None) => String::new(),
                                };
                                let cats = ev.category.clone();
                                let desc = ev.description.clone().unwrap_or_default();
                                let base_price = ev.display_price;
                                let ev_slug = ev.slug.clone();
                                let ev_id = ev.id.clone();
                                let ev_name = ev.name.clone();
                                let ev_cover = ev.cover_url.clone().unwrap_or_default();
                                let variants = ev.event_variants.clone();
                                let has_coords = ev.latitude.is_some() && ev.longitude.is_some();
                                let is_live = ev.status.eq_ignore_ascii_case("live");
                                let live_room_id = format!("live_{}", ev.merchant_id);
                                let organizer_href = format!("/m/{}", ev.merchant_id);
                                let organizer_name = ev
                                    .merchant_name
                                    .clone()
                                    .filter(|n| !n.is_empty())
                                    .unwrap_or_else(|| "Penyelenggara".to_string());
                                // Ringkasan merchant untuk bottom sheet — sudah ikut
                                // payload detail (1 query di server, tanpa fetch kedua).
                                let sheet_merchant = ev.merchant.clone();
                                let sold_count = ev.total_sold.max(0);
                                let quota = ev.total_quota.max(0);
                                let remaining = (quota - sold_count).max(0);
                                let sold_pct = if quota > 0 {
                                    ((sold_count as f64 / quota as f64) * 100.0)
                                        .round()
                                        .clamp(0.0, 100.0) as i32
                                } else {
                                    0
                                };
                                let low_stock = quota > 0 && remaining <= (quota / 10).max(5);
                                let ev_id_price = ev_id.clone();
                                let ev_id_btn = ev_id.clone();
                                let _share_slug = ev_slug.clone();
                                let _share_title = title.clone();
                                let _share_cover = cover.clone();
                                let _share_id = ev_id.clone();
                                let _share_desc = desc.clone();
                                let _share_venue = venue_str.clone();
                                let _share_price_str = format_price(base_price);
                                let _share_date_str = date_str.clone();
                                let _nav_story = navigate.clone();
                                let share_to_story = move |_: web_sys::MouseEvent| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        let params = web_sys::UrlSearchParams::new()
                                            .expect("UrlSearchParams");
                                        params.append("event_id", &_share_id);
                                        params.append("event_slug", &_share_slug);
                                        params.append("event_title", &_share_title);
                                        params.append("event_cover", &_share_cover);
                                        params.append("event_desc", &_share_desc);
                                        params.append("event_date", &_share_date_str);
                                        params.append("event_venue", &_share_venue);
                                        params.append("event_price", &_share_price_str);
                                        if let Some(win) = web_sys::window() {
                                            if let Ok(Some(storage)) = win.session_storage() {
                                                let _ = storage.set_item("story_hero_transition", "event");
                                                let _ = storage.set_item("story_hero_cover", &_share_cover);
                                            }
                                        }
                                        let qs = params.to_string();
                                        _nav_story(&format!("/story?{}", qs), Default::default());
                                    }
                                };
                                let variant_count = variants.len();
                                let tiers_view = variants
                                    .into_iter()
                                    .map(|v| {
                                        let vid = v.id.clone();
                                        let vid_add = v.id.clone();
                                        let vid_plus = v.id.clone();
                                        let vid_minus = v.id.clone();
                                        let vname = v.name.clone();
                                        let vdesc = v.description.clone();
                                        let vprice = format_price(v.display_price);
                                        let vprice_val = v.display_price as i64;
                                        let rem = v.remaining;
                                        let is_vip = v.name.to_lowercase().contains("vip");
                                        let card_cls = if is_vip {
                                            "tier-card tier-card--vip"
                                        } else {
                                            "tier-card"
                                        };
                                        let ev_id_a = ev_id.clone();
                                        let ev_nm_a = ev_name.clone();
                                        let venue_a = venue_str.clone();
                                        let cover_a = ev_cover.clone();
                                        let tname_a = vname.clone();
                                        let vid_qty = vid.clone();
                                        let on_add = {
                                            let ev_id_a = ev_id_a.clone();
                                            let ev_nm_a = ev_nm_a.clone();
                                            let venue_a = venue_a.clone();
                                            let cover_a = cover_a.clone();
                                            let tname_a = tname_a.clone();
                                            let vid_add = vid_add.clone();
                                            let cats_a = cats.clone();
                                            move |e: web_sys::MouseEvent| {
                                                e.stop_propagation();
                                                cart_ctx
                                                    .add_item(CartItem {
                                                        event_id: ev_id_a.clone(),
                                                        tier_id: vid_add.clone(),
                                                        event_title: ev_nm_a.clone(),
                                                        tier_name: tname_a.clone(),
                                                        venue_name: venue_a.clone(),
                                                        event_cover: cover_a.clone(),
                                                        quantity: 1,
                                                        unit_price: vprice_val,
                                                    });
                                                crate::web::behavior::record_signal(&cats_a, 3.0);
                                                let cats_srv = cats_a.clone();
                                                leptos::task::spawn_local(async move {
                                                    let _ = crate::web::api::record_affinity(
                                                            cats_srv,
                                                            Some("cart".into()),
                                                        )
                                                        .await;
                                                });
                                            }
                                        };
                                        let on_minus = move |e: web_sys::MouseEvent| {
                                            e.stop_propagation();
                                            let q = cart_ctx.get_qty(&vid_minus);
                                            if q > 0 {
                                                cart_ctx.update_qty(&vid_minus, q - 1);
                                            }
                                        };
                                        let on_plus = move |e: web_sys::MouseEvent| {
                                            e.stop_propagation();
                                            let q = cart_ctx.get_qty(&vid_plus);
                                            cart_ctx.update_qty(&vid_plus, q + 1);
                                        };
                                        // Nama toko penyelenggara (fallback label lama bila
                                        // event lama belum punya nama ter-join).

                                        // Live streaming: badge + embedded viewer hanya tampil saat
                                        // event berstatus "live". room_id mengikuti format SFU.
                                        // Link profil merchant publik (penyelenggara) — /m/{id}.
                                        // Social proof (gaya marketplace): terjual + sisa stok.
                                        // Sumber data asli: agregasi SUM(sold)/SUM(quota) dari
                                        // ticket_variants (ter-update tiap order via order.rs).

                                        // ── Cart total helpers: inline in each closure ─────────
                                        // cart_ctx is Copy. Each closure gets a distinct clone of ev_id.

                                        // ── Share to story handler ────────────────────────────

                                        // ── Tier cards (cart-based like CSR) ──────────────────

                                        // Behavior: memilih produk (add-to-cart) = sinyal
                                        // minat kuat (bobot 3). Anonim → localStorage;
                                        // login → buffer server (tanpa blokir UI).

                                        view! {
                                            <div class=card_cls>
                                                <div class="tier-top">
                                                    <div class="tier-name">{vname.clone()}</div>
                                                    {vdesc.map(|d| view! { <p class="tier-desc">{d}</p> })}
                                                    {(is_vip && rem <= 15)
                                                        .then(|| {
                                                            view! {
                                                                <span class="tier-scarcity">
                                                                    <span class="scarcity-dot"></span>
                                                                    {format!("Only {} Left", rem)}
                                                                </span>
                                                            }
                                                        })}
                                                </div>
                                                <div class="tier-bottom">
                                                    <span class="tier-price">{vprice}</span>
                                                    {move || {
                                                        let q = cart_ctx.get_qty(&vid_qty);
                                                        if q == 0 {
                                                            view! {
                                                                <button class="tier-add-btn" on:click=on_add.clone()>
                                                                    "Add"
                                                                </button>
                                                            }
                                                                .into_any()
                                                        } else {
                                                            view! {
                                                                <div class="qty-ctrl">
                                                                    <button
                                                                        class="qty-btn qty-btn--minus"
                                                                        on:click=on_minus.clone()
                                                                    >
                                                                        "−"
                                                                    </button>
                                                                    <span class="qty-val">{q}</span>
                                                                    <button
                                                                        class="qty-btn qty-btn--plus"
                                                                        on:click=on_plus.clone()
                                                                    >
                                                                        "+"
                                                                    </button>
                                                                </div>
                                                            }
                                                                .into_any()
                                                        }
                                                    }}
                                                </div>
                                            </div>
                                        }
                                    })
                                    .collect_view();
                                let meta_title = format!("{} — PULSE", title);
                                let meta_desc = format!(
                                    "{} | {} | {}",
                                    if desc.is_empty() { "Event seru di Indonesia" } else { &desc },
                                    venue_str,
                                    date_str,
                                );
                                let seo_path = format!("/events/{}", ev_slug);
                                let seo_image = cover.clone();
                                let ld_event = crate::web::seo::safe_ld(
                                    &serde_json::json!(
                                        {
                                        "@context": "https://schema.org",
                                        "@type": "Event",
                                        "name": ev.name.clone(),
                                        "description": (!desc.is_empty()).then(|| desc.clone()),
                                        "startDate": ev.event_date.to_rfc3339(),
                                        "endDate": ev.end_time.map(|d| d.to_rfc3339()),
                                        "eventStatus": "https://schema.org/EventScheduled",
                                        "eventAttendanceMode": "https://schema.org/OfflineEventAttendanceMode",
                                        "image": ev.cover_url.clone().map(|c| vec![c]),
                                        "location": {
                                            "@type": "Place",
                                            "name": ev.venue.clone(),
                                            "address": ev.city.clone(),
                                        },
                                        "offers": {
                                            "@type": "Offer",
                                            "price": ev.display_price,
                                            "priceCurrency": "IDR",
                                            "availability": "https://schema.org/InStock",
                                            "url": crate::web::seo::abs_url(&seo_path),
                                        },
                                        "organizer": {
                                            "@type": "Organization",
                                            "url": crate::web::seo::abs_url(
                                                &format!("/m/{}", ev.merchant_id),
                                            ),
                                        },
                                    }
                                    ),
                                );
                                view! {
                                    <SeoMeta
                                        title=meta_title
                                        description=meta_desc
                                        path=seo_path
                                        image=seo_image
                                        og_type="article"
                                    />
                                    <Script type_="application/ld+json">{ld_event}</Script>
                                    // Overlay live melayang: muncul saat merchant pemilik
                                    // event ini sedang siaran (room SFU benar-benar ada).
                                    <MerchantLivePip room_id=live_room_id.clone() />
                                    // ── Header ───────────────────────────────────────────
                                    <header class="page-header ed-header">
                                        <button
                                            class="back-btn"
                                            aria-label="Back"
                                            on:click=move |_| {
                                                #[cfg(target_arch = "wasm32")]
                                                if let Some(win) = web_sys::window() {
                                                    let _ = win.history().ok().map(|h| h.back());
                                                }
                                            }
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
                                        <span class="page-logo">"KINETIC"</span>
                                        <div class="header-actions">
                                            <ThemeToggle />
                                            <button
                                                class="icon-btn"
                                                on:click=share_to_story
                                                aria-label="Bagikan ke Cerita"
                                            >
                                                <svg
                                                    width="16"
                                                    height="16"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2.2"
                                                    stroke-linecap="round"
                                                >
                                                    <circle cx="18" cy="5" r="3" />
                                                    <circle cx="6" cy="12" r="3" />
                                                    <circle cx="18" cy="19" r="3" />
                                                    <line x1="8.59" y1="13.51" x2="15.42" y2="17.49" />
                                                    <line x1="15.41" y1="6.51" x2="8.59" y2="10.49" />
                                                </svg>
                                            </button>
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
                                            </A>
                                            <A href="/profile" attr:class="nav-avatar">
                                                <svg
                                                    width="18"
                                                    height="18"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2"
                                                    stroke-linecap="round"
                                                >
                                                    <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
                                                    <circle cx="12" cy="7" r="4" />
                                                </svg>
                                            </A>
                                        </div>
                                    </header>

                                    // ── Hero ─────────────────────────────────────────────
                                    <div class="ed-hero">
                                        {if cover.is_empty() {
                                            view! {
                                                <div
                                                    class="ed-hero-img"
                                                    style="background:var(--bg-elevated)"
                                                />
                                            }
                                                .into_any()
                                        } else {
                                            view! {
                                                <img src=cover alt=title.clone() class="ed-hero-img" />
                                            }
                                                .into_any()
                                        }} <div class="ed-hero-gradient"></div>
                                        <div class="ed-hero-overlay-content">
                                            <div class="ed-hero-badges">
                                                {is_live
                                                    .then(|| {
                                                        view! {
                                                            // Klik untuk loncat ke pemutar siaran langsung di bawah.
                                                            <a
                                                                href="#ed-live-section"
                                                                class="ed-live-badge ed-live-badge--link"
                                                            >
                                                                <span class="ed-live-dot"></span>
                                                                "LIVE — TONTON"
                                                                <svg
                                                                    width="11"
                                                                    height="11"
                                                                    viewBox="0 0 24 24"
                                                                    fill="none"
                                                                    stroke="currentColor"
                                                                    stroke-width="3"
                                                                    stroke-linecap="round"
                                                                >
                                                                    <polyline points="6 9 12 15 18 9" />
                                                                </svg>
                                                            </a>
                                                        }
                                                    })}
                                                {cats
                                                    .first()
                                                    .map(|c| {
                                                        view! { <span class="ed-cat-badge">{c.clone()}</span> }
                                                    })}
                                            </div>
                                            <h1 class="ed-hero-title">{title.clone()}</h1>
                                            <div class="ed-hero-meta">
                                                <div class="ed-hero-meta-item">
                                                    <svg
                                                        width="13"
                                                        height="13"
                                                        viewBox="0 0 24 24"
                                                        fill="none"
                                                        stroke="currentColor"
                                                        stroke-width="2"
                                                        stroke-linecap="round"
                                                    >
                                                        <rect x="3" y="4" width="18" height="18" rx="2" />
                                                        <line x1="16" y1="2" x2="16" y2="6" />
                                                        <line x1="8" y1="2" x2="8" y2="6" />
                                                        <line x1="3" y1="10" x2="21" y2="10" />
                                                    </svg>
                                                    {date_str}
                                                </div>
                                                {(!venue_str.is_empty())
                                                    .then({
                                                        let vs = venue_str.clone();
                                                        move || {
                                                            view! {
                                                                <div class="ed-hero-meta-item">
                                                                    <svg
                                                                        width="13"
                                                                        height="13"
                                                                        viewBox="0 0 24 24"
                                                                        fill="none"
                                                                        stroke="currentColor"
                                                                        stroke-width="2"
                                                                        stroke-linecap="round"
                                                                    >
                                                                        <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0118 0z" />
                                                                        <circle cx="12" cy="10" r="3" />
                                                                    </svg>
                                                                    {vs.clone()}
                                                                </div>
                                                            }
                                                        }
                                                    })}
                                            </div>
                                        </div>
                                    </div>

                                    // ── Body ─────────────────────────────────────────────
                                    <div class="ed-body">
                                        <div class="ed-main">

                                            // Social proof strip (gaya marketplace) — data asli variant
                                            <div class="ed-social-proof">
                                                <div class="ed-sp-head">
                                                    <div class="ed-sp-item">
                                                        <svg
                                                            width="15"
                                                            height="15"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="2"
                                                            stroke-linecap="round"
                                                            stroke-linejoin="round"
                                                        >
                                                            <path d="M4 4h16a2 2 0 012 2v3a2 2 0 000 4v3a2 2 0 01-2 2H4a2 2 0 01-2-2v-3a2 2 0 000-4V6a2 2 0 012-2z" />
                                                            <line
                                                                x1="12"
                                                                y1="4"
                                                                x2="12"
                                                                y2="20"
                                                                stroke-dasharray="2 3"
                                                            />
                                                        </svg>
                                                        <span>
                                                            <b>{sold_count}</b>
                                                            " terjual"
                                                        </span>
                                                    </div>
                                                    <div class="ed-sp-item ed-sp-verified">
                                                        <svg
                                                            width="15"
                                                            height="15"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="2"
                                                            stroke-linecap="round"
                                                            stroke-linejoin="round"
                                                        >
                                                            <path d="M9 12l2 2 4-4" />
                                                            <path d="M12 3l7 4v5c0 4.5-3 8-7 9-4-1-7-4.5-7-9V7l7-4z" />
                                                        </svg>
                                                        <span>"Terverifikasi"</span>
                                                    </div>
                                                </div>
                                                {(quota > 0)
                                                    .then(|| {
                                                        view! {
                                                            <div class="ed-sp-stock">
                                                                <div class="ed-sp-bar">
                                                                    <div
                                                                        class=if low_stock {
                                                                            "ed-sp-bar-fill ed-sp-bar-fill--low"
                                                                        } else {
                                                                            "ed-sp-bar-fill"
                                                                        }
                                                                        style=format!("width:{}%", sold_pct.max(3))
                                                                    ></div>
                                                                </div>
                                                                <div class=if low_stock {
                                                                    "ed-sp-remain ed-sp-low"
                                                                } else {
                                                                    "ed-sp-remain"
                                                                }>
                                                                    {if low_stock {
                                                                        view! {
                                                                            <span>
                                                                                "🔥 Segera habis — sisa "<b>{remaining}</b>" tiket"
                                                                            </span>
                                                                        }
                                                                            .into_any()
                                                                    } else {
                                                                        view! {
                                                                            <span>
                                                                                "Sisa "<b>{remaining}</b>" dari "{quota}" tiket"
                                                                            </span>
                                                                        }
                                                                            .into_any()
                                                                    }}
                                                                </div>
                                                            </div>
                                                        }
                                                    })}
                                            </div>

                                            // Penyelenggara → profil merchant publik (/m/{id}):
                                            // rating, follower, dan semua event si penyelenggara.
                                            // Lingkaran story penyelenggara (ala story-bar
                                            // explore) — tampil hanya bila ada story aktif
                                            // (termasuk story ulasan). Klik → StoryViewer.
                                            {
                                                let ring_name = organizer_name.clone();
                                                let ring_logo = sheet_merchant
                                                    .as_ref()
                                                    .and_then(|m| m.logo_url.clone())
                                                    .unwrap_or_default();
                                                move || {
                                                    let ring_name = ring_name.clone();
                                                    let ring_logo = ring_logo.clone();
                                                    (!mch_stories.get().is_empty()).then(move || {
                                                        let initial: String = ring_name
                                                            .chars()
                                                            .next()
                                                            .unwrap_or('P')
                                                            .to_uppercase()
                                                            .to_string();
                                                        view! {
                                                            <button
                                                                class="ed-story-ring"
                                                                on:click=open_merchant_stories
                                                                aria-label="Lihat story penyelenggara"
                                                            >
                                                                <span class="ed-story-ring-circle">
                                                                    {if ring_logo.is_empty() {
                                                                        view! {
                                                                            <span class="ed-story-ring-fallback">
                                                                                {initial}
                                                                            </span>
                                                                        }
                                                                            .into_any()
                                                                    } else {
                                                                        view! {
                                                                            <img
                                                                                src=ring_logo.clone()
                                                                                alt=""
                                                                                loading="lazy"
                                                                            />
                                                                        }
                                                                            .into_any()
                                                                    }}
                                                                </span>
                                                                <span class="ed-story-ring-label">
                                                                    "Story " {ring_name.clone()}
                                                                </span>
                                                            </button>
                                                        }
                                                    })
                                                }
                                            }

                                            // Klik → bottom sheet info merchant dulu
                                            // (logo/header/rating), BUKAN langsung /m/{id}.
                                            <button
                                                class="ed-organizer"
                                                on:click=move |_| merchant_sheet.set(true)
                                            >
                                                <span class="ed-org-icon">
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
                                                        <path d="M3 9l1-5h16l1 5" />
                                                        <path d="M4 9v11a1 1 0 001 1h14a1 1 0 001-1V9" />
                                                        <path d="M3 9a3 3 0 006 0 3 3 0 006 0 3 3 0 006 0" />
                                                    </svg>
                                                </span>
                                                <span class="ed-org-text">
                                                    {organizer_name.clone()}
                                                    <span class="ed-org-sub">
                                                        "Lihat profil, rating & event lainnya"
                                                    </span>
                                                </span>
                                                <span class="ed-org-arrow">
                                                    <svg
                                                        width="16"
                                                        height="16"
                                                        viewBox="0 0 24 24"
                                                        fill="none"
                                                        stroke="currentColor"
                                                        stroke-width="2.5"
                                                        stroke-linecap="round"
                                                    >
                                                        <polyline points="9 18 15 12 9 6" />
                                                    </svg>
                                                </span>
                                            </button>

                                            // Live stream (tampil saat event sedang live)
                                            {is_live
                                                .then({
                                                    let rid = live_room_id.clone();
                                                    move || {
                                                        view! {
                                                            <section
                                                                class="section ed-live-section"
                                                                id="ed-live-section"
                                                            >
                                                                <p class="ed-section-eyebrow">"SIARAN LANGSUNG"</p>
                                                                <LiveStreamViewer room_id=rid.clone() />
                                                            </section>
                                                        }
                                                    }
                                                })}

                                            // About
                                            {(!desc.is_empty())
                                                .then({
                                                    let d = desc.clone();
                                                    move || {
                                                        view! {
                                                            <section class="section">
                                                                <p class="ed-section-eyebrow">"ABOUT THE EVENT"</p>
                                                                <p class="about-text">{d.clone()}</p>
                                                            </section>
                                                        }
                                                    }
                                                })}

                                            // Categories
                                            {(!cats.is_empty())
                                                .then({
                                                    let c2 = cats.clone();
                                                    move || {
                                                        view! {
                                                            <div class="ed-categories-section">
                                                                <p class="ed-section-eyebrow">"CATEGORIES"</p>
                                                                <div class="ed-chips-row">
                                                                    {c2
                                                                        .iter()
                                                                        .map(|c| view! { <span class="ed-chip">{c.clone()}</span> })
                                                                        .collect_view()}
                                                                </div>
                                                            </div>
                                                        }
                                                    }
                                                })}

                                            // Select Tickets — varian dipindah ke bottom
                                            // sheet; di body cukup tombol pemicu.
                                            <div class="ed-tickets-header">
                                                <span class="ed-tickets-title">"Select Tickets"</span>
                                                <span class="ed-tickets-avail">
                                                    "Available until sale ends"
                                                </span>
                                            </div>
                                            <section class="section ed-mobile-tiers">
                                                <button
                                                    class="ed-tickets-trigger"
                                                    on:click=move |_| tickets_sheet.set(true)
                                                >
                                                    <span class="ed-tickets-trigger-text">
                                                        "Pilih Tiket"
                                                        <span class="ed-tickets-trigger-sub">
                                                            {format!(
                                                                "{} varian · mulai {}",
                                                                variant_count,
                                                                format_price(base_price),
                                                            )}
                                                        </span>
                                                    </span>
                                                    <span class="ed-tickets-trigger-arrow">
                                                        <svg
                                                            width="16"
                                                            height="16"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="2.5"
                                                            stroke-linecap="round"
                                                        >
                                                            <polyline points="6 9 12 15 18 9" />
                                                        </svg>
                                                    </span>
                                                </button>
                                            </section>

                                            // Venue
                                            <section class="section">
                                                <h2 class="section-title">"The Venue"</h2>
                                            </section>
                                            <div class="map-card" id="ed-venue-card">
                                                // Peta OpenStreetMap asli langsung tampil bila event
                                                // punya koordinat; fallback visual dekoratif bila kosong.
                                                {if has_coords {
                                                    view! {
                                                        <div
                                                            id="ed-venue-map"
                                                            class="map-visual map-visual--live"
                                                        ></div>
                                                    }
                                                        .into_any()
                                                } else {
                                                    view! {
                                                        <div class="map-visual">
                                                            <div class="map-grid"></div>
                                                            <div class="map-pin">
                                                                <svg width="28" height="36" viewBox="0 0 32 40">
                                                                    <path
                                                                        d="M16 0C7.163 0 0 7.163 0 16c0 11 16 24 16 24s16-13 16-24C32 7.163 24.837 0 16 0z"
                                                                        fill="#c8ff5e"
                                                                    />
                                                                    <circle cx="16" cy="16" r="6" fill="#0d0d1a" />
                                                                </svg>
                                                            </div>
                                                        </div>
                                                    }
                                                        .into_any()
                                                }}
                                                <div class="map-info">
                                                    {(!ev_slug.is_empty())
                                                        .then({
                                                            let es = ev_slug.clone();
                                                            move || {
                                                                view! {
                                                                    <A
                                                                        href=format!("/events/{}/location", es)
                                                                        attr:class="directions-btn"
                                                                    >
                                                                        <svg
                                                                            width="14"
                                                                            height="14"
                                                                            viewBox="0 0 24 24"
                                                                            fill="none"
                                                                            stroke="currentColor"
                                                                            stroke-width="2"
                                                                            stroke-linecap="round"
                                                                        >
                                                                            <line x1="5" y1="12" x2="19" y2="12" />
                                                                            <polyline points="12 5 19 12 12 19" />
                                                                        </svg>
                                                                        "Get Directions"
                                                                    </A>
                                                                }
                                                            }
                                                        })}
                                                </div>
                                            </div>

                                            // ── Event lain (arah marketplace) ─────────
                                            <section class="section ed-more">
                                                <h2 class="section-title">"Event Berkaitan"</h2>
                                                {move || {
                                                    let cur = slug.get();
                                                    let items = rel_items.get();
                                                    let loading = rel_loading.get();
                                                    let cards = items
                                                        .into_iter()
                                                        .filter(|e| e.slug != cur)
                                                        .map(|e| {
                                                            let ev = crate::web::state::events::event_to_explore_pub(
                                                                &e,
                                                            );
                                                            view! { <EventCardPub ev=ev /> }
                                                        })
                                                        .collect_view();
                                                    let shims = loading
                                                        .then(|| {
                                                            (0..2i32)
                                                                .map(|_| {
                                                                    // Shimmer kecil saat chunk berikutnya sedang
                                                                    // dimuat (append), konsisten dengan Explore.
                                                                    view! {
                                                                        <div
                                                                            class="shim"
                                                                            style="height:220px;border-radius:14px"
                                                                        ></div>
                                                                    }
                                                                })
                                                                .collect_view()
                                                        });
                                                    view! {
                                                        <div class="exp-mkt-grid ed-more-grid">
                                                            {cards}
                                                            {shims}
                                                        </div>
                                                    }
                                                        .into_any()
                                                }}
                                            </section>
                                        </div>
                                    </div>

                                    // ── Sticky footer: starting price + Secure Tickets ───
                                    // Otomatis slide-down (hilang) begitu user melewati kartu
                                    // venue — konten "Event Berkaitan" tampil tanpa halangan.
                                    <div class=move || {
                                        if past_dirs.get() {
                                            "sticky-footer ed-mobile-footer ed-footer-hidden"
                                        } else {
                                            "sticky-footer ed-mobile-footer"
                                        }
                                    }>
                                        <div class="ed-footer-starting">
                                            {move || {
                                                let qty: i32 = cart_ctx
                                                    .items
                                                    .with(|v| {
                                                        v.iter()
                                                            .filter(|i| i.event_id == ev_id_price)
                                                            .map(|i| i.quantity)
                                                            .sum()
                                                    });
                                                if qty > 0 {
                                                    view! {
                                                        <span class="footer-label">"IN CART"</span>
                                                        <span class="footer-cart-qty">
                                                            {format!(
                                                                "{} TICKET{}",
                                                                qty,
                                                                if qty == 1 { "" } else { "S" },
                                                            )}
                                                        </span>
                                                    }
                                                        .into_any()
                                                } else {
                                                    view! {
                                                        <span class="footer-label">"STARTING FROM"</span>
                                                        <span class="starting-price">
                                                            {format_price(base_price)}
                                                        </span>
                                                    }
                                                        .into_any()
                                                }
                                            }}
                                        </div>
                                        {move || {
                                            // Secure Tickets → buka sheet pilih varian dulu
                                            // (bukan langsung /cart); lanjut ke keranjang
                                            // lewat CTA di dalam sheet.
                                            if is_logged_in() {
                                                view! {
                                                    <button
                                                        class="ed-secure-btn"
                                                        on:click=move |_| tickets_sheet.set(true)
                                                    >
                                                        "Secure Tickets"
                                                    </button>
                                                }
                                                    .into_any()
                                            } else {
                                                view! {
                                                    <a href="/login" class="ed-secure-btn">
                                                        "Masuk untuk Beli"
                                                    </a>
                                                }
                                                    .into_any()
                                            }
                                        }}
                                    </div>

                                    // ── Bottom sheet: pilih tiket (varian) ────
                                    <div
                                        class="edsheet"
                                        class:edsheet--open=move || tickets_sheet.get()
                                    >
                                        <div
                                            class="edsheet-backdrop"
                                            on:click=move |_| tickets_sheet.set(false)
                                        ></div>
                                        <div class="edsheet-panel">
                                            <div class="edsheet-grip"></div>
                                            <div class="edsheet-head">
                                                <span class="edsheet-title">"Pilih Tiket"</span>
                                                <button
                                                    class="edsheet-close"
                                                    aria-label="Tutup"
                                                    on:click=move |_| tickets_sheet.set(false)
                                                >
                                                    "\u{2715}"
                                                </button>
                                            </div>
                                            <div class="edsheet-body">{tiers_view}</div>
                                            // CTA lanjut ke keranjang — flow: Secure
                                            // Tickets → pilih varian di sheet → /cart.
                                            <div class="edsheet-foot">
                                                {move || {
                                                    let ti: i32 = cart_ctx
                                                        .items
                                                        .with(|v| {
                                                            v.iter()
                                                                .filter(|i| i.event_id == ev_id_btn)
                                                                .map(|i| i.quantity)
                                                                .sum()
                                                        });
                                                    if ti > 0 {
                                                        view! {
                                                            <A href="/cart" attr:class="edsheet-mch-cta">
                                                                {format!(
                                                                    "Lanjut ke Keranjang ({ti} tiket)",
                                                                )}
                                                            </A>
                                                        }
                                                            .into_any()
                                                    } else {
                                                        view! {
                                                            <span class="edsheet-mch-cta edsheet-cta--disabled">
                                                                "Pilih tiket dulu"
                                                            </span>
                                                        }
                                                            .into_any()
                                                    }
                                                }}
                                            </div>
                                        </div>
                                    </div>

                                    // ── Bottom sheet: info merchant ───────────
                                    <div
                                        class="edsheet"
                                        class:edsheet--open=move || merchant_sheet.get()
                                    >
                                        <div
                                            class="edsheet-backdrop"
                                            on:click=move |_| merchant_sheet.set(false)
                                        ></div>
                                        <div class="edsheet-panel">
                                            <div class="edsheet-grip"></div>
                                            <div class="edsheet-head">
                                                <span class="edsheet-title">"Penyelenggara"</span>
                                                <button
                                                    class="edsheet-close"
                                                    aria-label="Tutup"
                                                    on:click=move |_| merchant_sheet.set(false)
                                                >
                                                    "\u{2715}"
                                                </button>
                                            </div>
                                            <div class="edsheet-body">
                                                {
                                                    // Data sudah di tangan (ikut payload event) —
                                                    // render statis, tanpa loading state.
                                                    let header = sheet_merchant
                                                        .as_ref()
                                                        .and_then(|m| m.header_url.clone())
                                                        .unwrap_or_default();
                                                    let logo = sheet_merchant
                                                        .as_ref()
                                                        .and_then(|m| m.logo_url.clone())
                                                        .unwrap_or_default();
                                                    let desc = sheet_merchant
                                                        .as_ref()
                                                        .and_then(|m| m.description.clone())
                                                        .unwrap_or_default();
                                                    let verified = sheet_merchant
                                                        .as_ref()
                                                        .map(|m| m.verified)
                                                        .unwrap_or(false);
                                                    let followers = sheet_merchant
                                                        .as_ref()
                                                        .map(|m| m.followers)
                                                        .unwrap_or(0);
                                                    let events_count = sheet_merchant
                                                        .as_ref()
                                                        .map(|m| m.events_count)
                                                        .unwrap_or(0);
                                                    let rating_avg = sheet_merchant
                                                        .as_ref()
                                                        .map(|m| m.rating_avg)
                                                        .unwrap_or(0.0);
                                                    let rating_count = sheet_merchant
                                                        .as_ref()
                                                        .map(|m| m.rating_count)
                                                        .unwrap_or(0);
                                                    let initial: String = organizer_name
                                                        .chars()
                                                        .next()
                                                        .unwrap_or('P')
                                                        .to_uppercase()
                                                        .to_string();
                                                    view! {
                                                        <div class="edsheet-mch">
                                                            {(!header.is_empty())
                                                                .then(|| {
                                                                    view! {
                                                                        <div class="edsheet-mch-hero">
                                                                            <img src=header.clone() alt="" loading="lazy" />
                                                                        </div>
                                                                    }
                                                                })}
                                                            <div class="edsheet-mch-head">
                                                                {if logo.is_empty() {
                                                                    view! {
                                                                        <span class="edsheet-mch-avatar edsheet-mch-avatar--fallback">
                                                                            {initial}
                                                                        </span>
                                                                    }
                                                                        .into_any()
                                                                } else {
                                                                    view! {
                                                                        <img
                                                                            class="edsheet-mch-avatar"
                                                                            src=logo.clone()
                                                                            alt="Logo merchant"
                                                                        />
                                                                    }
                                                                        .into_any()
                                                                }}
                                                                <span class="edsheet-mch-name">
                                                                    {organizer_name.clone()}
                                                                    {verified
                                                                        .then(|| {
                                                                            view! {
                                                                                <span
                                                                                    class="edsheet-mch-verified"
                                                                                    title="Terverifikasi"
                                                                                >
                                                                                    "\u{2713}"
                                                                                </span>
                                                                            }
                                                                        })}
                                                                </span>
                                                            </div>
                                                            <div class="edsheet-mch-stats">
                                                                <span>
                                                                    <b>{crate::web::pages::merchant_public::fmt_count(followers)}</b>
                                                                    " Followers"
                                                                </span>
                                                                <span>
                                                                    <b>{events_count}</b>
                                                                    " Events"
                                                                </span>
                                                                <span>
                                                                    <b>{format!("{rating_avg:.1}")}</b>
                                                                    " \u{2605} ("
                                                                    {rating_count}
                                                                    ")"
                                                                </span>
                                                            </div>
                                                            {(!desc.is_empty())
                                                                .then(|| {
                                                                    view! { <p class="edsheet-mch-desc">{desc.clone()}</p> }
                                                                })}
                                                            <A
                                                                href=organizer_href.clone()
                                                                attr:class="edsheet-mch-cta"
                                                            >
                                                                "Kunjungi Profil"
                                                            </A>
                                                        </div>
                                                    }
                                                }
                                            </div>
                                        </div>
                                    </div>
                                }
                                    .into_any()
                            }
                        }
                    })
                }}
            </Suspense>

            // Viewer fullscreen story penyelenggara (dibuka via lingkaran story).
            <StoryViewer />
        </div>
    }
}
