//! meet/webrtc.rs — Mesh WebRTC per-peer: bikin RtcPeerConnection, offer/answer,
//! dan ICE candidate. Semua signaling diteruskan lewat `Ctx::ws_send`.

use serde_json::json;
use wasm_bindgen::prelude::*;

use super::tiles::{attach_remote_tile, set_tile_state};
use super::Ctx;

pub(super) fn new_peer_connection(ctx: &Ctx, peer_id: &str) -> Option<web_sys::RtcPeerConnection> {
    if let Some(pc) = ctx.pcs.borrow().get(peer_id) {
        return Some(pc.clone());
    }
    let config = web_sys::RtcConfiguration::new();
    let servers = ctx
        .ice
        .borrow()
        .clone()
        .unwrap_or_else(crate::web::rtc::ice_fallback);
    config.set_ice_servers(servers.as_ref());
    let pc = web_sys::RtcPeerConnection::new_with_configuration(&config).ok()?;

    // Lampirkan track lokal (kamera + mic) → mesh dua arah.
    if let Some(stream) = ctx.local.borrow().as_ref() {
        let tracks = stream.get_tracks();
        for i in 0..tracks.length() {
            let track: web_sys::MediaStreamTrack = tracks.get(i).unchecked_into();
            let _ = pc.add_track(&track, stream, &js_sys::Array::new());
        }
    }

    // Kirim kandidat ICE lokal ke peer lewat relay server.
    {
        let ctx2 = ctx.clone();
        let pid = peer_id.to_string();
        let cb = Closure::<dyn FnMut(web_sys::RtcPeerConnectionIceEvent)>::new(
            move |e: web_sys::RtcPeerConnectionIceEvent| {
                if let Some(c) = e.candidate() {
                    ctx2.ws_send(json!({
                        "type": "signal",
                        "to": pid,
                        "data": {
                            "candidate": c.candidate(),
                            "sdp_mid": c.sdp_mid(),
                            "sdp_mline_index": c.sdp_m_line_index(),
                        }
                    }));
                }
            },
        );
        pc.set_onicecandidate(Some(cb.as_ref().unchecked_ref()));
        cb.forget();
    }

    // Terima media remote → pasang ke tile, lalu terapkan status terakhir yang
    // mungkin sudah diterima sebelum media tiba.
    {
        let tiles = ctx.tiles;
        let pid = peer_id.to_string();
        let nm = ctx.name_of(peer_id);
        let states = ctx.states.clone();
        let cb = Closure::<dyn FnMut(web_sys::RtcTrackEvent)>::new(move |e: web_sys::RtcTrackEvent| {
            let streams = e.streams();
            if streams.length() > 0 {
                let ms: web_sys::MediaStream = streams.get(0).unchecked_into();
                attach_remote_tile(tiles, &pid, &nm, &ms);
                if let Some((mic, cam)) = states.borrow().get(&pid).copied() {
                    set_tile_state(&pid, mic, cam);
                }
            }
        });
        pc.set_ontrack(Some(cb.as_ref().unchecked_ref()));
        cb.forget();
    }

    ctx.pcs.borrow_mut().insert(peer_id.to_string(), pc.clone());
    Some(pc)
}

fn reflect_sdp(obj: &JsValue) -> String {
    js_sys::Reflect::get(obj, &JsValue::from_str("sdp"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

pub(super) async fn make_offer(ctx: &Ctx, peer_id: &str) {
    let Some(pc) = new_peer_connection(ctx, peer_id) else {
        return;
    };
    let Ok(offer) = wasm_bindgen_futures::JsFuture::from(pc.create_offer()).await else {
        return;
    };
    let sdp = reflect_sdp(&offer);
    let desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Offer);
    desc.set_sdp(&sdp);
    if wasm_bindgen_futures::JsFuture::from(pc.set_local_description(&desc))
        .await
        .is_err()
    {
        return;
    }
    ctx.ws_send(json!({
        "type": "signal",
        "to": peer_id,
        "data": { "sdp_type": "offer", "sdp": sdp }
    }));
}

pub(super) async fn handle_offer(ctx: &Ctx, from: &str, sdp: &str) {
    let Some(pc) = new_peer_connection(ctx, from) else {
        return;
    };
    let desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Offer);
    desc.set_sdp(sdp);
    if wasm_bindgen_futures::JsFuture::from(pc.set_remote_description(&desc))
        .await
        .is_err()
    {
        return;
    }
    let Ok(answer) = wasm_bindgen_futures::JsFuture::from(pc.create_answer()).await else {
        return;
    };
    let asdp = reflect_sdp(&answer);
    let adesc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Answer);
    adesc.set_sdp(&asdp);
    if wasm_bindgen_futures::JsFuture::from(pc.set_local_description(&adesc))
        .await
        .is_err()
    {
        return;
    }
    ctx.ws_send(json!({
        "type": "signal",
        "to": from,
        "data": { "sdp_type": "answer", "sdp": asdp }
    }));
}

pub(super) async fn handle_answer(ctx: &Ctx, from: &str, sdp: &str) {
    let pc = ctx.pcs.borrow().get(from).cloned();
    if let Some(pc) = pc {
        let desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Answer);
        desc.set_sdp(sdp);
        let _ = wasm_bindgen_futures::JsFuture::from(pc.set_remote_description(&desc)).await;
    }
}

pub(super) async fn handle_candidate(ctx: &Ctx, from: &str, data: &serde_json::Value) {
    let pc = ctx.pcs.borrow().get(from).cloned();
    let Some(pc) = pc else { return };
    let cand = data.get("candidate").and_then(|c| c.as_str()).unwrap_or("");
    if cand.is_empty() {
        return;
    }
    let init = web_sys::RtcIceCandidateInit::new(cand);
    if let Some(mid) = data.get("sdp_mid").and_then(|m| m.as_str()) {
        init.set_sdp_mid(Some(mid));
    }
    if let Some(idx) = data.get("sdp_mline_index").and_then(|i| i.as_u64()) {
        init.set_sdp_m_line_index(Some(idx as u16));
    }
    let _ =
        wasm_bindgen_futures::JsFuture::from(pc.add_ice_candidate_with_opt_rtc_ice_candidate_init(
            Some(&init),
        ))
        .await;
}
