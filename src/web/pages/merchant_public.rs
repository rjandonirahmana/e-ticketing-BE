//! merchant_public.rs — Profil merchant publik (/m/:id), sisi user.
//!
//! Hero cover (dari product terbaru), avatar/logo, tombol Follow, statistik
//! (followers / products / rating → klik rating ke halaman reviews), dan panel
//! yang bisa DIGESER (swipe horizontal / klik tab): EVENTS · TENTANG · ULASAN ·
//! STORY. Story merchant = story user pemilik (buka viewer via StoryViewer).
//! Entry point: tombol penyelenggara di product detail & chip penyelenggara di
//! kartu explore.

use leptos::html::Div;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::web::api::{get_merchant_public_products, get_merchant_public_page, set_follow_merchant};
use crate::web::app::AuthResource;
use crate::web::components::story_viewer::StoryViewer;
use crate::web::components::{ProductGrid, ProductGridShimmer};
use crate::web::hooks::ThemeToggle;
use crate::web::seo::SeoMeta;
use crate::web::state::stories::{use_stories_store, StoryMediaType};
use leptos_meta::Script;

/// Timestamp milidetik (performance.now) untuk hitung kecepatan swipe (flick).
/// No-op di server (0.0) — kode ini hanya berjalan di jalur pointer wasm.
/// `pub(crate)`: dipakai ulang oleh swipe panel di `user_public.rs`.
pub(crate) fn now_ms() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.0
    }
}

/// 12500 → "12.5k", 999 → "999".
pub(crate) fn fmt_count(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}jt", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Baris bintang statis untuk panel ulasan in-page (mirror dari reviews.rs).
#[component]
fn Stars(#[prop(into)] rating: f64) -> impl IntoView {
    view! {
        <span class="mrv-stars" aria-label=format!("{rating:.1} dari 5")>
            {(1..=5)
                .map(|i| {
                    let cls = if (i as f64) <= rating + 0.25 {
                        "mrv-star mrv-star--on"
                    } else {
                        "mrv-star"
                    };
                    view! { <span class=cls>"★"</span> }
                })
                .collect_view()}
        </span>
    }
}

/// Skeleton grid story 9:16 (dipakai panel STORY di /m/{id} dan /u/{id}).
#[component]
pub(crate) fn StoryGridShimmer() -> impl IntoView {
    view! {
        <div class="mp-story-grid">
            {(0..6)
                .map(|_| {
                    view! {
                        <div
                            class="shimmer-bg"
                            style="aspect-ratio:9/16;border-radius:10px;"
                        ></div>
                    }
                })
                .collect_view()}
        </div>
    }
}

