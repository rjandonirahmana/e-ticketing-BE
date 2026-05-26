// ═══════════════════════════════════════════════════════════════════════════════
//  STORY CREATOR — Zero Network · Canvas-First · Instagram Frame
// ═══════════════════════════════════════════════════════════════════════════════
//
//  Dua mode:
//  A) Tanpa slug → user pick dari galeri, upload file langsung.
//  B) Dengan slug → data (cover_url, title) dari query params (sudah ada di
//     EventDetailPage), TIDAK ada fetch API. Canvas merender bingkai Instagram
//     (cover + separator + title + CTA) lalu hasilnya di-upload sebagai PNG.
//
//  Kunci arsitektur:
//  - Mode B: gambar event di-load oleh <img> di DOM dengan crossorigin="anonymous"
//    Jika server CDN sudah kirim CORS header (Access-Control-Allow-Origin: *),
//    canvas bisa drawImage langsung dari DOM img — zero extra fetch.
//  - Jika CORS gagal (localhost dev), fallback: gambar crop & render pakai canvas
//    sebagai solid-color bg + teks overlay saja (tetap sharable).
//  - Upload: selalu via upload_story_canvas (PNG blob) untuk mode B,
//    dan upload_story (File multipart) untuk mode A.
//  - EXIT ANIMATION: Instagram-style dismiss dengan spring physics.
//    spawn_local background tasks tetap jalan meski UI sudah exit.
//  - NAVIGASI KEMBALI: baca sessionStorage "story_from_page" yang di-set oleh
//    StoryBar saat user klik tombol add story. Fallback ke /explore.
// ═══════════════════════════════════════════════════════════════════════════════

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_query_map;
use leptos::wasm_bindgen::prelude::*;
use leptos::wasm_bindgen::JsCast;
use web_sys::{
    AddEventListenerOptions, CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement,
    HtmlInputElement, HtmlVideoElement,
};

use crate::csr::components::draggable_overlay::DraggableOverlay;
use crate::csr::hooks::use_nav;
use crate::csr::services::story as story_svc;
use crate::csr::services::story::EventStoryMeta;
use crate::csr::state::stories::{use_stories_store, OverlayType, StoryOverlay};

// ── Binding manual ke JS canvas.toBlob(callback, mimeType, quality) ───────────
// web_sys hanya meng-generate overload 2-argumen (callback + mime). Parameter
// qualityArgument bertipe `any` di WebIDL sehingga di-skip oleh generator.
// ═══════════════════════════════════════════════════════════════════════════════
//  ENUMS & KONSTANTA
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone, PartialEq, Debug, Copy)]
enum Alat {
    None,
    Teks,
    Stiker,
    Musik,
    Filter,
    Latar,
}

const BG_SOLID_COLORS: &[&str] = &[
    "#000000", "#1a1a2e", "#16213e", "#0f3460", "#533483", "#e94560", "#ff9f43", "#10ac84",
    "#0984e3", "#6c5ce7", "#fd79a8", "#00cec9", "#2d3436", "#636e72",
];

const BG_GRADIENTS: &[(&str, &str, &str, &str)] = &[
    (
        "purple-haze",
        "linear-gradient(135deg,#667eea 0%,#764ba2 100%)",
        "#667eea",
        "#764ba2",
    ),
    (
        "sunset",
        "linear-gradient(135deg,#f093fb 0%,#f5576c 100%)",
        "#f093fb",
        "#f5576c",
    ),
    (
        "ocean",
        "linear-gradient(135deg,#4facfe 0%,#00f2fe 100%)",
        "#4facfe",
        "#00f2fe",
    ),
    (
        "forest",
        "linear-gradient(135deg,#43e97b 0%,#38f9d7 100%)",
        "#43e97b",
        "#38f9d7",
    ),
    (
        "midnight",
        "linear-gradient(135deg,#0f2027 0%,#203a43 50%,#2c5364 100%)",
        "#0f2027",
        "#2c5364",
    ),
    (
        "candy",
        "linear-gradient(135deg,#ff9a9e 0%,#fecfef 100%)",
        "#ff9a9e",
        "#fecfef",
    ),
];

const STIKER: &[&str] = &[
    "❤️", "🔥", "😂", "😍", "👏", "🎉", "💯", "✨", "🙌", "😭", "🥰", "🤔", "👍", "🎵", "🎸", "🌟",
    "🎤", "🏆", "🎶", "💫", "🌈", "🍀", "🎯", "💎", "🦋", "🌺", "🔮", "🍕", "🌙", "☀️",
];

const DAFTAR_MUSIK: &[(&str, &str, &str)] = &[
    ("1", "As It Was", "Harry Styles"),
    ("2", "Heat Waves", "Glass Animals"),
    ("3", "Stay", "The Kid LAROI & Justin Bieber"),
    ("4", "Levitating", "Dua Lipa"),
    ("5", "Good 4 U", "Olivia Rodrigo"),
    ("6", "Montero", "Lil Nas X"),
    ("7", "Peaches", "Justin Bieber"),
    ("8", "Kiss Me More", "Doja Cat"),
    ("9", "Save Your Tears", "The Weeknd"),
    ("10", "Butter", "BTS"),
];

const DAFTAR_FILTER: &[(&str, &str)] = &[
    ("normal", "Normal"),
    ("clarendon", "Cerah"),
    ("gingham", "Hangat"),
    ("moon", "Monokrom"),
    ("lark", "Lark"),
    ("reyes", "Reyes"),
    ("juno", "Juno"),
    ("slumber", "Slumber"),
    ("crema", "Crema"),
    ("ludwig", "Ludwig"),
];

const WARNA_TEKS: &[&str] = &[
    "#ffffff", "#000000", "#ff3040", "#ffcc00", "#39ff8a", "#4f6bff", "#ff00ff", "#00ffff",
    "#ff6b35", "#7209b7", "#f72585", "#4cc9f0",
];

// ═══════════════════════════════════════════════════════════════════════════════
//  EXIT ANIMATION CONFIG
// ═══════════════════════════════════════════════════════════════════════════════

const EXIT_DURATION_MS: f64 = 480.0;

// ── Baca halaman asal dari sessionStorage (di-set oleh StoryBar) ─────────────
fn read_from_page() -> String {
    web_sys::window()
        .and_then(|w| w.session_storage().ok())
        .flatten()
        .and_then(|s| s.get_item("story_from_page").ok())
        .flatten()
        .unwrap_or_else(|| "/explore".to_string())
}

fn clear_from_page() {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.session_storage().ok())
        .flatten()
    {
        let _ = storage.remove_item("story_from_page");
    }
}

// ── Animasi exit lalu navigasi ke halaman asal ───────────────────────────────
fn exit_page_to<F>(is_exiting: RwSignal<bool>, nav: F, delay_ms: f64)
where
    F: FnOnce() + 'static,
{
    is_exiting.set(true);
    spawn_local(async move {
        let p = web_sys::js_sys::Promise::new(&mut |resolve, _| {
            let _ = web_sys::window()
                .unwrap()
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, delay_ms as i32);
        });
        let _ = wasm_bindgen_futures::JsFuture::from(p).await;
        nav();
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
//  HELPERS
// ═══════════════════════════════════════════════════════════════════════════════

static ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn buat_id(awalan: &str) -> String {
    let ts = web_sys::js_sys::Date::now() as u64;
    let seq = ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    format!("{}_{}_{}", awalan, ts, seq)
}

fn revoke_url(url: &str) {
    let _ = web_sys::Url::revoke_object_url(url);
}

fn css_filter_string(filter: &str) -> &'static str {
    match filter {
        "clarendon" => "contrast(1.2) saturate(1.3)",
        "gingham" => "brightness(1.05) hue-rotate(-10deg)",
        "moon" => "grayscale(1) contrast(1.1)",
        "lark" => "contrast(0.9)",
        "reyes" => "sepia(0.5) contrast(0.9)",
        "juno" => "contrast(1.1) saturate(1.2)",
        "slumber" => "brightness(0.9) saturate(0.8)",
        "crema" => "sepia(0.3) contrast(0.95)",
        "ludwig" => "saturate(1.1) contrast(1.05)",
        _ => "none",
    }
}

fn cover_src_rect(img_w: f64, img_h: f64, canvas_w: f64, canvas_h: f64) -> (f64, f64, f64, f64) {
    let scale = (canvas_w / img_w).max(canvas_h / img_h);
    let scaled_w = img_w * scale;
    let scaled_h = img_h * scale;
    let sx = ((scaled_w - canvas_w) / 2.0) / scale;
    let sy = ((scaled_h - canvas_h) / 2.0) / scale;
    let sw = canvas_w / scale;
    let sh = canvas_h / scale;
    (sx, sy, sw, sh)
}

fn font_for_style(style: &str) -> &'static str {
    match style.to_ascii_lowercase().as_str() {
        "modern" => "\"Instagram Sans\", -apple-system, BlinkMacSystemFont, sans-serif",
        "strong" => "\"Arial Black\", \"Helvetica Neue\", sans-serif",
        "typewriter" => "\"Courier New\", Courier, monospace",
        _ => "\"Bebas Neue\", \"Arial Black\", sans-serif",
    }
}

fn gradient_colors(key: &str) -> Option<(&'static str, &'static str)> {
    BG_GRADIENTS
        .iter()
        .find(|(k, _, _, _)| *k == key)
        .map(|(_, _, s, e)| (*s, *e))
}

// ═══════════════════════════════════════════════════════════════════════════════
//  CANVAS ENGINE
// ═══════════════════════════════════════════════════════════════════════════════

struct DownloadGuard {
    sig: RwSignal<bool>,
}
impl DownloadGuard {
    fn new(sig: RwSignal<bool>) -> Self {
        sig.set(true);
        Self { sig }
    }
}
impl Drop for DownloadGuard {
    fn drop(&mut self) {
        self.sig.set(false);
    }
}

fn get_dpr() -> f64 {
    web_sys::window()
        .map(|w| w.device_pixel_ratio())
        .unwrap_or(1.0)
        .clamp(1.0, 3.0)
}

fn create_export_canvas(
    dpr: f64,
) -> Option<(HtmlCanvasElement, CanvasRenderingContext2d, f64, f64)> {
    // DPR capped at 2.0 untuk export: 2160×3840 sudah cukup tajam,
    // DPR 3 menghasilkan canvas 3240×5760 (18 MP) yang hanya memboroskan memori & waktu encode.
    let export_dpr = dpr.min(2.0);
    let doc = web_sys::window()?.document()?;
    let canvas: HtmlCanvasElement = doc.create_element("canvas").ok()?.unchecked_into();
    let lw = 1080.0_f64;
    let lh = 1920.0_f64;
    canvas.set_width((lw * export_dpr) as u32);
    canvas.set_height((lh * export_dpr) as u32);
    let ctx: CanvasRenderingContext2d = canvas.get_context("2d").ok()??.unchecked_into();
    ctx.set_transform(export_dpr, 0.0, 0.0, export_dpr, 0.0, 0.0)
        .ok()?;
    Some((canvas, ctx, lw, lh))
}

static FONTS_PRELOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

async fn preload_fonts() -> Result<(), JsValue> {
    use std::sync::atomic::Ordering;
    if FONTS_PRELOADED.load(Ordering::Acquire) {
        return Ok(());
    }
    let Some(win) = web_sys::window() else {
        return Ok(());
    };
    let Some(doc) = win.document() else {
        return Ok(());
    };
    let fv = web_sys::js_sys::Reflect::get(&doc, &wasm_bindgen::JsValue::from_str("fonts"))?;
    let fs: web_sys::FontFaceSet = fv.unchecked_into();
    let faces = [
        "bold 48px \"Bebas Neue\"",
        "bold 28px \"Bebas Neue\"",
        "bold 18px \"Bebas Neue\"",
        "bold 28px -apple-system, BlinkMacSystemFont, sans-serif",
        "bold 28px \"Arial Black\"",
    ];
    let arr = web_sys::js_sys::Array::new();
    for f in faces {
        let p: web_sys::js_sys::Promise = fs.load(f).unchecked_into();
        arr.push(&p);
    }
    wasm_bindgen_futures::JsFuture::from(web_sys::js_sys::Promise::all(&arr)).await?;
    let ready: web_sys::js_sys::Promise = fs.ready()?;
    let result = wasm_bindgen_futures::JsFuture::from(ready)
        .await
        .map(|_| ());
    if result.is_ok() {
        FONTS_PRELOADED.store(true, Ordering::Release);
    }
    result
}

