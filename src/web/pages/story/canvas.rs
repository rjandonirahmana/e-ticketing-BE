// ═══════════════════════════════════════════════════════════════════════════════
//  STORY — Canvas drawing helpers
// ═══════════════════════════════════════════════════════════════════════════════

use leptos::prelude::{RwSignal, Set};
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement, HtmlVideoElement};

use crate::web::state::stories::StoryOverlay;
use crate::web::state::stories::OverlayType;

// ── Geometry helpers ──────────────────────────────────────────────────────────

pub(super) fn cover_src_rect(
    img_w: f64, img_h: f64, canvas_w: f64, canvas_h: f64,
) -> (f64, f64, f64, f64) {
    let scale = (canvas_w / img_w).max(canvas_h / img_h);
    let sx = ((img_w * scale - canvas_w) / 2.0) / scale;
    let sy = ((img_h * scale - canvas_h) / 2.0) / scale;
    (sx, sy, canvas_w / scale, canvas_h / scale)
}

pub(super) fn cover_factor(img_w: f64, img_h: f64, canvas_w: f64, canvas_h: f64) -> f64 {
    if img_w <= 0.0 || img_h <= 0.0 { return 1.0; }
    let contain = (canvas_w / img_w).min(canvas_h / img_h);
    let cover   = (canvas_w / img_w).max(canvas_h / img_h);
    if contain <= 0.0 { 1.0 } else { cover / contain }
}

pub(super) fn gradient_colors(key: &str) -> Option<(&'static str, &'static str)> {
    use super::types::BG_GRADIENTS;
    BG_GRADIENTS.iter().find(|(k,_,_,_)| *k == key).map(|(_,_,s,e)| (*s,*e))
}

// ── CSS filter string ─────────────────────────────────────────────────────────

pub(super) fn css_filter_string(filter: &str) -> &'static str {
    match filter {
        "clarendon" => "contrast(1.2) saturate(1.3)",
        "gingham"   => "brightness(1.05) hue-rotate(-10deg)",
        "moon"      => "grayscale(1) contrast(1.1)",
        "lark"      => "contrast(0.9)",
        "reyes"     => "sepia(0.5) contrast(0.9)",
        "juno"      => "contrast(1.1) saturate(1.2)",
        "slumber"   => "brightness(0.9) saturate(0.8)",
        "crema"     => "sepia(0.3) contrast(0.95)",
        "ludwig"    => "saturate(1.1) contrast(1.05)",
        _           => "none",
    }
}

// ── Font helpers ──────────────────────────────────────────────────────────────

pub(super) fn font_for_style(style: &str) -> &'static str {
    match style.to_ascii_lowercase().as_str() {
        "modern"     => "\"Instagram Sans\", -apple-system, BlinkMacSystemFont, sans-serif",
        "strong"     => "\"Arial Black\", \"Helvetica Neue\", sans-serif",
        "typewriter" => "\"Courier New\", Courier, monospace",
        _            => "\"Bebas Neue\", \"Arial Black\", sans-serif",
    }
}

// ── Device pixel ratio ────────────────────────────────────────────────────────

pub(super) fn get_dpr() -> f64 {
    #[cfg(target_arch = "wasm32")]
    return web_sys::window().map(|w| w.device_pixel_ratio()).unwrap_or(1.0).clamp(1.0, 3.0);
    #[cfg(not(target_arch = "wasm32"))]
    1.0
}

// ── Export canvas creation ────────────────────────────────────────────────────

pub(super) fn create_export_canvas(
    dpr: f64,
) -> Option<(HtmlCanvasElement, CanvasRenderingContext2d, f64, f64)> {
    let export_dpr = dpr.min(2.0);
    let doc = web_sys::window()?.document()?;
    let canvas: HtmlCanvasElement = doc.create_element("canvas").ok()?.unchecked_into();
    let (lw, lh) = (1080.0_f64, 1920.0_f64);
    canvas.set_width((lw * export_dpr) as u32);
    canvas.set_height((lh * export_dpr) as u32);
    let ctx: CanvasRenderingContext2d = canvas.get_context("2d").ok()??.unchecked_into();
    ctx.set_transform(export_dpr, 0.0, 0.0, export_dpr, 0.0, 0.0).ok()?;
    Some((canvas, ctx, lw, lh))
}

// ── Font preloading ───────────────────────────────────────────────────────────

static FONTS_PRELOADED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub(super) async fn preload_fonts() -> Result<(), JsValue> {
    use std::sync::atomic::Ordering;
    if FONTS_PRELOADED.load(Ordering::Acquire) { return Ok(()); }
    #[cfg(target_arch = "wasm32")]
    {
        let Some(win) = web_sys::window() else { return Ok(()); };
        let Some(doc) = win.document() else { return Ok(()); };
        let fv = web_sys::js_sys::Reflect::get(&doc, &JsValue::from_str("fonts"))?;
        let fs: web_sys::FontFaceSet = fv.unchecked_into();
        let arr = web_sys::js_sys::Array::new();
        for f in ["bold 48px \"Bebas Neue\"", "bold 28px \"Bebas Neue\"",
                  "bold 28px -apple-system, BlinkMacSystemFont, sans-serif"] {
            let p: web_sys::js_sys::Promise = fs.load(f).unchecked_into();
            arr.push(&p);
        }
        wasm_bindgen_futures::JsFuture::from(web_sys::js_sys::Promise::all(&arr)).await?;
        let ready: web_sys::js_sys::Promise = fs.ready()?;
        let result = wasm_bindgen_futures::JsFuture::from(ready).await.map(|_| ());
        if result.is_ok() { FONTS_PRELOADED.store(true, Ordering::Release); }
        return result;
    }
    #[cfg(not(target_arch = "wasm32"))]
    Ok(())
}

// ── Draw image helpers ────────────────────────────────────────────────────────

pub(super) fn draw_img_contain(
    ctx: &CanvasRenderingContext2d,
    img: &HtmlImageElement,
    iw: f64, ih: f64, cw: f64, ch: f64, scale: f64,
) -> Result<(), String> {
    let prev = ctx.filter();
    ctx.set_filter("none");
    ctx.set_fill_style_str("#000");
    ctx.fill_rect(0.0, 0.0, cw, ch);
    ctx.set_filter(&prev);
    let base = (cw / iw).min(ch / ih);
    let s = base * scale;
    let w = iw * s; let h = ih * s;
    ctx.draw_image_with_html_image_element_and_dw_and_dh(img, (cw-w)/2.0, (ch-h)/2.0, w, h)
        .map_err(|e| format!("{:?}", e))
}

