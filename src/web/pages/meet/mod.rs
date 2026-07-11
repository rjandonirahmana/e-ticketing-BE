//! web/pages/meet — Halaman "Meet" (konferensi video P2P mesh, gaya Google Meet).
//!
//! Dua peran dalam satu halaman, ditentukan dari rute:
//! - `/meet/host` → HOST (merchant): buat room, lihat ruang tunggu,
//!   izinkan/tolak tamu, bagikan link undangan.
//! - `/meet/{id}` → TAMU: minta masuk, tunggu izin host, lalu mesh.
//!
//! Alur gaya Google Meet: PREJOIN (green room) → WAITING → INMEET (grid +
//! control bar mic/kamera/keluar + panel orang).
//!
//! Modul:
//! - [`tiles`]     : tile video remote (imperatif DOM) + ikon SVG.
//! - [`webrtc`]    : mesh per-peer (RtcPeerConnection, offer/answer/ICE).
//! - [`signaling`] : loop pesan WS (admit/roster/signal/state) + lifecycle.
//!
//! Server hanya relay signaling + admit + status media (lihat `src/meet/`).
//! Media mengalir langsung antar-browser (mesh).

mod signaling;
mod tiles;
mod webrtc;

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};
use serde::Deserialize;
use serde_json::json;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use wasm_bindgen::prelude::*;

use crate::web::app::AuthResource;
use signaling::{connect, setup_preview, teardown};
use tiles::{initial_of, CAM_OFF_SVG, CAM_ON_SVG, MIC_OFF_SVG, MIC_ON_SVG};
use webrtc::replace_video_everywhere;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Loading,
    Prejoin,
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

/// Minta izin kamera/mic via sumber tunggal `rtc::request_camera_mic`. Error
/// dikembalikan sebagai pesan actionable + panduan izin (lihat `MediaError`).
async fn get_user_media() -> Result<web_sys::MediaStream, String> {
    crate::web::rtc::request_camera_mic()
        .await
        .map_err(|e| e.user_message())
}

async fn get_display_media() -> Result<web_sys::MediaStream, String> {
    let window = web_sys::window().ok_or("No window")?;
    let promise = window
        .navigator()
        .media_devices()
        .map_err(|_| "MediaDevices tidak didukung".to_string())?
        .get_display_media()
        .map_err(|_| "getDisplayMedia gagal".to_string())?;
    let val = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("Berbagi layar dibatalkan: {e:?}"))?;
    Ok(web_sys::MediaStream::from(val))
}