/// Skeleton daftar ulasan (avatar-nama + dua baris teks) selama loading.
#[component]
pub(crate) fn ReviewListShimmer() -> impl IntoView {
    view! {
        <div class="mrv-list">
            {(0..3)
                .map(|_| {
                    view! {
                        <div style="display:flex;flex-direction:column;gap:9px;padding:14px 0;border-bottom:1px solid var(--border-soft)">
                            <div style="display:flex;align-items:center;gap:10px">
                                <div
                                    class="shimmer-bg"
                                    style="width:34px;height:34px;border-radius:50%;flex:none"
                                ></div>
                                <div
                                    class="shimmer-bg"
                                    style="width:38%;height:13px;border-radius:6px;"
                                ></div>
                            </div>
                            <div
                                class="shimmer-bg"
                                style="width:92%;height:12px;border-radius:6px;"
                            ></div>
                            <div
                                class="shimmer-bg"
                                style="width:64%;height:12px;border-radius:6px;"
                            ></div>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}

/// Skeleton profil merchant (hero + avatar + nama + statistik) selama loading.
#[component]
pub(crate) fn MerchantProfileShimmer() -> impl IntoView {
    view! {
        <div class="mp-hero shimmer-bg"></div>
        <div class="mp-head">
            <div class="mp-avatar-wrap">
                <div class="mp-avatar shimmer-bg"></div>
            </div>
            <div class="mp-head-actions">
                <div class="shimmer-bg" style="width:112px;height:42px;border-radius:999px;"></div>
                <div class="shimmer-bg" style="width:42px;height:42px;border-radius:50%;"></div>
            </div>
        </div>
        <div class="mp-container">
            <div
                class="shimmer-bg"
                style="width:62%;height:26px;border-radius:8px;margin-top:14px;"
            ></div>
            <div
                class="shimmer-bg"
                style="width:38%;height:14px;border-radius:6px;margin-top:10px;"
            ></div>
            <div class="mp-stats">
                {(0..3)
                    .map(|_| {
                        view! {
                            <div class="mp-stat">
                                <span
                                    class="shimmer-bg"
                                    style="width:44px;height:18px;border-radius:6px;"
                                ></span>
                                <span
                                    class="shimmer-bg"
                                    style="width:52px;height:9px;border-radius:4px;margin-top:6px;"
                                ></span>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
            <ProductGridShimmer count=4 />
        </div>
    }
}

#[component]
pub fn MerchantPublicPage() -> impl IntoView {
    let params = use_params_map();
    let mid = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");

    // SATU resource untuk seluruh halaman (profil + products + ulasan + story):
    // 1 round-trip HTTP dari klien, server join semua query paralel — dulu 4
    // POST /api-fn terpisah. Derived signal di bawah mempertahankan bentuk
    // `.get() -> Option<Result<T>>` yang sama sehingga view tak perlu berubah.
    let page_data = Resource::new(mid, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("not_ready".into()));
        }
        get_merchant_public_page(id).await
    });
    let profile = Signal::derive(move || page_data.get().map(|r| r.map(|d| d.profile)));
    let products = Signal::derive(move || page_data.get().map(|r| r.map(|d| d.products)));
    let reviews = Signal::derive(move || page_data.get().map(|r| r.map(|d| d.reviews)));
    let stories = Signal::derive(move || page_data.get().map(|r| r.map(|d| d.stories)));

    // ── Paginasi EVENTS (append "Muat lebih banyak") ────────────────────────────
    // `products` resource = halaman 1 (juga sumber hero/kota); halaman berikutnya
    // diambil terpisah & di-append ke `ev_extra`. Grid render = data page1 + extra.
    // ── Pencarian & urutan katalog toko ─────────────────────────────────────
    // `hasil_saring = None` berarti "tak ada saringan aktif" — grid memakai
    // jalur biasa (halaman-1 dari `page_data` + akumulasi `ev_extra`). Ini
    // disengaja: selama pengunjung tak mencari apa pun, halaman tetap secepat
    // sebelumnya dan tak ada satu permintaan tambahan pun.
    let cari = RwSignal::new(String::new());
    let urut = RwSignal::new(String::new());
    let hasil_saring: RwSignal<Option<Vec<crate::web::models::Product>>> = RwSignal::new(None);
    let saring_loading = RwSignal::new(false);

    let jalankan_saring = move || {
        let q = cari.get_untracked().trim().to_string();
        let u = urut.get_untracked();
        // Kembali ke jalur biasa saat kedua saringan kosong — bukan memanggil
        // server untuk meminta "semuanya", yang datanya sudah ada di tangan.
        if q.is_empty() && u.is_empty() {
            hasil_saring.set(None);
            return;
        }
        let id = mid();
        if id.is_empty() {
            return;
        }
        saring_loading.set(true);
        leptos::task::spawn_local(async move {
            let q_opt = (!q.is_empty()).then_some(q);
            let u_opt = (!u.is_empty()).then_some(u);
            if let Ok(pe) = get_merchant_public_products(id, Some(1), q_opt, u_opt).await {
                hasil_saring.set(Some(pe.data));
            }
            saring_loading.set(false);
        });
    };

    let ev_extra = RwSignal::new(Vec::<crate::web::models::Product>::new());
    let ev_page = RwSignal::new(1i64);
    let ev_total_pages = RwSignal::new(1i64);
    // Jumlah SELURUH product toko ini. Dipakai penanda kemajuan di bawah daftar
    // ("Menampilkan 12 dari 47"). Tanpa angka ini, "MUAT LEBIH BANYAK" tak
    // memberi tahu apa pun soal seberapa jauh lagi daftarnya — orang menekan
    // berulang kali tanpa tahu apakah tinggal satu product atau seratus.
    let ev_total = RwSignal::new(0i64);
    let ev_loading = RwSignal::new(false);
    // Saat merchant (mid) berganti → resource refetch page 1 → reset akumulasi.
    Effect::new(move |_| {
        if let Some(Ok(pe)) = products.get() {
            ev_total_pages.set(pe.total_pages);
            ev_total.set(pe.total);
            ev_extra.set(Vec::new());
            ev_page.set(1);
        }
    });
    let ev_has_more = move || ev_page.get() < ev_total_pages.get();
    // Berapa product yang SEDANG tampil: halaman pertama (dari resource) plus
    // yang sudah ditambahkan lewat "muat lebih banyak".
    let jml_tampil = move || {
        let hal1 = products
            .get()
            .and_then(|r| r.ok())
            .map(|pe| pe.data.len())
            .unwrap_or(0);
        (hal1 + ev_extra.with(|v| v.len())) as i64
    };
    // Tanpa argumen agar bisa dipanggil dari tombol DAN listener scroll (infinite
    // scroll ala /explore). Guard loading/total_pages → aman dipanggil berkali-
    // kali per product scroll tanpa fetch ganda.
    let do_load_more = move || {
        if ev_loading.get_untracked() {
            return;
        }
        let next = ev_page.get_untracked() + 1;
        if next > ev_total_pages.get_untracked() {
            return;
        }
        let id = mid();
        ev_loading.set(true);
        leptos::task::spawn_local(async move {
            if let Ok(pe) = get_merchant_public_products(id, Some(next), None, None).await {
                ev_extra.update(|v| v.extend(pe.data));
                ev_page.set(next);
            }
            ev_loading.set(false);
        });
    };
    let load_more_products = move |_| do_load_more();

    // State follow lokal (optimistic): diisi dari profile saat termuat.
    let following = RwSignal::new(false);
    let followers = RwSignal::new(0i64);
    let follow_init = RwSignal::new(false);
    Effect::new(move |_| {
        if let Some(Ok(p)) = profile.get() {
            if !follow_init.get_untracked() {
                following.set(p.is_following);
                followers.set(p.followers);
                follow_init.set(true);
            }
        }
    });

    // Feedback "tautan disalin" untuk tombol share.
    let share_ok = RwSignal::new(false);

    let follow_busy = RwSignal::new(false);
    let on_follow = move |_| {
        // Belum login → arahkan ke login (follow butuh identitas).
        let logged_in = auth
            .get_untracked()
            .and_then(|r| r.ok())
            .flatten()
            .is_some();
        if !logged_in {
            #[cfg(target_arch = "wasm32")]
            if let Some(w) = web_sys::window() {
                let _ = w.location().assign("/login");
            }
            return;
        }
        if follow_busy.get_untracked() {
            return;
        }
        let id = mid();
        let target = !following.get_untracked();
        // Optimistic update; rollback bila server gagal.
        following.set(target);
        followers.update(|f| *f += if target { 1 } else { -1 });
        follow_busy.set(true);
        leptos::task::spawn_local(async move {
            if set_follow_merchant(id, target).await.is_err() {
                following.set(!target);
                followers.update(|f| *f -= if target { 1 } else { -1 });
            }
            follow_busy.set(false);
        });
    };

    // Tab aktif: 0 = EVENTS, 1 = TENTANG, 2 = ULASAN, 3 = STORY.
    // Panel bisa "digeser" (swipe horizontal) antar-tab, atau lewat klik tab.
    const TAB_COUNT: usize = 4;
    let tab = RwSignal::new(0usize);

    // Infinite scroll ala /explore: listener "scroll" window, prefetch mulai
    // ~2.5 layar sebelum ujung dokumen. HANYA saat tab EVENTS aktif — scroll di
    // panel lain (ulasan/story) tak boleh memicu fetch products tersembunyi.
    // Closure DIPEGANG + di-remove & drop saat unmount (bukan .forget()).
    #[cfg(feature = "hydrate")]
    {
        use send_wrapper::SendWrapper;
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;

        let scroll_cb: StoredValue<Option<SendWrapper<Closure<dyn Fn()>>>> =
            StoredValue::new(None);
        Effect::new(move |_| {
            let cb = Closure::<dyn Fn()>::new(move || {
                if tab.get_untracked() != 0 {
                    return;
                }
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
                    do_load_more();
                }
            });
            if let Some(win) = web_sys::window() {
                let _ = win
                    .add_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref());
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

    // ── Swipe antar-panel (carousel: 4 panel selalu dirender) ───────────────────
    // Panel AKTIF `position:relative` → dialah yang menentukan tinggi container;
    // panel lain `position:absolute` (keluar flow) digeser ±100% via
    // translateX(calc(N% + Dpx)) sehingga tetangga "mengintip" saat drag dan
    // commit-nya meluncur mulus — tanpa perlu mengukur lebar/tinggi.
    // Ambang 45px ATAU flick (kecepatan) untuk memindah tab; sumbu dikunci pada
    // gerak pertama agar tak membajak scroll vertikal (didukung touch-action:pan-y).
    const SWIPE_PX: f64 = 45.0; // ambang jarak
    const SWIPE_VEL: f64 = 0.4; // ambang kecepatan px/ms (flick)
    let swipe_ref = NodeRef::<Div>::new();
    let drag_start = RwSignal::new(None::<(f64, f64)>);
    let drag_dx = RwSignal::new(0f64);
    let dragging = RwSignal::new(false);
    // 0 = belum terkunci, 1 = horizontal, 2 = vertikal.
    let drag_axis = RwSignal::new(0i8);
    let drag_t0 = RwSignal::new(0f64);

    let on_pointer_down = move |ev: leptos::ev::PointerEvent| {
        drag_start.set(Some((ev.client_x() as f64, ev.client_y() as f64)));
        drag_axis.set(0);
        drag_dx.set(0.0);
        drag_t0.set(now_ms());
    };
    let on_pointer_move = move |ev: leptos::ev::PointerEvent| {
        let Some((sx, sy)) = drag_start.get_untracked() else {
            return;
        };
        let dx = ev.client_x() as f64 - sx;
        let dy = ev.client_y() as f64 - sy;
        if drag_axis.get_untracked() == 0 {
            if dx.abs() > 8.0 || dy.abs() > 8.0 {
                if dx.abs() > dy.abs() {
                    drag_axis.set(1);
                    dragging.set(true);
                    if let Some(el) = swipe_ref.get_untracked() {
                        let _ = el.set_pointer_capture(ev.pointer_id());
                    }
                } else {
                    drag_axis.set(2);
                }
            }
        }
        if drag_axis.get_untracked() == 1 {
            let t = tab.get_untracked();
            // Tahanan di tepi (tak ada panel sebelum 0 / sesudah terakhir).
            let d = if (t == 0 && dx > 0.0) || (t == TAB_COUNT - 1 && dx < 0.0) {
                dx * 0.35
            } else {
                dx
            };
            drag_dx.set(d);
        }
    };
    let on_pointer_up = move |ev: leptos::ev::PointerEvent| {
        let was_h = drag_axis.get_untracked() == 1;
        drag_start.set(None);
        drag_axis.set(0);
        if was_h {
            if let Some(el) = swipe_ref.get_untracked() {
                if el.has_pointer_capture(ev.pointer_id()) {
                    let _ = el.release_pointer_capture(ev.pointer_id());
                }
            }
            let d = drag_dx.get_untracked();
            let dt = (now_ms() - drag_t0.get_untracked()).max(1.0);
            let vel = d / dt; // px/ms, bertanda (negatif = geser kiri)
            let t = tab.get_untracked();
            // Pindah bila lewat ambang jarak ATAU flick cepat.
            let go_next = d <= -SWIPE_PX || vel <= -SWIPE_VEL;
            let go_prev = d >= SWIPE_PX || vel >= SWIPE_VEL;
            if go_next && t < TAB_COUNT - 1 {
                tab.set(t + 1);
            } else if go_prev && t > 0 {
                tab.set(t - 1);
            }
        }
        dragging.set(false);
        drag_dx.set(0.0);
    };

    // Transform per panel: posisi dasar (i - tab)*100% + geseran drag (px).
    // calc() mencampur % (lebar panel) + px → tak perlu ukur lebar container.
    let panel_tf = move |i: usize| {
        let base = (i as f64 - tab.get() as f64) * 100.0;
        let dx = if dragging.get() { drag_dx.get() } else { 0.0 };
        format!("transform:translateX(calc({base}% + {dx}px))")
    };

    // Buka viewer story merchant pada indeks tertentu (login required).
    let ctx = use_stories_store();
    let navigate = use_navigate();
    let open_story = {
        move |list: Vec<crate::web::state::stories::StoryGroup>, idx: usize| {
            ctx.groups.set(list);
            ctx.open_at(0, idx);
        }
    };

    view! {
        <div class="mp-page">
            <header class="page-header mp-header">
                <A href="/explore" attr:class="back-btn" attr:aria-label="Kembali">
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
                <span class="page-logo">"PULSE"</span>
                <div class="header-actions">
                    <ThemeToggle />
                </div>
            </header>

            <Suspense fallback=|| {
                view! { <MerchantProfileShimmer /> }
            }>
                {move || {
                    match profile.get() {
                        None => view! { <MerchantProfileShimmer /> }.into_any(),
                        Some(Err(e)) if e.to_string().contains("not_ready") => {
                            view! { <MerchantProfileShimmer /> }.into_any()
                        }
                        Some(Err(_)) => {
                            view! {
                                <div class="mp-container">
                                    <div class="medit-error-banner">
                                        "Merchant tidak ditemukan."
                                    </div>
                                    <A href="/explore" attr:class="medit-cancel-btn">
                                        "← Kembali"
                                    </A>
                                </div>
                            }
                                .into_any()
                        }
                        Some(Ok(p)) => {
                            let merchant_id = p.merchant_id.clone();
                            let store_name = p.store_name.clone();
                            let logo = p.logo_url.clone().unwrap_or_default();
                            let verified = p.verified;
                            let initial = p
                                .store_name
                                .chars()
                                .next()
                                .unwrap_or('P')
                                .to_uppercase()
                                .to_string();
                            let desc = p.description.clone().unwrap_or_default();
                            // Header kustom merchant → hero; kosong = fallback cover product terbaru.
                            let header = p.header_url.clone().unwrap_or_default();
                            let reviews_href = format!("/m/{}/reviews", merchant_id);
                            let followers_href = format!("/m/{}/followers", merchant_id);
                            // Detail profil USER di balik merchant ini (merchant_id ==
                            // user_id). Tombol di header → /u/{id}: story + ulasan yang
                            // ditulis, sisi "orang"-nya, terpisah dari sisi toko.
                            let user_href = format!("/u/{}", merchant_id);
                            // Buat STORY berisi kartu profil merchant ini (share toko).
                            // Konvensi slug "m/{id}" → viewer tap-through ke /m/{id}.
                            // Pola sama dengan share_to_story di product detail:
                            // UrlSearchParams (wasm) agar nama/URL ter-encode aman.
                            #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
                            let story_mid = merchant_id.clone();
                            #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
                            let story_name = store_name.clone();
                            #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
                            let story_cover = logo.clone(); // avatar kartu story
                            #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
                            let story_header = header.clone(); // header image kartu story
                            #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
                            let story_stats = (p.followers, p.products_count, p.rating_avg);
                            let nav_story = navigate.clone();
                            let on_share_story = move |_: leptos::ev::MouseEvent| {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    let params = web_sys::UrlSearchParams::new()
                                        .expect("UrlSearchParams");
                                    params.append("merchant", "1");
                                    params.append("event_slug", &format!("m/{story_mid}"));
                                    params.append("event_title", &story_name);
                                    params.append("event_cover", &story_cover);
                                    params.append("merchant_header", &story_header);
                                    params.append("verified", if verified { "1" } else { "0" });
                                    params.append("followers", &story_stats.0.to_string());
                                    params.append("products_count", &story_stats.1.to_string());
                                    params.append("rating", &format!("{:.1}", story_stats.2));
                                    nav_story(
                                        &format!("/story?{}", params.to_string()),
                                        Default::default(),
                                    );
                                }
                                #[cfg(not(target_arch = "wasm32"))]
                                let _ = &nav_story;
                            };
                            // ── SEO: meta + JSON-LD Organization ──
                            let seo_title = format!("{} — PULSE", store_name);
                            let seo_desc = if desc.is_empty() {
                                format!("Profil toko {store_name} di PULSE.")
                            } else {
                                desc.clone()
                            };
                            let seo_path = format!("/m/{}", merchant_id);
                            let seo_image = if !header.is_empty() {
                                header.clone()
                            } else {
                                logo.clone()
                            };
                            let ld_org = crate::web::seo::safe_ld(
                                &serde_json::json!({
                                    "@context": "https://schema.org",
                                    "@type": "Organization",
                                    "name": store_name.clone(),
                                    "description": (!desc.is_empty()).then(|| desc.clone()),
                                    "url": crate::web::seo::abs_url(&seo_path),
                                    "logo": (!logo.is_empty()).then(|| logo.clone()),
                                    "image": (!seo_image.is_empty()).then(|| seo_image.clone()),
                                    "aggregateRating": (p.rating_count > 0).then(|| {
                                        serde_json::json!({
                                            "@type": "AggregateRating",
                                            "ratingValue": format!("{:.1}", p.rating_avg),
                                            "reviewCount": p.rating_count,
                                        })
                                    }),
                                }),
                            );
                            #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
                            let share_url = format!("/m/{}", merchant_id);
                            let on_share = move |_| {
                                #[cfg(target_arch = "wasm32")]
                                if let Some(w) = web_sys::window() {
                                    let origin = w.location().origin().unwrap_or_default();
                                    let full = format!("{origin}{share_url}");
                                    let _ = w.navigator().clipboard().write_text(&full);
                                }
                                share_ok.set(true);
                                // set_timeout (leptos) pakai Closure::once_into_js →
                                // callback dibebaskan setelah fire (TIDAK bocor spt
                                // gloo Timeout::forget()); no-op di server.
                                set_timeout(
                                    move || share_ok.set(false),
                                    std::time::Duration::from_millis(1600),
                                );
                            };
                            // Dipakai hanya di jalur wasm (clipboard); no-op di native.
                            view! {
                                <SeoMeta
                                    title=seo_title
                                    description=seo_desc
                                    path=seo_path
                                    image=seo_image
                                    og_type="profile"
                                />
                                <Script type_="application/ld+json">{ld_org}</Script>
                                // ── Hero: header kustom, fallback cover product terbaru ──
                                <div class="mp-hero">
                                    {
                                        let header = header.clone();
                                        move || {
                                            let custom = (!header.is_empty()).then(|| header.clone());
                                            custom
                                                .or_else(|| {
                                                    products
                                                        .get()
                                                        .and_then(|r| r.ok())
                                                        .and_then(|pe| {
                                                            pe.data.first().and_then(|e| e.cover_url.clone())
                                                        })
                                                        .filter(|c| !c.is_empty())
                                                })
                                                .map(|cover| {
                                                    // Hero = kandidat LCP: WAJIB eager + prioritas
                                                    // tinggi (lazy menunda paint elemen terbesar).
                                                    // CLS aman: .mp-hero fixed height 200px di CSS.
                                                    view! {
                                                        <img
                                                            src=cover
                                                            alt=""
                                                            loading="eager"
                                                            fetchpriority="high"
                                                        />
                                                    }
                                                })
                                        }
                                    } <div class="mp-hero-grad"></div>
                                </div>

                                // ── Kepala profil ─────────────────────────────
                                <div class="mp-head">
                                    <div class="mp-avatar-wrap">
                                        {
                                            // Bingkai story pada avatar — SAMA seperti product
                                            // detail (.ed-story-ring): gradient ring + klik buka
                                            // StoryViewer. Muncul HANYA bila merchant punya story
                                            // aktif; kalau tidak, avatar polos (tanpa cincin).
                                            let logo = logo.clone();
                                            let initial = initial.clone();
                                            let open_story = open_story.clone();
                                            move || {
                                                let logo = logo.clone();
                                                let initial = initial.clone();
                                                let avatar = if logo.is_empty() {
                                                    view! {
                                                        <div class="mp-avatar mp-avatar-fallback">
                                                            {initial}
                                                        </div>
                                                    }
                                                        .into_any()
                                                } else {
                                                    view! {
                                                        <img class="mp-avatar" src=logo alt="Logo merchant" />
                                                    }
                                                        .into_any()
                                                };
                                                // Grup story merchant (grup 0) bila ada isinya.
                                                let list = stories
                                                    .get()
                                                    .and_then(|r| r.ok())
                                                    .filter(|l| {
                                                        l.first().map(|g| !g.stories.is_empty()).unwrap_or(false)
                                                    });
                                                match list {
                                                    Some(list) => {
                                                        let open_story = open_story.clone();
                                                        view! {
                                                            <button
                                                                class="mp-avatar-ring"
                                                                on:click=move |_| open_story(list.clone(), 0)
                                                                aria-label="Lihat story toko"
                                                            >
                                                                {avatar}
                                                            </button>
                                                        }
                                                            .into_any()
                                                    }
                                                    None => avatar,
                                                }
                                            }
                                        }
                                        {verified
                                            .then(|| {
                                                view! {
                                                    <span class="mp-avatar-badge" title="Terverifikasi">
                                                        <svg
                                                            width="14"
                                                            height="14"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="3"
                                                            stroke-linecap="round"
                                                            stroke-linejoin="round"
                                                        >
                                                            <polyline points="20 6 9 17 4 12" />
                                                        </svg>
                                                    </span>
                                                }
                                            })}
                                    </div>
                                    <div class="mp-head-actions">
                                        <button
                                            class="mp-follow-btn"
                                            data-on=move || following.get().to_string()
                                            disabled=move || follow_busy.get()
                                            on:click=on_follow
                                        >
                                            {move || {
                                                if following.get() { "Mengikuti" } else { "Follow" }
                                            }}
                                        </button>
                                        <button
                                            class="mp-icon-btn"
                                            on:click=on_share_story
                                            aria-label="Bagikan sebagai story"
                                            title="Buat story toko ini"
                                        >
                                            <svg
                                                width="18"
                                                height="18"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2"
                                                stroke-linecap="round"
                                            >
                                                <circle cx="12" cy="12" r="9" />
                                                <line x1="12" y1="8" x2="12" y2="16" />
                                                <line x1="8" y1="12" x2="16" y2="12" />
                                            </svg>
                                        </button>
                                        <a
                                            class="mp-icon-btn"
                                            href=user_href
                                            aria-label="Lihat profil user"
                                            title="Profil user"
                                        >
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
                                                <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" />
                                                <circle cx="12" cy="7" r="4" />
                                            </svg>
                                        </a>
                                        <button
                                            class="mp-icon-btn"
                                            on:click=on_share
                                            aria-label="Bagikan"
                                        >
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
                                                <circle cx="18" cy="5" r="3" />
                                                <circle cx="6" cy="12" r="3" />
                                                <circle cx="18" cy="19" r="3" />
                                                <line x1="8.6" y1="13.5" x2="15.4" y2="17.5" />
                                                <line x1="15.4" y1="6.5" x2="8.6" y2="10.5" />
                                            </svg>
                                        </button>
                                        {move || {
                                            share_ok
                                                .get()
                                                .then(|| {
                                                    view! {
                                                        <span class="mp-share-toast">"Tautan disalin"</span>
                                                    }
                                                })
                                        }}
                                    </div>
                                </div>

                                <div class="mp-container">
                                    <div class="mp-name-row">
                                        <h1 class="mp-name">{store_name.clone()}</h1>
                                    </div>

                                    // ── Lokasi (kota product terbaru) ───────────
                                    {move || {
                                        products
                                            .get()
                                            .and_then(|r| r.ok())
                                            .and_then(|pe| {
                                                pe.data.first().and_then(|e| e.city.clone())
                                            })
                                            .filter(|c| !c.is_empty())
                                            .map(|city| {
                                                view! {
                                                    <p class="mp-loc">
                                                        <svg
                                                            width="14"
                                                            height="14"
                                                            viewBox="0 0 24 24"
                                                            fill="none"
                                                            stroke="currentColor"
                                                            stroke-width="2"
                                                            stroke-linecap="round"
                                                            stroke-linejoin="round"
                                                        >
                                                            <path d="M21 10c0 7-9 12-9 12s-9-5-9-12a9 9 0 0 1 18 0z" />
                                                            <circle cx="12" cy="10" r="3" />
                                                        </svg>
                                                        {city}
                                                    </p>
                                                }
                                            })
                                    }}

                                    // ── Statistik ─────────────────────────────
                                    <div class="mp-stats">
                                        <a class="mp-stat mp-stat-link" href=followers_href>
                                            <span class="mp-stat-num">
                                                {move || fmt_count(followers.get())}
                                            </span>
                                            <span class="mp-stat-label">"FOLLOWERS"</span>
                                        </a>
                                        <div class="mp-stat">
                                            <span class="mp-stat-num">{fmt_count(p.products_count)}</span>
                                            <span class="mp-stat-label">"PRODUCT"</span>
                                        </div>
                                        <a class="mp-stat mp-stat-link" href=reviews_href.clone()>
                                            <span class="mp-stat-num">
                                                {format!("{:.1}", p.rating_avg)}
                                                <span class="mp-stat-star">"★"</span>
                                            </span>
                                            <span class="mp-stat-label">"RATING"</span>
                                        </a>
                                    </div>

                                    // ── Tabs ──────────────────────────────────
                                    // Klik ATAU geser (swipe) panel di bawah untuk
                                    // berpindah antar: EVENTS · TENTANG · ULASAN · STORY.
                                    // ── Bilah lengket: tab + pencarian ────────
                                    // Toolbar pencarian DIPINDAH KELUAR dari
                                    // `.mp-swipe`. Kontainer itu ber-`overflow:
                                    // hidden` (perlu untuk geser antar-tab), dan
                                    // `position: sticky` tak bisa keluar dari
                                    // leluhur ber-overflow — ia akan menempel
                                    // pada kotak yang ikut tergulir, jadi tak
                                    // ada yang terlihat menempel sama sekali.
                                    //
                                    // Di luar sini ia juga berhenti ikut bergeser
                                    // saat berpindah tab, yang memang benar:
                                    // pencarian milik halaman, bukan milik salah
                                    // satu panel.
                                    <div class="mp-stickybar">
                                    <div class="mp-tabs">
                                        {["PRODUCT", "TENTANG", "ULASAN", "STORY"]
                                            .into_iter()
                                            .enumerate()
                                            .map(|(i, label)| {
                                                view! {
                                                    <button
                                                        class=move || {
                                                            if tab.get() == i { "mp-tab mp-tab--on" } else { "mp-tab" }
                                                        }
                                                        on:click=move |_| tab.set(i)
                                                    >
                                                        {label}
                                                    </button>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                    // Hanya pada tab PRODUCT — mencari di tab
                                    // "Tentang" atau "Ulasan" tak berarti apa pun.
                                    {move || (tab.get() == 0).then(|| view! {
                                    // ── Pencarian & urutan ──────────────────
                                    // Dicari saat ENTER atau saat urutan
                                    // diganti, BUKAN pada tiap ketukan
                                    // tombol: mencari per huruf berarti
                                    // satu permintaan per karakter, dan
                                    // jawaban yang datang tak berurutan
                                    // membuat hasil berkedip-kedip.
                                    <div class="flex items-center gap-2 px-4 pb-3">
                                        // Ikon di DALAM kolom: tanpa itu kolom
                                        // cari dan kolom urut tampak sebagai dua
                                        // pil kosong yang setara, dan mana yang
                                        // bisa diketik tak terbaca sampai dicoba.
                                        <div class="relative flex-1 min-w-0">
                                            <svg
                                                width="15" height="15" viewBox="0 0 24 24" fill="none"
                                                stroke="currentColor" stroke-width="2"
                                                stroke-linecap="round"
                                                class="absolute left-3.5 top-1/2 -translate-y-1/2 \
                                                       text-content-muted pointer-events-none"
                                            >
                                                <circle cx="11" cy="11" r="7" />
                                                <line x1="21" y1="21" x2="16.65" y2="16.65" />
                                            </svg>
                                            <input
                                                class="w-full h-10 pl-10 pr-3.5 rounded-full bg-card \
                                                       border border-solid border-line text-content \
                                                       text-sm placeholder:text-content-muted"
                                                r#type="search"
                                                placeholder="Cari di toko ini…"
                                                prop:value=move || cari.get()
                                                on:input=move |e| cari.set(event_target_value(&e))
                                                on:change=move |_| jalankan_saring()
                                            />
                                        </div>
                                        // `appearance-none` + chevron sendiri:
                                        // panah bawaan sistem muncul sebagai dua
                                        // segitiga bertumpuk yang tak mengikuti
                                        // tema mana pun dan terlihat asing di
                                        // antara komponen lain.
                                        <div class="relative shrink-0">
                                            <select
                                                class="appearance-none h-10 pl-3.5 pr-8 rounded-full \
                                                       bg-card border border-solid border-line \
                                                       text-content text-[12px] cursor-pointer"
                                                aria-label="Urutkan product"
                                                prop:value=move || urut.get()
                                                on:change=move |e| {
                                                    urut.set(event_target_value(&e));
                                                    jalankan_saring();
                                                }
                                            >
                                            <option value="">"Paling sesuai"</option>
                                            <option value="harga_asc">"Harga termurah"</option>
                                            <option value="harga_desc">"Harga tertinggi"</option>
                                            <option value="terlaris">"Terlaris"</option>
                                                <option value="terbaru">"Terbaru"</option>
                                                <option value="acak">"Acak"</option>
                                            </select>
                                            <svg
                                                width="12" height="12" viewBox="0 0 24 24" fill="none"
                                                stroke="currentColor" stroke-width="2.5"
                                                stroke-linecap="round"
                                                class="absolute right-3 top-1/2 -translate-y-1/2 \
                                                       text-content-muted pointer-events-none"
                                            >
                                                <polyline points="6 9 12 15 18 9" />
                                            </svg>
                                        </div>
                                    </div>
                                    })}

                                    </div>

                                    // ── Panel yang bisa digeser ───────────────
                                    <div
                                        class="mp-swipe"
                                        node_ref=swipe_ref
                                        on:pointerdown=on_pointer_down
                                        on:pointermove=on_pointer_move
                                        on:pointerup=on_pointer_up
                                        on:pointercancel=on_pointer_up
                                    >
                                        <div
                                            class="mp-panel"
                                            class:mp-panel--active=move || tab.get() == 0
                                            class:mp-panel--drag=move || dragging.get()
                                            style=move || panel_tf(0)
                                        >
                                            <Suspense fallback=|| {
                                                view! { <ProductGridShimmer count=4 /> }
                                            }>
                                                {move || {
                                                    // Saringan aktif menggantikan daftar biasa
                                                    // sepenuhnya — termasuk tombol "muat lebih
                                                    // banyak", yang paginasinya milik daftar
                                                    // biasa dan tak berlaku bagi hasil pencarian.
                                                    if saring_loading.get() {
                                                        return Some(
                                                            view! { <ProductGridShimmer count=4 /> }.into_any(),
                                                        );
                                                    }
                                                    if let Some(hasil) = hasil_saring.get() {
                                                        return Some(
                                                            view! {
                                                                <ProductGrid
                                                                    products=hasil
                                                                    empty="Tak ada product yang cocok."
                                                                />
                                                            }
                                                                .into_any(),
                                                        );
                                                    }
                                                    products
                                                        .get()
                                                        .map(|r| match r {
                                                            Ok(pe) => {
                                                                // Gabung page-1 + halaman yang sudah di-append.
                                                                let mut all = pe.data.clone();
                                                                all.extend(ev_extra.get());
                                                                view! {
                                                                    <ProductGrid
                                                                        products=all
                                                                        empty="Belum ada product aktif."
                                                                    />
                                                                    {move || {
                                                                        Some(
                                                                            view! {
                                                                                <div class="mp-more-wrap">
                                                                                    // Penanda kemajuan. Tanpa ini
                                                                                    // "MUAT LEBIH BANYAK" tak memberi
                                                                                    // tahu seberapa jauh lagi daftarnya
                                                                                    // — orang menekan berulang tanpa
                                                                                    // tahu tinggal satu product atau
                                                                                    // seratus.
                                                                                    <span class="mp-more-count">
                                                                                        {move || {
                                                                                            let tampil = jml_tampil();
                                                                                            let total = ev_total.get();
                                                                                            if total <= 0 {
                                                                                                String::new()
                                                                                            } else {
                                                                                                format!(
                                                                                                    "Menampilkan {tampil} dari {total} product",
                                                                                                )
                                                                                            }
                                                                                        }}
                                                                                    </span>
                                                                                    {move || {
                                                                                        if ev_has_more() {
                                                                                            view! {
                                                                                                <button
                                                                                                    class="mp-more-btn"
                                                                                                    disabled=move || ev_loading.get()
                                                                                                    on:click=load_more_products
                                                                                                >
                                                                                                    {move || {
                                                                                                        if ev_loading.get() {
                                                                                                            "MEMUAT…"
                                                                                                        } else {
                                                                                                            "MUAT LEBIH BANYAK"
                                                                                                        }
                                                                                                    }}
                                                                                                </button>
                                                                                            }
                                                                                                .into_any()
                                                                                        } else {
                                                                                            // Akhir daftar dinyatakan,
                                                                                            // bukan dibiarkan senyap:
                                                                                            // tombol yang hilang begitu
                                                                                            // saja tak bisa dibedakan
                                                                                            // dari gagal memuat.
                                                                                            view! {
                                                                                                <span class="mp-more-end">
                                                                                                    "— semua product sudah tampil —"
                                                                                                </span>
                                                                                            }
                                                                                                .into_any()
                                                                                        }
                                                                                    }}
                                                                                </div>
                                                                            },
                                                                        )
                                                                    }}
                                                                }
                                                                    .into_any()
                                                            }
                                                            Err(_) => {
                                                                view! { <p class="mp-empty">"Gagal memuat product."</p> }
                                                                    .into_any()
                                                            }
                                                        })
                                                }}
                                                </Suspense>
                                        </div>
                                        <div
                                            class="mp-panel"
                                            class:mp-panel--active=move || tab.get() == 1
                                            class:mp-panel--drag=move || dragging.get()
                                            style=move || panel_tf(1)
                                        >
                                            {
                                                let d = desc.clone();
                                                view! {
                                                    <div class="mp-about">
                                                                    {if d.is_empty() {
                                                                        view! {
                                                                            <p class="mp-empty">
                                                                                "Merchant belum menulis deskripsi."
                                                                            </p>
                                                                        }
                                                                            .into_any()
                                                                    } else {
                                                                        view! { <p class="mp-about-text">{d}</p> }.into_any()
                                                                    }}
                                                    </div>
                                                }
                                            }
                                        </div>
                                        <div
                                            class="mp-panel"
                                            class:mp-panel--active=move || tab.get() == 2
                                            class:mp-panel--drag=move || dragging.get()
                                            style=move || panel_tf(2)
                                        >
                                            {
                                                let reviews_href = reviews_href.clone();
                                                view! {
                                                    <div class="mp-reviews">
                                                                    <Suspense fallback=|| {
                                                                        view! { <ReviewListShimmer /> }
                                                                    }>
                                                                        {
                                                                            let reviews_href = reviews_href.clone();
                                                                            move || {
                                                                                let reviews_href = reviews_href.clone();
                                                                                reviews
                                                                                    .get()
                                                                                    .map(|r| match r {
                                                                                        Ok(d) => {
                                                                                            let total = d.total.max(0);
                                                                                            if total == 0 && d.items.is_empty() {
                                                                                                view! {
                                                                                                    <p class="mp-empty">
                                                                                                        "Belum ada ulasan. "
                                                                                                        <a class="mp-reviews-all" href=reviews_href.clone()>
                                                                                                            "Jadilah yang pertama →"
                                                                                                        </a>
                                                                                                    </p>
                                                                                                }
                                                                                                    .into_any()
                                                                                            } else {
                                                                                                view! {
                                                                                                    <div class="mrv-big">
                                                                                                        <span class="mrv-avg">{format!("{:.1}", d.avg)}</span>
                                                                                                        <div class="mrv-big-side">
                                                                                                            <Stars rating=d.avg />
                                                                                                            <span class="mrv-outof">
                                                                                                                {fmt_count(total)} " ulasan"
                                                                                                            </span>
                                                                                                        </div>
                                                                                                    </div>
                                                                                                    <div class="mrv-list" style="margin-top:14px;">
                                                                                                        {d
                                                                                                            .items
                                                                                                            .iter()
                                                                                                            .take(5)
                                                                                                            .map(|r| {
                                                                                                                let initial: String = r
                                                                                                                    .user_name
                                                                                                                    .chars()
                                                                                                                    .next()
                                                                                                                    .unwrap_or('P')
                                                                                                                    .to_uppercase()
                                                                                                                    .to_string();
                                                                                                                let date = r.created_at.format("%d %b %Y").to_string();
                                                                                                                let user_href = format!("/u/{}", r.user_id);
                                                                                                                view! {
                                                                                                                    <div class="mrv-item">
                                                                                                                        <div class="mrv-item-head">
                                                                                                                            <a class="mrv-item-avatar" href=user_href.clone()>
                                                                                                                                {initial}
                                                                                                                            </a>
                                                                                                                            <a class="mrv-item-who mrv-item-who--link" href=user_href>
                                                                                                                                <span class="mrv-item-name">
                                                                                                                                    {r.user_name.clone()}
                                                                                                                                </span>
                                                                                                                                <span class="mrv-item-date">{date}</span>
                                                                                                                            </a>
                                                                                                                            <Stars rating=r.rating as f64 />
                                                                                                                        </div>
                                                                                                                        {(!r.comment.is_empty())
                                                                                                                            .then(|| {
                                                                                                                                view! {
                                                                                                                                    <p class="mrv-item-text">{r.comment.clone()}</p>
                                                                                                                                }
                                                                                                                            })}
                                                                                                                    </div>
                                                                                                                }
                                                                                                            })
                                                                                                            .collect_view()}
                                                                                                    </div>
                                                                                                    <a
                                                                                                        class="mp-reviews-all"
                                                                                                        href=reviews_href.clone()
                                                                                                    >
                                                                                                        "Lihat semua & tulis ulasan →"
                                                                                                    </a>
                                                                                                }
                                                                                                    .into_any()
                                                                                            }
                                                                                        }
                                                                                        Err(_) => {
                                                                                            view! { <p class="mp-empty">"Gagal memuat ulasan."</p> }
                                                                                                .into_any()
                                                                                        }
                                                                                    })
                                                                            }
                                                                        }
                                                                    </Suspense>
                                                    </div>
                                                }
                                            }
                                        </div>
                                        <div
                                            class="mp-panel"
                                            class:mp-panel--active=move || tab.get() == 3
                                            class:mp-panel--drag=move || dragging.get()
                                            style=move || panel_tf(3)
                                        >
                                            {
                                                let open_story = open_story.clone();
                                                view! {
                                                    <div class="mp-stories">
                                                                    <Suspense fallback=|| {
                                                                        view! { <StoryGridShimmer /> }
                                                                    }>
                                                                        {
                                                                            let open_story = open_story.clone();
                                                                            move || {
                                                                                let open_story = open_story.clone();
                                                                                stories
                                                                                    .get()
                                                                                    .map(|r| match r {
                                                                                        Ok(list) => {
                                                                                            let items = list
                                                                                                .first()
                                                                                                .map(|g| g.stories.clone())
                                                                                                .unwrap_or_default();
                                                                                            if items.is_empty() {
                                                                                                view! {
                                                                                                    <p class="mp-empty">"Merchant belum punya story."</p>
                                                                                                }
                                                                                                    .into_any()
                                                                                            } else {
                                                                                                view! {
                                                                                                    <div class="mp-story-grid">
                                                                                                        {items
                                                                                                            .iter()
                                                                                                            .enumerate()
                                                                                                            .map(|(i, s)| {
                                                                                                                let is_video = s.media_type == StoryMediaType::Video;
                                                                                                                let media = s.media_url.clone();
                                                                                                                let list_c = list.clone();
                                                                                                                let open_story = open_story.clone();
                                                                                                                view! {
                                                                                                                    <button
                                                                                                                        class="mp-story-cell"
                                                                                                                        on:click=move |_| open_story(list_c.clone(), i)
                                                                                                                    >
                                                                                                                        {if is_video {
                                                                                                                            view! {
                                                                                                                                <video
                                                                                                                                    class="mp-story-media"
                                                                                                                                    src=media.clone()
                                                                                                                                    muted=true
                                                                                                                                    playsinline=true
                                                                                                                                    preload="metadata"
                                                                                                                                ></video>
                                                                                                                                <span class="mp-story-play">
                                                                                                                                    <svg
                                                                                                                                        width="12"
                                                                                                                                        height="12"
                                                                                                                                        viewBox="0 0 24 24"
                                                                                                                                        fill="currentColor"
                                                                                                                                    >
                                                                                                                                        <polygon points="5 3 19 12 5 21 5 3" />
                                                                                                                                    </svg>
                                                                                                                                </span>
                                                                                                                            }
                                                                                                                                .into_any()
                                                                                                                        } else {
                                                                                                                            view! {
                                                                                                                                <img
                                                                                                                                    class="mp-story-media"
                                                                                                                                    src=media.clone()
                                                                                                                                    alt=""
                                                                                                                                    loading="lazy"
                                                                                                                                />
                                                                                                                            }
                                                                                                                                .into_any()
                                                                                                                        }}
                                                                                                                    </button>
                                                                                                                }
                                                                                                            })
                                                                                                            .collect_view()}
                                                                                                    </div>
                                                                                                }
                                                                                                    .into_any()
                                                                                            }
                                                                                        }
                                                                                        Err(_) => {
                                                                                            view! { <p class="mp-empty">"Gagal memuat story."</p> }
                                                                                                .into_any()
                                                                                        }
                                                                                    })
                                                                            }
                                                                        }
                                                                    </Suspense>
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
                }}
            </Suspense>

            // Viewer fullscreen story merchant (overlay global; buka via panel STORY).
            <StoryViewer />
        </div>
    }
}