pub(super) fn draw_img_cover(
    ctx: &CanvasRenderingContext2d,
    img: &HtmlImageElement,
    iw: f64, ih: f64, cw: f64, ch: f64, scale: f64,
) -> Result<(), String> {
    let (sx, sy, sw, sh) = cover_src_rect(iw, ih, cw, ch);
    let zsw = sw / scale; let zsh = sh / scale;
    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
        img, sx+(sw-zsw)/2.0, sy+(sh-zsh)/2.0, zsw, zsh, 0.0, 0.0, cw, ch,
    ).map_err(|e| format!("{:?}", e))
}

// ── Blob / download helpers ───────────────────────────────────────────────────

pub(super) fn trigger_download_blob(blob: &web_sys::Blob, nama: &str) {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return; };
    let Ok(url) = web_sys::Url::create_object_url_with_blob(blob) else {
        web_sys::Url::revoke_object_url("").ok();
        return;
    };
    let Ok(el) = doc.create_element("a") else {
        web_sys::Url::revoke_object_url(&url).ok();
        return;
    };
    let a: web_sys::HtmlAnchorElement = el.unchecked_into();
    a.set_href(&url); a.set_download(nama);
    a.set_attribute("style", "display:none").ok();
    if let Some(body) = doc.body() {
        let _ = body.append_child(&a); a.click(); let _ = body.remove_child(&a);
    }
    web_sys::Url::revoke_object_url(&url).ok();
}

// ── WebP export support detection ─────────────────────────────────────────────

static WEBP_CHECKED: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub(super) fn webp_export_supported() -> bool {
    use std::sync::atomic::Ordering;
    let v = WEBP_CHECKED.load(Ordering::Relaxed);
    if v != 0 { return v == 1; }
    let supported = web_sys::window().and_then(|w| w.document())
        .and_then(|d| d.create_element("canvas").ok())
        .and_then(|c| c.dyn_into::<HtmlCanvasElement>().ok())
        .map(|c| {
            c.set_width(1); c.set_height(1);
            c.to_data_url_with_type("image/webp")
                .map(|s| s.starts_with("data:image/webp"))
                .unwrap_or(false)
        }).unwrap_or(false);
    WEBP_CHECKED.store(if supported { 1 } else { 2 }, Ordering::Relaxed);
    supported
}

pub(super) fn export_mime() -> &'static str {
    if webp_export_supported() { "image/webp" } else { "image/jpeg" }
}

pub(super) fn export_ext() -> &'static str {
    if webp_export_supported() { "webp" } else { "jpg" }
}

// ── canvas_to_blob ────────────────────────────────────────────────────────────

pub(super) async fn canvas_to_blob(canvas: &HtmlCanvasElement) -> Result<web_sys::Blob, String> {
    let mime = export_mime();
    let quality = 0.92_f64;
    let mut res = None; let mut rej = None;
    let p = web_sys::js_sys::Promise::new(&mut |r, e| { res = Some(r); rej = Some(e); });
    let resolve = res.unwrap(); let reject = rej.unwrap();
    let cb = Closure::once(move |blob: JsValue| {
        if blob.is_null() || blob.is_undefined() { let _ = reject.call0(&JsValue::NULL); }
        else { let _ = resolve.call1(&JsValue::NULL, &blob); }
    });
    let to_blob_fn = js_sys::Reflect::get(canvas.as_ref(), &JsValue::from_str("toBlob"))
        .map_err(|_| "canvas.toBlob not found")?;
    let to_blob_fn: js_sys::Function = to_blob_fn.unchecked_into();
    let args = js_sys::Array::new();
    args.push(cb.as_ref().unchecked_ref());
    args.push(&JsValue::from_str(mime));
    args.push(&JsValue::from_f64(quality));
    to_blob_fn.apply(canvas.as_ref(), &args).map_err(|e| format!("toBlob: {:?}", e))?;
    let result = wasm_bindgen_futures::JsFuture::from(p).await;
    drop(cb);
    match result {
        Ok(v) if !v.is_null() && !v.is_undefined() => Ok(v.unchecked_into()),
        _ => {
            let mut res2 = None; let mut rej2 = None;
            let p2 = web_sys::js_sys::Promise::new(&mut |r, e| { res2 = Some(r); rej2 = Some(e); });
            let resolve2 = res2.unwrap(); let reject2 = rej2.unwrap();
            let cb2 = Closure::once(move |blob: JsValue| {
                if blob.is_null() || blob.is_undefined() { let _ = reject2.call0(&JsValue::NULL); }
                else { let _ = resolve2.call1(&JsValue::NULL, &blob); }
            });
            canvas.to_blob_with_type(cb2.as_ref().unchecked_ref(), "image/png")
                .map_err(|e| format!("{:?}", e))?;
            let r2 = wasm_bindgen_futures::JsFuture::from(p2).await;
            drop(cb2);
            r2.map(|v| v.unchecked_into()).map_err(|e| format!("png fallback: {:?}", e))
        }
    }
}

// ── Image/video loading onto canvas ──────────────────────────────────────────

pub(super) async fn load_img_to_canvas(
    src: &str, ctx: &CanvasRenderingContext2d,
    cw: f64, ch: f64, scale: f64, try_dom_reuse: bool, contain: bool,
) -> Result<(), String> {
    let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    if try_dom_reuse {
        if let Some(existing) = doc.query_selector("img.sc-media").ok().flatten()
            .and_then(|el| el.dyn_into::<HtmlImageElement>().ok())
            .filter(|img| img.complete() && img.natural_width() > 0
                && (img.src().starts_with("blob:") || img.cross_origin().map(|co| !co.is_empty()).unwrap_or(false)))
        {
            let iw = existing.natural_width() as f64;
            let ih = existing.natural_height() as f64;
            return if contain { draw_img_contain(ctx, &existing, iw, ih, cw, ch, scale) }
                   else { draw_img_cover(ctx, &existing, iw, ih, cw, ch, scale) };
        }
    }
    let fetch_src = if src.starts_with("blob:") || src.contains("_c=1") { src.to_string() }
        else if src.contains('?') { format!("{}&_c=1", src) }
        else { format!("{}?_c=1", src) };

    let img: HtmlImageElement = doc.create_element("img").map_err(|e| format!("{:?}",e))?.unchecked_into();
    let mut res: Option<web_sys::js_sys::Function> = None;
    let mut rej: Option<web_sys::js_sys::Function> = None;
    let p = web_sys::js_sys::Promise::new(&mut |r, e| { res = Some(r); rej = Some(e); });
    let res = res.unwrap(); let rej = rej.unwrap();
    let on_load  = Closure::once(move || { let _ = res.call0(&JsValue::NULL); });
    let on_error = Closure::once(move || { let _ = rej.call0(&JsValue::NULL); });
    img.set_onload(Some(on_load.as_ref().unchecked_ref()));
    img.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    img.set_cross_origin(Some("anonymous"));
    img.set_src(&fetch_src);
    wasm_bindgen_futures::JsFuture::from(p).await.map_err(|_| "img load gagal".to_string())?;
    img.set_onload(None); img.set_onerror(None);
    drop(on_load); drop(on_error);
    let iw = img.natural_width() as f64; let ih = img.natural_height() as f64;
    if iw > 0.0 && ih > 0.0 {
        if contain { draw_img_contain(ctx, &img, iw, ih, cw, ch, scale) }
        else { draw_img_cover(ctx, &img, iw, ih, cw, ch, scale) }
    } else { Err("gambar kosong".to_string()) }
}

