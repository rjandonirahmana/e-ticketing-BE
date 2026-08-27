//! product_detail.rs — Halaman Detail Product (SSR + hydration).
//!
//! Cart-based ticket selection matching CSR design:
//! Add button → qty ctrl, footer subtotal → Secure Tickets → /cart.

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};
use crate::web::api::{get_product_detail, get_products};
use crate::web::app::{AuthResource, CartContext};
use crate::web::components::story_viewer::StoryViewer;
use crate::web::components::{CartButton, ProductCardPub, LiveStreamViewer, MerchantLivePip};
use crate::web::hooks::ThemeToggle;
use crate::web::models::{CartItem, format_date, format_price};
use crate::web::seo::SeoMeta;

#[component]
pub fn ProductDetailPage() -> impl IntoView {
    let params    = use_params_map();
    let slug      = Memo::new(move |_| params.read().get("slug").unwrap_or_default());
    // ── `new_blocking` HANYA BERARTI DI SERVER ───────────────────────────────
    // Flag `blocking` sebuah Resource dibaca leptos SATU tempat saja, dan tempat
    // itu ada di balik `#[cfg(feature = "ssr")]`:
    //
    //     // leptos_server-0.8.7/src/resource.rs:334
    //     #[cfg(feature = "ssr")]
    //     if let Some(shared_context) = shared_context {
    //         if blocking { shared_context.defer_stream(...); }
    //
    // Parameternya bahkan ditandai `#[allow(unused)] // this is used with
    // feature = "ssr"`. Artinya di bundel WASM `Resource::new_blocking` dan
    // `Resource::new` menghasilkan perilaku yang PERSIS SAMA.
    //
    // Versi sebelumnya memecah baris ini dengan `#[cfg(feature = "ssr")]` /
    // `#[cfg(not(...))]` untuk "membuat perpindahan halaman seketika". Pemecahan
    // itu tidak mengubah apa pun — kedua cabang berkompilasi menjadi kode klien
    // yang sama — dan gejala yang hendak diobatinya tetap ada. Akar sebenarnya
    // ada di `web/app/guards.rs` (navigasi kedua di tengah pembangunan view);
    // baca catatan panjang di sana.
    //
    // Blocking tetap dipakai karena SSR memang membutuhkannya: HTML pertama
    // harus sudah berisi isi halaman, untuk SEO dan agar kunjungan langsung tak
    // berkedip dari kerangka ke konten.
    let product_res = Resource::new_blocking(move || slug.get(), |s| get_product_detail(s));
    // Kategori product ini → dipakai untuk (a) mencari product BERKAITAN, dan
    // (b) mencatat minat user untuk rekomendasi "Untuk Kamu".
    let rel_cat = Memo::new(move |_| {
        product_res
            .get()
            .and_then(|r| r.ok())
            .and_then(|ev| ev.category.first().cloned())
    });
    // ── Product Berkaitan: daftar inkremental (LIMIT/OFFSET) ────────────────────
    // Dulu fetch 24 item sekaligus. Sekarang chunk kecil per halaman
    // (page/per_page get_products = LIMIT/OFFSET di DB); chunk berikutnya diminta
    // otomatis saat user scroll mendekati ujung halaman.
    const REL_PAGE_SIZE: i64 = 12;
    let rel_items: RwSignal<Vec<crate::web::models::Product>> = RwSignal::new(Vec::new());
    let rel_page = RwSignal::new(1i64);
    let rel_has_more = RwSignal::new(false);
    let rel_loading = RwSignal::new(false);

    // Muat satu halaman berikutnya & APPEND. Guard rangkap (loading/has_more)
    // agar aman dipanggil bertubi-tubi dari product scroll tanpa fetch ganda.
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
            if let Ok(res) = get_products(Some(next), None, cat, None, Some(REL_PAGE_SIZE)).await {
                rel_has_more.set(res.page < res.total_pages);
                rel_page.set(next + 1);
                rel_items.update(|v| v.extend(res.data));
            }
            rel_loading.set(false);
        });
    };

    // Reset + muat halaman pertama begitu detail product termuat / slug berganti.
    Effect::new(move |_| {
        if product_res.get().map(|r| r.is_ok()) != Some(true) {
            return;
        }
        let _ = rel_cat.get(); // ikut re-run bila kategori (slug) berubah
        rel_items.set(Vec::new());
        rel_page.set(1);
        rel_has_more.set(false);
        load_rel();
    });

    // Footer beli disembunyikan begitu user scroll melewati kartu venue
    // ("Get Directions" = bagian detail terakhir) → area "Produk Berkaitan"
    // bersih tanpa tombol beli menutupi konten.
    let past_dirs = RwSignal::new(false);

    // Peta venue asli (OpenStreetMap) langsung dirender di detail product bila
    // koordinat tersedia — init post-hydration (pola sama dgn venue_location).
    Effect::new(move |_| {
        if let Some(Ok(ev)) = product_res.get() {
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
    // Rekomendasi implisit (tanpa "like"): catat kategori product yang dibuka user
    // ke localStorage. Effect hanya jalan di client saat detail sudah termuat.
    Effect::new(move |_| {
        if let Some(Ok(ev)) = product_res.get() {
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
    // slide-up/fade berjalan (lihat styles/parts/39-product-sheets.css).
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
        let Some(Ok(ev)) = product_res.get() else { return };
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
    // Info merchant TIDAK di-fetch terpisah: sudah ikut payload detail product
    // (`ev.merchant`, JOIN + agregat satu query di server) — sheet render
    // langsung dari data yang ada, tanpa fetch kedua & tanpa loading state.

    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    // Id pengguna yang sedang masuk — dipakai membandingkan dengan pemilik
    // produk. Dipisah jadi closure sendiri supaya pembacaan `auth` tetap satu
    // tempat; membacanya langsung di tengah markup membuat setiap bagian
    // halaman ikut berlangganan perubahan sesi tanpa alasan.
    let my_id = move || {
        auth.get()
            .and_then(|r| r.ok())
            .flatten()
            .map(|u| u.id)
            .unwrap_or_default()
    };

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
                        match product_res.await {
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
                                            "PRODUK TIDAK DITEMUKAN"
                                        </h2>
                                        <p style="color:var(--text-muted);font-size:.85rem;margin-bottom:20px">
                                            "Produk ini mungkin sudah tidak dijual atau telah dihapus."
                                        </p>
                                        <A href="/explore" attr:class="tier-add-btn">
                                            "JELAJAHI PRODUK"
                                        </A>
                                    </div>
                                }
                                    .into_any()
                            }
                            Ok(ev) => {
                                let title = ev.name.clone();
                                let cover = ev.cover_url.clone().unwrap_or_default();
                                // Titik fokus cover → `object-position`. Hero ini
                                // jauh lebih lebar daripada kartu di grid, jadi
                                // justru di sinilah potongan "selalu tengah"
                                // paling sering memenggal kepala orang atau judul
                                // poster. Data lama (nilai kosong) jatuh ke tengah
                                // — sama persis dengan perilaku sebelumnya.
                                let cover_pos = {
                                    let f = ev.cover_focus.trim();
                                    let f = if f.is_empty() { "50% 50%" } else { f };
                                    format!("object-position:{f}")
                                };

                                // ── Slide galeri produk ──────────────────────────
                                // Cover lebih dulu, lalu foto detail sesuai urutan
                                // yang disusun merchant. Urutan itu bukan sekadar
                                // rapi-rapi: slide pertama adalah satu-satunya yang
                                // pasti dilihat setiap pembeli.
                                //
                                // `image_type` (denah lokasi / peta kursi / info
                                // harga) sengaja TIDAK lagi dibaca di sini. Itu
                                // konsep dari masa aplikasi ini menjual tiket acara;
                                // untuk sebuah barang, setiap foto adalah foto
                                // produk dan semuanya masuk galeri yang sama.
                                let slides: Vec<(String, String)> = {
                                    let mut v: Vec<(String, String)> = Vec::new();
                                    if !cover.is_empty() {
                                        v.push((cover.clone(), cover_pos.clone()));
                                    }
                                    for d in ev.detail_images.iter() {
                                        if d.url.trim().is_empty() {
                                            continue;
                                        }
                                        let f = d.focus.trim();
                                        let f = if f.is_empty() { "50% 50%" } else { f };
                                        v.push((d.url.clone(), format!("object-position:{f}")));
                                    }
                                    v
                                };
                                let slide_count = slides.len();
                                // `>` dihitung DI SINI, bukan di dalam `view!` —
                                // parser makro memperlakukan `>` di dalam markup
                                // sebagai penutup tag (lihat catatan yang sama di
                                // `components/variant_editor.rs`).
                                let banyak_slide = slide_count > 1;
                                let slide_aktif = RwSignal::new(0usize);

                                // Indeks slide dihitung dari posisi gulir, bukan
                                // dari tombol: yang menggeser adalah jari, dan
                                // scroll-snap-lah yang memutuskan slide mana yang
                                // berhenti di tengah.
                                let on_slide_scroll = move |_ev: leptos::ev::Event| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        use wasm_bindgen::JsCast;
                                        if let Some(el) = _ev
                                            .target()
                                            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                                        {
                                            let lebar = el.client_width();
                                            if lebar != 0 {
                                                let idx = (el.scroll_left() as f64 / lebar as f64)
                                                    .round()
                                                    .max(0.0)
                                                    as usize;
                                                // Dijaga: `set` tanpa perbandingan
                                                // akan menjalankan ulang efek titik
                                                // indikator pada SETIAP piksel gulir.
                                                if slide_aktif.get_untracked() != idx {
                                                    slide_aktif.set(idx);
                                                }
                                            }
                                        }
                                    }
                                };
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
                                let variants = ev.product_variants.clone();
                                let has_coords = ev.latitude.is_some() && ev.longitude.is_some();
                                let is_live = ev.status.eq_ignore_ascii_case("live");
                                let live_room_id = format!("live_{}", ev.merchant_id);
                                let organizer_href = format!("/m/{}", ev.merchant_id);

                                // Tautan chat: alamatnya cukup dari `merchant_id`,
                                // jadi tak ada panggilan server sama sekali di sini.
                                let chat_href = format!("/pulse/toko/{}", ev.merchant_id);

                                // ── PRODUK MILIK SENDIRI ────────────────────────
                                // Merchant yang membuka produknya sendiri tak punya
                                // lawan bicara. Server memang sudah menolaknya
                                // (`ensure_dm` gagal bila kedua id sama), tapi
                                // penolakan itu baru datang sesudah pesan diketik dan
                                // dikirim — jauh lebih terlambat daripada perlu.
                                let pemilik_id = ev.merchant_id.clone();
                                let milik_sendiri = Memo::new(move |_| {
                                    let me = my_id();
                                    !me.is_empty() && me == pemilik_id
                                });
                                let organizer_name = ev
                                    .merchant_name
                                    .clone()
                                    .filter(|n| !n.is_empty())
                                    .unwrap_or_else(|| "Toko".to_string());
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
                                        params.append("product_desc", &_share_desc);
                                        params.append("event_date", &_share_date_str);
                                        params.append("event_venue", &_share_venue);
                                        params.append("product_price", &_share_price_str);
                                        if let Some(win) = web_sys::window() {
                                            if let Ok(Some(storage)) = win.session_storage() {
                                                let _ = storage.set_item("story_hero_transition", "produk");
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
                                        // product lama belum punya nama ter-join).

                                        // Live streaming: badge + embedded viewer hanya tampil saat
                                        // product berstatus "live". room_id mengikuti format SFU.
                                        // Link profil merchant publik (penyelenggara) — /m/{id}.
                                        // Social proof (gaya marketplace): terjual + sisa stok.
                                        // Sumber data asli: agregasi SUM(sold)/SUM(quota) dari
                                        // product_variants (ter-update tiap order via order.rs).

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
                                    if desc.is_empty() { "Produk pilihan di Indonesia" } else { &desc },
                                    venue_str,
                                    date_str,
                                );
                                let seo_path = format!("/products/{}", ev_slug);
                                let seo_image = cover.clone();
                                let ld_product = crate::web::seo::safe_ld(
                                    &serde_json::json!(
                                        {
                                        "@context": "https://schema.org",
                                        "@type": "Produk",
                                        "name": ev.name.clone(),
                                        "description": (!desc.is_empty()).then(|| desc.clone()),
                                        "startDate": ev.event_date.to_rfc3339(),
                                        "endDate": ev.end_time.map(|d| d.to_rfc3339()),
                                        "productStatus": "https://schema.org/EventScheduled",
                                        "productAttendanceMode": "https://schema.org/OfflineEventAttendanceMode",
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
                                    <Script type_="application/ld+json">{ld_product}</Script>
                                    // Overlay live melayang: muncul saat merchant pemilik
                                    // product ini sedang siaran (room SFU benar-benar ada).
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
                                            <CartButton />
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
                                        </div>
                                    </header>

                                    // ── Hero: GALERI YANG BISA DIGESER ───────────────────
                                    // Dulu satu foto mati (cover saja). Foto detail
                                    // yang diunggah merchant tersimpan di
                                    // `products.detail_images` dan dikirim API —
                                    // tapi TIDAK PERNAH dirender di mana pun, jadi
                                    // pembeli tak pernah melihatnya. Ini yang
                                    // menampilkannya.
                                    //
                                    // Geser memakai scroll-snap NATIF, bukan
                                    // pustaka carousel dan bukan JS. Konsekuensinya
                                    // penting: galerinya sudah bisa digeser sejak
                                    // HTML server tiba, sebelum WASM diunduh —
                                    // sedangkan carousel ber-JS baru hidup setelah
                                    // hidrasi, dan pada kunjungan dingin itu berarti
                                    // beberapa detik pertama foto tak bisa digeser
                                    // sama sekali. Hidrasi di sini hanya menambah
                                    // titik indikator.
                                    <div class="ed-hero">
                                        {if slides.is_empty() {
                                            view! {
                                                <div
                                                    class="ed-hero-img"
                                                    style="background:var(--bg-elevated)"
                                                />
                                            }
                                                .into_any()
                                        } else {
                                            view! {
                                                <div
                                                    class="absolute inset-0 flex overflow-x-auto \
                                                           snap-x snap-mandatory no-scrollbar"
                                                    on:scroll=on_slide_scroll
                                                >
                                                    {slides
                                                        .iter()
                                                        .map(|(url, pos)| {
                                                            view! {
                                                                <img
                                                                    src=url.clone()
                                                                    alt=title.clone()
                                                                    // `flex-none w-full`: tanpa
                                                                    // keduanya flexbox memampatkan
                                                                    // semua slide ke dalam satu
                                                                    // layar dan tak ada yang bisa
                                                                    // digeser.
                                                                    class="flex-none w-full h-full \
                                                                           snap-start object-cover"
                                                                    style=pos.clone()
                                                                    // Foto pertama dimuat segera;
                                                                    // sisanya menunggu digeser —
                                                                    // galeri sepuluh foto kalau
                                                                    // tidak akan mengunduh
                                                                    // semuanya pada muat pertama.
                                                                    loading="lazy"
                                                                    decoding="async"
                                                                />
                                                            }
                                                        })
                                                        .collect_view()}
                                                </div>
                                            }
                                                .into_any()
                                        }}
                                        // Titik indikator — hanya bila memang ada
                                        // lebih dari satu foto.
                                        {banyak_slide
                                            .then(|| {
                                                view! {
                                                    <div class="absolute bottom-3 left-1/2 -translate-x-1/2 z-20 \
                                                                flex items-center gap-1.5">
                                                        {(0..slide_count)
                                                            .map(|i| {
                                                                view! {
                                                                    <span class=move || {
                                                                        if slide_aktif.get() == i {
                                                                            "w-4 h-1.5 rounded-full bg-white transition-all"
                                                                        } else {
                                                                            "w-1.5 h-1.5 rounded-full bg-white/50 transition-all"
                                                                        }
                                                                    } />
                                                                }
                                                            })
                                                            .collect_view()}
                                                    </div>
                                                }
                                            })}
                                        <div class="ed-hero-gradient"></div>
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
                                            //
                                            // `px-5` = 20px, angka yang sama dengan
                                            // `.section` di `08-page-event-detail.css` dan
                                            // dengan baris toko+chat di bawah. `.ed-social-proof`
                                            // sendiri hanya menyetel padding VERTIKAL
                                            // (`12px 0 14px`), dan induknya `.ed-main` tak
                                            // punya padding sama sekali — jadi strip ini
                                            // menempel ke tepi kolom sementara seluruh
                                            // tetangganya menjorok.
                                            //
                                            // Garis bawahnya ikut menjorok, dan itu memang
                                            // yang diinginkan: pemisah yang membentang penuh
                                            // sementara isinya menjorok akan terbaca sebagai
                                            // dua sistem tata letak yang berbeda.
                                            <div class="ed-social-proof px-5">
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
                                                                                "🔥 Segera habis — sisa "<b>{remaining}</b>" barang"
                                                                            </span>
                                                                        }
                                                                            .into_any()
                                                                    } else {
                                                                        view! {
                                                                            <span>
                                                                                "Sisa "<b>{remaining}</b>" dari "{quota}" barang"
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
                                            // rating, follower, dan semua product si penyelenggara.
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
                                                                aria-label="Lihat story toko"
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

                                            // Kartu toko dan ikon chat SEBARIS.
                                            //
                                            // Keduanya tak bisa disarangkan — kartu toko
                                            // sudah `<button>` (membuka bottom sheet), dan
                                            // tombol di dalam tombol adalah HTML tak sah
                                            // yang perilaku kliknya berbeda-beda antar
                                            // peramban. Jadi bersaudara di dalam flex.
                                            //
                                            // `flex-1` + `min-w-0` pada kartunya: tanpa
                                            // `min-w-0`, nama toko yang panjang menolak
                                            // mengecil dan mendorong ikon chat keluar dari
                                            // kolom 480px.
                                            //
                                            // `items-stretch`, BUKAN `items-center`:
                                            // itulah yang membuat ikon chat setinggi kartu
                                            // toko tanpa satu pun angka ajaib. Tingginya
                                            // ikut apa pun isi kartunya — nama toko yang
                                            // membungkus ke dua baris tak akan membuat
                                            // keduanya jadi berbeda tinggi.
                                            //
                                            // Margin `14px 0 4px` milik `.ed-organizer`
                                            // dipindah ke pembungkus dan dimatikan di
                                            // kartunya (`m-0`). Kalau dibiarkan, kotak
                                            // margin kartu jadi lebih tinggi dari kotak
                                            // borderna, dan `stretch` menarik ikon chat
                                            // melewati tepi kartu — sejajar di angka,
                                            // meleset di mata.
                                            //
                                            // `m-0` menang atas CSS lama karena utility
                                            // Tailwind kini berada di layer SESUDAH
                                            // `legacy` (lihat build.rs & style/tailwind.css).
                                            //
                                            // `px-5` = 20px, ANGKA YANG SAMA dengan
                                            // `.section { padding: … 20px … }` di
                                            // `08-page-event-detail.css`. Induknya
                                            // (`.ed-main`) sendiri tak punya padding, jadi
                                            // baris ini satu-satunya yang menempel ke tepi
                                            // kolom sementara semua tetangganya menjorok —
                                            // itulah yang membuatnya terlihat lepas.
                                            //
                                            // Kalau suatu saat angka di `.section` diubah,
                                            // ubah juga di sini: keduanya harus sama, dan
                                            // tak ada yang memaksanya.
                                            <div class="flex items-stretch gap-3 mt-3.5 mb-1 px-5">
                                            <button
                                                class="ed-organizer flex-1 min-w-0 m-0"
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
                                                        "Lihat profil, rating & product lainnya"
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

                                            // ── Chat penjual ────────────────────────
                                            // Menggantikan grup-otomatis-setelah-beli.
                                            // Ditaruh TEPAT di bawah kartu toko, bukan di
                                            // dekat tombol beli: yang membuka percakapan
                                            // biasanya orang yang BELUM yakin, dan
                                            // pertanyaannya tentang toko itu — stok,
                                            // ukuran, ongkir, kapan bisa diambil.
                                            //
                                            // Tak mensyaratkan pernah membeli. Pertanyaan
                                            // seperti itu justru datang sebelum orang
                                            // memutuskan, dan mensyaratkan pesanan lebih
                                            // dulu menutup percakapan tepat saat ia paling
                                            // berguna.
                                            // ── IKON CHAT SAJA ─────────────────────
                                            // Sebelumnya baris penuh dengan judul dan
                                            // keterangan. Di halaman yang sudah padat,
                                            // baris sebesar itu untuk satu aksi kecil
                                            // mendorong isi produk semakin ke bawah —
                                            // dan keterangannya menjelaskan sesuatu yang
                                            // sudah jelas dari ikonnya.
                                            {move || if milik_sendiri.get() {
                                                // Produk sendiri: ikon dihilangkan, diganti
                                                // penanda kecil. Ikon chat yang ada tapi mati
                                                // selalu terbaca sebagai rusak.
                                                view! {
                                                    <span class="self-stretch inline-flex items-center px-3 \
                                                                 rounded-[14px] bg-elevated border border-solid \
                                                                 border-line text-[10px] font-bold \
                                                                 tracking-[0.08em] text-content-muted \
                                                                 whitespace-nowrap">
                                                        "PRODUKMU"
                                                    </span>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    // `<A>`, BUKAN `<button>` ber-`on:click`.
                                                    //
                                                    // Sejak room tak lagi dibuat saat diklik,
                                                    // tak ada yang perlu ditunggu — jadi tak
                                                    // ada pula alasan memakai tombol yang
                                                    // menjalankan JS. Sebagai tautan sungguhan
                                                    // ia sudah bisa diklik SEBELUM WASM turun,
                                                    // bisa dibuka di tab baru, dan bisa disalin
                                                    // alamatnya.
                                                    //
                                                    // Cincin berputarnya ikut hilang bersama
                                                    // penantiannya: penantiannya pindah ke
                                                    // halaman tujuan (memuat riwayat), dan
                                                    // penandanya pindah ke sana juga.
                                                    <A
                                                        href=chat_href.clone()
                                                        attr:class="relative self-stretch inline-flex items-center \
                                                                    justify-center w-14 shrink-0 rounded-[14px] \
                                                                    bg-elevated border border-solid border-line \
                                                                    text-content transition-colors \
                                                                    hover:bg-card-hover active:scale-95"
                                                        attr:aria-label="Chat penjual"
                                                        attr:title="Chat penjual"
                                                    >
                                                        <svg width="19" height="19" viewBox="0 0 24 24"
                                                             fill="none" stroke="currentColor"
                                                             stroke-width="2" stroke-linecap="round"
                                                             stroke-linejoin="round">
                                                            <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
                                                        </svg>
                                                    </A>
                                                }.into_any()
                                            }}
                                            </div>

                                            // Live stream (tampil saat product sedang live)
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
                                                                <p class="ed-section-eyebrow">"TENTANG PRODUK"</p>
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
                                                <span class="ed-tickets-title">"Pilih Varian"</span>
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
                                                        "Pilih Varian"
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
                                                <h2 class="section-title">"Lokasi"</h2>
                                            </section>
                                            <div class="map-card" id="ed-venue-card">
                                                // Peta OpenStreetMap asli langsung tampil bila product
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
                                                                        href=format!("/products/{}/location", es)
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

                                            // ── Product lain (arah marketplace) ─────────
                                            <section class="section ed-more">
                                                <h2 class="section-title">"Produk Berkaitan"</h2>
                                                {move || {
                                                    let cur = slug.get();
                                                    let items = rel_items.get();
                                                    let loading = rel_loading.get();
                                                    let cards = items
                                                        .into_iter()
                                                        .filter(|e| e.slug != cur)
                                                        .map(|e| {
                                                            let ev = crate::web::state::products::product_to_explore_pub(
                                                                &e,
                                                            );
                                                            view! { <ProductCardPub ev=ev /> }
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
                                    // venue — konten "Produk Berkaitan" tampil tanpa halangan.
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
                                                                "{} BARANG{}",
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
                                                        "Lanjut Belanja"
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
                                                <span class="edsheet-title">"Pilih Varian"</span>
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
                                                                    "Lanjut ke Keranjang ({ti} barang)",
                                                                )}
                                                            </A>
                                                        }
                                                            .into_any()
                                                    } else {
                                                        view! {
                                                            <span class="edsheet-mch-cta edsheet-cta--disabled">
                                                                "Pilih varian dulu"
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
                                                <span class="edsheet-title">"Toko"</span>
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
                                                    // Data sudah di tangan (ikut payload product) —
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
                                                    let products_count = sheet_merchant
                                                        .as_ref()
                                                        .map(|m| m.products_count)
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
                                                                    <b>{products_count}</b>
                                                                    " Produk"
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