/// Hentikan screen share: stop track layar, kembalikan track kamera ke semua
/// peer (via replace_track) & ke preview lokal. Idempoten (tombol + event
/// `onended` browser bisa dua-duanya memanggil).
fn stop_share(
    ctx: &Ctx,
    sharing: RwSignal<bool>,
    local_sig: RwSignal<Option<send_wrapper::SendWrapper<web_sys::MediaStream>>>,
    screen_store: StoredValue<Option<send_wrapper::SendWrapper<web_sys::MediaStream>>>,
    cam: RwSignal<bool>,
) {
    if !sharing.get_untracked() {
        return;
    }
    if let Some(s) = screen_store.get_value() {
        let tracks = s.get_tracks();
        for i in 0..tracks.length() {
            let t: web_sys::MediaStreamTrack = tracks.get(i).unchecked_into();
            // Lepas onended agar tak memicu stop_share lagi (dan agar closure yang
            // dipegang bisa aman di-drop saat digantikan / unmount).
            t.set_onended(None);
            t.stop();
        }
    }
    screen_store.set_value(None);
    let cam_stream = ctx.local.borrow().clone();
    if let Some(cam_stream) = cam_stream {
        let vtracks = cam_stream.get_video_tracks();
        let first = vtracks.get(0);
        if !first.is_undefined() {
            let t: web_sys::MediaStreamTrack = first.unchecked_into();
            t.set_enabled(cam.get_untracked());
            replace_video_everywhere(ctx, Some(&t));
        }
        local_sig.set(Some(send_wrapper::SendWrapper::new(cam_stream)));
    }
    sharing.set(false);
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

/// State bersama yang dibawa ke dalam closure WS & fungsi signaling. Field-nya
/// privat di modul `meet` tapi dapat diakses submodul ([`signaling`], [`webrtc`])
/// karena submodul adalah keturunan modul ini.
/// Dua closure JS milik satu peer connection (onicecandidate + ontrack).
/// DIPEGANG di `Ctx::pc_closures` (bukan `.forget()`) supaya di mesh N-orang tak
/// bocor tiap peer join/leave — di-drop saat peer keluar atau meet berakhir.
type PeerClosures = (
    Closure<dyn FnMut(web_sys::RtcPeerConnectionIceEvent)>,
    Closure<dyn FnMut(web_sys::RtcTrackEvent)>,
);

/// Closure WebSocket signaling (onmessage, onopen, onerror). Dipegang di
/// `Ctx::ws_closures` (bukan `.forget()`) → satu set per sesi meet, di-drop saat
/// teardown alih-alih bocor tiap kali join room.
type WsClosures = (
    Closure<dyn FnMut(web_sys::MessageEvent)>,
    Closure<dyn FnMut(web_sys::Event)>,
    Closure<dyn FnMut(web_sys::Event)>,
);

#[derive(Clone)]
struct Ctx {
    ws: Rc<RefCell<Option<web_sys::WebSocket>>>,
    pcs: Rc<RefCell<HashMap<String, web_sys::RtcPeerConnection>>>,
    /// Closure per-peer (onicecandidate, ontrack) — dipegang agar bisa di-drop
    /// saat peer keluar / teardown. Kunci = peer_id, sinkron dengan `pcs`.
    pc_closures: Rc<RefCell<HashMap<String, PeerClosures>>>,
    /// Closure WebSocket signaling — dipegang untuk sesi ini, di-drop saat teardown.
    ws_closures: Rc<RefCell<Option<WsClosures>>>,
    names: Rc<RefCell<HashMap<String, String>>>,
    /// Status mic/kamera terakhir tiap peer remote (untuk dipasang ke tile yang
    /// mungkin dibuat setelah pesan state tiba).
    states: Rc<RefCell<HashMap<String, (bool, bool)>>>,
    /// ICE candidate yang tiba SEBELUM remote description di-set → dibuffer per
    /// peer lalu di-flush setelah SRD. Mencegah `addIceCandidate` gagal.
    pending_ice: Rc<RefCell<HashMap<String, Vec<serde_json::Value>>>>,
    /// Peer yang remote description-nya sudah di-set (boleh terima candidate).
    remote_ready: Rc<RefCell<HashSet<String>>>,
    local: Rc<RefCell<Option<web_sys::MediaStream>>>,
    /// Daftar ICE server (STUN + TURN) yang diambil sekali saat connect.
    ice: Rc<RefCell<Option<js_sys::Array>>>,
    tiles: NodeRef<leptos::html::Div>,
    phase: RwSignal<Phase>,
    pending: RwSignal<Vec<(String, String)>>,
    error_msg: RwSignal<Option<String>>,
    self_id: RwSignal<String>,
    /// Status mic/kamera diri sendiri.
    mic: RwSignal<bool>,
    cam: RwSignal<bool>,
    /// Jumlah peer remote (peserta = ini + 1).
    count: RwSignal<usize>,
    /// Riwayat chat dalam meet: (nama_pengirim, teks).
    chat: RwSignal<Vec<(String, String)>>,
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
    /// Broadcast status mic/kamera sendiri ke peserta lain.
    fn send_state(&self) {
        self.ws_send(json!({
            "type": "state",
            "mic": self.mic.get_untracked(),
            "cam": self.cam.get_untracked(),
        }));
    }
    fn sync_count(&self) {
        self.count.set(self.pcs.borrow().len());
    }
}

#[component]
pub fn MeetPage() -> impl IntoView {
    let params = use_params_map();
    let route_id = move || params.read().get("id").unwrap_or_default();
    let navigate = use_navigate();

    let auth = use_context::<AuthResource>();

    let phase = RwSignal::new(Phase::Loading);
    let pending = RwSignal::new(Vec::<(String, String)>::new());
    let error_msg = RwSignal::new(None::<String>);
    let self_id = RwSignal::new(String::new());
    let room_id = RwSignal::new(String::new());
    let self_name = RwSignal::new(String::new());
    let invite_url = RwSignal::new(String::new());
    let copied = RwSignal::new(false);
    let show_people = RwSignal::new(false);
    let mic = RwSignal::new(true);
    let cam = RwSignal::new(true);
    let count = RwSignal::new(0usize);
    let chat = RwSignal::new(Vec::<(String, String)>::new());
    let show_chat = RwSignal::new(false);
    let chat_input = RwSignal::new(String::new());
    let sharing = RwSignal::new(false);
    let screen_store: StoredValue<Option<send_wrapper::SendWrapper<web_sys::MediaStream>>> =
        StoredValue::new(None);
    // onended track screen ("Stop sharing" bawaan browser) — DIPEGANG (bukan
    // `.forget()`). Di-drop saat share berikutnya menggantikan atau saat page
    // unmount (StoredValue). TIDAK di-drop dari dalam stop_share karena closure
    // bisa sedang mengeksekusi stop_share → drop-saat-jalan = panic.
    let screen_onended: StoredValue<
        Option<send_wrapper::SendWrapper<Closure<dyn FnMut(web_sys::Event)>>>,
    > = StoredValue::new(None);

    let local_sig: RwSignal<Option<send_wrapper::SendWrapper<web_sys::MediaStream>>> =
        RwSignal::new(None);
    let local_ref: NodeRef<leptos::html::Video> = NodeRef::new();
    let tiles_ref: NodeRef<leptos::html::Div> = NodeRef::new();

    let ctx = Ctx {
        ws: Rc::new(RefCell::new(None)),
        pcs: Rc::new(RefCell::new(HashMap::new())),
        pc_closures: Rc::new(RefCell::new(HashMap::new())),
        ws_closures: Rc::new(RefCell::new(None)),
        names: Rc::new(RefCell::new(HashMap::new())),
        states: Rc::new(RefCell::new(HashMap::new())),
        pending_ice: Rc::new(RefCell::new(HashMap::new())),
        remote_ready: Rc::new(RefCell::new(HashSet::new())),
        local: Rc::new(RefCell::new(None)),
        ice: Rc::new(RefCell::new(None)),
        tiles: tiles_ref,
        phase,
        pending,
        error_msg,
        self_id,
        mic,
        cam,
        count,
        chat,
    };
    let ctx = StoredValue::new_local(ctx);

    let is_host = move || route_id() == "host";

    // Prefill nama dari sesi (bila login).
    if let Some(auth) = auth {
        Effect::new(move |_| {
            if let Some(Ok(Some(u))) = auth.get() {
                if self_name.with(|n| n.is_empty()) {
                    self_name.set(u.name.clone());
                }
            }
        });
    }

    // Pasang preview lokal ke <video> self (sama untuk green room & in-meet).
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

    // Mount: minta izin kamera/mic → tampilkan green room.
    let preview = Action::new_local(move |_: &()| {
        let ctx = ctx.get_value();
        async move { setup_preview(ctx, local_sig).await }
    });
    Effect::new(move |prev: Option<()>| {
        if prev.is_none() {
            preview.dispatch(());
        }
    });

    // Tombol "Gabung" / "Minta masuk" dari green room.
    let join = Action::new_local(move |_: &()| {
        let ctx = ctx.get_value();
        let host = is_host();
        let name = self_name.get_untracked();
        async move {
            if host {
                match api_create_room().await {
                    Ok(rid) => {
                        room_id.set(rid.clone());
                        invite_url.set(format!("{}/meet/{}", origin(), rid));
                        connect(ctx, rid, true, name).await;
                    }
                    Err(e) => {
                        error_msg.set(Some(e));
                        phase.set(Phase::Error);
                    }
                }
            } else {
                let rid = route_id();
                room_id.set(rid.clone());
                connect(ctx, rid, false, name).await;
            }
        }
    });

    // Toggle mic & kamera (update track lokal + broadcast status).
    let toggle_mic = move |_| {
        let c = ctx.get_value();
        let on = !mic.get_untracked();
        mic.set(on);
        if let Some(s) = c.local.borrow().as_ref() {
            let tracks = s.get_audio_tracks();
            for i in 0..tracks.length() {
                let t: web_sys::MediaStreamTrack = tracks.get(i).unchecked_into();
                t.set_enabled(on);
            }
        }
        c.send_state();
    };
    let toggle_cam = move |_| {
        let c = ctx.get_value();
        let on = !cam.get_untracked();
        cam.set(on);
        if let Some(s) = c.local.borrow().as_ref() {
            let tracks = s.get_video_tracks();
            for i in 0..tracks.length() {
                let t: web_sys::MediaStreamTrack = tracks.get(i).unchecked_into();
                t.set_enabled(on);
            }
        }
        c.send_state();
    };

    // Bagikan layar: getDisplayMedia → replace_track ke semua peer; preview
    // lokal jadi layar. Stop (tombol / event onended browser) → balik ke kamera.
    let toggle_share = Action::new_local(move |_: &()| {
        let c = ctx.get_value();
        async move {
            if sharing.get_untracked() {
                stop_share(&c, sharing, local_sig, screen_store, cam);
                return;
            }
            let screen = match get_display_media().await {
                Ok(s) => s,
                Err(_) => return, // user batal / tak didukung
            };
            let first = screen.get_video_tracks().get(0);
            if first.is_undefined() {
                return;
            }
            let strack: web_sys::MediaStreamTrack = first.unchecked_into();
            replace_video_everywhere(&c, Some(&strack));
            local_sig.set(Some(send_wrapper::SendWrapper::new(screen.clone())));
            // Browser punya tombol "Stop sharing" sendiri → track akan 'ended'.
            {
                let c2 = c.clone();
                let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                    stop_share(&c2, sharing, local_sig, screen_store, cam);
                });
                strack.set_onended(Some(cb.as_ref().unchecked_ref()));
                // Pegang (replace grup lama → drop). Bukan `.forget()`.
                screen_onended.set_value(Some(send_wrapper::SendWrapper::new(cb)));
            }
            screen_store.set_value(Some(send_wrapper::SendWrapper::new(screen)));
            sharing.set(true);
        }
    });

    let leave = move |_| {
        teardown(&ctx.get_value());
        phase.set(Phase::Ended);
    };

    // Keluar / meet dibubarkan → langsung ke halaman utama (tanpa layar "Ended").
    Effect::new(move |_| {
        if phase.get() == Phase::Ended {
            navigate("/", Default::default());
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

    // Kirim pesan chat ke semua peserta.
    let send_chat = move || {
        let t = chat_input.get_untracked().trim().to_string();
        if t.is_empty() {
            return;
        }
        ctx.get_value().ws_send(json!({ "type": "chat", "text": t }));
        chat_input.set(String::new());
    };

    // Auto-reconnect signaling: bila WS meet putus saat masih in-meet, sambung
    // ulang tiap 3 dtk. Media WebRTC yang sudah jalan tetap hidup; reconnect
    // memulihkan jalur signaling (admit / chat / peer baru).
    // Catatan: bila HOST yang putus, server membubarkan room (by design) →
    // reconnect host = membuat room baru; reconnect tamu = minta izin lagi.
    Effect::new(move |_| {
        let interval = send_wrapper::SendWrapper::new(gloo_timers::callback::Interval::new(
            3_000,
            move || {
                if phase.get_untracked() != Phase::InMeet {
                    return;
                }
                let closed = ctx
                    .get_value()
                    .ws
                    .borrow()
                    .as_ref()
                    .map(|w| w.ready_state() == web_sys::WebSocket::CLOSED)
                    .unwrap_or(true);
                if !closed {
                    return;
                }
                let rid = room_id.get_untracked();
                if rid.is_empty() {
                    return;
                }
                let host = is_host();
                let name = self_name.get_untracked();
                let c = ctx.get_value();
                wasm_bindgen_futures::spawn_local(async move {
                    connect(c, rid, host, name).await;
                });
            },
        ));
        on_cleanup(move || drop(interval));
    });

    on_cleanup(move || teardown(&ctx.get_value()));

    let stage_visible = move || matches!(phase.get(), Phase::Prejoin | Phase::InMeet);
    let participants = move || count.get() + 1;

    view! {
        <div class="page meet-page" class:meet-prejoin-mode=move || phase.get() == Phase::Prejoin>
            // ── Top bar (hanya saat in-meet) ────────────────────────────────────
            {move || (phase.get() == Phase::InMeet).then(|| view! {
                <header class="meet-topbar">
                    <span class="meet-topbar-title">
                        {move || if is_host() { "Meet · Host" } else { "Meet" }}
                    </span>
                    <span class="meet-topbar-count">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2"/>
                            <circle cx="9" cy="7" r="4"/>
                            <path d="M23 21v-2a4 4 0 00-3-3.87"/>
                        </svg>
                        {move || participants()}
                    </span>
                </header>
            })}

            // ── Panggung video (green room + in-meet) ───────────────────────────
            <div class="meet-stage" class:meet-hidden=move || !stage_visible()>
                <div
                    class="meet-tiles"
                    class:meet-tiles--solo=move || phase.get() == Phase::Prejoin
                    node_ref=tiles_ref
                >
                    <div class="meet-tile meet-tile--self"
                         class:cam-off=move || !cam.get()
                         class:mic-off=move || !mic.get()
                         class:meet-sharing=move || sharing.get()>
                        <video
                            node_ref=local_ref
                            class="meet-tile-video"
                            autoplay=true
                            muted=true
                            playsinline=true
                        />
                        <div class="meet-tile-avatar">
                            <span>{move || initial_of(&self_name.get())}</span>
                        </div>
                        <div class="meet-tile-bar">
                            <span class="meet-tile-mic" inner_html=MIC_OFF_SVG></span>
                            <span class="meet-tile-name">"Anda"</span>
                        </div>
                    </div>
                </div>
            </div>

            // ── Green room card ─────────────────────────────────────────────────
            {move || (phase.get() == Phase::Prejoin).then(|| view! {
                <div class="meet-prejoin">
                    <h2 class="meet-prejoin-title">"Siap bergabung?"</h2>
                    <p class="meet-prejoin-sub">
                        {move || if is_host() {
                            "Mulai meet dan bagikan link undangan ke peserta.".to_string()
                        } else {
                            "Host akan mengizinkan Anda masuk.".to_string()
                        }}
                    </p>
                    {move || (!is_host()).then(|| view! {
                        <input
                            class="meet-input"
                            placeholder="Nama Anda"
                            prop:value=move || self_name.get()
                            on:input=move |e| self_name.set(event_target_value(&e))
                        />
                    })}
                    <div class="meet-prejoin-toggles">
                        <button
                            class="meet-ctrl"
                            class:meet-ctrl--off=move || !mic.get()
                            on:click=toggle_mic
                            aria-label="Mic"
                            inner_html=move || if mic.get() { MIC_ON_SVG } else { MIC_OFF_SVG }
                        ></button>
                        <button
                            class="meet-ctrl"
                            class:meet-ctrl--off=move || !cam.get()
                            on:click=toggle_cam
                            aria-label="Kamera"
                            inner_html=move || if cam.get() { CAM_ON_SVG } else { CAM_OFF_SVG }
                        ></button>
                    </div>
                    <button
                        class="meet-btn meet-btn--primary meet-prejoin-join"
                        disabled=move || join.pending().get()
                            || (!is_host() && self_name.with(|n| n.trim().is_empty()))
                        on:click=move |_| { join.dispatch(()); }
                    >
                        {move || if join.pending().get() {
                            "Menghubungkan...".to_string()
                        } else if is_host() {
                            "Mulai meet".to_string()
                        } else {
                            "Minta masuk".to_string()
                        }}
                    </button>
                    <A href="/merchant" attr:class="meet-prejoin-back">"Batal"</A>
                </div>
            })}

            // ── Control bar (in-meet) ───────────────────────────────────────────
            {move || (phase.get() == Phase::InMeet).then(|| view! {
                <div class="meet-controls">
                    <button class="meet-ctrl" class:meet-ctrl--off=move || !mic.get()
                        on:click=toggle_mic aria-label="Mic"
                        inner_html=move || if mic.get() { MIC_ON_SVG } else { MIC_OFF_SVG }>
                    </button>
                    <button class="meet-ctrl" class:meet-ctrl--off=move || !cam.get()
                        on:click=toggle_cam aria-label="Kamera"
                        inner_html=move || if cam.get() { CAM_ON_SVG } else { CAM_OFF_SVG }>
                    </button>
                    <button class="meet-ctrl" class:meet-ctrl--on=move || show_chat.get()
                        on:click=move |_| show_chat.update(|s| *s = !*s) aria-label="Chat">
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M21 11.5a8.38 8.38 0 01-.9 3.8 8.5 8.5 0 01-7.6 4.7 8.38 8.38 0 01-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 01-.9-3.8 8.5 8.5 0 014.7-7.6 8.38 8.38 0 013.8-.9h.5a8.48 8.48 0 018 8v.5z"/>
                        </svg>
                    </button>
                    <button class="meet-ctrl" class:meet-ctrl--on=move || sharing.get()
                        on:click=move |_| { toggle_share.dispatch(()); } aria-label="Bagikan layar">
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <rect x="2" y="3" width="20" height="14" rx="2"/>
                            <line x1="8" y1="21" x2="16" y2="21"/>
                            <line x1="12" y1="17" x2="12" y2="21"/>
                        </svg>
                    </button>
                    {move || is_host().then(|| view! {
                        <button class="meet-ctrl meet-ctrl--people"
                            on:click=move |_| show_people.update(|s| *s = !*s)
                            aria-label="Peserta">
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                <path d="M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2"/>
                                <circle cx="9" cy="7" r="4"/>
                                <path d="M23 21v-2a4 4 0 00-3-3.87"/>
                            </svg>
                            {move || {
                                let n = pending.get().len();
                                (n > 0).then(|| view! { <span class="meet-ctrl-badge">{n}</span> })
                            }}
                        </button>
                    })}
                    <button class="meet-ctrl meet-ctrl--leave" on:click=leave aria-label="Keluar">
                        <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M9 21H5a2 2 0 01-2-2V5a2 2 0 012-2h4"/>
                            <polyline points="16 17 21 12 16 7"/>
                            <line x1="21" y1="12" x2="9" y2="12"/>
                        </svg>
                    </button>
                </div>
            })}

            // ── Panel orang (host): undangan + ruang tunggu ─────────────────────
            {move || (phase.get() == Phase::InMeet && is_host() && show_people.get()).then(|| view! {
                <div class="meet-people">
                    <div class="meet-people-head">
                        <span>"Orang"</span>
                        <button class="meet-people-close"
                            on:click=move |_| show_people.set(false)>"✕"</button>
                    </div>
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
                                <For each=move || pending.get() key=|p| p.0.clone() let:item>
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

            // ── Panel chat (semua peserta) ──────────────────────────────────────
            {move || (phase.get() == Phase::InMeet && show_chat.get()).then(|| view! {
                <div class="meet-chat">
                    <div class="meet-chat-head">
                        <span>"Chat"</span>
                        <button class="meet-people-close"
                            on:click=move |_| show_chat.set(false)>"✕"</button>
                    </div>
                    <div class="meet-chat-msgs">
                        {move || {
                            let msgs = chat.get();
                            if msgs.is_empty() {
                                view! { <p class="meet-waitroom-empty">"Belum ada pesan."</p> }.into_any()
                            } else {
                                msgs.into_iter().map(|(name, text)| view! {
                                    <div class="meet-chat-msg">
                                        <span class="meet-chat-name">{name}</span>
                                        <span class="meet-chat-text">{text}</span>
                                    </div>
                                }).collect_view().into_any()
                            }
                        }}
                    </div>
                    <div class="meet-chat-input-row">
                        <input
                            class="meet-input"
                            placeholder="Tulis pesan..."
                            prop:value=move || chat_input.get()
                            on:input=move |e| chat_input.set(event_target_value(&e))
                            on:keydown=move |e: leptos::ev::KeyboardEvent| {
                                if e.key() == "Enter" { e.prevent_default(); send_chat(); }
                            }
                        />
                        <button class="meet-btn meet-btn--primary meet-btn--small"
                            on:click=move |_| send_chat()>"Kirim"</button>
                    </div>
                </div>
            })}

            // ── Layar status (loading / waiting / hasil) ────────────────────────
            {move || match phase.get() {
                Phase::Loading => view! {
                    <div class="meet-center"><div class="meet-spinner"></div>
                        <p>"Menyiapkan kamera..."</p></div>
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
                        <p>"Anda telah keluar dari meet."</p>
                        <A href="/" attr:class="meet-btn">"Kembali"</A>
                    </div>
                }.into_any(),
                Phase::Error => view! {
                    <div class="meet-center">
                        <p class="meet-result-icon">"⚠️"</p>
                        <p>{move || error_msg.get().unwrap_or_else(|| "Terjadi kesalahan".into())}</p>
                        <div class="meet-error-actions">
                            // "Coba Lagi": minta ulang izin kamera/mic (setelah user
                            // mengizinkan di address bar) tanpa perlu reload halaman.
                            <button
                                class="meet-btn"
                                on:click=move |_| {
                                    error_msg.set(None);
                                    preview.dispatch(());
                                }
                            >
                                "Coba Lagi"
                            </button>
                            <A href="/" attr:class="meet-btn meet-btn--ghost">"Kembali"</A>
                        </div>
                    </div>
                }.into_any(),
                _ => view! {}.into_any(),
            }}
        </div>
    }
}