pub(super) async fn capture_video_frame(
    video: &HtmlVideoElement, ctx: &CanvasRenderingContext2d,
    cw: f64, ch: f64, scale: f64,
) -> Result<(), String> {
    let _ = video.pause();
    let win = web_sys::window().ok_or("no window")?;
    let mut res: Option<web_sys::js_sys::Function> = None;
    let p = web_sys::js_sys::Promise::new(&mut |r, _| { res = Some(r); });
    let res = res.unwrap();
    let cl = Closure::once(move || { let _ = res.call0(&JsValue::NULL); });
    let id = win.request_animation_frame(cl.as_ref().unchecked_ref()).map_err(|_| "raf")?;
    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
    win.cancel_animation_frame(id).ok(); drop(cl);
    let vw = video.video_width() as f64; let vh = video.video_height() as f64;
    if vw > 0.0 && vh > 0.0 {
        let (sx, sy, sw, sh) = cover_src_rect(vw, vh, cw, ch);
        let zsw = sw/scale; let zsh = sh/scale;
        ctx.draw_image_with_html_video_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            video, sx+(sw-zsw)/2.0, sy+(sh-zsh)/2.0, zsw, zsh, 0.0, 0.0, cw, ch,
        ).map_err(|e| format!("{:?}", e))
    } else {
        ctx.draw_image_with_html_video_element_and_dw_and_dh(video, 0.0, 0.0, cw, ch)
            .map_err(|e| format!("{:?}", e))
    }
}

// ── Overlay rendering ─────────────────────────────────────────────────────────

