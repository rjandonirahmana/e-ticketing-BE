// ═══════════════════════════════════════════════════════════════════════════════
//  STORY — StoryPage main component
// ═══════════════════════════════════════════════════════════════════════════════

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use web_sys::{HtmlImageElement, HtmlInputElement};

use crate::web::components::draggable_overlay::DraggableOverlay;
use crate::web::state::stories::{use_stories_store, OverlayType, StoryOverlay};

use super::canvas::{
    BgExport, canvas_to_blob,
    cover_factor, create_export_canvas, css_filter_string, export_ext,
    export_mime, export_story_canvas, get_dpr, gradient_colors,
    load_img_to_canvas, preload_fonts, render_product_card_to_canvas,
    render_merchant_card_to_canvas,
    render_overlays_to_canvas, trigger_download_blob,
};
#[cfg(target_arch = "wasm32")]
use super::canvas::compress_image_file;
use super::components::{PanelGeser, TombolAlat, TombolWarna};
use super::types::{
    Alat, BG_GRADIENTS, BG_SOLID_COLORS, DAFTAR_FILTER, DAFTAR_MUSIK, STIKER, WARNA_TEKS,
};
use super::types::ProductStoryMeta;

#[cfg(target_arch = "wasm32")]
use super::upload::upload_story_file;

// ── Navigation helper (SSR-compatible wrapper) ────────────────────────────────
fn use_leptos_navigate() -> impl Fn(&str, leptos_router::NavigateOptions) + Clone + 'static {
    leptos_router::hooks::use_navigate()
}

// ── Constants ─────────────────────────────────────────────────────────────────
const EXIT_DURATION_MS: f64 = 480.0;

// ── Browser-only helpers ──────────────────────────────────────────────────────

fn read_from_page() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(s) = web_sys::window()
            .and_then(|w| w.session_storage().ok()).flatten()
            .and_then(|s| s.get_item("story_from_page").ok()).flatten()
        {
            return s;
        }
    }
    "/explore".to_string()
}

fn clear_from_page() {
    #[cfg(target_arch = "wasm32")]
    if let Some(storage) = web_sys::window().and_then(|w| w.session_storage().ok()).flatten() {
        let _ = storage.remove_item("story_from_page");
    }
}

fn exit_page_to<F>(is_exiting: RwSignal<bool>, nav: F, delay_ms: f64)
where
    F: FnOnce() + 'static,
{
    is_exiting.set(true);
    spawn_local(async move {
        #[cfg(target_arch = "wasm32")]
        {
            let p = web_sys::js_sys::Promise::new(&mut |resolve, _| {
                let _ = web_sys::window()
                    .unwrap()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, delay_ms as i32);
            });
            let _ = wasm_bindgen_futures::JsFuture::from(p).await;
        }
        let _ = delay_ms;
        nav();
    });
}

static ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn buat_id(awalan: &str) -> String {
    let seq = ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    #[cfg(target_arch = "wasm32")]
    {
        let ts = web_sys::js_sys::Date::now() as u64;
        return format!("{}_{}_{}", awalan, ts, seq);
    }
    #[cfg(not(target_arch = "wasm32"))]
    format!("{}_{}", awalan, seq)
}

fn revoke_url(url: &str) {
    #[cfg(target_arch = "wasm32")]
    let _ = web_sys::Url::revoke_object_url(url);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = url;
}

// ── StoryPage component ───────────────────────────────────────────────────────

