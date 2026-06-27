//! web/rtc.rs — Sumber tunggal daftar ICE server untuk semua WebRTC client
//! (live publisher, live viewer, dan meet mesh).
//!
//! Server mengekspos daftar ICE (STUN + TURN dari env) di `GET /api/rtc/ice`.
//! Browser tidak bisa membaca env server, jadi creds TURN diambil dari endpoint
//! ini saat akan membuat `RtcPeerConnection`. Bila fetch gagal, fallback ke
//! STUN publik (cukup untuk LAN/demo). Lihat `TURN_*` di `.env.example`.

use wasm_bindgen::prelude::*;

/// Daftar STUN-only sebagai fallback (dipakai bila endpoint gagal/ kosong).
pub fn ice_fallback() -> js_sys::Array {
    let urls = js_sys::Array::new();
    urls.push(&JsValue::from_str("stun:stun.l.google.com:19302"));
    urls.push(&JsValue::from_str("stun:stun1.l.google.com:19302"));
    let server = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&server, &JsValue::from_str("urls"), &urls);
    let servers = js_sys::Array::new();
    servers.push(&server);
    servers
}

/// Bangun `js_sys::Array` ICE server dari payload JSON server
/// (`{ "data": [ { "urls": [...], "username"?, "credential"? }, ... ] }`).
fn build_from_json(v: &serde_json::Value) -> Option<js_sys::Array> {
    let list = v.get("data")?.as_array()?;
    if list.is_empty() {
        return None;
    }
    let servers = js_sys::Array::new();
    for entry in list {
        let urls_json = entry.get("urls").and_then(|u| u.as_array())?;
        let urls = js_sys::Array::new();
        for u in urls_json {
            if let Some(s) = u.as_str() {
                urls.push(&JsValue::from_str(s));
            }
        }
        let obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("urls"), &urls);
        if let Some(user) = entry.get("username").and_then(|x| x.as_str()) {
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("username"), &JsValue::from_str(user));
        }
        if let Some(cred) = entry.get("credential").and_then(|x| x.as_str()) {
            let _ =
                js_sys::Reflect::set(&obj, &JsValue::from_str("credential"), &JsValue::from_str(cred));
        }
        servers.push(&obj);
    }
    Some(servers)
}

/// Ambil daftar ICE server (STUN + TURN) dari server; fallback STUN bila gagal.
pub async fn fetch_ice_servers() -> js_sys::Array {
    let resp = match gloo_net::http::Request::get("/api/rtc/ice").send().await {
        Ok(r) => r,
        Err(_) => return ice_fallback(),
    };
    match resp.json::<serde_json::Value>().await {
        Ok(v) => build_from_json(&v).unwrap_or_else(ice_fallback),
        Err(_) => ice_fallback(),
    }
}