fn trigger_download_blob(blob: &web_sys::Blob, nama: &str) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(blob) else {
        return;
    };
    let Ok(el) = doc.create_element("a") else {
        web_sys::Url::revoke_object_url(&url).ok();
        return;
    };
    let a: web_sys::HtmlAnchorElement = el.unchecked_into();
    a.set_href(&url);
    a.set_download(nama);
    a.set_attribute("style", "display:none").ok();
    if let Some(body) = doc.body() {
        let _ = body.append_child(&a);
        a.click();
        let _ = body.remove_child(&a);
    }
    web_sys::Url::revoke_object_url(&url).ok();
}

async fn load_img_to_canvas(
    src: &str,
    ctx: &CanvasRenderingContext2d,
    cw: f64,
    ch: f64,
    scale: f64,
    try_dom_reuse: bool,
) -> Result<(), String> {
    let doc = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;

    if try_dom_reuse {
        if let Some(existing) = doc
            .query_selector("img.sc-media")
            .ok()
            .flatten()
            .and_then(|el| el.dyn_into::<HtmlImageElement>().ok())
            .filter(|img| {
                img.complete()
                    && img.natural_width() > 0
                    && (img.src().starts_with("blob:")
                        || img.cross_origin().map(|co| !co.is_empty()).unwrap_or(false))
            })
        {
            let iw = existing.natural_width() as f64;
            let ih = existing.natural_height() as f64;
            return draw_img_cover(ctx, &existing, iw, ih, cw, ch, scale);
        }
    }

    let fetch_src = if src.starts_with("blob:") || src.contains("_c=1") {
        src.to_string()
    } else if src.contains('?') {
        format!("{}&_c=1", src)
    } else {
        format!("{}?_c=1", src)
    };

    let img: HtmlImageElement = doc
        .create_element("img")
        .map_err(|e| format!("{:?}", e))?
        .unchecked_into();

    let mut res: Option<web_sys::js_sys::Function> = None;
    let mut rej: Option<web_sys::js_sys::Function> = None;
    let p = web_sys::js_sys::Promise::new(&mut |r, e| {
        res = Some(r);
        rej = Some(e);
    });
    let res = res.unwrap();
    let rej = rej.unwrap();
    let on_load = Closure::once(move || {
        let _ = res.call0(&JsValue::NULL);
    });
    let on_error = Closure::once(move || {
        let _ = rej.call0(&JsValue::NULL);
    });
    img.set_onload(Some(on_load.as_ref().unchecked_ref()));
    img.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    img.set_cross_origin(Some("anonymous"));
    img.set_src(&fetch_src);
    wasm_bindgen_futures::JsFuture::from(p)
        .await
        .map_err(|_| "img load gagal — periksa CORS header CDN".to_string())?;
    img.set_onload(None);
    img.set_onerror(None);
    drop(on_load);
    drop(on_error);

    let iw = img.natural_width() as f64;
    let ih = img.natural_height() as f64;
    if iw > 0.0 && ih > 0.0 {
        draw_img_cover(ctx, &img, iw, ih, cw, ch, scale)
    } else {
        Err("gambar kosong (0x0)".to_string())
    }
}

fn draw_img_cover(
    ctx: &CanvasRenderingContext2d,
    img: &HtmlImageElement,
    iw: f64,
    ih: f64,
    cw: f64,
    ch: f64,
    scale: f64,
) -> Result<(), String> {
    let (sx, sy, sw, sh) = cover_src_rect(iw, ih, cw, ch);
    let zsw = sw / scale;
    let zsh = sh / scale;
    let zsx = sx + (sw - zsw) / 2.0;
    let zsy = sy + (sh - zsh) / 2.0;
    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
        img, zsx, zsy, zsw, zsh, 0.0, 0.0, cw, ch,
    )
    .map_err(|e| format!("draw: {:?}", e))
}

async fn capture_video_frame(
    video: &HtmlVideoElement,
    ctx: &CanvasRenderingContext2d,
    cw: f64,
    ch: f64,
    scale: f64,
) -> Result<(), String> {
    let _ = video.pause();
    let win = web_sys::window().ok_or("no window")?;
    let mut res: Option<web_sys::js_sys::Function> = None;
    let p = web_sys::js_sys::Promise::new(&mut |r, _| {
        res = Some(r);
    });
    let res = res.unwrap();
    let cl = Closure::once(move || {
        let _ = res.call0(&JsValue::NULL);
    });
    let id = win
        .request_animation_frame(cl.as_ref().unchecked_ref())
        .map_err(|_| "raf")?;
    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
    win.cancel_animation_frame(id).ok();
    drop(cl);

    let vw = video.video_width() as f64;
    let vh = video.video_height() as f64;
    if vw > 0.0 && vh > 0.0 {
        let (sx, sy, sw, sh) = cover_src_rect(vw, vh, cw, ch);
        let zsw = sw / scale;
        let zsh = sh / scale;
        let zsx = sx + (sw - zsw) / 2.0;
        let zsy = sy + (sh - zsh) / 2.0;
        ctx.draw_image_with_html_video_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            video, zsx, zsy, zsw, zsh, 0.0, 0.0, cw, ch,
        )
        .map_err(|e| format!("{:?}", e))
    } else {
        ctx.draw_image_with_html_video_element_and_dw_and_dh(video, 0.0, 0.0, cw, ch)
            .map_err(|e| format!("{:?}", e))
    }
}

fn render_overlays_to_canvas(
    ctx: &CanvasRenderingContext2d,
    overlays: &[StoryOverlay],
    cw: f64,
    ch: f64,
    dom_w: f64,
    _dom_h: f64,
    _dpr: f64,
) {
    let px = if dom_w > 0.0 { cw / dom_w } else { cw / 390.0 };
    for ov in overlays {
        let cx = ov.x / 100.0 * cw;
        let cy = ov.y / 100.0 * ch;
        let sc = ov.scale.unwrap_or(1.0);
        let rot = ov.rotation.unwrap_or(0.0) * std::f64::consts::PI / 180.0;
        ctx.save();
        let _ = ctx.translate(cx, cy);
        let _ = ctx.rotate(rot);
        match ov.overlay_type {
            OverlayType::Text => {
                let content = ov.content.as_deref().unwrap_or("");
                let color = ov.color.as_deref().unwrap_or("#ffffff");
                let fs = ov.font_size.unwrap_or(28) as f64 * px * sc;
                let style = ov.text_style.as_deref().unwrap_or("classic");
                let align = ov.text_align.as_deref().unwrap_or("center");
                let sb = 4.0 * px;
                ctx.set_shadow_color("rgba(0,0,0,0.60)");
                ctx.set_shadow_blur(sb);
                ctx.set_shadow_offset_x(0.0);
                ctx.set_shadow_offset_y(sb * 0.5);
                ctx.set_font(&format!("bold {}px {}", fs, font_for_style(style)));
                ctx.set_fill_style_str(color);
                ctx.set_text_align(align);
                ctx.set_text_baseline("middle");
                let _ = ctx.fill_text(content, 0.0, 0.0);
                ctx.set_shadow_color("transparent");
                ctx.set_shadow_blur(0.0);
            }
            OverlayType::Sticker => {
                let emoji = ov.emoji.as_deref().unwrap_or("");
                let fs = 52.0 * px * sc;
                ctx.set_shadow_color("rgba(0,0,0,0.35)");
                ctx.set_shadow_blur(6.0 * px);
                ctx.set_shadow_offset_x(0.0);
                ctx.set_shadow_offset_y(3.0 * px);
                ctx.set_font(&format!("{}px -apple-system, sans-serif", fs));
                ctx.set_text_align("center");
                ctx.set_text_baseline("middle");
                let _ = ctx.fill_text(emoji, 0.0, 0.0);
                ctx.set_shadow_color("transparent");
                ctx.set_shadow_blur(0.0);
            }
        }
        ctx.restore();
    }
}