pub(super) fn render_overlays_to_canvas(
    ctx: &CanvasRenderingContext2d, overlays: &[StoryOverlay],
    cw: f64, ch: f64, dom_w: f64, _dom_h: f64, _dpr: f64,
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
                let color   = ov.color.as_deref().unwrap_or("#ffffff");
                let fs      = ov.font_size.unwrap_or(28) as f64 * px * sc;
                let style   = ov.text_style.as_deref().unwrap_or("classic");
                let align   = ov.text_align.as_deref().unwrap_or("center");
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

// ── Image compression ─────────────────────────────────────────────────────────

#[allow(dead_code)]
pub(super) async fn compress_image_file(
    file: &web_sys::File, max_px: u32, _quality: f64,
) -> web_sys::Blob {
    if !file.type_().starts_with("image/") { return file.clone().into(); }
    let inner = async move {
        let doc = web_sys::window().and_then(|w| w.document()).ok_or("no doc")?;
        let img: HtmlImageElement = doc.create_element("img").map_err(|_| "create img")?.unchecked_into();
        let obj_url = web_sys::Url::create_object_url_with_blob(file).map_err(|_| "createObjectURL")?;
        let mut ok_fn = None; let mut err_fn = None;
        let p = web_sys::js_sys::Promise::new(&mut |r, e| { ok_fn = Some(r); err_fn = Some(e); });
        let r = ok_fn.unwrap(); let e = err_fn.unwrap();
        let on_load  = Closure::once(move || { let _ = r.call0(&JsValue::NULL); });
        let on_error = Closure::once(move || { let _ = e.call0(&JsValue::NULL); });
        img.set_onload(Some(on_load.as_ref().unchecked_ref()));
        img.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        img.set_src(&obj_url);
        wasm_bindgen_futures::JsFuture::from(p).await.map_err(|_| "load failed")?;
        img.set_onload(None); img.set_onerror(None);
        drop(on_load); drop(on_error);
        web_sys::Url::revoke_object_url(&obj_url).ok();
        let iw = img.natural_width() as f64; let ih = img.natural_height() as f64;
        if iw < 1.0 || ih < 1.0 { return Err("zero size"); }
        let scale = (max_px as f64 / iw.max(ih)).min(1.0);
        let ow = (iw * scale).round() as u32; let oh = (ih * scale).round() as u32;
        let canvas: HtmlCanvasElement = doc.create_element("canvas").map_err(|_| "create canvas")?.unchecked_into();
        canvas.set_width(ow); canvas.set_height(oh);
        let ctx: CanvasRenderingContext2d = canvas.get_context("2d").ok().flatten().ok_or("no ctx")?.unchecked_into();
        ctx.draw_image_with_html_image_element_and_dw_and_dh(&img, 0.0, 0.0, ow as f64, oh as f64)
            .map_err(|_| "drawImage")?;
        let blob = canvas_to_blob(&canvas).await.map_err(|_| "blob failed")?;
        canvas.set_width(0); canvas.set_height(0);
        let base = file.name();
        let stem = base.rsplit_once('.').map(|(s,_)| s).unwrap_or(&base);
        let bits = web_sys::js_sys::Array::of1(&blob);
        let opts = web_sys::FilePropertyBag::new();
        opts.set_type(export_mime());
        let cf = web_sys::File::new_with_blob_sequence_and_options(
            &bits, &format!("{}.{}", stem, export_ext()), &opts,
        ).map_err(|_| "File::new")?;
        Ok::<web_sys::Blob, &'static str>(cf.into())
    };
    match inner.await {
        Ok(b) => b,
        Err(_) => file.clone().into(),
    }
}

// ── Text wrapping helper ──────────────────────────────────────────────────────

fn wrap_text(ctx: &CanvasRenderingContext2d, text: &str, max_w: f64) -> Vec<String> {
    let mut lines = Vec::<String>::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        let w = ctx.measure_text(&candidate).map(|m| m.width()).unwrap_or(0.0);
        if w > max_w && !current.is_empty() {
            lines.push(std::mem::replace(&mut current, word.to_string()));
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() { lines.push(current); }
    if lines.is_empty() { lines.push(String::new()); }
    lines
}

// ── Rounded rectangle path helper ─────────────────────────────────────────────

pub(super) fn round_rect_path(
    ctx: &CanvasRenderingContext2d,
    x: f64, y: f64, w: f64, h: f64, r: f64,
) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    ctx.begin_path();
    let _ = ctx.move_to(x + r, y);
    let _ = ctx.line_to(x + w - r, y);
    let _ = ctx.arc_to(x + w, y,     x + w, y + r,     r);
    let _ = ctx.arc_to(x + w, y + h, x + w - r, y + h, r);
    let _ = ctx.arc_to(x,     y + h, x,     y + h - r, r);
    let _ = ctx.arc_to(x,     y,     x + r, y,          r);
    ctx.close_path();
}

// ── Cover image loader (tries to reuse DOM element first) ─────────────────────

async fn load_cover_img(src: &str) -> Result<HtmlImageElement, String> {
    let doc = web_sys::window()
        .and_then(|w| w.document())
        .ok_or_else(|| "no document".to_string())?;

    // Only reuse the DOM element if it was loaded with crossOrigin="anonymous".
    // An element loaded without CORS would taint the canvas and make toBlob() fail.
    let reused = doc
        .query_selector("img.sc-event-card-cover-img")
        .ok().flatten()
        .and_then(|el| el.dyn_into::<HtmlImageElement>().ok())
        .filter(|img| {
            img.complete()
                && img.natural_width() > 0
                && (img.src().starts_with("blob:")
                    || img.cross_origin().map(|co| !co.is_empty()).unwrap_or(false))
        });
    if let Some(img) = reused { return Ok(img); }

    let img: HtmlImageElement = doc
        .create_element("img").map_err(|e| format!("{e:?}"))?
        .unchecked_into();
    let mut ok_fn = None;
    let mut err_fn = None;
    let p = web_sys::js_sys::Promise::new(&mut |r, e| { ok_fn = Some(r); err_fn = Some(e); });
    let on_load  = Closure::once(move || { let _ = ok_fn.unwrap().call0(&JsValue::NULL); });
    let on_error = Closure::once(move || { let _ = err_fn.unwrap().call0(&JsValue::NULL); });
    img.set_onload(Some(on_load.as_ref().unchecked_ref()));
    img.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    img.set_cross_origin(Some("anonymous"));
    let fetch_src = if src.starts_with("blob:") || src.contains("_c=1") {
        src.to_string()
    } else if src.contains('?') {
        format!("{src}&_c=1")
    } else {
        format!("{src}?_c=1")
    };
    img.set_src(&fetch_src);
    wasm_bindgen_futures::JsFuture::from(p).await
        .map_err(|_| "cover image load failed".to_string())?;
    img.set_onload(None);
    img.set_onerror(None);
    drop(on_load); drop(on_error);
    if img.natural_width() == 0 { return Err("cover image zero width".to_string()); }
    Ok(img)
}

// ── Event card canvas render ──────────────────────────────────────────────────
//
// Renders the full story preview for an event: background, card (cover + body),
// and returns so the caller can draw overlays on top.
//
// Mirrors the DOM layout of `.sc-event-preview-frame > .sc-event-card`.

pub(super) async fn render_event_card_to_canvas(
    ctx: &CanvasRenderingContext2d,
    cw: f64, ch: f64,
    cover_url: &str,
    bg_mode: &str,
    bg_color: &str,
    filter: &str,
    title: &str,
    date: &str,
    venue: &str,
    price: &str,
    is_ticket: bool,
) -> Result<(), String> {
    // ── Load cover image ─────────────────────────────────────────────────────
    let img = load_cover_img(cover_url).await?;
    let iw = img.natural_width() as f64;
    let ih = img.natural_height() as f64;

    // ── Draw background ──────────────────────────────────────────────────────
    match bg_mode {
        "blur" => {
            ctx.set_filter("blur(28px) brightness(0.5)");
            draw_img_cover(ctx, &img, iw, ih, cw, ch, 1.0)?;
            ctx.set_filter("none");
            ctx.set_fill_style_str("rgba(0,0,0,0.45)");
            ctx.fill_rect(0.0, 0.0, cw, ch);
        }
        "solid" => {
            ctx.set_fill_style_str(bg_color);
            ctx.fill_rect(0.0, 0.0, cw, ch);
        }
        key => {
            if let Some((cs, ce)) = gradient_colors(key) {
                if let Ok(g) = ctx.create_linear_gradient(0.0, 0.0, 0.0, ch)
                    .dyn_into::<web_sys::CanvasGradient>()
                {
                    let _ = g.add_color_stop(0.0, cs);
                    let _ = g.add_color_stop(1.0, ce);
                    let _ = ctx.set_fill_style_canvas_gradient(&g);
                    ctx.fill_rect(0.0, 0.0, cw, ch);
                }
            } else {
                ctx.set_fill_style_str("#0d0d18");
                ctx.fill_rect(0.0, 0.0, cw, ch);
            }
        }
    }

    // ── Card layout constants (proportional to cw=1080) ──────────────────────
    let mx          = cw * 0.067;
    let card_x      = mx;
    let card_w      = cw - 2.0 * mx;
    let cover_h     = card_w * 0.5625;     // 16:9 ratio for cover area
    let corner_r    = cw * 0.026;

    let pad         = cw * 0.033;
    let badge_fs    = cw * 0.0195;
    let badge_dot_r = badge_fs * 0.38;
    let badge_h     = badge_fs * 2.5;
    let title_gap   = 14.0;
    let title_fs    = cw * 0.046;
    let sep_gap     = cw * 0.016;
    let meta_lbl_fs = cw * 0.0175;
    let meta_row_fs = cw * 0.024;
    let pill_fs       = cw * 0.025;
    let pill_h        = pill_fs * 2.2;
    let pill_pad_x    = cw * 0.022;
    let ticket_lbl_fs = cw * 0.019;
    let ticket_lbl_h  = ticket_lbl_fs * 2.0;

    // Measure wrapped title (needs correct font set first)
    ctx.set_font(&format!(
        "bold {title_fs}px \"Bebas Neue\", \"Arial Black\", sans-serif"
    ));
    let title_lines  = wrap_text(ctx, title, card_w - 2.0 * pad);
    let title_line_h = title_fs * 1.22;
    let title_h      = title_lines.len() as f64 * title_line_h;

    let meta_row_h   = meta_row_fs * 1.4;
    let body_h = pad + badge_h + title_gap + title_h
               + sep_gap + 2.0 + sep_gap
               + (meta_lbl_fs + 8.0) + 8.0
               + meta_row_h + 20.0
               + pill_h
               + (if is_ticket { 10.0 + ticket_lbl_h } else { 0.0 })
               + pad;
    let card_h = cover_h + body_h;
    let card_y = ((ch - card_h) * 0.42).max(cw * 0.05);

    // ── Card shadow + background ─────────────────────────────────────────────
    ctx.set_shadow_color("rgba(0,0,0,0.65)");
    ctx.set_shadow_blur(cw * 0.055);
    ctx.set_shadow_offset_x(0.0);
    ctx.set_shadow_offset_y(cw * 0.018);
    ctx.set_fill_style_str("#0d0d18");
    round_rect_path(ctx, card_x, card_y, card_w, card_h, corner_r);
    ctx.fill();
    ctx.set_shadow_color("transparent");
    ctx.set_shadow_blur(0.0);
    ctx.set_shadow_offset_y(0.0);

    // ── Cover image (clipped to card top corners) ────────────────────────────
    ctx.save();
    round_rect_path(ctx, card_x, card_y, card_w, cover_h + corner_r, corner_r);
    ctx.clip();

    ctx.set_fill_style_str("#111827");
    ctx.fill_rect(card_x, card_y, card_w, cover_h);

    let fs = css_filter_string(filter);
    if fs != "none" { ctx.set_filter(fs); }

    if iw > 0.0 && ih > 0.0 {
        let s  = (card_w / iw).min(cover_h / ih);
        let dw = iw * s; let dh = ih * s;
        let dx = card_x + (card_w - dw) / 2.0;
        let dy = card_y + (cover_h - dh) / 2.0;
        ctx.draw_image_with_html_image_element_and_dw_and_dh(&img, dx, dy, dw, dh)
            .map_err(|e| format!("{e:?}"))?;
    }
    ctx.set_filter("none");
    ctx.restore();

    // ── Bottom gradient on cover (fade into card body) ───────────────────────
    let fade_start = card_y + cover_h * 0.5;
    let fade_end   = card_y + cover_h;
    if let Ok(g) = ctx.create_linear_gradient(0.0, fade_start, 0.0, fade_end)
        .dyn_into::<web_sys::CanvasGradient>()
    {
        let _ = g.add_color_stop(0.0, "rgba(13,13,24,0.0)");
        let _ = g.add_color_stop(1.0, "rgba(13,13,24,0.95)");
        let _ = ctx.set_fill_style_canvas_gradient(&g);
        ctx.fill_rect(card_x, fade_start, card_w, fade_end - fade_start);
    }

    // ── Card body ────────────────────────────────────────────────────────────
    let body_top = card_y + cover_h;

    // Badge: green dot + "KINETIC EXCLUSIVE" label
    let badge_mid_y = body_top + pad + badge_h * 0.5;
    let dot_cx      = card_x + pad + badge_dot_r;
    ctx.begin_path();
    let _ = ctx.arc(dot_cx, badge_mid_y, badge_dot_r, 0.0, std::f64::consts::TAU);
    ctx.set_fill_style_str("#39ff8a");
    ctx.fill();

    ctx.set_font(&format!(
        "700 {badge_fs}px -apple-system, BlinkMacSystemFont, sans-serif"
    ));
    ctx.set_fill_style_str("rgba(255,255,255,0.65)");
    ctx.set_text_align("left");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(
        "KINETIC EXCLUSIVE",
        dot_cx + badge_dot_r + badge_fs * 0.55,
        badge_mid_y,
    );

    // Title (multi-line, Bebas Neue)
    let title_top = body_top + pad + badge_h + title_gap;
    ctx.set_font(&format!(
        "bold {title_fs}px \"Bebas Neue\", \"Arial Black\", sans-serif"
    ));
    ctx.set_fill_style_str("#ffffff");
    ctx.set_text_baseline("top");
    for (i, line) in title_lines.iter().enumerate() {
        let _ = ctx.fill_text(line, card_x + pad, title_top + i as f64 * title_line_h);
    }

    // Separator line
    let sep_y = title_top + title_h + sep_gap;
    ctx.set_fill_style_str("rgba(255,255,255,0.14)");
    ctx.fill_rect(card_x + pad, sep_y, card_w - 2.0 * pad, 2.0);

    // "DATE & VENUE" eyebrow label
    let meta_lbl_top = sep_y + 2.0 + sep_gap;
    ctx.set_font(&format!(
        "{meta_lbl_fs}px -apple-system, BlinkMacSystemFont, sans-serif"
    ));
    ctx.set_fill_style_str("rgba(255,255,255,0.42)");
    ctx.set_text_baseline("top");
    let _ = ctx.fill_text("DATE & VENUE", card_x + pad, meta_lbl_top);

    // Date (left) + Venue (right)
    let meta_row_top = meta_lbl_top + meta_lbl_fs + 8.0 + 8.0;
    let meta_mid_y   = meta_row_top + meta_row_fs * 0.68;
    ctx.set_font(&format!(
        "600 {meta_row_fs}px -apple-system, BlinkMacSystemFont, sans-serif"
    ));
    ctx.set_fill_style_str("#ffffff");
    ctx.set_text_baseline("middle");
    let display_date = if date.trim().is_empty() { "—" } else { date };
    ctx.set_text_align("left");
    let _ = ctx.fill_text(display_date, card_x + pad, meta_mid_y);
    if !venue.trim().is_empty() {
        ctx.set_text_align("right");
        let _ = ctx.fill_text(venue, card_x + card_w - pad, meta_mid_y);
    }

    // Price pill
    if !price.trim().is_empty() {
        let pill_top = meta_row_top + meta_row_h + 20.0;
        ctx.set_font(&format!(
            "bold {pill_fs}px -apple-system, BlinkMacSystemFont, sans-serif"
        ));
        let pill_tw = ctx.measure_text(price).map(|m| m.width()).unwrap_or(200.0);
        let pill_w  = pill_tw + 2.0 * pill_pad_x;

        ctx.set_fill_style_str("rgba(57,255,138,0.11)");
        round_rect_path(ctx, card_x + pad, pill_top, pill_w, pill_h, pill_h / 2.0);
        ctx.fill();

        ctx.set_stroke_style_str("rgba(57,255,138,0.5)");
        ctx.set_line_width(1.8);
        round_rect_path(ctx, card_x + pad, pill_top, pill_w, pill_h, pill_h / 2.0);
        ctx.stroke();

        ctx.set_fill_style_str("#39ff8a");
        ctx.set_text_align("left");
        ctx.set_text_baseline("middle");
        let _ = ctx.fill_text(price, card_x + pad + pill_pad_x, pill_top + pill_h * 0.5);

        // Ticket success label
        if is_ticket {
            let lbl_top = pill_top + pill_h + 10.0;
            ctx.set_font(&format!(
                "500 {ticket_lbl_fs}px -apple-system, BlinkMacSystemFont, sans-serif"
            ));
            ctx.set_fill_style_str("rgba(255,255,255,0.55)");
            ctx.set_text_baseline("middle");
            let _ = ctx.fill_text(
                "✓ Tiket berhasil dibeli",
                card_x + pad,
                lbl_top + ticket_lbl_h * 0.5,
            );
        }
    }

    Ok(())
}

// ── BgExport enum (used in page.rs and canvas export) ─────────────────────────

#[derive(Clone)]
pub(super) enum BgExport {
    Solid(String),
    Gradient { color_start: &'static str, color_end: &'static str },
}

// ── Full story canvas export ──────────────────────────────────────────────────

pub(super) fn export_story_canvas(
    pratinjau_url: String, is_vid: bool, overlays: Vec<StoryOverlay>,
    filter: String, nama_file: String, scale: f64,
    bg_info: Option<BgExport>, guard_sig: RwSignal<bool>,
) {
    spawn_local(async move {
        guard_sig.set(true);
        if let Err(e) = preload_fonts().await {
            web_sys::console::warn_1(&format!("font: {:?}", e).into());
        }
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else { guard_sig.set(false); return; };
        let (dom_w, dom_h) = doc.query_selector(".sc-canvas-frame").ok().flatten()
            .and_then(|el| el.dyn_into::<web_sys::Element>().ok())
            .map(|el| { let r = el.get_bounding_client_rect(); (r.width(), r.height()) })
            .filter(|(w,h)| *w > 1.0 && *h > 1.0).unwrap_or((390.0, 844.0));
        let dpr = get_dpr();
        let Some((canvas, ctx, cw, ch)) = create_export_canvas(dpr) else { guard_sig.set(false); return; };
        if let Some(ref bg) = bg_info {
            match bg {
                BgExport::Solid(c) => { ctx.set_fill_style_str(c); ctx.fill_rect(0.0,0.0,cw,ch); }
                BgExport::Gradient { color_start, color_end } => {
                    if let Ok(g) = ctx.create_linear_gradient(0.0,0.0,0.0,ch).dyn_into::<web_sys::CanvasGradient>() {
                        let _ = g.add_color_stop(0.0, color_start);
                        let _ = g.add_color_stop(1.0, color_end);
                        let _ = ctx.set_fill_style_canvas_gradient(&g);
                        ctx.fill_rect(0.0,0.0,cw,ch);
                    }
                }
            }
        }
        let fs = css_filter_string(&filter);
        if fs != "none" { ctx.set_filter(fs); }
        let result = if is_vid {
            match doc.query_selector("video.sc-media").ok().flatten()
                .and_then(|el| el.dyn_into::<HtmlVideoElement>().ok())
            {
                Some(v) => capture_video_frame(&v, &ctx, cw, ch, scale).await,
                None    => Err("video.sc-media tidak ditemukan".into()),
            }
        } else {
            load_img_to_canvas(&pratinjau_url, &ctx, cw, ch, scale, true, true).await
        };
        if let Err(e) = result {
            if let Some(win) = web_sys::window() { let _ = win.alert_with_message(&format!("Export gagal: {}", e)); }
            guard_sig.set(false); return;
        }
        ctx.set_filter("none");
        render_overlays_to_canvas(&ctx, &overlays, cw, ch, dom_w, dom_h, dpr);
        match canvas_to_blob(&canvas).await {
            Ok(blob) => {
                let stem = nama_file.rsplit_once('.').map(|(n,_)| n.to_string()).unwrap_or(nama_file);
                let ts = web_sys::js_sys::Date::now() as u64;
                trigger_download_blob(&blob, &format!("{}_story_{}.{}", stem, ts, export_ext()));
            }
            Err(e) => web_sys::console::error_1(&format!("to_blob: {}", e).into()),
        }
        ctx.clear_rect(0.0, 0.0, cw * dpr, ch * dpr);
        canvas.set_width(0); canvas.set_height(0);
        guard_sig.set(false);
    });
}

// ─── Kartu MERCHANT / REVIEW untuk story ─────────────────────────────────────
//
// Bingkai KHUSUS profil toko (bukan bingkai event): meniru halaman /m/{id} —
// header image di atas (fade ke gelap), avatar logo bulat overlap dengan badge
// centang lime, nama toko, lalu baris statistik FOLLOWERS · EVENTS · RATING
// dalam kartu rounded (mode share-toko) ATAU blok bintang+kutipan (mode
// share-ulasan). Background story = blur header (fallback logo → gradient).
#[allow(clippy::too_many_arguments)]
pub(super) async fn render_merchant_card_to_canvas(
    ctx: &CanvasRenderingContext2d,
    cw: f64,
    ch: f64,
    logo_url: &str,
    header_url: &str,
    bg_mode: &str,
    bg_color: &str,
    name: &str,
    verified: bool,
    followers: i64,
    events_count: i64,
    rating: f64,
    review: Option<(u8, String)>, // (rating 1..=5, komentar) — mode ulasan
) -> Result<(), String> {
    let logo_img = if logo_url.is_empty() {
        None
    } else {
        load_cover_img(logo_url).await.ok()
    };
    let header_img = if header_url.is_empty() {
        None
    } else {
        load_cover_img(header_url).await.ok()
    };

    // ── Background story: blur header → blur logo → solid/gradient ──────────
    let bg_img = header_img.as_ref().or(logo_img.as_ref());
    match (bg_mode, bg_img) {
        ("blur", Some(img)) => {
            let (iw, ih) = (img.natural_width() as f64, img.natural_height() as f64);
            ctx.set_filter("blur(30px) brightness(0.4)");
            draw_img_cover(ctx, img, iw, ih, cw, ch, 1.0)?;
            ctx.set_filter("none");
        }
        ("solid", _) => {
            ctx.set_fill_style_str(bg_color);
            ctx.fill_rect(0.0, 0.0, cw, ch);
        }
        (key, _) => {
            if let Some((cs, ce)) = gradient_colors(key) {
                if let Ok(g) = ctx
                    .create_linear_gradient(0.0, 0.0, 0.0, ch)
                    .dyn_into::<web_sys::CanvasGradient>()
                {
                    let _ = g.add_color_stop(0.0, cs);
                    let _ = g.add_color_stop(1.0, ce);
                    let _ = ctx.set_fill_style_canvas_gradient(&g);
                    ctx.fill_rect(0.0, 0.0, cw, ch);
                }
            } else {
                ctx.set_fill_style_str("#0d0d18");
                ctx.fill_rect(0.0, 0.0, cw, ch);
            }
        }
    }

    // ── Layout (proporsional cw=1080) ────────────────────────────────────────
    let mx = cw * 0.07;
    let card_x = mx;
    let card_w = cw - 2.0 * mx;
    let corner_r = cw * 0.03;
    let pad = cw * 0.045;
    let hero_h = card_w * 0.52; // area header image
    let avatar_r = cw * 0.075;
    let name_fs = cw * 0.048;
    let stat_num_fs = cw * 0.042;
    let stat_lbl_fs = cw * 0.017;
    let star_fs = cw * 0.05;
    let quote_fs = cw * 0.028;
    let pill_fs = cw * 0.024;
    let pill_h = pill_fs * 2.3;

    // Blok bawah: stats (share toko) ATAU bintang+kutipan (share ulasan).
    let quote_lines: Vec<String> = match &review {
        Some((_, comment)) if !comment.trim().is_empty() => {
            ctx.set_font(&format!("italic {quote_fs}px \"Space Mono\", monospace"));
            let quoted = format!("\u{201C}{}\u{201D}", comment.trim());
            let mut ls = wrap_text(ctx, &quoted, card_w - 2.0 * pad);
            ls.truncate(4);
            ls
        }
        _ => Vec::new(),
    };
    let quote_line_h = quote_fs * 1.5;
    let stats_h = stat_num_fs + 10.0 + stat_lbl_fs + pad * 1.1;
    let bottom_h = if review.is_some() {
        star_fs * 1.2 + 14.0 + quote_lines.len() as f64 * quote_line_h + 12.0
    } else {
        stats_h
    };
    // Tinggi body di bawah hero: ruang avatar overlap + nama + blok bawah + CTA.
    let body_h = avatar_r          // bagian avatar yang menjorok ke body
        + 18.0
        + name_fs * 1.2
        + 22.0
        + bottom_h
        + 20.0
        + pill_h
        + pad;
    let card_h = hero_h + body_h;
    let card_y = ((ch - card_h) * 0.40).max(cw * 0.05);

    // ── Kartu dasar ──────────────────────────────────────────────────────────
    ctx.set_shadow_color("rgba(0,0,0,0.65)");
    ctx.set_shadow_blur(cw * 0.055);
    ctx.set_shadow_offset_y(cw * 0.018);
    ctx.set_fill_style_str("#0d0d18");
    round_rect_path(ctx, card_x, card_y, card_w, card_h, corner_r);
    ctx.fill();
    ctx.set_shadow_color("transparent");
    ctx.set_shadow_blur(0.0);
    ctx.set_shadow_offset_y(0.0);

    // ── Header image (clip sudut atas) + fade ke gelap ───────────────────────
    ctx.save();
    round_rect_path(ctx, card_x, card_y, card_w, card_h, corner_r);
    ctx.clip();
    // Hero pakai header; bila kosong FALLBACK ke logo — samakan dgn pratinjau
    // (.sc-mch-hero yang jatuh ke logo saat header kosong). Gradient hanya bila
    // header DAN logo dua-duanya tak ada.
    let hero_img = header_img.as_ref().or(logo_img.as_ref());
    if let Some(img) = hero_img {
        let (iw, ih) = (img.natural_width() as f64, img.natural_height() as f64);
        // cover ke area hero
        let f = cover_factor(iw, ih, card_w, hero_h);
        let (dw, dh) = (iw * f, ih * f);
        let _ = ctx.draw_image_with_html_image_element_and_dw_and_dh(
            img,
            card_x + (card_w - dw) / 2.0,
            card_y + (hero_h - dh) / 2.0,
            dw,
            dh,
        );
    } else if let Ok(g) = ctx
        .create_linear_gradient(card_x, card_y, card_x + card_w, card_y + hero_h)
        .dyn_into::<web_sys::CanvasGradient>()
    {
        let _ = g.add_color_stop(0.0, "#1c2340");
        let _ = g.add_color_stop(1.0, "#3b2a63");
        let _ = ctx.set_fill_style_canvas_gradient(&g);
        ctx.fill_rect(card_x, card_y, card_w, hero_h);
    }
    // Fade bawah hero → menyatu dengan body gelap (seperti halaman profil).
    if let Ok(g) = ctx
        .create_linear_gradient(0.0, card_y + hero_h * 0.55, 0.0, card_y + hero_h)
        .dyn_into::<web_sys::CanvasGradient>()
    {
        let _ = g.add_color_stop(0.0, "rgba(13,13,24,0)");
        let _ = g.add_color_stop(1.0, "rgba(13,13,24,1)");
        let _ = ctx.set_fill_style_canvas_gradient(&g);
        ctx.fill_rect(card_x, card_y + hero_h * 0.5, card_w, hero_h * 0.5 + 2.0);
    }
    ctx.restore();

    // ── Avatar logo bulat overlap (kiri) + badge centang lime ───────────────
    let av_cx = card_x + pad + avatar_r;
    let av_cy = card_y + hero_h - avatar_r * 0.35;
    ctx.save();
    ctx.begin_path();
    let _ = ctx.arc(av_cx, av_cy, avatar_r, 0.0, std::f64::consts::TAU);
    ctx.clip();
    if let Some(img) = &logo_img {
        let (iw, ih) = (img.natural_width() as f64, img.natural_height() as f64);
        let f = cover_factor(iw, ih, avatar_r * 2.0, avatar_r * 2.0);
        let (dw, dh) = (iw * f, ih * f);
        let _ = ctx.draw_image_with_html_image_element_and_dw_and_dh(
            img,
            av_cx - dw / 2.0,
            av_cy - dh / 2.0,
            dw,
            dh,
        );
    } else {
        if let Ok(g) = ctx
            .create_linear_gradient(
                av_cx - avatar_r,
                av_cy - avatar_r,
                av_cx + avatar_r,
                av_cy + avatar_r,
            )
            .dyn_into::<web_sys::CanvasGradient>()
        {
            let _ = g.add_color_stop(0.0, "#4f7cff");
            let _ = g.add_color_stop(1.0, "#8b5cf6");
            let _ = ctx.set_fill_style_canvas_gradient(&g);
            ctx.fill_rect(av_cx - avatar_r, av_cy - avatar_r, avatar_r * 2.0, avatar_r * 2.0);
        }
        ctx.set_fill_style_str("#ffffff");
        ctx.set_font(&format!(
            "bold {}px \"Bebas Neue\", \"Arial Black\", sans-serif",
            avatar_r
        ));
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        let initial: String = name.chars().next().unwrap_or('P').to_uppercase().collect();
        let _ = ctx.fill_text(&initial, av_cx, av_cy + avatar_r * 0.06);
    }
    ctx.restore();
    // Ring avatar.
    ctx.begin_path();
    let _ = ctx.arc(av_cx, av_cy, avatar_r + 2.0, 0.0, std::f64::consts::TAU);
    ctx.set_stroke_style_str("#0d0d18");
    ctx.set_line_width(6.0);
    ctx.stroke();
    // Badge centang lime di kanan-bawah avatar (seperti halaman profil).
    if verified {
        let b_r = avatar_r * 0.30;
        let b_cx = av_cx + avatar_r * 0.72;
        let b_cy = av_cy + avatar_r * 0.72;
        ctx.begin_path();
        let _ = ctx.arc(b_cx, b_cy, b_r, 0.0, std::f64::consts::TAU);
        ctx.set_fill_style_str("#c8f04a");
        ctx.fill();
        ctx.set_fill_style_str("#101010");
        ctx.set_font(&format!("bold {}px sans-serif", b_r * 1.2));
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        let _ = ctx.fill_text("\u{2713}", b_cx, b_cy + 1.0);
    }

    // ── Nama toko (kiri, uppercase display) ──────────────────────────────────
    let mut y = av_cy + avatar_r + 18.0 + name_fs;
    ctx.set_text_align("left");
    ctx.set_text_baseline("alphabetic");
    ctx.set_fill_style_str("#ffffff");
    ctx.set_font(&format!(
        "bold {name_fs}px \"Bebas Neue\", \"Arial Black\", sans-serif"
    ));
    let _ = ctx.fill_text(&name.to_uppercase(), card_x + pad, y);

    // ── Blok bawah: stats ATAU review ────────────────────────────────────────
    y += 22.0;
    if let Some((r_stars, _)) = &review {
        // Bintang + kutipan (tengah).
        ctx.set_text_align("center");
        let cx = card_x + card_w / 2.0;
        y += star_fs;
        let stars: String = (1..=5)
            .map(|i| if i <= *r_stars { '\u{2605}' } else { '\u{2606}' })
            .collect();
        ctx.set_fill_style_str("#f5c518");
        ctx.set_font(&format!("{star_fs}px sans-serif"));
        let _ = ctx.fill_text(&stars, cx, y);
        y += 14.0;
        ctx.set_fill_style_str("rgba(255,255,255,0.85)");
        ctx.set_font(&format!("italic {quote_fs}px \"Space Mono\", monospace"));
        for line in &quote_lines {
            y += quote_line_h;
            let _ = ctx.fill_text(line, cx, y);
        }
        y += 12.0;
    } else {
        // Kartu stats rounded (lebih terang) dengan 3 kolom + divider.
        let s_x = card_x + pad;
        let s_w = card_w - 2.0 * pad;
        let s_h = stats_h;
        ctx.set_fill_style_str("#1b1b2a");
        round_rect_path(ctx, s_x, y, s_w, s_h, cw * 0.022);
        ctx.fill();
        let col_w = s_w / 3.0;
        let num_y = y + pad * 0.55 + stat_num_fs;
        let lbl_y = num_y + 10.0 + stat_lbl_fs;
        let vals = [
            (crate::web::pages::merchant_public::fmt_count(followers), "FOLLOWERS", false),
            (crate::web::pages::merchant_public::fmt_count(events_count), "EVENTS", false),
            (format!("{rating:.1}"), "RATING", true),
        ];
        ctx.set_text_align("center");
        for (i, (num, lbl, is_rating)) in vals.iter().enumerate() {
            let cx_col = s_x + col_w * (i as f64 + 0.5);
            ctx.set_fill_style_str("#ffffff");
            ctx.set_font(&format!("bold {stat_num_fs}px \"Space Mono\", monospace"));
            if *is_rating {
                // "4.8 ★" — bintang kecil lime setelah angka.
                let num_w = ctx.measure_text(num).map(|m| m.width()).unwrap_or(0.0);
                let _ = ctx.fill_text(num, cx_col - stat_num_fs * 0.3, num_y);
                ctx.set_fill_style_str("#c8f04a");
                ctx.set_font(&format!("{}px sans-serif", stat_num_fs * 0.65));
                let _ = ctx.fill_text(
                    "\u{2605}",
                    cx_col - stat_num_fs * 0.3 + num_w / 2.0 + stat_num_fs * 0.42,
                    num_y - stat_num_fs * 0.08,
                );
            } else {
                let _ = ctx.fill_text(num, cx_col, num_y);
            }
            ctx.set_fill_style_str("rgba(255,255,255,0.45)");
            ctx.set_font(&format!("{stat_lbl_fs}px \"Space Mono\", monospace"));
            // Letter-spacing manual: sisip spasi tipis antar huruf label.
            let spaced: String = lbl.chars().flat_map(|c| [c, '\u{2009}']).collect();
            let _ = ctx.fill_text(spaced.trim_end(), cx_col, lbl_y);
            // Divider antar kolom.
            if i > 0 {
                ctx.set_stroke_style_str("rgba(255,255,255,0.10)");
                ctx.set_line_width(2.0);
                ctx.begin_path();
                ctx.move_to(s_x + col_w * i as f64, y + s_h * 0.22);
                ctx.line_to(s_x + col_w * i as f64, y + s_h * 0.78);
                ctx.stroke();
            }
        }
        y += s_h;
    }

    // ── Pill CTA ─────────────────────────────────────────────────────────────
    let pill_text = "KUNJUNGI PROFIL \u{2197}";
    ctx.set_font(&format!("bold {pill_fs}px \"Space Mono\", monospace"));
    let tw = ctx.measure_text(pill_text).map(|m| m.width()).unwrap_or(200.0);
    let pill_w = tw + pill_fs * 2.4;
    let cx = card_x + card_w / 2.0;
    let pill_x = cx - pill_w / 2.0;
    let pill_y = y + 20.0;
    ctx.set_fill_style_str("#c8f04a");
    round_rect_path(ctx, pill_x, pill_y, pill_w, pill_h, pill_h / 2.0);
    ctx.fill();
    ctx.set_fill_style_str("#101010");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(pill_text, cx, pill_y + pill_h / 2.0 + 1.0);
    ctx.set_text_baseline("alphabetic");

    Ok(())
}