#[component]
pub fn StoryPage() -> impl IntoView {
    let store_ctx  = use_stories_store();
    let navigate_sv = StoredValue::new(use_leptos_navigate());

    // ── State ─────────────────────────────────────────────────────────────────
    let file_ref            = NodeRef::<leptos::html::Input>::new();
    let file_terpilih       = RwSignal::new(None::<web_sys::File>);
    let url_pratinjau       = RwSignal::new(None::<String>);
    let is_video            = RwSignal::new(false);
    let overlays            = RwSignal::new(Vec::<StoryOverlay>::new());
    let alat_aktif          = RwSignal::new(Alat::None);
    let input_teks          = RwSignal::new(String::new());
    let warna_teks_sig      = RwSignal::new("#ffffff".to_string());
    let ukuran_teks         = RwSignal::new(28_i32);
    let hero_transition     = RwSignal::new(false);
    let _hero_timer_active  : StoredValue<bool> = StoredValue::new(false);
    let trash_zone_active   = RwSignal::new(false);
    let text_style          = RwSignal::new("classic".to_string());
    let text_align          = RwSignal::new("center".to_string());
    let text_bg_enabled     = RwSignal::new(false);
    let filter_dipilih      = RwSignal::new("normal".to_string());
    let musik_dipilih       = RwSignal::new(None::<(String, String, String)>);
    let sedang_mengunggah   = RwSignal::new(false);
    let bg_mode             = RwSignal::new("blur".to_string());
    let bg_solid_color      = RwSignal::new("#1a1a2e".to_string());
    let error_unggah        = RwSignal::new(None::<String>);
    let sedang_mengunduh    = RwSignal::new(false);
    let z_counter           = RwSignal::new(1_i32);
    let last_tap            : StoredValue<(String, f64)> = StoredValue::new((String::new(), 0.0));
    let selected_overlay_id = RwSignal::new(String::new());
    let bg_scale            = RwSignal::new(1.0_f64);
    let media_cover_factor  = RwSignal::new(1.0_f64);
    let _bg_pinch_start     : StoredValue<(f64, f64)> = StoredValue::new((0.0, 1.0));
    let media_layer_ref     = NodeRef::<leptos::html::Div>::new();
    let cover_img_ready     = RwSignal::new(false);
    let cover_load_version  = RwSignal::new(0_u32);
    let is_exiting          = RwSignal::new(false);
    let swipe_start_y       : StoredValue<f64> = StoredValue::new(0.0);
    let swipe_start_x       : StoredValue<f64> = StoredValue::new(0.0);
    let swipe_active        : StoredValue<bool> = StoredValue::new(false);

    // ── Query params ──────────────────────────────────────────────────────────
    let query           = use_query_map();
    let prefill_slug    = move || query.with(|q| q.get("event_slug").unwrap_or_default());
    let prefill_title   = move || query.with(|q| q.get("event_title").unwrap_or_default());
    let prefill_cover   = move || query.with(|q| q.get("event_cover").unwrap_or_default());
    let prefill_id      = move || query.with(|q| q.get("event_id").unwrap_or_default());
    let prefill_desc    = move || query.with(|q| q.get("product_desc").unwrap_or_default());
    let prefill_date    = move || query.with(|q| q.get("event_date").unwrap_or_default());
    let prefill_venue   = move || query.with(|q| q.get("event_venue").unwrap_or_default());
    let prefill_price   = move || query.with(|q| q.get("product_price").unwrap_or_default());
    // Ticket-share mode: story dibuat dari ticket detail. Tidak ada event_slug
    // (jadi tidak pernah di-persist sebagai product link) — hanya tampil di kartu canvas.
    let prefill_is_ticket = move || query.with(|q| q.get("is_ticket").unwrap_or_default()) == "1";
    let prefill_ticket_ref = move || query.with(|q| q.get("ticket_ref").unwrap_or_default());
    // Merchant-share mode (dari /m/{id} atau halaman ulasan): kartu profil toko
    // (bukan kartu produk). event_slug memakai konvensi "m/{merchant_id}" —
    // viewer menerjemahkannya ke /m/{id}. event_cover = logo/header (boleh kosong
    // → renderer fallback gradient + inisial). review_* opsional (share ulasan).
    let prefill_is_merchant = move || query.with(|q| q.get("merchant").unwrap_or_default()) == "1";
    let prefill_verified = move || query.with(|q| q.get("verified").unwrap_or_default()) == "1";
    // Bingkai profil toko: header image + statistik (FOLLOWERS/EVENTS/RATING).
    let prefill_mch_header =
        move || query.with(|q| q.get("merchant_header").unwrap_or_default());
    let prefill_followers = move || {
        query.with(|q| q.get("followers").unwrap_or_default()).parse::<i64>().unwrap_or(0)
    };
    let prefill_products_count = move || {
        query.with(|q| q.get("products_count").unwrap_or_default()).parse::<i64>().unwrap_or(0)
    };
    let prefill_rating = move || {
        query.with(|q| q.get("rating").unwrap_or_default()).parse::<f64>().unwrap_or(0.0)
    };
    let prefill_review_rating = move || {
        query.with(|q| q.get("review_rating").unwrap_or_default())
            .parse::<u8>()
            .ok()
            .filter(|r| (1..=5).contains(r))
    };
    let prefill_review_comment =
        move || query.with(|q| q.get("review_comment").unwrap_or_default());

    let has_product_prefill = Memo::new(move |_| !prefill_slug().is_empty() || prefill_is_ticket());
    let user_overrode_prefill = RwSignal::new(false);
    let product_meta_sig  : RwSignal<Option<ProductStoryMeta>> = RwSignal::new(None);
    let event_cover_url : StoredValue<String> = StoredValue::new(String::new());
    let product_desc_sig  : StoredValue<String> = StoredValue::new(String::new());
    let last_prefilled_slug : StoredValue<String> = StoredValue::new(String::new());

    // ── Helper: navigasi kembali ──────────────────────────────────────────────
    let do_exit = move || {
        let from = read_from_page();
        clear_from_page();
        let nav = navigate_sv.get_value();
        exit_page_to(is_exiting, move || { nav(&from, Default::default()); }, EXIT_DURATION_MS);
    };

    // ── Effect: inisiasi mode product ───────────────────────────────────────────
    Effect::new(move |_| {
        let slug      = prefill_slug();
        let title     = prefill_title();
        let cover     = prefill_cover();
        let id        = prefill_id();
        let desc      = prefill_desc();
        let is_ticket = prefill_is_ticket();
        let ticket_ref = prefill_ticket_ref();
        // Mode merchant: cover (logo) boleh kosong — renderer punya fallback.
        if (slug.is_empty() && !is_ticket) || (cover.is_empty() && !prefill_is_merchant()) {
            return;
        }

        let from_create = query.with(|q| q.get("from_create").unwrap_or_default()) == "1";
        if from_create && slug == "draft" {
            is_video.set(false);
            url_pratinjau.set(Some(cover.clone()));
            event_cover_url.set_value(cover.clone());
            product_desc_sig.set_value(desc.clone());
            last_prefilled_slug.set_value(slug.clone());
            product_meta_sig.set(None);
            return;
        }
        // Dedup key: slug untuk mode product, "ticket:<ref>" untuk mode ticket-share
        // (tidak pernah dikirim sebagai event_slug ke backend).
        let dedup_key = if is_ticket { format!("ticket:{}", ticket_ref) } else { slug.clone() };
        let last = last_prefilled_slug.get_value();
        if !last.is_empty() && last == dedup_key { return; }
        last_prefilled_slug.set_value(dedup_key);
        cover_load_version.update(|v| *v = v.wrapping_add(1));
        cover_img_ready.set(false);
        is_video.set(false);
        url_pratinjau.set(Some(cover.clone()));
        event_cover_url.set_value(cover.clone());
        product_desc_sig.set_value(desc.clone());

        #[cfg(target_arch = "wasm32")]
        {
            let has_hero = web_sys::window().and_then(|w| w.session_storage().ok()).flatten()
                .and_then(|s| s.get_item("story_hero_transition").ok()).flatten()
                .map(|v| v == "produk").unwrap_or(false);
            if has_hero {
                hero_transition.set(true);
                if let Some(storage) = web_sys::window().and_then(|w| w.session_storage().ok()).flatten() {
                    let _ = storage.remove_item("story_hero_transition");
                    let _ = storage.remove_item("story_hero_cover");
                }
                _hero_timer_active.set_value(true);
                spawn_local(async move {
                    let p = web_sys::js_sys::Promise::new(&mut |resolve, _| {
                        let _ = web_sys::window().unwrap()
                            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 650);
                    });
                    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
                    if _hero_timer_active.get_value() {
                        _hero_timer_active.set_value(false);
                        hero_transition.set(false);
                    }
                });
            }
        }

        // Slug dikirim juga di mode ticket agar story yang dipublish dari tiket
        // tetap tertaut ke halaman product (viewer bisa tap-through).
        let event_slug = slug;
        product_meta_sig.set(Some(ProductStoryMeta { event_id: id, event_slug, event_title: title }));

        // Mode ticket: sisipkan otomatis teks "Pembelian Berhasil" yang bisa
        // digeser & diedit di layar story create/edit. Mode product biasa: kosong.
        if is_ticket {
            z_counter.set(1);
            overlays.set(vec![StoryOverlay {
                id: buat_id("teks"),
                overlay_type: OverlayType::Text,
                x: 50.0,
                y: 16.0,
                content: Some("Pembelian Berhasil! 🎉".to_string()),
                color: Some("#ffffff".to_string()),
                font_size: Some(30),
                rotation: Some(0.0),
                emoji: None,
                scale: Some(1.0),
                z_index: 1,
                text_style: Some("classic".to_string()),
                text_align: Some("center".to_string()),
            }]);
        } else {
            overlays.set(Vec::new());
        }
    });

    // ── Pinch-zoom touch listeners ─────────────────────────────────────────────
    #[cfg(target_arch = "wasm32")]
    {
        use web_sys::AddEventListenerOptions;
        let bg_ts_fn: StoredValue<Option<web_sys::js_sys::Function>> = StoredValue::new(None);
        let bg_tm_fn: StoredValue<Option<web_sys::js_sys::Function>> = StoredValue::new(None);
        let bg_ts_cl: StoredValue<Option<JsValue>> = StoredValue::new(None);
        let bg_tm_cl: StoredValue<Option<JsValue>> = StoredValue::new(None);

        Effect::new(move |_| {
            let Some(el) = media_layer_ref.get() else { return; };
            let target: web_sys::EventTarget = el.unchecked_ref::<web_sys::HtmlElement>().clone().into();
            if let Some(f) = bg_ts_fn.get_value() { let _ = target.remove_event_listener_with_callback("touchstart", &f); bg_ts_fn.set_value(None); }
            if let Some(f) = bg_tm_fn.get_value() { let _ = target.remove_event_listener_with_callback("touchmove", &f); bg_tm_fn.set_value(None); }

            let on_ts = Closure::<dyn Fn(web_sys::TouchEvent)>::new(move |ev: web_sys::TouchEvent| {
                let t = ev.touches();
                if t.length() == 2 {
                    let t0 = t.get(0).unwrap(); let t1 = t.get(1).unwrap();
                    let dx = (t1.client_x() - t0.client_x()) as f64;
                    let dy = (t1.client_y() - t0.client_y()) as f64;
                    _bg_pinch_start.set_value(((dx*dx+dy*dy).sqrt(), bg_scale.get_untracked()));
                }
            });
            let ts_fn: web_sys::js_sys::Function = on_ts.as_ref().unchecked_ref::<web_sys::js_sys::Function>().clone();
            let opts = AddEventListenerOptions::new(); opts.set_passive(true);
            let _ = target.add_event_listener_with_callback_and_add_event_listener_options("touchstart", &ts_fn, &opts);
            bg_ts_fn.set_value(Some(ts_fn));
            bg_ts_cl.set_value(Some(on_ts.into_js_value()));

            let on_tm = Closure::<dyn Fn(web_sys::TouchEvent)>::new(move |ev: web_sys::TouchEvent| {
                let t = ev.touches();
                if t.length() == 2 {
                    ev.prevent_default();
                    let t0 = t.get(0).unwrap(); let t1 = t.get(1).unwrap();
                    let dx = (t1.client_x() - t0.client_x()) as f64;
                    let dy = (t1.client_y() - t0.client_y()) as f64;
                    let dist = (dx*dx+dy*dy).sqrt();
                    let (sd, ss) = _bg_pinch_start.get_value();
                    if sd < 1.0 { return; }
                    let max_scale = if is_video.get_untracked() { 4.0 }
                        else { (media_cover_factor.get_untracked() * 4.0).max(4.0) };
                    bg_scale.set((ss * dist / sd).clamp(1.0, max_scale));
                }
            });
            let tm_fn: web_sys::js_sys::Function = on_tm.as_ref().unchecked_ref::<web_sys::js_sys::Function>().clone();
            let opts2 = AddEventListenerOptions::new(); opts2.set_passive(false);
            let _ = target.add_event_listener_with_callback_and_add_event_listener_options("touchmove", &tm_fn, &opts2);
            bg_tm_fn.set_value(Some(tm_fn));
            bg_tm_cl.set_value(Some(on_tm.into_js_value()));
        });

        on_cleanup(move || {
            if let Some(el) = media_layer_ref.get_untracked() {
                let target: web_sys::EventTarget = el.unchecked_ref::<web_sys::HtmlElement>().clone().into();
                if let Some(f) = bg_ts_fn.get_value() { let _ = target.remove_event_listener_with_callback("touchstart", &f); }
                if let Some(f) = bg_tm_fn.get_value() { let _ = target.remove_event_listener_with_callback("touchmove", &f); }
            }
            bg_ts_cl.set_value(None); bg_tm_cl.set_value(None);
            bg_ts_fn.set_value(None); bg_tm_fn.set_value(None);
        });
    }

    // Font preload
    Effect::new(move |_| {
        spawn_local(async { let _ = preload_fonts().await; });
    });

    // Cover image readiness check
    Effect::new(move |_| {
        let Some(_url) = url_pratinjau.get() else { return; };
        if !has_product_prefill.get() || user_overrode_prefill.get() { return; }
        let version_snapshot = cover_load_version.get_untracked();
        if cover_img_ready.get_untracked() { return; }
        spawn_local(async move {
            for _ in 0..40 {
                if cover_load_version.get_untracked() != version_snapshot { return; }
                #[cfg(target_arch = "wasm32")]
                {
                    let ready = web_sys::window().and_then(|w| w.document())
                        .and_then(|d| d.query_selector("img.sc-product-card-cover-img").ok().flatten())
                        .and_then(|el| el.dyn_into::<HtmlImageElement>().ok())
                        .map(|img| img.complete() && img.natural_width() > 0)
                        .unwrap_or(false);
                    if ready {
                        if cover_load_version.get_untracked() == version_snapshot { cover_img_ready.set(true); }
                        return;
                    }
                    let p = web_sys::js_sys::Promise::new(&mut |resolve, _| {
                        let _ = web_sys::window().unwrap()
                            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
                    });
                    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
                }
            }
            if cover_load_version.get_untracked() == version_snapshot { cover_img_ready.set(true); }
        });
    });

    on_cleanup(move || {
        if let Some(url) = url_pratinjau.get_untracked() {
            if url.starts_with("blob:") && !sedang_mengunduh.get_untracked() && !sedang_mengunggah.get_untracked() {
                revoke_url(&url);
            }
        }
    });

    // ── File picker ───────────────────────────────────────────────────────────
    let saat_file_dipilih = move |ev: leptos::ev::Event| {
        #[cfg(target_arch = "wasm32")]
        {
            let input: HtmlInputElement = ev.target().unwrap().unchecked_into();
            if let Some(files) = input.files() {
                if let Some(file) = files.get(0) {
                    if let Some(old) = url_pratinjau.get_untracked() {
                        if !sedang_mengunduh.get_untracked() && !sedang_mengunggah.get_untracked() && old.starts_with("blob:") {
                            revoke_url(&old);
                        }
                    }
                    const MAX: f64 = 100.0 * 1024.0 * 1024.0;
                    if file.size() > MAX { error_unggah.set(Some("File terlalu besar (maks 100 MB).".into())); return; }
                    let ftype = file.type_();
                    const REJECT: &[&str] = &["image/heic","image/heif","image/x-raw"];
                    if REJECT.iter().any(|t| ftype.as_str() == *t) {
                        error_unggah.set(Some("Format HEIC/RAW tidak didukung. Gunakan JPG, PNG, MP4, atau MOV.".into()));
                        return;
                    }
                    error_unggah.set(None);
                    is_video.set(file.type_().starts_with("video"));
                    let is_img = file.type_().starts_with("image/");
                    let url_existing = web_sys::Url::create_object_url_with_blob(&file).unwrap();
                    alat_aktif.set(Alat::None);
                    bg_scale.set(1.0);
                    product_meta_sig.set(None);
                    user_overrode_prefill.set(true);
                    last_prefilled_slug.set_value(String::new());
                    overlays.set(Vec::new());
                    if is_img {
                        url_pratinjau.set(Some(url_existing.clone()));
                        spawn_local(async move {
                            let compressed = compress_image_file(&file, 1080, 0.92).await;
                            let compressed_url = web_sys::Url::create_object_url_with_blob(&compressed)
                                .unwrap_or(url_existing.clone());
                            if let Some(old) = url_pratinjau.get_untracked() {
                                if old != compressed_url && old.starts_with("blob:") { revoke_url(&old); }
                            }
                            url_pratinjau.set(Some(compressed_url));
                            let bits = web_sys::js_sys::Array::of1(&compressed);
                            let opts = web_sys::FilePropertyBag::new();
                            opts.set_type(export_mime());
                            if let Ok(cf) = web_sys::File::new_with_blob_sequence_and_options(
                                &bits, &format!("story.{}", export_ext()), &opts,
                            ) { file_terpilih.set(Some(cf)); }
                        });
                    } else {
                        file_terpilih.set(Some(file));
                        url_pratinjau.set(Some(url_existing));
                    }
                }
            }
        }
        let _ = ev;
    };

    // ── Unduh ─────────────────────────────────────────────────────────────────
    let unduh_story = move || {
        if sedang_mengunduh.get_untracked() { return; }
        let Some(url) = url_pratinjau.get_untracked() else { return; };
        let ovls: Vec<StoryOverlay> = overlays.with(|o| o.clone());
        let filter = filter_dipilih.get_untracked();
        let scale  = bg_scale.get_untracked();

        if has_product_prefill.get_untracked() && !user_overrode_prefill.get_untracked() {
            let cover       = event_cover_url.get_value();
            let bg_m        = bg_mode.get_untracked();
            let bg_c        = bg_solid_color.get_untracked();
            let ev_filter   = filter_dipilih.get_untracked();
            let title       = prefill_title();
            let date        = prefill_date();
            let venue       = prefill_venue();
            let price       = prefill_price();
            let is_ticket_d = prefill_is_ticket();
            let is_merchant_d = prefill_is_merchant();
            let verified_d = prefill_verified();
            let review_d = prefill_review_rating()
                .map(|r| (r, prefill_review_comment()));
            let mch_header_d = prefill_mch_header();
            let followers_d = prefill_followers();
            let products_count_d = prefill_products_count();
            let rating_d = prefill_rating();
            sedang_mengunduh.set(true);
            let navigate_sv2 = navigate_sv;
            spawn_local(async move {
                if let Err(e) = preload_fonts().await { web_sys::console::warn_1(&format!("font: {:?}", e).into()); }
                let dpr = get_dpr();
                let Some((canvas, ctx, cw, ch)) = create_export_canvas(dpr) else { sedang_mengunduh.set(false); return; };
                let render_res = if is_merchant_d {
                    render_merchant_card_to_canvas(
                        &ctx, cw, ch, &cover, &mch_header_d, &bg_m, &bg_c, &title,
                        verified_d, followers_d, products_count_d, rating_d, review_d,
                    ).await
                } else {
                    render_product_card_to_canvas(
                        &ctx, cw, ch, &cover, &bg_m, &bg_c, &ev_filter,
                        &title, &date, &venue, &price, is_ticket_d,
                    ).await
                };
                if let Err(e) = render_res {
                    web_sys::console::warn_1(&format!("story card render: {}", e).into());
                    ctx.set_fill_style_str("#0d0d18");
                    ctx.fill_rect(0.0, 0.0, cw, ch);
                }
                let (dom_w, dom_h) = web_sys::window().and_then(|w| w.document())
                    .and_then(|d| d.query_selector(".sc-canvas-frame").ok().flatten())
                    .and_then(|el| el.dyn_into::<web_sys::Element>().ok())
                    .map(|el| { let r = el.get_bounding_client_rect(); (r.width(), r.height()) })
                    .unwrap_or((390.0, 844.0));
                render_overlays_to_canvas(&ctx, &ovls, cw, ch, dom_w, dom_h, dpr);
                match canvas_to_blob(&canvas).await {
                    Ok(blob) => {
                        let ts = web_sys::js_sys::Date::now() as u64;
                        trigger_download_blob(&blob, &format!("product_story_{}.{}", ts, export_ext()));
                        let from = read_from_page(); clear_from_page();
                        let nav = navigate_sv2.get_value();
                        exit_page_to(is_exiting, move || { nav(&from, Default::default()); }, EXIT_DURATION_MS);
                    }
                    Err(e) => { error_unggah.set(Some(format!("Export gagal: {}", e))); }
                }
                canvas.set_width(0); canvas.set_height(0);
                sedang_mengunduh.set(false);
            });
            return;
        }

        let vid = is_video.get_untracked();
        let bg_info = {
            let mode = bg_mode.get_untracked();
            if mode == "blur" { None }
            else if mode == "solid" { Some(BgExport::Solid(bg_solid_color.get_untracked())) }
            else { gradient_colors(&mode).map(|(s,e)| BgExport::Gradient { color_start: s, color_end: e }) }
        };
        let nama = file_terpilih.get_untracked().map(|f| f.name()).filter(|n| !n.is_empty())
            .unwrap_or_else(|| "story".to_string());
        if url.starts_with("blob:") {
            export_story_canvas(url, vid, ovls, filter, nama, scale, bg_info, sedang_mengunduh);
        } else {
            error_unggah.set(Some("Pilih gambar dari galeri terlebih dahulu.".to_string()));
        }
    };

    // ── Bagikan ───────────────────────────────────────────────────────────────
    let bagikan = move || {
        let slug = product_meta_sig.get().map(|m| m.event_slug).filter(|s| !s.is_empty());

        if let Some(file) = file_terpilih.get() {
            let ovls_snapshot: Vec<StoryOverlay> = overlays.with(|o| o.clone());
            let pratinjau_url = url_pratinjau.get_untracked().unwrap_or_default();
            let filter_val    = filter_dipilih.get_untracked();
            let scale_val     = bg_scale.get_untracked();
            let bg_mode_val   = bg_mode.get_untracked();
            let bg_color_val  = bg_solid_color.get_untracked();
            let is_vid        = is_video.get_untracked();
            sedang_mengunggah.set(true);
            store_ctx.uploading.set(true);
            error_unggah.set(None);
            let ctx = store_ctx;
            do_exit();

            spawn_local(async move {
                let has_overlays = !ovls_snapshot.is_empty();
                if !has_overlays || is_vid {
                    #[cfg(target_arch = "wasm32")]
                    match upload_story_file(&file, None, None).await {
                        Ok(_)  => { ctx.uploading.set(false); ctx.load(); }
                        Err(e) => { ctx.uploading.set(false); web_sys::console::error_1(&format!("Upload gagal: {}", e).into()); }
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    { let _ = &file; ctx.uploading.set(false); }
                    return;
                }
                if let Err(e) = preload_fonts().await { web_sys::console::warn_1(&format!("font: {:?}", e).into()); }
                let dpr = get_dpr();
                let Some((canvas, render_ctx, cw, ch)) = create_export_canvas(dpr) else { ctx.uploading.set(false); return; };

                if bg_mode_val != "blur" {
                    if bg_mode_val == "solid" {
                        render_ctx.set_fill_style_str(&bg_color_val);
                        render_ctx.fill_rect(0.0, 0.0, cw, ch);
                    } else if let Some((cs, ce)) = gradient_colors(&bg_mode_val) {
                        if let Ok(g) = render_ctx.create_linear_gradient(0.0,0.0,0.0,ch).dyn_into::<web_sys::CanvasGradient>() {
                            let _ = g.add_color_stop(0.0, cs); let _ = g.add_color_stop(1.0, ce);
                            let _ = render_ctx.set_fill_style_canvas_gradient(&g); render_ctx.fill_rect(0.0,0.0,cw,ch);
                        }
                    }
                }
                let fs = css_filter_string(&filter_val);
                if fs != "none" { render_ctx.set_filter(fs); }
                let _ = load_img_to_canvas(&pratinjau_url, &render_ctx, cw, ch, scale_val, false, true).await;
                render_ctx.set_filter("none");

                let (dom_w, dom_h) = web_sys::window().and_then(|w| w.document())
                    .and_then(|d| d.query_selector(".sc-canvas-frame").ok().flatten())
                    .and_then(|el| el.dyn_into::<web_sys::Element>().ok())
                    .map(|el| { let r = el.get_bounding_client_rect(); (r.width(), r.height()) })
                    .unwrap_or((390.0, 844.0));
                render_overlays_to_canvas(&render_ctx, &ovls_snapshot, cw, ch, dom_w, dom_h, dpr);

                let blob = match canvas_to_blob(&canvas).await {
                    Ok(b) => b,
                    Err(e) => { ctx.uploading.set(false); web_sys::console::error_1(&format!("blob: {}", e).into()); canvas.set_width(0); canvas.set_height(0); return; }
                };
                canvas.set_width(0); canvas.set_height(0);

                let bits = web_sys::js_sys::Array::new(); bits.push(&blob);
                let opts = web_sys::FilePropertyBag::new(); opts.set_type(export_mime());
                let upload_file = match web_sys::File::new_with_blob_sequence_and_options(&bits, &format!("story.{}", export_ext()), &opts) {
                    Ok(f) => f, Err(_) => { ctx.uploading.set(false); return; }
                };
                #[cfg(target_arch = "wasm32")]
                match upload_story_file(&upload_file, None, None).await {
                    Ok(_)  => { ctx.uploading.set(false); ctx.load(); }
                    Err(e) => { ctx.uploading.set(false); web_sys::console::error_1(&format!("upload overlay: {}", e).into()); }
                }
                #[cfg(not(target_arch = "wasm32"))]
                { let _ = upload_file; ctx.uploading.set(false); }
            });
            return;
        }

        // Mode B: product story
        if has_product_prefill.get() && !user_overrode_prefill.get() {
            let cover        = event_cover_url.get_value();
            let ovls_snapshot: Vec<StoryOverlay> = overlays.with(|o| o.clone());
            let ev_filter    = filter_dipilih.get_untracked();
            let ev_bg_mode   = bg_mode.get_untracked();
            let ev_bg_color  = bg_solid_color.get_untracked();
            let title        = prefill_title();
            let date         = prefill_date();
            let venue        = prefill_venue();
            let price        = prefill_price();
            let is_ticket_u  = prefill_is_ticket();
            let is_merchant_u = prefill_is_merchant();
            let verified_u = prefill_verified();
            let review_u = prefill_review_rating().map(|r| (r, prefill_review_comment()));
            let mch_header_u = prefill_mch_header();
            let followers_u = prefill_followers();
            let products_count_u = prefill_products_count();
            let rating_u = prefill_rating();
            if cover.is_empty() && !is_merchant_u {
                error_unggah.set(Some("Cover produk tidak tersedia di URL.".into()));
                return;
            }
            sedang_mengunggah.set(true);
            store_ctx.uploading.set(true);
            error_unggah.set(None);
            let ctx = store_ctx;
            do_exit();

            spawn_local(async move {
                if let Err(e) = preload_fonts().await { web_sys::console::warn_1(&format!("font: {:?}", e).into()); }
                let dpr = get_dpr();
                let Some((canvas, render_ctx, cw, ch)) = create_export_canvas(dpr) else { ctx.uploading.set(false); return; };
                let render_res = if is_merchant_u {
                    render_merchant_card_to_canvas(
                        &render_ctx, cw, ch, &cover, &mch_header_u, &ev_bg_mode, &ev_bg_color,
                        &title, verified_u, followers_u, products_count_u, rating_u, review_u,
                    ).await
                } else {
                    render_product_card_to_canvas(
                        &render_ctx, cw, ch, &cover, &ev_bg_mode, &ev_bg_color, &ev_filter,
                        &title, &date, &venue, &price, is_ticket_u,
                    ).await
                };
                if let Err(e) = render_res {
                    web_sys::console::warn_1(&format!("story card render: {}", e).into());
                }
                let (dom_w, dom_h) = web_sys::window().and_then(|w| w.document())
                    .and_then(|d| d.query_selector(".sc-canvas-frame").ok().flatten())
                    .and_then(|el| el.dyn_into::<web_sys::Element>().ok())
                    .map(|el| { let r = el.get_bounding_client_rect(); (r.width(), r.height()) })
                    .unwrap_or((390.0, 844.0));
                render_overlays_to_canvas(&render_ctx, &ovls_snapshot, cw, ch, dom_w, dom_h, dpr);
                let blob = match canvas_to_blob(&canvas).await {
                    Ok(b) => b,
                    Err(e) => { ctx.uploading.set(false); web_sys::console::error_1(&format!("blob: {}", e).into()); canvas.set_width(0); canvas.set_height(0); return; }
                };
                canvas.set_width(0); canvas.set_height(0);
                let bits = web_sys::js_sys::Array::new(); bits.push(&blob);
                let opts = web_sys::FilePropertyBag::new(); opts.set_type(export_mime());
                let file = match web_sys::File::new_with_blob_sequence_and_options(&bits, &format!("product_story.{}", export_ext()), &opts) {
                    Ok(f) => f, Err(_) => { ctx.uploading.set(false); return; }
                };
                let title_opt = if title.is_empty() { None } else { Some(title.clone()) };
                #[cfg(target_arch = "wasm32")]
                match upload_story_file(&file, slug, title_opt).await {
                    Ok(_)  => { ctx.uploading.set(false); ctx.load(); }
                    Err(e) => { ctx.uploading.set(false); web_sys::console::error_1(&format!("produk story upload: {}", e).into()); }
                }
                #[cfg(not(target_arch = "wasm32"))]
                { let _ = (file, slug, title_opt); ctx.uploading.set(false); }
            });
        }
    };

    // ── Overlay operations ────────────────────────────────────────────────────
    let tambah_teks = move || {
        let teks = input_teks.get();
        if teks.trim().is_empty() { return; }
        let y = overlays.with(|o| (30.0 + (o.len() % 5) as f64 * 10.0).min(70.0));
        let z = z_counter.get_untracked() + 1;
        z_counter.set(z);
        overlays.update(|o| o.push(StoryOverlay {
            id: buat_id("teks"), overlay_type: OverlayType::Text,
            x: 50.0, y, content: Some(teks), color: Some(warna_teks_sig.get()),
            font_size: Some(ukuran_teks.get()), rotation: Some(0.0),
            emoji: None, scale: Some(1.0), z_index: z,
            text_style: Some(text_style.get()), text_align: Some(text_align.get()),
        }));
        input_teks.set(String::new());
        alat_aktif.set(Alat::None);
    };

    let tambah_stiker = move |emoji: String| {
        let (x, y) = overlays.with(|o| { let n = o.len() % 5; ((45.0 + n as f64 * 6.0).min(70.0), (35.0 + n as f64 * 9.0).min(70.0)) });
        let z = z_counter.get_untracked() + 1;
        z_counter.set(z);
        overlays.update(|o| o.push(StoryOverlay {
            id: buat_id("stiker"), overlay_type: OverlayType::Sticker,
            x, y, emoji: Some(emoji), scale: Some(1.2), rotation: Some(0.0),
            content: None, color: None, font_size: None, z_index: z,
            text_style: None, text_align: None,
        }));
        alat_aktif.set(Alat::None);
    };

    let perbarui_overlay = move |updated: StoryOverlay| {
        overlays.update(|o| { if let Some(p) = o.iter().position(|x| x.id == updated.id) { o[p] = updated; } });
    };
    let hapus_overlay = move |id: String| {
        overlays.update(|o| o.retain(|x| x.id != id));
        if selected_overlay_id.get_untracked() == id { selected_overlay_id.set(String::new()); }
    };
    let pilih_overlay = move |id: String| {
        let now = web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0);
        let (lid, lts) = last_tap.get_value();
        let dbl = lid == id && (now - lts) < 300.0;
        last_tap.set_value((id.clone(), now));
        if dbl && !id.is_empty() { selected_overlay_id.set(id); return; }
        let cur = selected_overlay_id.get_untracked();
        if cur == id && !id.is_empty() {
            selected_overlay_id.set(String::new());
        } else {
            selected_overlay_id.set(id.clone());
            let z = z_counter.get_untracked() + 1; z_counter.set(z);
            overlays.update(|o| { if let Some(p) = o.iter().position(|x| x.id == id) { o[p].z_index = z; } });
        }
    };

    let kelas_media = move || {
        let f = filter_dipilih.get();
        if f == "normal" || !DAFTAR_FILTER.iter().any(|(k,_)| *k == f.as_str()) { "sc-media".to_string() }
        else { format!("sc-media filter-{}", f) }
    };

    let tutup_panel = move |ev: leptos::ev::KeyboardEvent| { if ev.key() == "Escape" { alat_aktif.set(Alat::None); } };
    let can_share = Memo::new(move |_| {
        // Mode merchant tak digate cover_img_ready — kartu punya fallback & canvas
        // memuat gambarnya sendiri; header kosong tak boleh menghalangi share.
        let is_merchant_mode = prefill_is_merchant() && !user_overrode_prefill.get();
        let is_product_mode = has_product_prefill.get() && !user_overrode_prefill.get() && !is_merchant_mode;
        !is_product_mode || cover_img_ready.get()
    });

    let on_swipe_start = move |ev: leptos::ev::TouchEvent| {
        if let Some(t) = ev.touches().get(0) { swipe_start_y.set_value(t.client_y() as f64); swipe_start_x.set_value(t.client_x() as f64); swipe_active.set_value(true); }
    };
    let on_swipe_end = move |ev: leptos::ev::TouchEvent| {
        if !swipe_active.get_value() { return; }
        swipe_active.set_value(false);
        if let Some(t) = ev.changed_touches().get(0) {
            let dy = (t.client_y() as f64) - swipe_start_y.get_value();
            let dx = (t.client_x() as f64) - swipe_start_x.get_value();
            if dy > 120.0 && dx.abs() < 80.0 { do_exit(); }
        }
    };

    // ── VIEW ──────────────────────────────────────────────────────────────────
    view! {
        <div
            class="sc-halaman"
            class:is-exiting=move || is_exiting.get()
            on:keydown=tutup_panel
            tabindex="-1"
        >
            <header class="sc-appbar">
                <button class="sc-tombol-bulat" aria-label="Tutup" on:click=move |_| do_exit()>
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <line x1="18" y1="6" x2="6" y2="18"/>
                        <line x1="6"  y1="6" x2="18" y2="18"/>
                    </svg>
                </button>
                <div class="sc-badge-langsung">
                    <span class="sc-dot-merah"></span>
                    "CERITA"
                </div>
                <button class="sc-tombol-bulat" aria-label="Unduh"
                    disabled=move || url_pratinjau.get().is_none() || sedang_mengunduh.get()
                    on:click=move |_| unduh_story()>
                    {move || if sedang_mengunduh.get() {
                        view! { <span class="sc-spinner" style="width:16px;height:16px;border-width:2px;"></span> }.into_any()
                    } else {
                        view! {
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/>
                                <polyline points="7 10 12 15 17 10"/>
                                <line x1="12" y1="15" x2="12" y2="3"/>
                            </svg>
                        }.into_any()
                    }}
                </button>
            </header>

            <Show when=move || error_unggah.get().is_some()>
                <div class="sc-notif-gagal" role="alert">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <circle cx="12" cy="12" r="10"/>
                        <line x1="12" y1="8" x2="12" y2="12"/>
                        <line x1="12" y1="16" x2="12.01" y2="16"/>
                    </svg>
                    <span>{move || error_unggah.get().unwrap_or_default()}</span>
                    <button class="sc-tombol-coba-lagi" on:click=move |_| { error_unggah.set(None); bagikan(); }>"Coba Lagi"</button>
                </div>
            </Show>

            <main class="sc-area-pratinjau"
                style=move || {
                    let mode = bg_mode.get();
                    if mode == "blur" { "background-color:#000;".to_string() }
                    else if mode == "solid" { format!("background-color:{};", bg_solid_color.get()) }
                    else if let Some(css) = BG_GRADIENTS.iter().find(|(k,_,_,_)| *k == mode.as_str()).map(|(_,c,_,_)| *c) {
                        format!("background:{};", css)
                    } else { "background-color:#000;".to_string() }
                }>

                <input type="file" accept="image/*,video/*" class="sc-input-file"
                    node_ref=file_ref on:change=saat_file_dipilih />

                <Show when=move || url_pratinjau.get().is_none()>
                    <div class="sc-panduan-fokus" on:click=move |_| { if let Some(i) = file_ref.get() { i.click(); } }>
                        <div class="sc-sudut sc-sudut--kiri-atas"></div>
                        <div class="sc-sudut sc-sudut--kanan-atas"></div>
                        <div class="sc-sudut sc-sudut--kiri-bawah"></div>
                        <div class="sc-sudut sc-sudut--kanan-bawah"></div>
                        <div class="sc-teks-panduan">
                            <div class="sc-ikon-kamera">
                                <svg width="44" height="44" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                    <path d="M23 19a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2h4l2-3h6l2 3h4a2 2 0 012 2z"/>
                                    <circle cx="12" cy="13" r="4"/>
                                </svg>
                            </div>
                            <p class="sc-judul-panduan">"Ketuk untuk memilih foto atau video"</p>
                            <p class="sc-sub-panduan">"Rasio terbaik 9:16"</p>
                        </div>
                    </div>
                </Show>

                <Show when=move || url_pratinjau.get().is_some()>
                    <div
                        class="sc-canvas-frame"
                        class:sc-hero-transition=move || hero_transition.get()
                        on:touchstart=on_swipe_start
                        on:touchend=on_swipe_end
                    >
                        <Show when=move || trash_zone_active.get()>
                            <div class="sc-trash-zone sc-trash-zone--active" aria-hidden="true">
                                <div class="sc-trash-zone-icon">
                                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                        <polyline points="3 6 5 6 21 6"/>
                                        <path d="M19 6l-1 14H6L5 6"/>
                                        <path d="M10 11v6M14 11v6"/>
                                        <path d="M9 6V4h6v2"/>
                                    </svg>
                                </div>
                            </div>
                        </Show>

                        <Show when=move || bg_mode.get() == "blur" && !has_product_prefill.get()>
                            {move || url_pratinjau.get().map(|url| view! {
                                <div class="sc-canvas-bg" style=format!("background-image:url('{}');", url) />
                            })}
                        </Show>

                        <Show when=move || bg_mode.get() != "blur" && (user_overrode_prefill.get() || !has_product_prefill.get())>
                            <div class="sc-canvas-bg sc-canvas-bg--solid"
                                style=move || {
                                    let mode = bg_mode.get();
                                    if mode == "solid" { format!("background-color:{};", bg_solid_color.get()) }
                                    else if let Some(css) = BG_GRADIENTS.iter().find(|(k,_,_,_)| *k == mode.as_str()).map(|(_,c,_,_)| *c) {
                                        format!("background:{};", css)
                                    } else { "background:#000;".to_string() }
                                } />
                        </Show>

                        <div node_ref=media_layer_ref class="sc-layer-media">
                            {move || url_pratinjau.get().map(|url| {
                                // Mode merchant (share toko) memakai konvensi slug "m/{id}"
                                // sehingga has_product_prefill juga true — WAJIB dicek lebih
                                // dulu agar pratinjau memakai kartu MERCHANT (hero+avatar+
                                // container FOLLOWERS/EVENTS/RATING, mirip halaman /m/{id}),
                                // bukan kartu product.
                                let is_merchant = prefill_is_merchant() && !user_overrode_prefill.get();
                                let is_product = has_product_prefill.get() && !user_overrode_prefill.get() && !is_merchant;
                                let is_blob  = url.starts_with("blob:");
                                if is_merchant {
                                    let initial: String = prefill_title().chars().next()
                                        .unwrap_or('P').to_uppercase().collect();
                                    view! {
                                        <div class="sc-mch-preview-frame"
                                            style=move || {
                                                let mode = bg_mode.get();
                                                if mode == "blur" { String::new() }
                                                else if mode == "solid" { format!("background-color:{};", bg_solid_color.get()) }
                                                else if let Some(css) = BG_GRADIENTS.iter().find(|(k,_,_,_)| *k == mode.as_str()).map(|(_,c,_,_)| *c) { format!("background:{};", css) }
                                                else { "background-color:#0d0d18;".to_string() }
                                            }>
                                            <Show when=move || bg_mode.get() == "blur">
                                                <img
                                                    src=move || { let h = prefill_mch_header(); if h.is_empty() { prefill_cover() } else { h } }
                                                    class="sc-product-bg-img" alt=""
                                                    on:load=move |_| cover_img_ready.set(true) />
                                                <div class="sc-product-dark-overlay" />
                                            </Show>
                                            <div class="sc-mch-card">
                                                <div class="sc-mch-hero">
                                                    {move || {
                                                        let h = prefill_mch_header();
                                                        let src = if h.is_empty() { prefill_cover() } else { h };
                                                        (!src.is_empty()).then(|| view! {
                                                            <img src=src class="sc-mch-hero-img" alt=""
                                                                on:load=move |_| cover_img_ready.set(true) />
                                                        })
                                                    }}
                                                    <div class="sc-mch-hero-fade" />
                                                </div>
                                                <div class="sc-mch-body">
                                                    <div class="sc-mch-avatar-wrap">
                                                        {move || {
                                                            let logo = prefill_cover();
                                                            if logo.is_empty() {
                                                                view! { <div class="sc-mch-avatar sc-mch-avatar--fallback">{initial.clone()}</div> }.into_any()
                                                            } else {
                                                                view! { <img class="sc-mch-avatar" src=logo alt="" /> }.into_any()
                                                            }
                                                        }}
                                                        <Show when=move || prefill_verified()>
                                                            <span class="sc-mch-badge" aria-hidden="true">"✓"</span>
                                                        </Show>
                                                    </div>
                                                    <h2 class="sc-mch-name">{move || prefill_title().to_uppercase()}</h2>
                                                    <div class="sc-mch-stats">
                                                        <div class="sc-mch-stat">
                                                            <span class="sc-mch-stat-num">{move || crate::web::pages::merchant_public::fmt_count(prefill_followers())}</span>
                                                            <span class="sc-mch-stat-label">"FOLLOWERS"</span>
                                                        </div>
                                                        <div class="sc-mch-stat">
                                                            <span class="sc-mch-stat-num">{move || crate::web::pages::merchant_public::fmt_count(prefill_products_count())}</span>
                                                            <span class="sc-mch-stat-label">"PRODUK"</span>
                                                        </div>
                                                        <div class="sc-mch-stat">
                                                            <span class="sc-mch-stat-num">
                                                                {move || format!("{:.1}", prefill_rating())}
                                                                <span class="sc-mch-stat-star">"★"</span>
                                                            </span>
                                                            <span class="sc-mch-stat-label">"RATING"</span>
                                                        </div>
                                                    </div>
                                                    <div class="sc-mch-cta">"KUNJUNGI PROFIL ↗"</div>
                                                </div>
                                            </div>
                                        </div>
                                    }.into_any()
                                } else if is_product {
                                    let url_sv = StoredValue::new(url.clone());
                                    view! {
                                        <div class="sc-product-preview-frame"
                                            style=move || {
                                                let mode = bg_mode.get();
                                                if mode == "blur" { String::new() }
                                                else if mode == "solid" { format!("background-color:{};", bg_solid_color.get()) }
                                                else if let Some(css) = BG_GRADIENTS.iter().find(|(k,_,_,_)| *k == mode.as_str()).map(|(_,c,_,_)| *c) { format!("background:{};", css) }
                                                else { "background-color:#1a1a2e;".to_string() }
                                            }>
                                            <Show when=move || bg_mode.get() == "blur">
                                                <img src=move || url_sv.get_value() class="sc-product-bg-img sc-media" alt=""
                                                    on:load=move |_| cover_img_ready.set(true) />
                                                <div class="sc-product-dark-overlay" />
                                            </Show>
                                            <div class="sc-product-card">
                                                <div class="sc-product-card-cover-wrap">
                                                    <img src=move || url_sv.get_value() class="sc-product-card-cover-img" alt=""
                                                        on:load=move |_| cover_img_ready.set(true) />
                                                </div>
                                                <div class="sc-product-card-body">
                                                    <div class="sc-product-card-badge">
                                                        <span class="sc-product-card-dot"></span>
                                                        "KINETIC EXCLUSIVE"
                                                    </div>
                                                    <h2 class="sc-product-card-title">{move || prefill_title()}</h2>
                                                    <div class="sc-product-card-sep"></div>
                                                    <span class="sc-product-card-meta-label">"TANGGAL & LOKASI"</span>
                                                    <div class="sc-product-card-meta-row">
                                                        <span class="sc-product-card-date">{move || prefill_date()}</span>
                                                        <span class="sc-product-card-venue">{move || prefill_venue()}</span>
                                                    </div>
                                                    <div class="sc-product-card-price-pill">{move || prefill_price()}</div>
                                                    <Show when=move || prefill_is_ticket()>
                                                        <div class="sc-product-card-ticket-label">"✓ Pesanan berhasil dibuat"</div>
                                                    </Show>
                                                </div>
                                            </div>
                                        </div>
                                    }.into_any()
                                } else if is_video.get() {
                                    view! {
                                        <video src=url class=kelas_media()
                                            style=move || { let s = bg_scale.get(); if (s-1.0).abs() > 0.001 { format!("transform:scale({s:.3});transform-origin:center center;") } else { String::new() } }
                                            crossorigin=is_blob.then_some("anonymous")
                                            autoplay muted playsinline loop />
                                    }.into_any()
                                } else {
                                    let urlclone = url.clone();
                                    view! {
                                        <img src=url
                                            class=move || format!("{} sc-media--fit", kelas_media())
                                            style=move || { let s = bg_scale.get(); if (s-1.0).abs() > 0.001 { format!("transform:scale({s:.3});transform-origin:center center;") } else { String::new() } }
                                            crossorigin=is_blob.then_some("anonymous")
                                            alt="Story media"
                                            on:load=move |ev| {
                                                if let Some(t) = ev.target() {
                                                    if let Ok(im) = t.dyn_into::<web_sys::HtmlImageElement>() {
                                                        let iw = im.natural_width() as f64; let ih = im.natural_height() as f64;
                                                        let cf = cover_factor(iw, ih, 9.0, 16.0);
                                                        media_cover_factor.set(cf); bg_scale.set(cf);
                                                    }
                                                }
                                            }
                                            on:error=move |_| { web_sys::console::error_1(&format!("img err: {}", urlclone).into()); }
                                        />
                                    }.into_any()
                                }
                            })}
                        </div>

                        <Show when=move || !has_product_prefill.get() || user_overrode_prefill.get()>
                            <div class="sc-media-overlay" aria-hidden="true"></div>
                        </Show>

                        <Show when=move || has_product_prefill.get() && !user_overrode_prefill.get()>
                            <div class="sc-product-prefill-banner">
                                <div class="sc-product-prefill-icon">
                                    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
                                        <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z"/>
                                    </svg>
                                </div>
                                <div class="sc-product-prefill-text">
                                    <span class="sc-product-prefill-label">"Berbagi produk"</span>
                                    <span class="sc-product-prefill-title">{move || prefill_title()}</span>
                                </div>
                                <div class="sc-product-prefill-badge">"LIVE PULSE"</div>
                            </div>
                        </Show>

                        <div class="sc-layer-overlays"
                            on:click=move |ev| { if let (Some(t), Some(c)) = (ev.target(), ev.current_target()) { if t == c { selected_overlay_id.set(String::new()); } } }>
                            <For
                                each=move || { let mut o = overlays.get(); o.sort_by_key(|x| x.z_index); o }
                                key=|ov| ov.id.clone()
                                children=move |overlay| {
                                    let oid  = overlay.id.clone();
                                    let oid2 = oid.clone();
                                    view! {
                                        <DraggableOverlay
                                            overlay=overlay
                                            on_update=Callback::new(perbarui_overlay)
                                            on_delete=Callback::new(hapus_overlay)
                                            on_bring_to_front=Callback::new(move |id: String| {
                                                let z = z_counter.get_untracked() + 1; z_counter.set(z);
                                                overlays.update(|o| { if let Some(p) = o.iter().position(|x| x.id == id) { o[p].z_index = z; } });
                                            })
                                            is_selected=Signal::derive(move || { let s = selected_overlay_id.get(); !s.is_empty() && s == oid })
                                            on_select=Callback::new(move |_| pilih_overlay(oid2.clone()))
                                            on_trash_hover=Some(Callback::new(move |active: bool| { trash_zone_active.set(active); }))
                                        />
                                    }
                                }
                            />
                        </div>
                    </div>
                </Show>

                <Show when=move || alat_aktif.get() == Alat::Latar && url_pratinjau.get().is_some()>
                    <div class="sc-bg-picker">
                        <button class="sc-bg-chip"
                            class:sc-bg-chip--aktif=move || bg_mode.get() == "blur"
                            style="background:conic-gradient(#667eea,#764ba2,#f093fb,#f5576c,#ff9f43,#10ac84,#667eea);"
                            on:click=move |_| bg_mode.set("blur".to_string()) aria-label="Blur" />
                        {BG_SOLID_COLORS.iter().map(|&c| {
                            let col = c.to_string(); let ca = col.clone(); let cc = col.clone();
                            view! {
                                <button class="sc-bg-chip"
                                    class:sc-bg-chip--aktif=move || bg_mode.get() == "solid" && bg_solid_color.get() == ca
                                    style=format!("background-color:{};", col)
                                    on:click=move |_| { bg_mode.set("solid".to_string()); bg_solid_color.set(cc.clone()); }
                                    aria-label=format!("Warna {}", c) />
                            }
                        }).collect_view()}
                        {BG_GRADIENTS.iter().map(|&(key, css, _, _)| {
                            let k = key.to_string(); let ka = k.clone(); let kc = k.clone();
                            view! {
                                <button class="sc-bg-chip"
                                    class:sc-bg-chip--aktif=move || bg_mode.get() == ka
                                    style=format!("background:{};", css)
                                    on:click=move |_| bg_mode.set(kc.clone())
                                    aria-label=format!("Gradient {}", key) />
                            }
                        }).collect_view()}
                    </div>
                </Show>

                <Show when=move || musik_dipilih.get().is_some()>
                    <div class="sc-badge-musik">
                        <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
                            <path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/>
                        </svg>
                        <span class="sc-teks-badge-musik">{move || musik_dipilih.get().map(|(_,j,a)| format!("{} • {}", j, a)).unwrap_or_default()}</span>
                        <button class="sc-hapus-musik" aria-label="Hapus musik"
                            on:click=move |ev| { ev.stop_propagation(); musik_dipilih.set(None); }>
                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3">
                                <line x1="18" y1="6" x2="6" y2="18"/>
                                <line x1="6"  y1="6" x2="18" y2="18"/>
                            </svg>
                        </button>
                    </div>
                </Show>
            </main>

            <aside class="sc-toolbar-samping">
                <TombolAlat aktif=alat_aktif target=Alat::Teks label="Teks"
                    on_click=move |ev: leptos::ev::MouseEvent| { ev.stop_propagation(); alat_aktif.update(|t| *t = if *t == Alat::Teks { Alat::None } else { Alat::Teks }); }>
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <polyline points="4 7 4 4 20 4 20 7"/><line x1="9" y1="20" x2="15" y2="20"/><line x1="12" y1="4" x2="12" y2="20"/>
                    </svg>
                    <span class="sc-tombol-alat-label">"Teks"</span>
                </TombolAlat>
                <TombolAlat aktif=alat_aktif target=Alat::Stiker label="Stiker"
                    on_click=move |ev: leptos::ev::MouseEvent| { ev.stop_propagation(); alat_aktif.update(|t| *t = if *t == Alat::Stiker { Alat::None } else { Alat::Stiker }); }>
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <rect x="3" y="3" width="18" height="18" rx="4"/><path d="M8 12h8M12 8v8"/>
                    </svg>
                    <span class="sc-tombol-alat-label">"Stiker"</span>
                </TombolAlat>
                <TombolAlat aktif=alat_aktif target=Alat::Musik label="Musik"
                    on_click=move |ev: leptos::ev::MouseEvent| { ev.stop_propagation(); alat_aktif.update(|t| *t = if *t == Alat::Musik { Alat::None } else { Alat::Musik }); }>
                    <div class="sc-ikon-musik-wrap">
                        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>
                        </svg>
                        <Show when=move || musik_dipilih.get().is_some()>
                            <span class="sc-dot-musik"></span>
                        </Show>
                    </div>
                    <span class="sc-tombol-alat-label">"Musik"</span>
                </TombolAlat>
                <TombolAlat aktif=alat_aktif target=Alat::Filter label="Filter"
                    on_click=move |ev: leptos::ev::MouseEvent| { ev.stop_propagation(); alat_aktif.update(|t| *t = if *t == Alat::Filter { Alat::None } else { Alat::Filter }); }>
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M15 4V2M15 16v-2M8 9h2M20 9h2M17.8 11.8L19.2 13.2M17.8 6.2L19.2 4.8"/>
                        <circle cx="15" cy="9" r="3"/>
                    </svg>
                    <span class="sc-tombol-alat-label">"Filter"</span>
                </TombolAlat>
                <TombolAlat aktif=alat_aktif target=Alat::Latar label="Latar"
                    on_click=move |ev: leptos::ev::MouseEvent| { ev.stop_propagation(); alat_aktif.update(|t| *t = if *t == Alat::Latar { Alat::None } else { Alat::Latar }); }>
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <circle cx="13.5" cy="6.5"  r="2.5" fill="currentColor" stroke="none"/>
                        <circle cx="17.5" cy="10.5" r="2.5" fill="currentColor" stroke="none" opacity="0.6"/>
                        <circle cx="8.5"  cy="11.5" r="2.5" fill="currentColor" stroke="none" opacity="0.4"/>
                        <path d="M2 12C2 6.477 6.477 2 12 2s10 4.477 10 10-4.477 10-10 10S2 17.523 2 12z"/>
                    </svg>
                    <span class="sc-tombol-alat-label">"Latar"</span>
                </TombolAlat>
            </aside>

            <Show when=move || alat_aktif.get() == Alat::Teks>
                <PanelGeser judul="Teks" on_tutup=move || alat_aktif.set(Alat::None)>
                    <div class="sc-text-preview-box"
                        style=move || format!("color:{}; font-size:{}px; text-align:{}; background:{};",
                            warna_teks_sig.get(), ukuran_teks.get(), text_align.get(),
                            if text_bg_enabled.get() { "rgba(0,0,0,0.75)" } else { "transparent" })>
                        {move || if input_teks.get().is_empty() { "Ketik sesuatu...".to_string().into_view() } else { input_teks.get().into_view() }}
                    </div>
                    <div class="sc-text-edit-row">
                        <div class="sc-size-rail">
                            <input type="range" min="14" max="120" step="2" class="sc-slider-vertical"
                                prop:value=move || ukuran_teks.get().to_string()
                                on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<i32>() { ukuran_teks.set(v); } } />
                            <span class="sc-size-label">{move || ukuran_teks.get().to_string()}</span>
                        </div>
                        <div class="sc-text-controls">
                            <div class="sc-font-carousel">
                                {[("classic","Classic"),("modern","Modern"),("strong","Strong"),("typewriter","Typewriter")].iter().map(|(k,l)| {
                                    let fk = *k;
                                    view! {
                                        <button class="sc-font-chip"
                                            class:sc-font-chip--aktif=move || text_style.get() == fk
                                            on:click=move |_| text_style.set(fk.to_string())>{*l}</button>
                                    }
                                }).collect_view()}
                            </div>
                            <div class="sc-align-row">
                                {[("left","⬅"),("center","↔"),("right","➡")].iter().map(|(a,icon)| {
                                    let al = *a;
                                    view! {
                                        <button class="sc-align-btn"
                                            class:sc-align-btn--aktif=move || text_align.get() == al
                                            on:click=move |_| text_align.set(al.to_string())>{*icon}</button>
                                    }
                                }).collect_view()}
                            </div>
                            <button class="sc-bg-toggle"
                                class:sc-bg-toggle--on=move || text_bg_enabled.get()
                                on:click=move |_| text_bg_enabled.update(|b| *b = !*b)>"BG"</button>
                        </div>
                    </div>
                    <div class="sc-baris-warna">
                        {WARNA_TEKS.iter().map(|&c| view! { <TombolWarna warna=c.to_string() dipilih=warna_teks_sig /> }).collect_view()}
                    </div>
                    <div class="sc-area-input-teks">
                        <input type="text" class="sc-input-teks-besar" placeholder="Ketik sesuatu..." autofocus
                            prop:value=move || input_teks.get()
                            on:input=move |ev| input_teks.set(event_target_value(&ev))
                            on:keydown=move |ev| { if ev.key() == "Enter" { tambah_teks(); } } />
                        <button class="sc-tombol-done" on:click=move |_| tambah_teks()>"Selesai"</button>
                    </div>
                </PanelGeser>
            </Show>

            <Show when=move || alat_aktif.get() == Alat::Stiker>
                <PanelGeser judul="Stiker" on_tutup=move || alat_aktif.set(Alat::None)>
                    <div class="sc-grid-stiker">
                        {STIKER.iter().map(|&e| view! {
                            <button class="sc-tombol-stiker" aria-label=format!("Stiker {}", e)
                                on:click=move |_| tambah_stiker(e.to_string())>{e}</button>
                        }).collect_view()}
                    </div>
                </PanelGeser>
            </Show>

            <Show when=move || alat_aktif.get() == Alat::Musik>
                <PanelGeser judul="Musik" on_tutup=move || alat_aktif.set(Alat::None)>
                    <div class="sc-list-musik">
                        {DAFTAR_MUSIK.iter().map(|&(id, judul, artis)| {
                            let ids = id.to_string(); let idc = ids.clone();
                            let j = judul.to_string(); let a = artis.to_string();
                            view! {
                                <button class="sc-item-musik"
                                    class:sc-item-musik--aktif=move || musik_dipilih.get().map(|(m,_,_)| m == idc).unwrap_or(false)
                                    on:click=move |_| { musik_dipilih.set(Some((ids.clone(), j.clone(), a.clone()))); alat_aktif.set(Alat::None); }>
                                    <div class="sc-cover-musik">
                                        <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
                                            <path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/>
                                        </svg>
                                    </div>
                                    <div class="sc-info-musik">
                                        <span class="sc-judul-musik">{judul}</span>
                                        <span class="sc-artis-musik">{artis}</span>
                                    </div>
                                    {move || if musik_dipilih.get().map(|(m,_,_)| m == id).unwrap_or(false) {
                                        view! { <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#39ff8a" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg> }.into_any()
                                    } else { view! { <span></span> }.into_any() }}
                                </button>
                            }
                        }).collect_view()}
                    </div>
                </PanelGeser>
            </Show>

            <Show when=move || alat_aktif.get() == Alat::Filter>
                <PanelGeser judul="Filter" on_tutup=move || alat_aktif.set(Alat::None)>
                    <div class="sc-scroll-filter">
                        {DAFTAR_FILTER.iter().map(|&(k, l)| view! {
                            <button class="sc-item-filter"
                                class:sc-item-filter--aktif=move || filter_dipilih.get() == k
                                on:click=move |_| filter_dipilih.set(k.to_string())>
                                <div class=format!("sc-thumb-filter{}", if k == "normal" { String::new() } else { format!(" filter-{}", k) })>
                                    {if k == "normal" { view! { <span class="sc-label-normal">"●"</span> }.into_any() } else { view! { <span></span> }.into_any() }}
                                </div>
                                <span class="sc-nama-filter">{l}</span>
                            </button>
                        }).collect_view()}
                    </div>
                </PanelGeser>
            </Show>

            <footer class="sc-area-bawah">
                <button class="sc-tombol-galeri" aria-label="Buka galeri"
                    on:click=move |_| { if let Some(i) = file_ref.get() { i.click(); } }>
                    {move || match url_pratinjau.get() {
                        Some(url) if !has_product_prefill.get() || user_overrode_prefill.get() =>
                            view! { <img src=url class="sc-thumb-galeri" /> }.into_any(),
                        _ => view! {
                            <div class="sc-ikon-galeri">
                                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                                    <rect x="2" y="3" width="20" height="14" rx="2"/>
                                    <circle cx="8.5" cy="8.5" r="1.5"/>
                                    <polyline points="21 15 16 10 5 21"/>
                                </svg>
                            </div>
                        }.into_any(),
                    }}
                </button>

                <div class="sc-wrap-shutter">
                    <Show when=move || url_pratinjau.get().is_some()>
                        <button class="sc-tombol-shutter sc-tombol-shutter--kirim"
                            class:sc-tombol-shutter--muat=move || sedang_mengunggah.get()
                            on:click=move |_| bagikan()
                            disabled=move || sedang_mengunggah.get() || !can_share.get()
                            aria-label="Bagikan cerita">
                            {move || if sedang_mengunggah.get() {
                                view! { <span class="sc-spinner"></span> }.into_any()
                            } else {
                                view! { <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"><polyline points="20 6 9 17 4 12"/></svg> }.into_any()
                            }}
                        </button>
                    </Show>
                    <Show when=move || url_pratinjau.get().is_none()>
                        <button class="sc-tombol-shutter" aria-label="Pilih foto atau video"
                            on:click=move |_| { if let Some(i) = file_ref.get() { i.click(); } }>
                            <div class="sc-lingkaran-dalam"></div>
                        </button>
                    </Show>
                </div>

                <button class="sc-tombol-bulat" aria-label="Balik kamera">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M20 7H4V3l-2 2 2 2M4 17h16v4l2-2-2-2"/>
                        <circle cx="12" cy="12" r="3"/>
                    </svg>
                </button>
            </footer>
        </div>
    }
}
