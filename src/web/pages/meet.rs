//! web/pages/meet.rs — Halaman "Zoom Meet" (konferensi video P2P mesh).
//!
//! Dua peran dalam satu halaman, ditentukan dari rute:
//! - `/meet/host` → HOST (merchant): buat room, lihat waiting room,
//!   izinkan/tolak tamu, bagikan link undangan.
//! - `/meet/{id}` → TAMU: minta masuk, tunggu izin host, lalu mesh.
//!
//! Server hanya relay signaling + kontrol admit (lihat `src/meet/`). Media
//! mengalir langsung antar-browser (mesh). Anti-glare: peer yang BARU di-admit
//! yang meng-initiate offer ke peer yang sudah ada.
//!
//! Catatan WASM: elemen `<video>` tiap peserta dikelola secara imperatif lewat
//! DOM (bukan reaktif Leptos) karena menautkan `MediaStream` ke elemen video
//! yang dibuat dinamis jauh lebih andal begitu.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use serde::Deserialize;
use serde_json::json;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

use crate::web::app::AuthResource;
use crate::web::components::GridBackground;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Loading,
    Lobby,
    Waiting,
    InMeet,
    Denied,
    Ended,
    Error,
}

#[derive(Debug, Deserialize)]
struct CreateRoomData {
    room_id: String,
}

fn parse_api_data<T: for<'de> Deserialize<'de>>(json: &serde_json::Value) -> Result<T, String> {
    if let Some(e) = json.get("error").and_then(|e| e.as_str()) {
        return Err(e.to_string());
    }
    let data = json.get("data").ok_or("Respons server tidak valid")?;
    serde_json::from_value(data.clone()).map_err(|e| e.to_string())
}

fn build_ws_url(path: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            let loc = win.location();
            let proto = if loc.protocol().unwrap_or_default() == "https:" {
                "wss"
            } else {
                "ws"
            };
            let host = loc.host().unwrap_or_default();
            return format!("{proto}://{host}{path}");
        }
    }
    format!("ws://localhost{path}")
}

fn origin() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            return win.location().origin().unwrap_or_default();
        }
    }
    String::new()
}

async fn get_user_media() -> Result<web_sys::MediaStream, String> {
    let window = web_sys::window().ok_or("No window")?;
    let constraints = web_sys::MediaStreamConstraints::new();
    constraints.set_audio(&JsValue::TRUE);
    constraints.set_video(&JsValue::TRUE);
    let promise = window
        .navigator()
        .media_devices()
        .map_err(|_| "MediaDevices tidak didukung".to_string())?
        .get_user_media_with_constraints(&constraints)
        .map_err(|_| "getUserMedia gagal".to_string())?;
    let val = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("Akses kamera/mic ditolak: {e:?}"))?;
    Ok(web_sys::MediaStream::from(val))
}

async fn api_create_room() -> Result<String, String> {
    let resp = gloo_net::http::Request::post("/api/meet/rooms")
        .json(&json!({}))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let data: CreateRoomData = parse_api_data(&v)?;
    Ok(data.room_id)
}

/// State bersama yang dibawa ke dalam closure WS & fungsi signaling.
#[derive(Clone)]
struct Ctx {
    ws: Rc<RefCell<Option<web_sys::WebSocket>>>,
    pcs: Rc<RefCell<HashMap<String, web_sys::RtcPeerConnection>>>,
    names: Rc<RefCell<HashMap<String, String>>>,
    local: Rc<RefCell<Option<web_sys::MediaStream>>>,
    /// Daftar ICE server (STUN + TURN) yang diambil sekali saat connect.
    ice: Rc<RefCell<Option<js_sys::Array>>>,
    tiles: NodeRef<leptos::html::Div>,
    phase: RwSignal<Phase>,
    pending: RwSignal<Vec<(String, String)>>,
    error_msg: RwSignal<Option<String>>,
    self_id: RwSignal<String>,
}

