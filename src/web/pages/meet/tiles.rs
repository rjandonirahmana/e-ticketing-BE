//! meet/tiles.rs — Tile video peserta (dikelola imperatif via DOM) + ikon SVG.
//!
//! Tile remote dibuat manual lewat `document.create_element` karena menautkan
//! `MediaStream` dinamis ke elemen `<video>` jauh lebih andal daripada lewat
//! reaktivitas Leptos. Status mic/kamera ditoggle lewat class CSS.

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

pub(super) const MIC_OFF_SVG: &str = r#"<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><line x1="1" y1="1" x2="23" y2="23"/><path d="M9 9v3a3 3 0 005.12 2.12M15 9.34V4a3 3 0 00-5.94-.6"/><path d="M17 16.95A7 7 0 015 12v-2m14 0v2a7 7 0 01-.11 1.23"/><line x1="12" y1="19" x2="12" y2="23"/></svg>"#;
pub(super) const MIC_ON_SVG: &str = r#"<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 1a3 3 0 00-3 3v8a3 3 0 006 0V4a3 3 0 00-3-3z"/><path d="M19 10v2a7 7 0 01-14 0v-2"/><line x1="12" y1="19" x2="12" y2="23"/></svg>"#;
pub(super) const CAM_ON_SVG: &str = r#"<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>"#;
pub(super) const CAM_OFF_SVG: &str = r#"<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="1" y1="1" x2="23" y2="23"/><path d="M16 16H3a2 2 0 01-2-2V6m5-1h9a2 2 0 012 2v3l5-3v9"/></svg>"#;

/// Inisial nama (huruf pertama, kapital) untuk avatar fallback saat kamera off.
pub(super) fn initial_of(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// Tautkan stream remote ke tile video (buat tile lengkap bila belum ada:
/// video + avatar fallback + label nama + ikon mic-off).
pub(super) fn attach_remote_tile(
    tiles: NodeRef<leptos::html::Div>,
    peer_id: &str,
    name: &str,
    stream: &web_sys::MediaStream,
) {
    let Some(container) = tiles.get_untracked() else {
        return;
    };
    let document = match web_sys::window().and_then(|w| w.document()) {
        Some(d) => d,
        None => return,
    };
    let video_id = format!("meet-video-{peer_id}");
    if let Some(el) = document.get_element_by_id(&video_id) {
        let video: web_sys::HtmlVideoElement = el.unchecked_into();
        video.set_src_object(Some(stream));
        return;
    }

    let create = |tag: &str| document.create_element(tag).ok();
    let (Some(wrap), Some(video), Some(avatar), Some(av_txt), Some(bar), Some(mic), Some(label)) = (
        create("div"),
        create("video"),
        create("div"),
        create("span"),
        create("div"),
        create("span"),
        create("span"),
    ) else {
        return;
    };

    let _ = wrap.set_attribute("id", &format!("meet-tile-{peer_id}"));
    let _ = wrap.set_attribute("class", "meet-tile");

    let video: web_sys::HtmlVideoElement = video.unchecked_into();
    let _ = video.set_attribute("id", &video_id);
    let _ = video.set_attribute("class", "meet-tile-video");
    video.set_autoplay(true);
    let _ = video.set_attribute("playsinline", "true");
    video.set_src_object(Some(stream));

    let _ = avatar.set_attribute("class", "meet-tile-avatar");
    av_txt.set_text_content(Some(&initial_of(name)));
    let _ = avatar.append_child(&av_txt);

    let _ = bar.set_attribute("class", "meet-tile-bar");
    let _ = mic.set_attribute("class", "meet-tile-mic");
    mic.set_inner_html(MIC_OFF_SVG);
    let _ = label.set_attribute("class", "meet-tile-name");
    label.set_text_content(Some(name));
    let _ = bar.append_child(&mic);
    let _ = bar.append_child(&label);

    let _ = wrap.append_child(&video);
    let _ = wrap.append_child(&avatar);
    let _ = wrap.append_child(&bar);
    let _ = container.append_child(&wrap);
}

/// Terapkan status mic/kamera ke tile remote (toggle class).
pub(super) fn set_tile_state(peer_id: &str, mic: bool, cam: bool) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(el) = doc.get_element_by_id(&format!("meet-tile-{peer_id}")) {
            let cl = el.class_list();
            let _ = cl.toggle_with_force("cam-off", !cam);
            let _ = cl.toggle_with_force("mic-off", !mic);
        }
    }
}

pub(super) fn remove_tile(peer_id: &str) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(el) = doc.get_element_by_id(&format!("meet-tile-{peer_id}")) {
            el.remove();
        }
    }
}