async fn render_event_frame_to_canvas(
    ctx: &CanvasRenderingContext2d,
    cw: f64,
    ch: f64,
    cover_src: &str,
    event_title: &str,
    event_date: &str,
    event_venue: &str,
    event_price: &str,
    filter: &str,
    bg_mode: &str,
    bg_color: &str,
) -> Result<(), String> {
    let doc = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no doc")?;

    // Selalu buat fresh HtmlImageElement off-DOM dengan crossorigin=anonymous.
    // DOM <img> preview TIDAK disentuh di sini — mereka tidak punya crossorigin
    // supaya foto event tetap tampil di preview. Kalau kita draw DOM img tanpa
    // crossorigin ke canvas, canvas langsung tainted dan export gagal.
    // Dengan path ini: jika CDN support CORS → canvas render dengan cover.
    //                  jika CDN tidak support CORS → has_img=false → solid/gradient bg.
    //
    // Tambahkan w=1080 sebagai hint resize ke CDN — jika CDN support (Imgix, Cloudflare, dll.)
    // akan mengirim gambar 1080px, bukan gambar 4K penuh. Jika CDN tidak support param ini,
    // tidak ada efek samping karena browser akan memuat gambar asli.
    let bust = if cover_src.starts_with("blob:") {
        cover_src.to_string()
    } else if cover_src.contains('?') {
        format!("{}&_cors=1&w=1080", cover_src)
    } else {
        format!("{}?_cors=1&w=1080", cover_src)
    };
    let fresh: HtmlImageElement = doc
        .create_element("img")
        .map_err(|e| format!("{:?}", e))?
        .unchecked_into();
    let mut res_ok: Option<web_sys::js_sys::Function> = None;
    let mut res_err: Option<web_sys::js_sys::Function> = None;
    let p = web_sys::js_sys::Promise::new(&mut |r, e| {
        res_ok = Some(r);
        res_err = Some(e);
    });
    let r = res_ok.unwrap();
    let e = res_err.unwrap();
    let on_load = Closure::once(move || {
        let _ = r.call0(&JsValue::NULL);
    });
    let on_error = Closure::once(move || {
        let _ = e.call0(&JsValue::NULL);
    });
    fresh.set_onload(Some(on_load.as_ref().unchecked_ref()));
    fresh.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    fresh.set_cross_origin(Some("anonymous"));
    fresh.set_src(&bust);
    let loaded = wasm_bindgen_futures::JsFuture::from(p).await.is_ok();
    fresh.set_onload(None);
    fresh.set_onerror(None);
    drop(on_load);
    drop(on_error);
    let iw_f = fresh.natural_width() as f64;
    let ih_f = fresh.natural_height() as f64;
    let (img, has_img) = (fresh, loaded && iw_f > 0.0 && ih_f > 0.0);

    if bg_mode == "blur" {
        ctx.set_fill_style_str("#0d0d18");
        ctx.fill_rect(0.0, 0.0, cw, ch);
        if has_img {
            let filt = css_filter_string(filter);
            if filt != "none" {
                ctx.set_filter(filt);
            }
            let iw = img.natural_width() as f64;
            let ih = img.natural_height() as f64;
            let (sx, sy, sw, sh) = cover_src_rect(iw, ih, cw, ch);
            ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                &img, sx, sy, sw, sh, 0.0, 0.0, cw, ch,
            )
            .ok();
            ctx.set_filter("none");
        }
        if let Ok(grad) = ctx
            .create_linear_gradient(0.0, 0.0, 0.0, ch)
            .dyn_into::<web_sys::CanvasGradient>()
        {
            let _ = grad.add_color_stop(0.0, "rgba(0,0,0,0.28)");
            let _ = grad.add_color_stop(1.0, "rgba(0,0,0,0.54)");
            ctx.set_fill_style(&grad.into());
            ctx.fill_rect(0.0, 0.0, cw, ch);
        }
    } else if bg_mode == "solid" {
        ctx.set_fill_style_str(if bg_color.is_empty() {
            "#1a1a2e"
        } else {
            bg_color
        });
        ctx.fill_rect(0.0, 0.0, cw, ch);
    } else {
        const GRADS: &[(&str, &str, &str)] = &[
            ("purple-haze", "#667eea", "#764ba2"),
            ("sunset", "#f093fb", "#f5576c"),
            ("ocean", "#4facfe", "#00f2fe"),
            ("forest", "#43e97b", "#38f9d7"),
            ("midnight", "#0f2027", "#2c5364"),
            ("candy", "#ff9a9e", "#fecfef"),
        ];
        let (cs, ce) = GRADS
            .iter()
            .find(|(k, _, _)| *k == bg_mode)
            .map(|(_, s, e)| (*s, *e))
            .unwrap_or(("#1a1a2e", "#0f3460"));
        if let Ok(grad) = ctx
            .create_linear_gradient(0.0, 0.0, cw * 0.8, ch)
            .dyn_into::<web_sys::CanvasGradient>()
        {
            let _ = grad.add_color_stop(0.0, cs);
            let _ = grad.add_color_stop(1.0, ce);
            ctx.set_fill_style(&grad.into());
            ctx.fill_rect(0.0, 0.0, cw, ch);
        }
    }

    let _ = filter;

    let words: Vec<&str> = event_title.split_whitespace().collect();
    let title_fs = 108.0_f64;
    let approx_char_w = title_fs * 0.52;
    let card_w = cw - 120.0;
    let pad_h = 80.0_f64;
    let content_w = card_w - pad_h * 2.0;
    let max_chars = ((content_w / approx_char_w) as usize).max(5);

    let mut title_lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_len = 0usize;
    for word in &words {
        let wl = word.chars().count();
        if cur.is_empty() {
            cur = word.to_string();
            cur_len = wl;
        } else if cur_len + 1 + wl <= max_chars {
            cur.push(' ');
            cur.push_str(word);
            cur_len += 1 + wl;
        } else {
            title_lines.push(cur);
            cur = word.to_string();
            cur_len = wl;
        }
    }
    if !cur.is_empty() {
        title_lines.push(cur);
    }

    let title_line_h = title_fs * 1.12;
    let title_block_h = title_line_h * title_lines.len() as f64;

    let cover_h = card_w * 0.5625;
    let gap_cover = 40.0_f64;

    let pad_v = 76.0_f64;
    let badge_h = 48.0_f64;
    let gap_badge = 34.0_f64;
    let gap_sep = 42.0_f64;
    let sep_h = 1.5_f64;
    let gap_meta = 40.0_f64;
    let meta_label_h = 28.0_f64;
    let gap_date = 28.0_f64;
    let date_h = 60.0_f64;
    let gap_pill = 48.0_f64;
    let pill_h = 100.0_f64;

    let card_h = cover_h
        + gap_cover
        + pad_v
        + badge_h
        + gap_badge
        + title_block_h
        + gap_sep
        + sep_h
        + gap_meta
        + meta_label_h
        + gap_date
        + date_h
        + gap_pill
        + pill_h
        + pad_v;

    let card_x = 60.0_f64;
    let card_y = (ch - card_h) / 2.0 - 56.0;
    let card_r = 44.0_f64;

    ctx.save();
    ctx.begin_path();
    ctx.move_to(card_x + card_r, card_y);
    ctx.line_to(card_x + card_w - card_r, card_y);
    ctx.arc(
        card_x + card_w - card_r,
        card_y + card_r,
        card_r,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    )
    .ok();
    ctx.line_to(card_x + card_w, card_y + card_h - card_r);
    ctx.arc(
        card_x + card_w - card_r,
        card_y + card_h - card_r,
        card_r,
        0.0,
        std::f64::consts::FRAC_PI_2,
    )
    .ok();
    ctx.line_to(card_x + card_r, card_y + card_h);
    ctx.arc(
        card_x + card_r,
        card_y + card_h - card_r,
        card_r,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    )
    .ok();
    ctx.line_to(card_x, card_y + card_r);
    ctx.arc(
        card_x + card_r,
        card_y + card_r,
        card_r,
        std::f64::consts::PI,
        std::f64::consts::PI + std::f64::consts::FRAC_PI_2,
    )
    .ok();
    ctx.close_path();
    ctx.set_fill_style_str("rgba(11,12,30,0.91)");
    ctx.fill();
    ctx.set_stroke_style_str("rgba(255,255,255,0.09)");
    ctx.set_line_width(1.5);
    ctx.stroke();
    ctx.restore();

    let cx = card_x + pad_h;
    let mut y = card_y;

    {
        ctx.save();
        ctx.begin_path();
        ctx.move_to(card_x + card_r, card_y);
        ctx.line_to(card_x + card_w - card_r, card_y);
        ctx.arc(
            card_x + card_w - card_r,
            card_y + card_r,
            card_r,
            -std::f64::consts::FRAC_PI_2,
            0.0,
        )
        .ok();
        ctx.line_to(card_x + card_w, card_y + cover_h);
        ctx.line_to(card_x, card_y + cover_h);
        ctx.line_to(card_x, card_y + card_r);
        ctx.arc(
            card_x + card_r,
            card_y + card_r,
            card_r,
            std::f64::consts::PI,
            std::f64::consts::PI + std::f64::consts::FRAC_PI_2,
        )
        .ok();
        ctx.close_path();
        ctx.clip();

        if has_img {
            let iw = img.natural_width() as f64;
            let ih = img.natural_height() as f64;
            let scale_x = card_w / iw;
            let scale_y = cover_h / ih;
            let scale_fit = scale_x.max(scale_y);
            let sw = card_w / scale_fit;
            let sh = cover_h / scale_fit;
            let sx = (iw - sw) / 2.0;
            let sy = (ih - sh) / 2.0;
            ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                &img, sx, sy, sw, sh, card_x, card_y, card_w, cover_h,
            )
            .ok();
        } else {
            ctx.set_fill_style_str("rgba(30,32,60,0.95)");
            ctx.fill_rect(card_x, card_y, card_w, cover_h);
        }
        ctx.restore();

        if let Ok(tgrad) = ctx
            .create_linear_gradient(0.0, card_y + cover_h - 80.0, 0.0, card_y + cover_h)
            .dyn_into::<web_sys::CanvasGradient>()
        {
            let _ = tgrad.add_color_stop(0.0, "rgba(0,0,0,0)");
            let _ = tgrad.add_color_stop(1.0, "rgba(0,0,0,0.45)");
            ctx.set_fill_style(&tgrad.into());
            ctx.fill_rect(card_x, card_y + cover_h - 80.0, card_w, 80.0);
        }

        y += cover_h + gap_cover;
    }

    y += pad_v;

    let dot_r = 11.0_f64;
    let dot_cx = cx + dot_r;
    let dot_cy = y + badge_h / 2.0;

    ctx.begin_path();
    ctx.arc(dot_cx, dot_cy, dot_r, 0.0, std::f64::consts::TAU)
        .ok();
    ctx.set_fill_style_str("#c8ff5e");
    ctx.set_shadow_color("rgba(200,255,94,0.70)");
    ctx.set_shadow_blur(10.0);
    ctx.fill();
    ctx.set_shadow_blur(0.0);
    ctx.set_shadow_color("transparent");

    ctx.set_font("700 26px -apple-system, BlinkMacSystemFont, sans-serif");
    ctx.set_fill_style_str("rgba(255,255,255,0.90)");
    ctx.set_text_align("left");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text("KINETIC EXCLUSIVE", dot_cx + dot_r + 14.0, dot_cy);

    y += badge_h + gap_badge;

    ctx.set_font(&format!(
        "italic 800 {}px \"Syne\", \"Arial Black\", sans-serif",
        title_fs
    ));
    ctx.set_fill_style_str("#8b9fe8");
    ctx.set_text_align("left");
    ctx.set_text_baseline("top");
    ctx.set_shadow_color("rgba(0,0,0,0.50)");
    ctx.set_shadow_blur(16.0);
    ctx.set_shadow_offset_y(3.0);

    for (i, line) in title_lines.iter().enumerate() {
        let _ = ctx.fill_text(line, cx, y + i as f64 * title_line_h);
    }
    ctx.set_shadow_blur(0.0);
    ctx.set_shadow_offset_y(0.0);
    ctx.set_shadow_color("transparent");

    y += title_block_h + gap_sep;

    if let Ok(sep) = ctx
        .create_linear_gradient(cx, 0.0, card_x + card_w - pad_h, 0.0)
        .dyn_into::<web_sys::CanvasGradient>()
    {
        let _ = sep.add_color_stop(0.0, "rgba(255,255,255,0)");
        let _ = sep.add_color_stop(0.25, "rgba(255,255,255,0.22)");
        let _ = sep.add_color_stop(0.75, "rgba(255,255,255,0.22)");
        let _ = sep.add_color_stop(1.0, "rgba(255,255,255,0)");
        ctx.set_fill_style(&sep.into());
        ctx.fill_rect(cx, y, content_w, sep_h);
    }

    y += sep_h + gap_meta;

    ctx.set_font("700 26px -apple-system, BlinkMacSystemFont, sans-serif");
    ctx.set_fill_style_str("rgba(255,255,255,0.42)");
    ctx.set_text_align("left");
    ctx.set_text_baseline("top");
    let _ = ctx.fill_text("DATE & VENUE", cx, y);

    y += meta_label_h + gap_date;

    let row_cy = y + date_h / 2.0;

    ctx.set_font("700 52px -apple-system, BlinkMacSystemFont, sans-serif");
    ctx.set_fill_style_str("#ffffff");
    ctx.set_text_align("left");
    ctx.set_text_baseline("middle");
    if !event_date.is_empty() {
        let _ = ctx.fill_text(event_date, cx, row_cy);
    }

    if !event_venue.is_empty() {
        ctx.set_font("500 38px -apple-system, BlinkMacSystemFont, sans-serif");
        ctx.set_fill_style_str("rgba(255,255,255,0.72)");
        ctx.set_text_align("right");
        let _ = ctx.fill_text(event_venue, card_x + card_w - pad_h, row_cy);
    }

    y += date_h + gap_pill;

    if !event_price.is_empty() {
        let pill_w = content_w.min(660.0);
        let pill_r_corner = pill_h / 2.0;

        ctx.begin_path();
        ctx.move_to(cx + pill_r_corner, y);
        ctx.line_to(cx + pill_w - pill_r_corner, y);
        ctx.arc(
            cx + pill_w - pill_r_corner,
            y + pill_r_corner,
            pill_r_corner,
            -std::f64::consts::FRAC_PI_2,
            std::f64::consts::FRAC_PI_2,
        )
        .ok();
        ctx.line_to(cx + pill_r_corner, y + pill_h);
        ctx.arc(
            cx + pill_r_corner,
            y + pill_r_corner,
            pill_r_corner,
            std::f64::consts::FRAC_PI_2,
            std::f64::consts::PI + std::f64::consts::FRAC_PI_2,
        )
        .ok();
        ctx.close_path();
        ctx.set_fill_style_str("#c8ff5e");
        ctx.set_shadow_color("rgba(200,255,94,0.32)");
        ctx.set_shadow_blur(22.0);
        ctx.fill();
        ctx.set_shadow_blur(0.0);
        ctx.set_shadow_color("transparent");

        ctx.set_font("700 50px -apple-system, BlinkMacSystemFont, sans-serif");
        ctx.set_fill_style_str("#0a0a14");
        ctx.set_text_align("left");
        ctx.set_text_baseline("middle");
        let _ = ctx.fill_text(event_price, cx + pill_r_corner + 24.0, y + pill_h / 2.0);
    }

    if ctx.get_image_data(0.0, 0.0, 1.0, 1.0).is_err() {
        return Err(
            "Canvas tainted — CDN tidak kirim CORS header (Access-Control-Allow-Origin). \
             Ganti latar ke Solid atau Gradient untuk export tanpa gambar cover, \
             atau hubungi admin CDN untuk enable CORS."
                .to_string(),
        );
    }

    Ok(())
}

// ── WebP support detection (cached) ──────────────────────────────────────────
static WEBP_CHECKED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
// 0 = belum dicek, 1 = supported, 2 = tidak supported

fn webp_export_supported() -> bool {
    use std::sync::atomic::Ordering;
    let v = WEBP_CHECKED.load(Ordering::Relaxed);
    if v != 0 {
        return v == 1;
    }

    let supported = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.create_element("canvas").ok())
        .and_then(|c| c.dyn_into::<HtmlCanvasElement>().ok())
        .map(|c| {
            c.set_width(1);
            c.set_height(1);
            c.to_data_url_with_type("image/webp")
                .map(|s| s.starts_with("data:image/webp"))
                .unwrap_or(false)
        })
        .unwrap_or(false);

    WEBP_CHECKED.store(if supported { 1 } else { 2 }, Ordering::Relaxed);
    supported
}