impl Ctx {
    fn ws_send(&self, v: serde_json::Value) {
        if let Some(ws) = self.ws.borrow().as_ref() {
            let _ = ws.send_with_str(&v.to_string());
        }
    }
    fn name_of(&self, id: &str) -> String {
        self.names
            .borrow()
            .get(id)
            .cloned()
            .unwrap_or_else(|| "Peserta".to_string())
    }
}

/// Tautkan stream remote ke tile video (buat tile bila belum ada).
fn attach_remote_tile(
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
    // Sudah ada → cukup perbarui srcObject.
    if let Some(el) = document.get_element_by_id(&video_id) {
        let video: web_sys::HtmlVideoElement = el.unchecked_into();
        video.set_src_object(Some(stream));
        return;
    }
    let wrap_id = format!("meet-tile-{peer_id}");
    let (Ok(wrap), Ok(video), Ok(label)) = (
        document.create_element("div"),
        document.create_element("video"),
        document.create_element("div"),
    ) else {
        return;
    };
    let _ = wrap.set_attribute("id", &wrap_id);
    let _ = wrap.set_attribute("class", "meet-tile");
    let video: web_sys::HtmlVideoElement = video.unchecked_into();
    let _ = video.set_attribute("id", &video_id);
    let _ = video.set_attribute("class", "meet-tile-video");
    video.set_autoplay(true);
    video.set_attribute("playsinline", "true").ok();
    video.set_src_object(Some(stream));
    let _ = label.set_attribute("class", "meet-tile-name");
    label.set_text_content(Some(name));
    let _ = wrap.append_child(&video);
    let _ = wrap.append_child(&label);
    let _ = container.append_child(&wrap);
}

fn remove_tile(peer_id: &str) {
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
        if let Some(el) = doc.get_element_by_id(&format!("meet-tile-{peer_id}")) {
            el.remove();
        }
    }
}

