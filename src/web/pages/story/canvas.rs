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