/// Export canvas ke blob terkompresi.
/// Otomatis pilih WebP (jika browser support) → JPEG fallback.
/// Dibanding PNG, WebP 0.92 menghasilkan file 5–10× lebih kecil untuk foto.
async fn canvas_to_blob(canvas: &HtmlCanvasElement) -> Result<web_sys::Blob, String> {
    canvas_to_blob_compressed(canvas, 0.92).await
}

async fn canvas_to_blob_compressed(
    canvas: &HtmlCanvasElement,
    quality: f64,
) -> Result<web_sys::Blob, String> {
    let mime = if webp_export_supported() {
        "image/webp"
    } else {
        "image/jpeg"
    };

    let mut res = None;
    let mut rej = None;
    let p = web_sys::js_sys::Promise::new(&mut |r, e| {
        res = Some(r);
        rej = Some(e);
    });
    let resolve = res.unwrap();
    let reject = rej.unwrap();
    let cb = Closure::once(move |blob: JsValue| {
        if blob.is_null() || blob.is_undefined() {
            let _ = reject.call0(&JsValue::NULL);
        } else {
            let _ = resolve.call1(&JsValue::NULL, &blob);
        }
    });

    // Panggil canvas.toBlob(callback, mime, quality) secara dinamis via Reflect.
    // web_sys tidak mengekspos overload 3-argumen karena qualityArgument
    // bertipe `any` di WebIDL dan di-skip oleh generator. js_sys::Reflect::apply
    // memanggil metode JS langsung tanpa perlu impl pada foreign type.
    let to_blob_fn = js_sys::Reflect::get(canvas.as_ref(), &JsValue::from_str("toBlob"))
        .map_err(|_| "canvas.toBlob not found")?;
    let to_blob_fn: js_sys::Function = to_blob_fn.unchecked_into();
    let args = js_sys::Array::new();
    args.push(cb.as_ref().unchecked_ref());
    args.push(&JsValue::from_str(mime));
    args.push(&JsValue::from_f64(quality));
    to_blob_fn
        .apply(canvas.as_ref(), &args)
        .map_err(|e| format!("toBlob apply failed: {:?}", e))?;

    let result = wasm_bindgen_futures::JsFuture::from(p).await;
    drop(cb);

    // Jika browser mengembalikan null (format tidak didukung), fallback ke PNG
    match result {
        Ok(v) if !v.is_null() && !v.is_undefined() => Ok(v.unchecked_into()),
        _ => {
            // Fallback: PNG lossless via to_blob_with_type yang ada di web_sys
            let mut res2 = None;
            let mut rej2 = None;
            let p2 = web_sys::js_sys::Promise::new(&mut |r, e| {
                res2 = Some(r);
                rej2 = Some(e);
            });
            let resolve2 = res2.unwrap();
            let reject2 = rej2.unwrap();
            let cb2 = Closure::once(move |blob: JsValue| {
                if blob.is_null() || blob.is_undefined() {
                    let _ = reject2.call0(&JsValue::NULL);
                } else {
                    let _ = resolve2.call1(&JsValue::NULL, &blob);
                }
            });
            canvas
                .to_blob_with_type(cb2.as_ref().unchecked_ref(), "image/png")
                .map_err(|e| format!("{:?}", e))?;
            let result2 = wasm_bindgen_futures::JsFuture::from(p2).await;
            drop(cb2);
            result2
                .map(|v| v.unchecked_into())
                .map_err(|e| format!("png fallback: {:?}", e))
        }
    }
}
/// Mime type untuk file export berdasarkan dukungan WebP browser.
fn export_mime() -> &'static str {
    if webp_export_supported() {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

/// Ekstensi file untuk export berdasarkan dukungan WebP browser.
fn export_ext() -> &'static str {
    if webp_export_supported() {
        "webp"
    } else {
        "jpg"
    }
}

/// Kompres & resize gambar image/File sebelum diupload (Mode A).
/// - max_px: lebar/tinggi maksimum (panjang sisi terbesar), default 1080
/// - quality: 0.85–0.95
/// Mengembalikan Blob terkompresi. Pada error, kembalikan blob asli (fail-open).
async fn compress_image_file(file: &web_sys::File, max_px: u32, quality: f64) -> web_sys::Blob {
    // Hanya kompres gambar, bukan video
    if !file.type_().starts_with("image/") {
        return file.clone().into();
    }

    let inner = async move {
        let doc = web_sys::window()
            .and_then(|w| w.document())
            .ok_or("no doc")?;

        let img: HtmlImageElement = doc
            .create_element("img")
            .map_err(|_| "create img")?
            .unchecked_into();

        let obj_url =
            web_sys::Url::create_object_url_with_blob(file).map_err(|_| "createObjectURL")?;

        // Load image dengan promise
        let mut ok_fn: Option<web_sys::js_sys::Function> = None;
        let mut err_fn: Option<web_sys::js_sys::Function> = None;
        let p = web_sys::js_sys::Promise::new(&mut |r, e| {
            ok_fn = Some(r);
            err_fn = Some(e);
        });
        let r = ok_fn.unwrap();
        let e = err_fn.unwrap();
        let on_load = Closure::once(move || {
            let _ = r.call0(&JsValue::NULL);
        });
        let on_error = Closure::once(move || {
            let _ = e.call0(&JsValue::NULL);
        });
        img.set_onload(Some(on_load.as_ref().unchecked_ref()));
        img.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        img.set_src(&obj_url);
        wasm_bindgen_futures::JsFuture::from(p)
            .await
            .map_err(|_| "load failed")?;
        img.set_onload(None);
        img.set_onerror(None);
        drop(on_load);
        drop(on_error);

        web_sys::Url::revoke_object_url(&obj_url).ok();

        let iw = img.natural_width() as f64;
        let ih = img.natural_height() as f64;
        if iw < 1.0 || ih < 1.0 {
            return Err("zero size");
        }

        // Hitung dimensi output — hanya downscale, tidak upscale
        let scale = (max_px as f64 / iw.max(ih)).min(1.0);
        let ow = (iw * scale).round() as u32;
        let oh = (ih * scale).round() as u32;

        // Buat canvas off-screen untuk resize
        let canvas: HtmlCanvasElement = doc
            .create_element("canvas")
            .map_err(|_| "create canvas")?
            .unchecked_into();
        canvas.set_width(ow);
        canvas.set_height(oh);
        let ctx: CanvasRenderingContext2d = canvas
            .get_context("2d")
            .ok()
            .flatten()
            .ok_or("no ctx")?
            .unchecked_into();

        ctx.draw_image_with_html_image_element_and_dw_and_dh(&img, 0.0, 0.0, ow as f64, oh as f64)
            .map_err(|_| "drawImage")?;

        // Export ke WebP/JPEG
        let blob = canvas_to_blob_compressed(&canvas, quality)
            .await
            .map_err(|e| {
                let _ = e;
                "blob failed"
            })?;
        canvas.set_width(0);
        canvas.set_height(0);
        Ok(blob)
    };

    match inner.await {
        Ok(blob) => blob,
        Err(e) => {
            web_sys::console::warn_1(
                &format!("[story] compress_image_file gagal ({}), pakai file asli", e).into(),
            );
            file.clone().into()
        }
    }
}

#[derive(Clone)]
enum BgExport {
    Solid(String),
    Gradient {
        color_start: &'static str,
        color_end: &'static str,
    },
}

fn export_story_canvas(
    pratinjau_url: String,
    is_vid: bool,
    overlays: Vec<StoryOverlay>,
    filter: String,
    nama_file: String,
    scale: f64,
    bg_info: Option<BgExport>,
    guard_sig: RwSignal<bool>,
) {
    spawn_local(async move {
        let _guard = DownloadGuard::new(guard_sig);
        if let Err(e) = preload_fonts().await {
            web_sys::console::warn_1(&format!("font preload: {:?}", e).into());
        }
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let (dom_w, dom_h) = doc
            .query_selector(".sc-canvas-frame")
            .ok()
            .flatten()
            .and_then(|el| el.dyn_into::<web_sys::Element>().ok())
            .map(|el| {
                let r = el.get_bounding_client_rect();
                (r.width(), r.height())
            })
            .filter(|(w, h)| *w > 1.0 && *h > 1.0)
            .unwrap_or((390.0, 844.0));

        let dpr = get_dpr();
        let Some((canvas, ctx, cw, ch)) = create_export_canvas(dpr) else {
            return;
        };

        if let Some(ref bg) = bg_info {
            match bg {
                BgExport::Solid(c) => {
                    ctx.set_fill_style_str(c);
                    ctx.fill_rect(0.0, 0.0, cw, ch);
                }
                BgExport::Gradient {
                    color_start,
                    color_end,
                } => {
                    if let Ok(g) = ctx
                        .create_linear_gradient(0.0, 0.0, 0.0, ch)
                        .dyn_into::<web_sys::CanvasGradient>()
                    {
                        let _ = g.add_color_stop(0.0, color_start);
                        let _ = g.add_color_stop(1.0, color_end);
                        ctx.set_fill_style(&g.into());
                        ctx.fill_rect(0.0, 0.0, cw, ch);
                    }
                }
            }
        }

        let fs = css_filter_string(&filter);
        if fs != "none" {
            ctx.set_filter(fs);
        }

        let result = if is_vid {
            match doc
                .query_selector("video.sc-media")
                .ok()
                .flatten()
                .and_then(|el| el.dyn_into::<HtmlVideoElement>().ok())
            {
                Some(v) => capture_video_frame(&v, &ctx, cw, ch, scale).await,
                None => Err("video.sc-media tidak ditemukan".into()),
            }
        } else {
            load_img_to_canvas(&pratinjau_url, &ctx, cw, ch, scale, true).await
        };

        if let Err(e) = result {
            web_sys::console::error_1(&format!("bg capture gagal: {}", e).into());
            if let Some(win) = web_sys::window() {
                let _ = win.alert_with_message(&format!("Export gagal: {}", e));
            }
            return;
        }

        ctx.set_filter("none");
        render_overlays_to_canvas(&ctx, &overlays, cw, ch, dom_w, dom_h, dpr);

        match canvas_to_blob(&canvas).await {
            Ok(blob) => {
                let stem = nama_file
                    .rsplit_once('.')
                    .map(|(n, _)| n.to_string())
                    .unwrap_or(nama_file);
                let ts = web_sys::js_sys::Date::now() as u64;
                let ext = export_ext();
                trigger_download_blob(&blob, &format!("{}_story_{}.{}", stem, ts, ext));
            }
            Err(e) => web_sys::console::error_1(&format!("to_blob: {}", e).into()),
        }

        ctx.clear_rect(0.0, 0.0, cw * dpr, ch * dpr);
        drop(ctx);
        canvas.set_width(0);
        canvas.set_height(0);
    });
}

// ═══════════════════════════════════════════════════════════════════════════════
//  KOMPONEN UTAMA
// ═══════════════════════════════════════════════════════════════════════════════