fn new_peer_connection(ctx: &Ctx, peer_id: &str) -> Option<web_sys::RtcPeerConnection> {
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

    // Terima media remote → pasang ke tile.
    {
        let tiles = ctx.tiles;
        let pid = peer_id.to_string();
        let nm = ctx.name_of(peer_id);
        let cb = Closure::<dyn FnMut(web_sys::RtcTrackEvent)>::new(move |e: web_sys::RtcTrackEvent| {
            let streams = e.streams();
            if streams.length() > 0 {
                let ms: web_sys::MediaStream = streams.get(0).unchecked_into();
                attach_remote_tile(tiles, &pid, &nm, &ms);
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

async fn make_offer(ctx: &Ctx, peer_id: &str) {
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

async fn handle_offer(ctx: &Ctx, from: &str, sdp: &str) {
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

async fn handle_answer(ctx: &Ctx, from: &str, sdp: &str) {
    let pc = ctx.pcs.borrow().get(from).cloned();
    if let Some(pc) = pc {
        let desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Answer);
        desc.set_sdp(sdp);
        let _ = wasm_bindgen_futures::JsFuture::from(pc.set_remote_description(&desc)).await;
    }
}

async fn handle_candidate(ctx: &Ctx, from: &str, data: &serde_json::Value) {
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

/// Tangani satu pesan JSON dari server.
async fn handle_msg(ctx: &Ctx, raw: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else {
        return;
    };
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match ty {
        "joined" | "admitted" => {
            if let Some(id) = v.get("self_id").and_then(|s| s.as_str()) {
                ctx.self_id.set(id.to_string());
            }
            if let Some(peers) = v.get("peers").and_then(|p| p.as_array()) {
                for p in peers {
                    if let (Some(id), Some(name)) = (
                        p.get("id").and_then(|x| x.as_str()),
                        p.get("name").and_then(|x| x.as_str()),
                    ) {
                        ctx.names.borrow_mut().insert(id.to_string(), name.to_string());
                    }
                }
            }
            if let Some(pending) = v.get("pending").and_then(|p| p.as_array()) {
                let list: Vec<(String, String)> = pending
                    .iter()
                    .filter_map(|p| {
                        Some((
                            p.get("id")?.as_str()?.to_string(),
                            p.get("name")?.as_str()?.to_string(),
                        ))
                    })
                    .collect();
                ctx.pending.set(list);
            }
            ctx.phase.set(Phase::InMeet);
            // Peer yang baru bergabung initiate offer ke tiap peer yang sudah ada.
            if let Some(peers) = v.get("peers").and_then(|p| p.as_array()) {
                for p in peers {
                    if let Some(id) = p.get("id").and_then(|x| x.as_str()) {
                        make_offer(ctx, id).await;
                    }
                }
            }
        }
        "waiting" => {
            if let Some(id) = v.get("self_id").and_then(|s| s.as_str()) {
                ctx.self_id.set(id.to_string());
            }
            ctx.phase.set(Phase::Waiting);
        }
        "denied" => {
            ctx.phase.set(Phase::Denied);
            if let Some(ws) = ctx.ws.borrow().as_ref() {
                let _ = ws.close();
            }
        }
        "join_request" => {
            if let (Some(id), Some(name)) = (
                v.get("peer_id").and_then(|x| x.as_str()),
                v.get("name").and_then(|x| x.as_str()),
            ) {
                ctx.names.borrow_mut().insert(id.to_string(), name.to_string());
                ctx.pending.update(|list| {
                    if !list.iter().any(|(pid, _)| pid == id) {
                        list.push((id.to_string(), name.to_string()));
                    }
                });
            }
        }
        "pending_left" => {
            if let Some(id) = v.get("peer_id").and_then(|x| x.as_str()) {
                ctx.pending.update(|list| list.retain(|(pid, _)| pid != id));
            }
        }
        "peer_joined" => {
            if let (Some(id), Some(name)) = (
                v.get("peer_id").and_then(|x| x.as_str()),
                v.get("name").and_then(|x| x.as_str()),
            ) {
                ctx.names.borrow_mut().insert(id.to_string(), name.to_string());
                // Tamu yang baru di-admit jadi penggagas offer; di sini cukup catat nama.
            }
        }
        "peer_left" => {
            if let Some(id) = v.get("peer_id").and_then(|x| x.as_str()) {
                remove_tile(id);
                if let Some(pc) = ctx.pcs.borrow_mut().remove(id) {
                    pc.close();
                }
                ctx.names.borrow_mut().remove(id);
            }
        }
        "roster" => {
            if let Some(peers) = v.get("peers").and_then(|p| p.as_array()) {
                for p in peers {
                    if let (Some(id), Some(name)) = (
                        p.get("id").and_then(|x| x.as_str()),
                        p.get("name").and_then(|x| x.as_str()),
                    ) {
                        ctx.names.borrow_mut().insert(id.to_string(), name.to_string());
                    }
                }
            }
        }
        "signal" => {
            let from = v.get("from").and_then(|x| x.as_str()).unwrap_or("");
            let data = v.get("data").cloned().unwrap_or(serde_json::Value::Null);
            if from.is_empty() {
                return;
            }
            match data.get("sdp_type").and_then(|t| t.as_str()) {
                Some("offer") => {
                    let sdp = data.get("sdp").and_then(|s| s.as_str()).unwrap_or("");
                    handle_offer(ctx, from, sdp).await;
                }
                Some("answer") => {
                    let sdp = data.get("sdp").and_then(|s| s.as_str()).unwrap_or("");
                    handle_answer(ctx, from, sdp).await;
                }
                _ => {
                    if data.get("candidate").is_some() {
                        handle_candidate(ctx, from, &data).await;
                    }
                }
            }
        }
        "meeting_ended" => {
            ctx.phase.set(Phase::Ended);
            for (_, pc) in ctx.pcs.borrow_mut().drain() {
                pc.close();
            }
            if let Some(ws) = ctx.ws.borrow().as_ref() {
                let _ = ws.close();
            }
        }
        "error" => {
            ctx.error_msg
                .set(Some(v.get("message").and_then(|m| m.as_str()).unwrap_or("Error").to_string()));
            ctx.phase.set(Phase::Error);
        }
        _ => {}
    }
}

/// Buka koneksi WS + media lalu kirim `join`. Dipakai host (on mount) & tamu
/// (saat klik "Minta masuk").
async fn start_connection(
    ctx: Ctx,
    room_id: String,
    as_host: bool,
    name: String,
    local_sig: RwSignal<Option<send_wrapper::SendWrapper<web_sys::MediaStream>>>,
) {
    // Kamera + mic.
    let stream = match get_user_media().await {
        Ok(s) => s,
        Err(e) => {
            ctx.error_msg.set(Some(e));
            ctx.phase.set(Phase::Error);
            return;
        }
    };
    *ctx.local.borrow_mut() = Some(stream.clone());
    local_sig.set(Some(send_wrapper::SendWrapper::new(stream)));

    // Ambil ICE server (STUN + TURN) sekali untuk semua peer connection.
    *ctx.ice.borrow_mut() = Some(crate::web::rtc::fetch_ice_servers().await);

    // WebSocket signaling.
    let url = build_ws_url(&format!("/ws/meet/{room_id}"));
    let ws = match web_sys::WebSocket::new(&url) {
        Ok(ws) => ws,
        Err(e) => {
            ctx.error_msg.set(Some(format!("WS gagal: {e:?}")));
            ctx.phase.set(Phase::Error);
            return;
        }
    };

    // onmessage → dispatch async.
    {
        let ctx2 = ctx.clone();
        let cb = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let s: String = txt.into();
                let ctx3 = ctx2.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    handle_msg(&ctx3, &s).await;
                });
            }
        });
        ws.set_onmessage(Some(cb.as_ref().unchecked_ref()));
        cb.forget();
    }
    // onopen → kirim join.
    {
        let ws_ref = ws.clone();
        let join = json!({ "type": "join", "as_host": as_host, "name": name });
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let _ = ws_ref.send_with_str(&join.to_string());
        });
        ws.set_onopen(Some(cb.as_ref().unchecked_ref()));
        cb.forget();
    }
    // onerror.
    {
        let ctx2 = ctx.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            ctx2.error_msg.set(Some("Koneksi signaling terputus".to_string()));
        });
        ws.set_onerror(Some(cb.as_ref().unchecked_ref()));
        cb.forget();
    }

    *ctx.ws.borrow_mut() = Some(ws);
}

#[component]
pub fn MeetPage() -> impl IntoView {
    let params = use_params_map();
    let route_id = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>();

    let phase = RwSignal::new(Phase::Loading);
    let pending = RwSignal::new(Vec::<(String, String)>::new());
    let error_msg = RwSignal::new(None::<String>);
    let self_id = RwSignal::new(String::new());
    let room_id = RwSignal::new(String::new());
    let guest_name = RwSignal::new(String::new());
    let invite_url = RwSignal::new(String::new());
    let copied = RwSignal::new(false);

    let local_sig: RwSignal<Option<send_wrapper::SendWrapper<web_sys::MediaStream>>> =
        RwSignal::new(None);
    let local_ref: NodeRef<leptos::html::Video> = NodeRef::new();
    let tiles_ref: NodeRef<leptos::html::Div> = NodeRef::new();

    let ctx = Ctx {
        ws: Rc::new(RefCell::new(None)),
        pcs: Rc::new(RefCell::new(HashMap::new())),
        names: Rc::new(RefCell::new(HashMap::new())),
        local: Rc::new(RefCell::new(None)),
        ice: Rc::new(RefCell::new(None)),
        tiles: tiles_ref,
        phase,
        pending,
        error_msg,
        self_id,
    };
    let ctx = StoredValue::new_local(ctx);

    // Prefill nama tamu dari sesi (bila login).
    if let Some(auth) = auth {
        Effect::new(move |_| {
            if let Some(Ok(Some(u))) = auth.get() {
                if guest_name.with(|n| n.is_empty()) {
                    guest_name.set(u.name.clone());
                }
            }
        });
    }

    // Pasang preview lokal.
    Effect::new(move |_| {
        if let Some(video) = local_ref.get() {
            match local_sig.get() {
                Some(s) => {
                    video.set_src_object(Some(&s));
                }
                None => video.set_src_object(None),
            }
        }
    });

    // Tentukan peran dari rute & mulai (host langsung, tamu nunggu klik).
    let host_start = Action::new_local(move |_: &()| {
        let ctx = ctx.get_value();
        async move {
            match api_create_room().await {
                Ok(rid) => {
                    room_id.set(rid.clone());
                    invite_url.set(format!("{}/meet/{}", origin(), rid));
                    start_connection(ctx, rid, true, String::new(), local_sig).await;
                }
                Err(e) => {
                    error_msg.set(Some(e));
                    phase.set(Phase::Error);
                }
            }
        }
    });

    let guest_join = Action::new_local(move |_: &()| {
        let ctx = ctx.get_value();
        let rid = route_id();
        let name = guest_name.get_untracked();
        async move {
            room_id.set(rid.clone());
            start_connection(ctx, rid, false, name, local_sig).await;
        }
    });

    // Mount: host mode jika rute "host", selain itu lobby tamu.
    Effect::new(move |prev: Option<()>| {
        if prev.is_some() {
            return;
        }
        if route_id() == "host" {
            host_start.dispatch(());
        } else {
            phase.set(Phase::Lobby);
        }
    });

    // Aksi host: admit / deny.
    let admit = move |peer_id: String| {
        ctx.get_value()
            .ws_send(json!({ "type": "admit", "peer_id": peer_id }));
        pending.update(|l| l.retain(|(pid, _)| pid != &peer_id));
    };
    let deny = move |peer_id: String| {
        ctx.get_value()
            .ws_send(json!({ "type": "deny", "peer_id": peer_id }));
        pending.update(|l| l.retain(|(pid, _)| pid != &peer_id));
    };

    let copy_link = move |_| {
        #[cfg(target_arch = "wasm32")]
        if let Some(win) = web_sys::window() {
            let _ = win.navigator().clipboard().write_text(&invite_url.get_untracked());
        }
        let _ = &invite_url;
        copied.set(true);
    };

    // Cleanup: tutup semua PC, hentikan media, tutup WS.
    on_cleanup(move || {
        let c = ctx.get_value();
        for (_, pc) in c.pcs.borrow_mut().drain() {
            pc.close();
        }
        let stream = c.local.borrow().clone();
        if let Some(stream) = stream {
            let tracks = stream.get_tracks();
            for i in 0..tracks.length() {
                let t: web_sys::MediaStreamTrack = tracks.get(i).unchecked_into();
                t.stop();
            }
        }
        let ws = c.ws.borrow().clone();
        if let Some(ws) = ws {
            let _ = ws.close();
        }
    });

    let is_host = move || route_id() == "host";

    view! {
        <GridBackground />
        <div class="page meet-page">
            <header class="meet-header">
                <A href="/merchant" attr:class="meet-back" attr:aria-label="Kembali">
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="meet-title">
                    {move || if is_host() { "Meet (Host)" } else { "Meet" }}
                </span>
            </header>

            // ── Video area (selalu ada; tile remote ditempel imperatif) ─────────
            <div class="meet-stage" class:meet-hidden=move || !matches!(phase.get(), Phase::InMeet | Phase::Waiting)>
                <div class="meet-tiles" node_ref=tiles_ref>
                    <div class="meet-tile meet-tile--self">
                        <video
                            node_ref=local_ref
                            class="meet-tile-video"
                            autoplay=true
                            muted=true
                            playsinline=true
                        />
                        <div class="meet-tile-name">"Anda"</div>
                    </div>
                </div>
            </div>

            // ── Panel berdasar fase ─────────────────────────────────────────────
            {move || match phase.get() {
                Phase::Loading => view! {
                    <div class="meet-center"><div class="meet-spinner"></div>
                        <p>"Menyiapkan meet..."</p></div>
                }.into_any(),

                Phase::Lobby => view! {
                    <div class="meet-center meet-lobby">
                        <h2 class="meet-lobby-title">"Gabung Meet"</h2>
                        <p class="meet-lobby-sub">"Masukkan nama Anda untuk meminta izin masuk."</p>
                        <input
                            class="meet-input"
                            placeholder="Nama Anda"
                            prop:value=move || guest_name.get()
                            on:input=move |e| guest_name.set(event_target_value(&e))
                        />
                        <button
                            class="meet-btn meet-btn--primary"
                            disabled=move || guest_name.with(|n| n.trim().is_empty())
                            on:click=move |_| { guest_join.dispatch(()); }
                        >"Minta masuk"</button>
                    </div>
                }.into_any(),

                Phase::Waiting => view! {
                    <div class="meet-center">
                        <div class="meet-spinner"></div>
                        <p class="meet-waiting-text">"Menunggu izin host untuk masuk..."</p>
                    </div>
                }.into_any(),

                Phase::Denied => view! {
                    <div class="meet-center">
                        <p class="meet-result-icon">"🚫"</p>
                        <p>"Permintaan masuk ditolak oleh host."</p>
                        <A href="/" attr:class="meet-btn">"Kembali"</A>
                    </div>
                }.into_any(),

                Phase::Ended => view! {
                    <div class="meet-center">
                        <p class="meet-result-icon">"👋"</p>
                        <p>"Meet telah berakhir."</p>
                        <A href="/" attr:class="meet-btn">"Kembali"</A>
                    </div>
                }.into_any(),

                Phase::Error => view! {
                    <div class="meet-center">
                        <p class="meet-result-icon">"⚠️"</p>
                        <p>{move || error_msg.get().unwrap_or_else(|| "Terjadi kesalahan".into())}</p>
                        <A href="/" attr:class="meet-btn">"Kembali"</A>
                    </div>
                }.into_any(),

                Phase::InMeet => view! {
                    // Host: link undangan + waiting room.
                    {move || is_host().then(|| view! {
                        <div class="meet-host-panel">
                            <div class="meet-invite">
                                <span class="meet-invite-label">"Link undangan"</span>
                                <div class="meet-invite-row">
                                    <input class="meet-input meet-invite-input" readonly=true
                                        prop:value=move || invite_url.get() />
                                    <button class="meet-btn meet-btn--small" on:click=copy_link>
                                        {move || if copied.get() { "Tersalin" } else { "Salin" }}
                                    </button>
                                </div>
                            </div>
                            <div class="meet-waitroom">
                                <span class="meet-waitroom-label">
                                    {move || format!("Ruang tunggu ({})", pending.get().len())}
                                </span>
                                {move || if pending.get().is_empty() {
                                    view! { <p class="meet-waitroom-empty">"Belum ada yang menunggu."</p> }.into_any()
                                } else {
                                    view! {
                                        <For
                                            each=move || pending.get()
                                            key=|p| p.0.clone()
                                            let:item
                                        >
                                            {
                                                let id_admit = item.0.clone();
                                                let id_deny = item.0.clone();
                                                view! {
                                                    <div class="meet-wait-item">
                                                        <span class="meet-wait-name">{item.1.clone()}</span>
                                                        <div class="meet-wait-actions">
                                                            <button class="meet-btn meet-btn--primary meet-btn--small"
                                                                on:click=move |_| admit(id_admit.clone())>
                                                                "Izinkan"</button>
                                                            <button class="meet-btn meet-btn--ghost meet-btn--small"
                                                                on:click=move |_| deny(id_deny.clone())>
                                                                "Tolak"</button>
                                                        </div>
                                                    </div>
                                                }
                                            }
                                        </For>
                                    }.into_any()
                                }}
                            </div>
                        </div>
                    })}
                }.into_any(),
            }}
        </div>
    }
}