#[component]
pub fn StoryCreatorPage() -> impl IntoView {
    let store_ctx = use_stories_store();
    let navigate_sv = StoredValue::new(use_nav());

    // ── State ────────────────────────────────────────────────────────────────
    let file_ref = NodeRef::<leptos::html::Input>::new();
    let file_terpilih = RwSignal::new(None::<web_sys::File>);
    let url_pratinjau = RwSignal::new(None::<String>);
    let is_video = RwSignal::new(false);
    let overlays = RwSignal::new(Vec::<StoryOverlay>::new());
    let alat_aktif = RwSignal::new(Alat::None);
    let input_teks = RwSignal::new(String::new());
    let warna_teks = RwSignal::new("#ffffff".to_string());
    let ukuran_teks = RwSignal::new(28_u32);
    let hero_transition = RwSignal::new(false);
    let hero_timer_active: StoredValue<bool> = StoredValue::new(false);
    let trash_zone_active = RwSignal::new(false);
    let text_style = RwSignal::new("classic".to_string());
    let text_align = RwSignal::new("center".to_string());
    let text_bg_enabled = RwSignal::new(false);
    let filter_dipilih = RwSignal::new("normal".to_string());
    let musik_dipilih = RwSignal::new(None::<(String, String, String)>);
    let sedang_mengunggah = RwSignal::new(false);
    let bg_mode = RwSignal::new("blur".to_string());
    let bg_solid_color = RwSignal::new("#1a1a2e".to_string());
    let error_unggah = RwSignal::new(None::<String>);
    let sedang_mengunduh = RwSignal::new(false);
    let z_counter = RwSignal::new(1_u32);
    let last_tap: StoredValue<(String, f64)> = StoredValue::new((String::new(), 0.0));
    let selected_overlay_id = RwSignal::new(String::new());
    let bg_scale = RwSignal::new(1.0_f64);
    let bg_pinch_start = StoredValue::new((0.0_f64, 1.0_f64));
    let media_layer_ref = NodeRef::<leptos::html::Div>::new();
    let cover_img_ready = RwSignal::new(false);
    let cover_load_version = RwSignal::new(0_u32);

    // EXIT ANIMATION STATE
    let is_exiting = RwSignal::new(false);

    // SWIPE DOWN TO DISMISS
    let swipe_start_y = StoredValue::new(0.0_f64);
    let swipe_start_x = StoredValue::new(0.0_f64);
    let swipe_active = StoredValue::new(false);

    // ── Query params ─────────────────────────────────────────────────────────
    let query = use_query_map();
    let prefill_slug = move || query.with(|q| q.get("event_slug").unwrap_or_default());
    let prefill_title = move || query.with(|q| q.get("event_title").unwrap_or_default());
    let prefill_cover = move || query.with(|q| q.get("event_cover").unwrap_or_default());
    let prefill_id = move || query.with(|q| q.get("event_id").unwrap_or_default());

    let has_event_prefill = Memo::new(move |_| {
        !query
            .with(|q| q.get("event_slug").unwrap_or_default())
            .is_empty()
    });
    let user_overrode_prefill = RwSignal::new(false);
    let event_meta_sig: RwSignal<Option<EventStoryMeta>> = RwSignal::new(None);
    let event_cover_url: StoredValue<String> = StoredValue::new(String::new());
    let event_desc_sig: StoredValue<String> = StoredValue::new(String::new());
    let last_prefilled_slug: StoredValue<String> = StoredValue::new(String::new());
    let prefill_desc = move || query.with(|q| q.get("event_desc").unwrap_or_default());
    let prefill_date = move || query.with(|q| q.get("event_date").unwrap_or_default());
    let prefill_venue = move || query.with(|q| q.get("event_venue").unwrap_or_default());
    let prefill_price = move || query.with(|q| q.get("event_price").unwrap_or_default());

    // ── Helper: navigasi kembali ke halaman asal ─────────────────────────────
    let do_exit = move || {
        let from = read_from_page();
        clear_from_page();
        let nav = navigate_sv.get_value();
        exit_page_to(
            is_exiting,
            move || {
                nav(&from, Default::default());
            },
            EXIT_DURATION_MS,
        );
    };

    // ── Effect: inisiasi mode event ──────────────────────────────────────────
    Effect::new(move |_| {
        let slug = prefill_slug();
        let title = prefill_title();
        let cover = prefill_cover();
        let id = prefill_id();
        let desc = prefill_desc();

        if slug.is_empty() || cover.is_empty() {
            return;
        }

        let from_create = query.with(|q| q.get("from_create").unwrap_or_default()) == "1";
        if from_create && slug == "draft" {
            is_video.set(false);
            url_pratinjau.set(Some(cover.clone()));
            event_cover_url.set_value(cover.clone());
            event_desc_sig.set_value(desc.clone());
            last_prefilled_slug.set_value(slug.clone());
            event_meta_sig.set(None);
            return;
        }

        let last = last_prefilled_slug.get_value();
        if !last.is_empty() && last == slug {
            return;
        }
        last_prefilled_slug.set_value(slug.clone());

        cover_load_version.update(|v| *v = v.wrapping_add(1));
        cover_img_ready.set(false);

        is_video.set(false);
        url_pratinjau.set(Some(cover.clone()));
        event_cover_url.set_value(cover.clone());
        event_desc_sig.set_value(desc.clone());

        let has_hero = web_sys::window()
            .and_then(|w| w.session_storage().ok())
            .flatten()
            .and_then(|s| s.get_item("story_hero_transition").ok())
            .flatten()
            .map(|v| v == "event")
            .unwrap_or(false);

        if has_hero {
            hero_transition.set(true);
            if let Some(storage) = web_sys::window()
                .and_then(|w| w.session_storage().ok())
                .flatten()
            {
                let _ = storage.remove_item("story_hero_transition");
                let _ = storage.remove_item("story_hero_cover");
            }
            hero_timer_active.set_value(false);
            hero_timer_active.set_value(true);
            spawn_local(async move {
                let p = web_sys::js_sys::Promise::new(&mut |resolve, _| {
                    let _ = web_sys::window()
                        .unwrap()
                        .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 650);
                });
                let _ = wasm_bindgen_futures::JsFuture::from(p).await;
                if hero_timer_active.get_value() {
                    hero_timer_active.set_value(false);
                    hero_transition.set(false);
                }
            });
        }

        url_pratinjau.set(Some(cover.clone()));
        event_cover_url.set_value(cover.clone());
        event_desc_sig.set_value(desc);

        event_meta_sig.set(Some(EventStoryMeta {
            event_id: id,
            event_slug: slug,
            event_title: title.clone(),
        }));

        overlays.set(Vec::new());
    });

    // ── Touch listener pinch-zoom ─────────────────────────────────────────────
    let bg_ts_fn: StoredValue<Option<web_sys::js_sys::Function>> = StoredValue::new(None);
    let bg_tm_fn: StoredValue<Option<web_sys::js_sys::Function>> = StoredValue::new(None);
    let bg_ts_cl: StoredValue<Option<JsValue>> = StoredValue::new(None);
    let bg_tm_cl: StoredValue<Option<JsValue>> = StoredValue::new(None);

    Effect::new(move |_| {
        let Some(el) = media_layer_ref.get() else {
            return;
        };
        let target: web_sys::EventTarget =
            el.unchecked_ref::<web_sys::HtmlElement>().clone().into();

        if let Some(f) = bg_ts_fn.get_value() {
            let _ = target.remove_event_listener_with_callback("touchstart", &f);
            bg_ts_fn.set_value(None);
        }
        if let Some(f) = bg_tm_fn.get_value() {
            let _ = target.remove_event_listener_with_callback("touchmove", &f);
            bg_tm_fn.set_value(None);
        }

        let on_ts = Closure::<dyn Fn(web_sys::TouchEvent)>::new(move |ev: web_sys::TouchEvent| {
            let t = ev.touches();
            if t.length() == 2 {
                let t0 = t.get(0).unwrap();
                let t1 = t.get(1).unwrap();
                let dx = (t1.client_x() - t0.client_x()) as f64;
                let dy = (t1.client_y() - t0.client_y()) as f64;
                bg_pinch_start.set_value(((dx * dx + dy * dy).sqrt(), bg_scale.get_untracked()));
            }
        });
        let ts_fn: web_sys::js_sys::Function = on_ts
            .as_ref()
            .unchecked_ref::<web_sys::js_sys::Function>()
            .clone();
        let mut opts = AddEventListenerOptions::new();
        opts.set_passive(true);
        let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
            "touchstart",
            &ts_fn,
            &opts,
        );
        bg_ts_fn.set_value(Some(ts_fn));
        bg_ts_cl.set_value(Some(on_ts.into_js_value()));

        let on_tm = Closure::<dyn Fn(web_sys::TouchEvent)>::new(move |ev: web_sys::TouchEvent| {
            let t = ev.touches();
            if t.length() == 2 {
                ev.prevent_default();
                let t0 = t.get(0).unwrap();
                let t1 = t.get(1).unwrap();
                let dx = (t1.client_x() - t0.client_x()) as f64;
                let dy = (t1.client_y() - t0.client_y()) as f64;
                let dist = (dx * dx + dy * dy).sqrt();
                let (sd, ss) = bg_pinch_start.get_value();
                if sd < 1.0 {
                    return;
                }
                bg_scale.set((ss * dist / sd).clamp(1.0, 4.0));
            }
        });
        let tm_fn: web_sys::js_sys::Function = on_tm
            .as_ref()
            .unchecked_ref::<web_sys::js_sys::Function>()
            .clone();
        let mut opts2 = AddEventListenerOptions::new();
        opts2.set_passive(false);
        let _ = target.add_event_listener_with_callback_and_add_event_listener_options(
            "touchmove",
            &tm_fn,
            &opts2,
        );
        bg_tm_fn.set_value(Some(tm_fn));
        bg_tm_cl.set_value(Some(on_tm.into_js_value()));
    });

    on_cleanup(move || {
        if let Some(el) = media_layer_ref.get_untracked() {
            let target: web_sys::EventTarget =
                el.unchecked_ref::<web_sys::HtmlElement>().clone().into();
            if let Some(f) = bg_ts_fn.get_value() {
                let _ = target.remove_event_listener_with_callback("touchstart", &f);
            }
            if let Some(f) = bg_tm_fn.get_value() {
                let _ = target.remove_event_listener_with_callback("touchmove", &f);
            }
        }
        bg_ts_cl.set_value(None);
        bg_tm_cl.set_value(None);
        bg_ts_fn.set_value(None);
        bg_tm_fn.set_value(None);
    });

    Effect::new(move |_| {
        spawn_local(async {
            let Some(w) = web_sys::window() else {
                return;
            };
            let Some(d) = w.document() else {
                return;
            };
            let Ok(fv) = web_sys::js_sys::Reflect::get(&d, &JsValue::from_str("fonts")) else {
                return;
            };
            let fs: web_sys::FontFaceSet = fv.unchecked_into();
            let p = fs.load("bold 28px \"Bebas Neue\"");
            let _ = wasm_bindgen_futures::JsFuture::from(p).await;
        });
    });

    Effect::new(move |_| {
        let Some(_url) = url_pratinjau.get() else {
            return;
        };
        let is_event_mode = has_event_prefill.get() && !user_overrode_prefill.get();
        if !is_event_mode {
            return;
        }
        let _bg = bg_mode.get();
        let version_snapshot = cover_load_version.get_untracked();
        if cover_img_ready.get_untracked() {
            return;
        }

        spawn_local(async move {
            for _ in 0..40 {
                if cover_load_version.get_untracked() != version_snapshot {
                    return;
                }
                let ready = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| {
                        d.query_selector("img.sc-event-card-cover-img")
                            .ok()
                            .flatten()
                    })
                    .and_then(|el| el.dyn_into::<HtmlImageElement>().ok())
                    .map(|img| img.complete() && img.natural_width() > 0)
                    .unwrap_or(false);

                if ready {
                    if cover_load_version.get_untracked() == version_snapshot {
                        cover_img_ready.set(true);
                    }
                    return;
                }
                let p = web_sys::js_sys::Promise::new(&mut |resolve, _| {
                    let _ = web_sys::window()
                        .unwrap()
                        .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 50);
                });
                let _ = wasm_bindgen_futures::JsFuture::from(p).await;
            }
            if cover_load_version.get_untracked() == version_snapshot {
                web_sys::console::warn_1(
                    &"[story] cover_img_ready timeout (2s) — force enabling share button".into(),
                );
                cover_img_ready.set(true);
            }
        });
    });

    on_cleanup(move || {
        if let Some(url) = url_pratinjau.get_untracked() {
            if url.starts_with("blob:") && !sedang_mengunduh.get_untracked() {
                revoke_url(&url);
            }
        }
    });

    // ── File picker ──────────────────────────────────────────────────────────
    let saat_file_dipilih = move |ev: leptos::ev::Event| {
        let input: HtmlInputElement = ev.target().unwrap().unchecked_into();
        if let Some(files) = input.files() {
            if let Some(file) = files.get(0) {
                if let Some(old) = url_pratinjau.get_untracked() {
                    if !sedang_mengunduh.get_untracked()
                        && !sedang_mengunggah.get_untracked()
                        && old.starts_with("blob:")
                    {
                        revoke_url(&old);
                    }
                }
                const MAX: f64 = 100.0 * 1024.0 * 1024.0;
                const REJECT: &[&str] = &[
                    "image/heic",
                    "image/heif",
                    "image/x-raw",
                    "image/x-adobe-dng",
                    "image/x-canon-cr2",
                    "image/x-nikon-nef",
                ];
                if file.size() > MAX {
                    error_unggah.set(Some("File terlalu besar (maks 100 MB).".into()));
                    return;
                }
                let ftype = file.type_();
                if REJECT.iter().any(|t| ftype.as_str() == *t) {
                    error_unggah.set(Some(
                        "Format HEIC/RAW tidak didukung. Gunakan JPG, PNG, MP4, atau MOV.".into(),
                    ));
                    return;
                }
                error_unggah.set(None);
                is_video.set(file.type_().starts_with("video"));
                let is_img = file.type_().starts_with("image/");
                let url_existing = web_sys::Url::create_object_url_with_blob(&file).unwrap();
                alat_aktif.set(Alat::None);
                bg_scale.set(1.0);
                event_meta_sig.set(None);
                user_overrode_prefill.set(true);
                last_prefilled_slug.set_value(String::new());
                overlays.set(Vec::new());

                if is_img {
                    // Preview langsung dengan URL asli agar user tidak menunggu
                    url_pratinjau.set(Some(url_existing.clone()));
                    // Kompres di background: resize ke maks 1080px sisi terpanjang, WebP 0.92
                    spawn_local(async move {
                        let compressed = compress_image_file(&file, 1080, 0.92).await;
                        // Buat blob URL baru dari hasil kompresi
                        let ext = export_ext();
                        let compressed_url = web_sys::Url::create_object_url_with_blob(&compressed)
                            .unwrap_or(url_existing.clone());
                        // Buat File dari blob terkompresi (nama pakai ekstensi yang sesuai)
                        let base = file.name();
                        let stem = base.rsplit_once('.').map(|(s, _)| s).unwrap_or(&base);
                        let new_name = format!("{}.{}", stem, ext);
                        let bits = web_sys::js_sys::Array::of1(&compressed);
                        let opts = web_sys::FilePropertyBag::new();
                        opts.set_type(export_mime());
                        let compressed_file = web_sys::File::new_with_blob_sequence_and_options(
                            &bits, &new_name, &opts,
                        )
                        .ok();

                        // Ganti preview dengan URL terkompresi dan simpan file terkompresi
                        if let Some(old) = url_pratinjau.get_untracked() {
                            if old != compressed_url && old.starts_with("blob:") {
                                revoke_url(&old);
                            }
                        }
                        url_pratinjau.set(Some(compressed_url));
                        if let Some(cf) = compressed_file {
                            file_terpilih.set(Some(cf));
                        } else {
                            file_terpilih.set(Some(file));
                        }
                    });
                } else {
                    // Video: tidak dikompres, langsung set
                    file_terpilih.set(Some(file));
                    url_pratinjau.set(Some(url_existing));
                }
            }
        }
    };

    // ── Unduh (download PNG dari canvas) ─────────────────────────────────────
    let unduh_story = move || {
        if sedang_mengunduh.get_untracked() {
            return;
        }
        let Some(url) = url_pratinjau.get_untracked() else {
            return;
        };

        // ✅ Capture overlays dengan .with() + clone agar tidak baca state lama (batch timing)
        let ovls: Vec<StoryOverlay> = overlays.with(|o| o.clone());
        let filter = filter_dipilih.get_untracked();
        let scale = bg_scale.get_untracked();

        if has_event_prefill.get_untracked() && !user_overrode_prefill.get_untracked() {
            let cover = event_cover_url.get_value();
            let title = prefill_title();
            let bg_m = bg_mode.get_untracked();
            let bg_c = bg_solid_color.get_untracked();
            sedang_mengunduh.set(true);
            spawn_local(async move {
                let _guard = DownloadGuard::new(sedang_mengunduh);
                if let Err(e) = preload_fonts().await {
                    web_sys::console::warn_1(&format!("font: {:?}", e).into());
                }
                let dpr = get_dpr();
                let Some((canvas, ctx, cw, ch)) = create_export_canvas(dpr) else {
                    return;
                };

                if let Err(e) = render_event_frame_to_canvas(
                    &ctx,
                    cw,
                    ch,
                    &cover,
                    &title,
                    &prefill_date(),
                    &prefill_venue(),
                    &prefill_price(),
                    &filter,
                    &bg_m,
                    &bg_c,
                )
                .await
                {
                    web_sys::console::error_1(&format!("frame render gagal: {}", e).into());
                    let _ = load_img_to_canvas(&cover, &ctx, cw, ch, scale, false).await;
                }

                let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
                    return;
                };
                let (dom_w, dom_h) = doc
                    .query_selector(".sc-canvas-frame")
                    .ok()
                    .flatten()
                    .and_then(|el| el.dyn_into::<web_sys::Element>().ok())
                    .map(|el| {
                        let r = el.get_bounding_client_rect();
                        (r.width(), r.height())
                    })
                    .unwrap_or((390.0, 844.0));
                render_overlays_to_canvas(&ctx, &ovls, cw, ch, dom_w, dom_h, dpr);

                match canvas_to_blob(&canvas).await {
                    Ok(blob) => {
                        let ts = web_sys::js_sys::Date::now() as u64;
                        let ext = export_ext();
                        trigger_download_blob(&blob, &format!("event_story_{}.{}", ts, ext));
                        let from = read_from_page();
                        clear_from_page();
                        let nav_dl = navigate_sv.get_value();
                        exit_page_to(
                            is_exiting,
                            move || {
                                nav_dl(&from, Default::default());
                            },
                            EXIT_DURATION_MS,
                        );
                    }
                    Err(e) => {
                        web_sys::console::error_1(&format!("blob: {}", e).into());
                        let msg = if e.contains("taint")
                            || e.contains("Security")
                            || e.contains("CORS")
                        {
                            "Export gagal: CDN tidak mengizinkan akses gambar (CORS). \
                             Coba ganti latar ke Solid atau Gradient."
                                .to_string()
                        } else {
                            format!("Export gagal: {}", e)
                        };
                        error_unggah.set(Some(msg));
                    }
                }
                canvas.set_width(0);
                canvas.set_height(0);
            });
            return;
        }

        let vid = is_video.get_untracked();
        let bg_info = {
            let mode = bg_mode.get_untracked();
            if mode == "blur" {
                None
            } else if mode == "solid" {
                Some(BgExport::Solid(bg_solid_color.get_untracked()))
            } else {
                gradient_colors(&mode).map(|(s, e)| BgExport::Gradient {
                    color_start: s,
                    color_end: e,
                })
            }
        };
        let nama = file_terpilih
            .get_untracked()
            .map(|f| f.name())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "story".to_string());

        if url.starts_with("blob:") {
            export_story_canvas(
                url,
                vid,
                ovls,
                filter,
                nama,
                scale,
                bg_info,
                sedang_mengunduh,
            );
        } else {
            error_unggah.set(Some(
                "Pilih gambar dari galeri terlebih dahulu untuk mengunduh.".to_string(),
            ));
        }
    };

    // ── Bagikan (upload ke backend) ──────────────────────────────────────────
    let bagikan = move || {
        let slug = event_meta_sig
            .get()
            .map(|m| m.event_slug)
            .filter(|s| !s.is_empty());

        // ─── MODE A: user upload file sendiri ────────────────────────────────
        if let Some(file) = file_terpilih.get() {
            // ✅ Capture SEMUA state sebelum do_exit() — setelah exit, signal bisa tidak valid
            let ovls_snapshot: Vec<StoryOverlay> = overlays.with(|o| o.clone());
            let pratinjau_url = url_pratinjau.get_untracked().unwrap_or_default();
            let filter_val = filter_dipilih.get_untracked();
            let scale_val = bg_scale.get_untracked();
            let bg_mode_val = bg_mode.get_untracked();
            let bg_color_val = bg_solid_color.get_untracked();
            let is_vid = is_video.get_untracked();

            sedang_mengunggah.set(true);
            store_ctx.uploading.set(true);
            error_unggah.set(None);
            let ctx = store_ctx;

            // ✅ Jika ada overlays (teks/stiker), render dulu ke canvas lalu upload hasilnya.
            // Tanpa ini, overlays TIDAK akan muncul di story yang diupload — file asli diupload
            // tanpa overlay apapun.
            let has_overlays = !ovls_snapshot.is_empty();

            do_exit();

            spawn_local(async move {
                // ── Fast path: tidak ada overlay & video → upload file langsung ──
                if !has_overlays || is_vid {
                    match story_svc::upload_story(&file, None).await {
                        Ok(_) => {
                            ctx.uploading.set(false);
                            ctx.load();
                        }
                        Err(e) => {
                            ctx.uploading.set(false);
                            let is_network_err = e.message.contains("NetworkError")
                                || e.message.contains("Failed to fetch")
                                || e.message.contains("fetch")
                                || e.message.contains("network")
                                || e.message.contains("refused")
                                || e.message.contains("ERR_");
                            if is_network_err && !pratinjau_url.is_empty() {
                                web_sys::console::warn_1(
                                    &"[story] Backend tidak tersedia — fallback ke download lokal"
                                        .into(),
                                );
                                let ts = web_sys::js_sys::Date::now() as u64;
                                if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                                    if let Ok(el) = doc.create_element("a") {
                                        let a: web_sys::HtmlAnchorElement = el.unchecked_into();
                                        a.set_href(&pratinjau_url);
                                        a.set_download(&format!("story_{}.png", ts));
                                        a.set_attribute("style", "display:none").ok();
                                        if let Some(body) = doc.body() {
                                            let _ = body.append_child(&a);
                                            a.click();
                                            let _ = body.remove_child(&a);
                                        }
                                    }
                                }
                            } else {
                                web_sys::console::error_1(
                                    &format!("Upload gagal: {}", e.message).into(),
                                );
                            }
                        }
                    }
                    return;
                }

                // ── Render path: ada overlay teks/stiker → bake ke canvas dulu ──
                if let Err(e) = preload_fonts().await {
                    web_sys::console::warn_1(&format!("[story] font preload: {:?}", e).into());
                }

                let dpr = get_dpr();
                let Some((canvas, render_ctx, cw, ch)) = create_export_canvas(dpr) else {
                    ctx.uploading.set(false);
                    return;
                };

                // Gambar background (solid/gradient) jika bukan blur
                if bg_mode_val != "blur" {
                    if bg_mode_val == "solid" {
                        render_ctx.set_fill_style_str(&bg_color_val);
                        render_ctx.fill_rect(0.0, 0.0, cw, ch);
                    } else if let Some((cs, ce)) = gradient_colors(&bg_mode_val) {
                        if let Ok(g) = render_ctx
                            .create_linear_gradient(0.0, 0.0, 0.0, ch)
                            .dyn_into::<web_sys::CanvasGradient>()
                        {
                            let _ = g.add_color_stop(0.0, cs);
                            let _ = g.add_color_stop(1.0, ce);
                            render_ctx.set_fill_style(&g.into());
                            render_ctx.fill_rect(0.0, 0.0, cw, ch);
                        }
                    }
                }

                // Gambar media (gambar dari galeri)
                let fs = css_filter_string(&filter_val);
                if fs != "none" {
                    render_ctx.set_filter(fs);
                }
                if let Err(e) =
                    load_img_to_canvas(&pratinjau_url, &render_ctx, cw, ch, scale_val, false).await
                {
                    web_sys::console::warn_1(
                        &format!("[story] load img gagal saat render overlay: {}", e).into(),
                    );
                }
                render_ctx.set_filter("none");

                // Ambil dimensi DOM untuk scaling overlay yang presisi
                let (dom_w, dom_h) = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.query_selector(".sc-canvas-frame").ok().flatten())
                    .and_then(|el| el.dyn_into::<web_sys::Element>().ok())
                    .map(|el| {
                        let r = el.get_bounding_client_rect();
                        (r.width(), r.height())
                    })
                    .unwrap_or((390.0, 844.0));

                // ✅ Render overlays (teks + stiker) ke canvas
                render_overlays_to_canvas(&render_ctx, &ovls_snapshot, cw, ch, dom_w, dom_h, dpr);

                // Export canvas → blob → File
                let blob = match canvas_to_blob(&canvas).await {
                    Ok(b) => b,
                    Err(e) => {
                        ctx.uploading.set(false);
                        web_sys::console::error_1(
                            &format!("[story] canvas to blob gagal: {}", e).into(),
                        );
                        canvas.set_width(0);
                        canvas.set_height(0);
                        return;
                    }
                };
                canvas.set_width(0);
                canvas.set_height(0);

                let ext = export_ext();
                let bits = web_sys::js_sys::Array::new();
                bits.push(&blob);
                let opts = web_sys::FilePropertyBag::new();
                opts.set_type(export_mime());
                let upload_file = match web_sys::File::new_with_blob_sequence_and_options(
                    &bits,
                    &format!("story.{}", ext),
                    &opts,
                ) {
                    Ok(f) => f,
                    Err(_) => {
                        ctx.uploading.set(false);
                        return;
                    }
                };

                match story_svc::upload_story(&upload_file, None).await {
                    Ok(_) => {
                        ctx.uploading.set(false);
                        ctx.load();
                    }
                    Err(e) => {
                        ctx.uploading.set(false);
                        web_sys::console::error_1(
                            &format!("[story] upload dengan overlay gagal: {}", e.message).into(),
                        );
                    }
                }
            });
            return;
        }

        // ─── MODE B: event story — render canvas, upload hasilnya ─────────────
        if has_event_prefill.get() && !user_overrode_prefill.get() {
            let cover = event_cover_url.get_value();
            let title = prefill_title();
            let bg_m = bg_mode.get();
            let bg_c = bg_solid_color.get();

            if cover.is_empty() {
                error_unggah.set(Some("Cover event tidak tersedia di URL.".into()));
                return;
            }

            // ✅ Capture overlays SEBELUM do_exit() — setelah exit, reactive scope
            // di-cleanup dan overlays.get() di dalam spawn_local bisa kembalikan empty/panic.
            let ovls_snapshot: Vec<StoryOverlay> = overlays.with(|o| o.clone());
            // ✅ Capture filter_dipilih sebelum exit juga
            let filter_snapshot = filter_dipilih.get();

            sedang_mengunggah.set(true);
            store_ctx.uploading.set(true);
            error_unggah.set(None);
            let ctx = store_ctx;

            do_exit();

            spawn_local(async move {
                if let Err(e) = preload_fonts().await {
                    web_sys::console::warn_1(&format!("font preload: {:?}", e).into());
                }

                let dpr = get_dpr();
                let Some((canvas, render_ctx, cw, ch)) = create_export_canvas(dpr) else {
                    ctx.uploading.set(false);
                    web_sys::console::error_1(&"Gagal membuat canvas export".into());
                    return;
                };

                // Gunakan filter_snapshot (sudah di-capture sebelum exit)
                if let Err(e) = render_event_frame_to_canvas(
                    &render_ctx,
                    cw,
                    ch,
                    &cover,
                    &title,
                    &prefill_date(),
                    &prefill_venue(),
                    &prefill_price(),
                    filter_snapshot.as_str(),
                    &bg_m,
                    &bg_c,
                )
                .await
                {
                    web_sys::console::warn_1(
                        &format!("frame render: {} — fallback solid bg", e).into(),
                    );
                    render_ctx.set_fill_style_str("#0d0d18");
                    render_ctx.fill_rect(0.0, 0.0, cw, ch);
                    render_ctx.set_font("700 84px \"Bebas Neue\", \"Arial Black\", sans-serif");
                    render_ctx.set_fill_style_str("#ffffff");
                    render_ctx.set_text_align("center");
                    render_ctx.set_text_baseline("middle");
                    let _ = render_ctx.fill_text(&title, cw / 2.0, ch / 2.0);
                }

                let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
                    ctx.uploading.set(false);
                    return;
                };
                let (dom_w, dom_h) = doc
                    .query_selector(".sc-canvas-frame")
                    .ok()
                    .flatten()
                    .and_then(|el| el.dyn_into::<web_sys::Element>().ok())
                    .map(|el| {
                        let r = el.get_bounding_client_rect();
                        (r.width(), r.height())
                    })
                    .unwrap_or((390.0, 844.0));
                // ✅ Gunakan ovls_snapshot yang sudah di-capture sebelum exit
                render_overlays_to_canvas(&render_ctx, &ovls_snapshot, cw, ch, dom_w, dom_h, dpr);

                let blob = match canvas_to_blob(&canvas).await {
                    Ok(b) => b,
                    Err(e) => {
                        ctx.uploading.set(false);
                        web_sys::console::error_1(&format!("canvas blob: {}", e).into());
                        canvas.set_width(0);
                        canvas.set_height(0);
                        return;
                    }
                };
                canvas.set_width(0);
                canvas.set_height(0);

                let bits = web_sys::js_sys::Array::new();
                bits.push(&blob);
                let opts = web_sys::FilePropertyBag::new();
                let ext = export_ext();
                opts.set_type(export_mime());
                let file = match web_sys::File::new_with_blob_sequence_and_options(
                    &bits,
                    &format!("event_story.{}", ext),
                    &opts,
                ) {
                    Ok(f) => f,
                    Err(_) => {
                        ctx.uploading.set(false);
                        web_sys::console::error_1(&"Gagal membuat File dari blob".into());
                        return;
                    }
                };

                match story_svc::upload_story(&file, slug).await {
                    Ok(_) => {
                        ctx.uploading.set(false);
                        ctx.load();
                    }
                    Err(e) => {
                        ctx.uploading.set(false);
                        web_sys::console::error_1(
                            &format!("Upload event story gagal: {}", e.message).into(),
                        );
                    }
                }
            });
        }
    };

    // ── Overlay operations ───────────────────────────────────────────────────
    let tambah_teks = move || {
        let teks = input_teks.get();
        if teks.trim().is_empty() {
            return;
        }
        let y = overlays.with(|o| (30.0 + (o.len() % 5) as f64 * 10.0).min(70.0));
        let z = z_counter.get_untracked() + 1;
        z_counter.set(z);
        overlays.update(|o| {
            o.push(StoryOverlay {
                id: buat_id("teks"),
                overlay_type: OverlayType::Text,
                x: 50.0,
                y,
                content: Some(teks),
                color: Some(warna_teks.get()),
                font_size: Some(ukuran_teks.get()),
                rotation: Some(0.0),
                emoji: None,
                scale: Some(1.0),
                z_index: z,
                text_style: Some(text_style.get()),
                text_align: Some(text_align.get()),
            })
        });
        input_teks.set(String::new());
        alat_aktif.set(Alat::None);
    };

    let tambah_stiker = move |emoji: String| {
        let (x, y) = overlays.with(|o| {
            let n = o.len() % 5;
            (
                (45.0 + n as f64 * 6.0).min(70.0),
                (35.0 + n as f64 * 9.0).min(70.0),
            )
        });
        let z = z_counter.get_untracked() + 1;
        z_counter.set(z);
        overlays.update(|o| {
            o.push(StoryOverlay {
                id: buat_id("stiker"),
                overlay_type: OverlayType::Sticker,
                x,
                y,
                emoji: Some(emoji),
                scale: Some(1.2),
                rotation: Some(0.0),
                content: None,
                color: None,
                font_size: None,
                z_index: z,
                text_style: None,
                text_align: None,
            })
        });
        alat_aktif.set(Alat::None);
    };

    let perbarui_overlay = move |updated: StoryOverlay| {
        overlays.update(|o| {
            if let Some(p) = o.iter().position(|x| x.id == updated.id) {
                o[p] = updated;
            }
        });
    };
    let hapus_overlay = move |id: String| {
        overlays.update(|o| o.retain(|x| x.id != id));
        if selected_overlay_id.get_untracked() == id {
            selected_overlay_id.set(String::new());
        }
    };
    let bring_to_front = move |id: String, zc: RwSignal<u32>, ovls: RwSignal<Vec<StoryOverlay>>| {
        let z = zc.get_untracked() + 1;
        zc.set(z);
        ovls.update(|o| {
            if let Some(p) = o.iter().position(|x| x.id == id) {
                o[p].z_index = z;
            }
        });
    };
    let pilih_overlay = move |id: String| {
        let now = web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0);
        let (lid, lts) = last_tap.get_value();
        let dbl = lid == id && (now - lts) < 300.0;
        last_tap.set_value((id.clone(), now));
        if dbl && !id.is_empty() {
            selected_overlay_id.set(id.clone());
            return;
        }
        let cur = selected_overlay_id.get_untracked();
        if cur == id && !id.is_empty() {
            selected_overlay_id.set(String::new());
        } else {
            selected_overlay_id.set(id.clone());
            bring_to_front(id, z_counter, overlays);
        }
    };

    let kelas_media = move || {
        let f = filter_dipilih.get();
        if f == "normal" || !DAFTAR_FILTER.iter().any(|(k, _)| *k == f.as_str()) {
            "sc-media".to_string()
        } else {
            format!("sc-media filter-{}", f)
        }
    };

    let tutup_panel = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Escape" {
            alat_aktif.set(Alat::None);
        }
    };

    let can_share = Memo::new(move |_| {
        let is_event_mode = has_event_prefill.get() && !user_overrode_prefill.get();
        !is_event_mode || cover_img_ready.get()
    });

    // ── Swipe down to dismiss ─────────────────────────────────────────────────
    let on_swipe_start = move |ev: leptos::ev::TouchEvent| {
        if let Some(t) = ev.touches().get(0) {
            swipe_start_y.set_value(t.client_y() as f64);
            swipe_start_x.set_value(t.client_x() as f64);
            swipe_active.set_value(true);
        }
    };

    let on_swipe_end = move |ev: leptos::ev::TouchEvent| {
        if !swipe_active.get_value() {
            return;
        }
        swipe_active.set_value(false);
        if let Some(t) = ev.changed_touches().get(0) {
            let dy = (t.client_y() as f64) - swipe_start_y.get_value();
            let dx = (t.client_x() as f64) - swipe_start_x.get_value();
            if dy > 120.0 && dx.abs() < 80.0 {
                do_exit();
            }
        }
    };

    // ── VIEW ─────────────────────────────────────────────────────────────────
    view! {
        <div
            class="sc-halaman"
            class:is-exiting=move || is_exiting.get()
            on:keydown=tutup_panel
            tabindex="-1"
        >

            <header class="sc-appbar">
                <button
                    class="sc-tombol-bulat"
                    attr:aria-label="Tutup"
                    on:click=move |_| do_exit()
                >
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <line x1="18" y1="6"  x2="6"  y2="18"/>
                        <line x1="6"  y1="6"  x2="18" y2="18"/>
                    </svg>
                </button>
                <div class="sc-badge-langsung">
                    <span class="sc-dot-merah"></span>
                    "CERITA"
                </div>
                <button class="sc-tombol-bulat" aria-label="Unduh" title="Unduh"
                    disabled=move || url_pratinjau.get().is_none() || sedang_mengunduh.get()
                    on:click=move |_| unduh_story()>
                    {move || if sedang_mengunduh.get() {
                        view! {
                            <span class="sc-spinner" style="width:16px;height:16px;border-width:2px;"></span>
                        }.into_any()
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

            <div class="sc-toolbar-atas">
                <button class="sc-tombol-alat-kecil" aria-label="Flash">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z"/>
                    </svg>
                </button>
                <button class="sc-tombol-alat-kecil" aria-label="Timer">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>
                    </svg>
                </button>
                <button class="sc-tombol-alat-kecil sc-label-pill" aria-label="Rasio aspek">"9:16"</button>
            </div>

            <Show when=move || error_unggah.get().is_some()>
                <div class="sc-notif-gagal" role="alert">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                        <circle cx="12" cy="12" r="10"/>
                        <line x1="12" y1="8" x2="12" y2="12"/>
                        <line x1="12" y1="16" x2="12.01" y2="16"/>
                    </svg>
                    <span>{move || error_unggah.get().unwrap_or_default()}</span>
                    <button class="sc-tombol-coba-lagi"
                        on:click=move |_| { error_unggah.set(None); bagikan(); }>
                        "Coba Lagi"
                    </button>
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
                    <div class="sc-panduan-fokus"
                         on:click=move |_| { if let Some(i) = file_ref.get() { i.click(); } }>
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

                        <Show when=move || bg_mode.get() == "blur" && !has_event_prefill.get()>
                            {move || url_pratinjau.get().map(|url| view! {
                                <div class="sc-canvas-bg" style=format!("background-image:url('{}');", url) />
                            })}
                        </Show>

                        <Show when=move || bg_mode.get() != "blur" && (user_overrode_prefill.get() || !has_event_prefill.get())>
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
                            {move || {
                                let scale = bg_scale.get();
                                let ts = if (scale - 1.0).abs() > 0.001 {
                                    format!("transform:scale({scale:.3});transform-origin:center center;")
                                } else { String::new() };

                                url_pratinjau.get().map(|url| {
                                    let uc = url.clone();
                                    let is_event = has_event_prefill.get() && !user_overrode_prefill.get();
                                    let is_blob  = url.starts_with("blob:");

                                    if is_event {
                                        let url_sv = StoredValue::new(url.clone());
                                        let uc_sv  = StoredValue::new(uc.clone());
                                        view! {
                                            <div class="sc-event-preview-frame"
                                                 style=move || {
                                                     let mode = bg_mode.get();
                                                     if mode == "blur" { String::new() }
                                                     else if mode == "solid" {
                                                         format!("background-color:{};", bg_solid_color.get())
                                                     } else if let Some(css) = BG_GRADIENTS.iter()
                                                         .find(|(k,_,_,_)| *k == mode.as_str())
                                                         .map(|(_,c,_,_)| *c)
                                                     {
                                                         format!("background:{};", css)
                                                     } else { "background-color:#1a1a2e;".to_string() }
                                                 }>
                                                <Show when=move || bg_mode.get() == "blur">
                                                    <img
                                                        src=move || url_sv.get_value()
                                                        class="sc-event-bg-img sc-media"
                                                        alt="Event cover"
                                                        on:load=move |_| cover_img_ready.set(true)
                                                        on:error=move |_| {
                                                            web_sys::console::warn_1(
                                                                &format!("[story] cover load err: {}", uc_sv.get_value()).into()
                                                            );
                                                        }
                                                    />
                                                    <div class="sc-event-dark-overlay" />
                                                </Show>
                                                <div class="sc-event-card">
                                                    <div class="sc-event-card-cover-wrap">
                                                        <img
                                                            src=move || url_sv.get_value()
                                                            class="sc-event-card-cover-img"
                                                            alt=""
                                                            on:load=move |_| cover_img_ready.set(true)
                                                        />
                                                    </div>
                                                    <div class="sc-event-card-body">
                                                        <div class="sc-event-card-badge">
                                                            <span class="sc-event-card-dot"></span>
                                                            "KINETIC EXCLUSIVE"
                                                        </div>
                                                        <h2 class="sc-event-card-title">
                                                            {move || prefill_title()}
                                                        </h2>
                                                        <div class="sc-event-card-sep"></div>
                                                        <span class="sc-event-card-meta-label">"DATE & VENUE"</span>
                                                        <div class="sc-event-card-meta-row">
                                                            <span class="sc-event-card-date">
                                                                {move || prefill_date()}
                                                            </span>
                                                            <span class="sc-event-card-venue">
                                                                {move || prefill_venue()}
                                                            </span>
                                                        </div>
                                                        <div class="sc-event-card-price-pill">
                                                            {move || prefill_price()}
                                                        </div>
                                                    </div>
                                                </div>
                                            </div>
                                        }.into_any()
                                    } else if is_video.get() {
                                        view! {
                                            <video src=url class=kelas_media() style=ts
                                                crossorigin=is_blob.then_some("anonymous")
                                                autoplay muted playsinline loop />
                                        }.into_any()
                                    } else {
                                        let urlclone = url.clone();
                                        view! {
                                            <img src=url class=kelas_media() style=ts
                                                crossorigin=is_blob.then_some("anonymous")
                                                alt="Story media"
                                                on:error=move |_| {
                                                    web_sys::console::error_1(
                                                        &format!("[story] img error: {}", urlclone).into()
                                                    );
                                                }
                                            />
                                        }.into_any()
                                    }
                                })
                            }}
                        </div>

                        <Show when=move || !has_event_prefill.get() || user_overrode_prefill.get()>
                            <div class="sc-media-overlay" aria-hidden="true"></div>
                        </Show>

                        <Show when=move || has_event_prefill.get() && !user_overrode_prefill.get()>
                            <div class="sc-event-prefill-banner">
                                <div class="sc-event-prefill-icon">
                                    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
                                        <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z"/>
                                    </svg>
                                </div>
                                <div class="sc-event-prefill-text">
                                    <span class="sc-event-prefill-label">"Berbagi event"</span>
                                    <span class="sc-event-prefill-title">{move || prefill_title()}</span>
                                </div>
                                <div class="sc-event-prefill-badge">"LIVE PULSE"</div>
                            </div>
                        </Show>

                        <div class="sc-layer-overlays"
                            on:click=move |ev| {
                                if let (Some(t), Some(c)) = (ev.target(), ev.current_target()) {
                                    if t == c { selected_overlay_id.set(String::new()); }
                                }
                            }>
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
                                                let z = z_counter.get_untracked() + 1;
                                                z_counter.set(z);
                                                overlays.update(|o| {
                                                    if let Some(p) = o.iter().position(|x| x.id == id) {
                                                        o[p].z_index = z;
                                                    }
                                                });
                                            })
                                            is_selected=Signal::derive(move || {
                                                let s = selected_overlay_id.get();
                                                !s.is_empty() && s == oid
                                            })
                                            on_select=Callback::new(move |_| pilih_overlay(oid2.clone()))
                                            on_trash_hover=Some(Callback::new(move |active: bool| {
                                                trash_zone_active.set(active);
                                            }))
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
                            on:click=move |_| bg_mode.set("blur".to_string())
                            aria-label="Blur" />
                        {BG_SOLID_COLORS.iter().map(|&c| {
                            let col = c.to_string();
                            let ca  = col.clone();
                            let cc  = col.clone();
                            view! {
                                <button class="sc-bg-chip"
                                    class:sc-bg-chip--aktif=move || bg_mode.get() == "solid" && bg_solid_color.get() == ca
                                    style=format!("background-color:{};", col)
                                    on:click=move |_| { bg_mode.set("solid".to_string()); bg_solid_color.set(cc.clone()); }
                                    aria-label=format!("Warna {}", c) />
                            }
                        }).collect_view()}
                        {BG_GRADIENTS.iter().map(|&(key, css, _, _)| {
                            let k  = key.to_string();
                            let ka = k.clone();
                            let kc = k.clone();
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
                        <span class="sc-teks-badge-musik">
                            {move || musik_dipilih.get().map(|(_,j,a)| format!("{} • {}", j, a)).unwrap_or_default()}
                        </span>
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
                    on_click=move |ev: leptos::ev::MouseEvent| {
                        ev.stop_propagation();
                        alat_aktif.update(|t| *t = if *t == Alat::Teks { Alat::None } else { Alat::Teks });
                    }>
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <polyline points="4 7 4 4 20 4 20 7"/><line x1="9" y1="20" x2="15" y2="20"/><line x1="12" y1="4" x2="12" y2="20"/>
                    </svg>
                    <span class="sc-tombol-alat-label">"Teks"</span>
                </TombolAlat>
                <TombolAlat aktif=alat_aktif target=Alat::Stiker label="Stiker"
                    on_click=move |ev: leptos::ev::MouseEvent| {
                        ev.stop_propagation();
                        alat_aktif.update(|t| *t = if *t == Alat::Stiker { Alat::None } else { Alat::Stiker });
                    }>
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <rect x="3" y="3" width="18" height="18" rx="4"/><path d="M8 12h8M12 8v8"/>
                    </svg>
                    <span class="sc-tombol-alat-label">"Stiker"</span>
                </TombolAlat>
                <TombolAlat aktif=alat_aktif target=Alat::Musik label="Musik"
                    on_click=move |ev: leptos::ev::MouseEvent| {
                        ev.stop_propagation();
                        alat_aktif.update(|t| *t = if *t == Alat::Musik { Alat::None } else { Alat::Musik });
                    }>
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
                    on_click=move |ev: leptos::ev::MouseEvent| {
                        ev.stop_propagation();
                        alat_aktif.update(|t| *t = if *t == Alat::Filter { Alat::None } else { Alat::Filter });
                    }>
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M15 4V2M15 16v-2M8 9h2M20 9h2M17.8 11.8L19.2 13.2M17.8 6.2L19.2 4.8"/>
                        <circle cx="15" cy="9" r="3"/>
                    </svg>
                    <span class="sc-tombol-alat-label">"Filter"</span>
                </TombolAlat>
                <TombolAlat aktif=alat_aktif target=Alat::Latar label="Latar"
                    on_click=move |ev: leptos::ev::MouseEvent| {
                        ev.stop_propagation();
                        alat_aktif.update(|t| *t = if *t == Alat::Latar { Alat::None } else { Alat::Latar });
                    }>
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
                         style=move || format!(
                             "color:{}; font-size:{}px; text-align:{}; background:{};",
                             warna_teks.get(), ukuran_teks.get(), text_align.get(),
                             if text_bg_enabled.get() { "rgba(0,0,0,0.75)" } else { "transparent" }
                         )>
                        {move || if input_teks.get().is_empty() {
                            "Ketik sesuatu...".to_string().into_view()
                        } else { input_teks.get().into_view() }}
                    </div>
                    <div class="sc-text-edit-row">
                        <div class="sc-size-rail">
                            <input type="range" min="14" max="120" step="2" class="sc-slider-vertical"
                                prop:value=move || ukuran_teks.get().to_string()
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<u32>() { ukuran_teks.set(v); }
                                } />
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
                                {[("left","⬅"),("center","↔"),("right","➡")].iter().map(|(a, icon)| {
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
                        {WARNA_TEKS.iter().map(|&c| {
                            let w = c.to_string();
                            view! { <TombolWarna warna=w.clone() dipilih=warna_teks /> }
                        }).collect_view()}
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
                            let ids = id.to_string();
                            let idc = ids.clone();
                            let j   = judul.to_string();
                            let a   = artis.to_string();
                            view! {
                                <button class="sc-item-musik"
                                    class:sc-item-musik--aktif=move || musik_dipilih.get().map(|(m,_,_)| m == idc).unwrap_or(false)
                                    on:click=move |_| {
                                        musik_dipilih.set(Some((ids.clone(), j.clone(), a.clone())));
                                        alat_aktif.set(Alat::None);
                                    }>
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
                                        view! {
                                            <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                                                 stroke="#39ff8a" stroke-width="2.5">
                                                <polyline points="20 6 9 17 4 12"/>
                                            </svg>
                                        }.into_any()
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
                                    {if k == "normal" { view! { <span class="sc-label-normal">"●"</span> }.into_any() }
                                     else { view! { <span></span> }.into_any() }}
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
                        Some(url) if !has_event_prefill.get() || user_overrode_prefill.get() =>
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
                                view! {
                                    <svg width="26" height="26" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2.5">
                                        <polyline points="20 6 9 17 4 12"/>
                                    </svg>
                                }.into_any()
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
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2" stroke-linecap="round">
                        <path d="M20 7H4V3l-2 2 2 2M4 17h16v4l2-2-2-2"/>
                        <circle cx="12" cy="12" r="3"/>
                    </svg>
                </button>
            </footer>
        </div>
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
//  SUB-KOMPONEN
// ═══════════════════════════════════════════════════════════════════════════════

#[component]
fn TombolAlat(
    aktif: RwSignal<Alat>,
    target: Alat,
    label: &'static str,
    on_click: impl Fn(leptos::ev::MouseEvent) + 'static,
    children: Children,
) -> impl IntoView {
    view! {
        <button class="sc-tombol-alat"
            class:sc-tombol-alat--aktif=move || aktif.get() == target
            on:click=on_click aria-label=label title=label>
            {children()}
        </button>
    }
}

#[component]
fn TombolWarna(warna: String, dipilih: RwSignal<String>) -> impl IntoView {
    let wk = warna.clone();
    let wc = warna.clone();
    view! {
        <button class="sc-tombol-warna"
            class:sc-tombol-warna--aktif=move || dipilih.get() == wc
            style=format!("background-color:{}", warna)
            on:click=move |_| dipilih.set(wk.clone())
            aria-label=format!("Pilih warna {}", warna.clone()) />
    }
}

#[component]
fn PanelGeser(
    #[prop(default = "")] judul: &'static str,
    on_tutup: impl Fn() + 'static,
    children: Children,
) -> impl IntoView {
    view! {
        <div
            class="sc-panel-geser"
            role="dialog"
            aria-modal="false"
            aria-label=format!("Panel {}", judul)
        >
            <div class="sc-panel-header">
                <div class="sc-garis-pemisah"></div>
                <Show when=move || !judul.is_empty()>
                    <div class="sc-judul-panel">{judul}</div>
                </Show>
                <button
                    class="sc-tombol-tutup-panel"
                    aria-label="Tutup panel"
                    on:click=move |_| on_tutup()
                >
                    <svg
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                    >
                        <line x1="18" y1="6" x2="6" y2="18" />
                        <line x1="6" y1="6" x2="18" y2="18" />
                    </svg>
                </button>
            </div>
            {children()}
        </div>
    }
}
