use leptos::prelude::*;
use leptos_router::components::A;
use send_wrapper::SendWrapper;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures;
use std::rc::Rc;
use std::cell::Cell;

use crate::web::app::AuthResource;
use crate::web::components::BottomNav;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ViewerInfo {
    id: String,
    name: String,
    #[serde(default)]
    photo_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoomInfo {
    room_id: String,
    merchant_id: String,
    merchant_name: String,
    event_slug: Option<String>,
    viewer_count: usize,
    started_at: i64,
    #[serde(default)]
    viewers: Vec<ViewerInfo>,
}

/// Membaca body `{ "data": ... }` atau `{ "error": "..." }` dari kontrol-plane
/// live. Tanpa ini, respons error membuat `json["data"]` bernilai null dan
/// deserialisasi gagal dengan pesan menyesatkan ("invalid type: null,
/// expected struct RoomInfo") alih-alih pesan error server yang sebenarnya.
fn parse_api_data<T: for<'de> Deserialize<'de>>(json: &serde_json::Value) -> Result<T, String> {
    if let Some(err) = json.get("error").and_then(|e| e.as_str()) {
        return Err(err.to_string());
    }
    let data = json.get("data").ok_or("Respons server tidak valid")?;
    serde_json::from_value(data.clone()).map_err(|e| e.to_string())
}

async fn api_create_room() -> Result<RoomInfo, String> {
    let resp = gloo_net::http::Request::post("/api/live/rooms")
        .json(&serde_json::json!({}))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    parse_api_data(&json)
}

async fn api_get_room(room_id: &str) -> Result<RoomInfo, String> {
    let url = format!("/api/live/rooms/{}", room_id);
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    parse_api_data(&json)
}

async fn api_stop_room(room_id: &str) -> Result<(), String> {
    let url = format!("/api/live/rooms/{}", room_id);
    let resp = gloo_net::http::Request::delete(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status() == 200 {
        Ok(())
    } else {
        Err(format!("Stop room failed: {}", resp.status()))
    }
}

/// Bangun URL WebSocket dari path relatif.
/// Otomatis menggunakan wss:// bila halaman di-serve via HTTPS.
fn build_ws_url(path: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window().expect("no window");
        let location = window.location();
        let proto = if location.protocol().unwrap_or_default() == "https:" {
            "wss"
        } else {
            "ws"
        };
        let host = location.host().unwrap_or_default();
        return format!("{}://{}{}", proto, host, path);
    }
    #[cfg(not(target_arch = "wasm32"))]
    format!("ws://localhost{}", path)
}

#[component]
pub fn MerchantLivePage() -> impl IntoView {
    let _auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_live = RwSignal::new(false);
    let room_id = RwSignal::new(String::new());
    let viewer_count = RwSignal::new(0u32);
    // Toast sementara "{nama} telah join" (tampil sekali, hilang setelah 5 dtk).
    let join_toast = RwSignal::new(None::<String>);
    let toast_gen = StoredValue::new(0u32);
    let seen_ids: StoredValue<std::collections::HashSet<String>> =
        StoredValue::new(std::collections::HashSet::new());
    let status_text = RwSignal::new("Ready to go live".to_string());
    let error_msg = RwSignal::new(None::<String>);
    let pc: RwSignal<Option<SendWrapper<web_sys::RtcPeerConnection>>> = RwSignal::new(None);
    let local_stream: RwSignal<Option<SendWrapper<web_sys::MediaStream>>> = RwSignal::new(None);
    // WS signaling aktif untuk publisher.
    // Menutup WS ini akan secara otomatis menghentikan room di server
    // (live_publish_ws_loop mendeteksi disconnect dan memanggil stop_room).
    let sig_ws: RwSignal<Option<SendWrapper<web_sys::WebSocket>>> = RwSignal::new(None);
    // Closure `onicecandidate` sesi berjalan — disimpan agar bisa di-DROP saat
    // stop (setelah detach), bukan `.forget()` yang bocor tiap go-live.
    // SendWrapper memenuhi bound Send StoredValue (single-thread WASM/no-op native).
    #[allow(clippy::type_complexity)]
    let ice_closure: StoredValue<
        Option<SendWrapper<Closure<dyn FnMut(web_sys::RtcPeerConnectionIceEvent)>>>,
    > = StoredValue::new(None);

    let start_live = Action::new_local(move |_: &()| {
        let is_live = is_live;
        let room_id = room_id;
        let status_text = status_text;
        let error_msg = error_msg;
        let pc = pc;
        let local_stream = local_stream;
        let sig_ws = sig_ws;
        let ice_closure = ice_closure;

        async move {
            error_msg.set(None);
            status_text.set("Creating room...".to_string());

            let room = match api_create_room().await {
                Ok(r) => r,
                Err(e) => {
                    error_msg.set(Some(e));
                    status_text.set("Failed to create room".to_string());
                    return;
                }
            };

            room_id.set(room.room_id.clone());
            status_text.set("Requesting camera...".to_string());

            let stream = match get_user_media().await {
                Ok(s) => s,
                Err(e) => {
                    // `e` sudah pesan actionable (mis. cara mengizinkan). Tombol
                    // "Go Live" tetap bisa ditekan lagi sebagai "Coba Lagi".
                    error_msg.set(Some(e));
                    status_text.set("Izin kamera/mikrofon diperlukan".to_string());
                    let _ = api_stop_room(&room.room_id).await;
                    return;
                }
            };

            // MediaStream adalah handle JS yang murah di-clone; satu salinan untuk
            // preview lokal (signal) dan satu lagi diserahkan ke publisher.
            local_stream.set(Some(SendWrapper::new(stream.clone())));
            status_text.set("Connecting...".to_string());

            let (peer_connection, signaling_ws, on_ice) =
                match create_publisher(&room.room_id, Some(stream)).await {
                    Ok(triple) => triple,
                    Err(e) => {
                        error_msg.set(Some(e));
                        status_text.set("Connection failed".to_string());
                        let _ = api_stop_room(&room.room_id).await;
                        return;
                    }
                };

            // Simpan handle — PC (track/close), WS (lifecycle room), & closure
            // onicecandidate agar bisa di-DROP saat stop (bukan .forget()/bocor).
            pc.set(Some(SendWrapper::new(peer_connection)));
            sig_ws.set(Some(SendWrapper::new(signaling_ws)));
            ice_closure.set_value(Some(SendWrapper::new(on_ice)));
            is_live.set(true);
            status_text.set("LIVE".to_string());
        }
    });

    let stop_live = Action::new_local(move |_: &()| {
        let is_live = is_live;
        let room_id = room_id;
        let pc = pc;
        let local_stream = local_stream;
        let sig_ws = sig_ws;
        let status_text = status_text;
        let viewer_count = viewer_count;
        let join_toast = join_toast;
        let seen_ids = seen_ids;
        let ice_closure = ice_closure;

        async move {
            // Hentikan track yang menempel di peer connection (track aktif kamera),
            // lalu tutup koneksi. Tanpa menghentikan track sender, lampu kamera
            // tetap menyala meski PC ditutup (penyebab "kamera tidak berhenti").
            if let Some(conn) = pc.get_untracked() {
                // Detach onicecandidate SEBELUM menutup & drop closure → cegah
                // "closure invoked after drop" bila product ICE masih menyusul.
                conn.set_onicecandidate(None);
                stop_pc_senders(&conn);
                let _ = conn.close();
            }
            pc.set(None);
            ice_closure.set_value(None); // drop closure onicecandidate → memori bebas

            if let Some(stream) = local_stream.get_untracked() {
                stop_all_tracks(&stream);
            }
            local_stream.set(None);

            let rid = room_id.get_untracked();
            if !rid.is_empty() {
                // HTTP DELETE untuk segera menghapus room dari daftar live.
                let _ = api_stop_room(&rid).await;
            }

            // Tutup WS signaling — server juga akan memanggil stop_room
            // saat mendeteksi disconnect (idempoten, tidak masalah dipanggil dua kali).
            if let Some(ws) = sig_ws.get_untracked() {
                let _ = ws.close();
            }
            sig_ws.set(None);

            is_live.set(false);
            room_id.set(String::new());
            viewer_count.set(0);
            join_toast.set(None);
            seen_ids.update_value(|s| s.clear());
            status_text.set("Ready to go live".to_string());
        }
    });

    // Polling info room (jumlah & daftar penonton) selama siaran berlangsung.
    Effect::new(move |_| {
        if !is_live.get() {
            return;
        }
        let rid = room_id.get_untracked();
        if rid.is_empty() {
            return;
        }
        // Ambil sekali segera, lalu tiap 3 detik.
        // Guard in-flight: lewati tick bila request sebelumnya belum selesai
        // (jaringan lambat → cegah tumpukan request paralel ke /api/live/rooms).
        let in_flight: StoredValue<bool> = StoredValue::new(false);
        let fetch = move |rid: String| {
            if in_flight.get_value() {
                return;
            }
            in_flight.set_value(true);
            wasm_bindgen_futures::spawn_local(async move {
                let room = api_get_room(&rid).await;
                in_flight.set_value(false);
                if let Ok(room) = room {
                    viewer_count.set(room.viewer_count as u32);

                    // Deteksi penonton baru → toast "{nama} telah join" 5 detik.
                    let mut newest: Option<String> = None;
                    seen_ids.update_value(|seen| {
                        for v in &room.viewers {
                            if seen.insert(v.id.clone()) {
                                newest = Some(v.name.clone());
                            }
                        }
                    });
                    if let Some(name) = newest {
                        let gen = toast_gen.get_value().wrapping_add(1);
                        toast_gen.set_value(gen);
                        join_toast.set(Some(format!("{name} telah join")));
                        wasm_bindgen_futures::spawn_local(async move {
                            gloo_timers::future::sleep(std::time::Duration::from_secs(5)).await;
                            // Bersihkan hanya jika belum ada toast yang lebih baru.
                            if toast_gen.get_value() == gen {
                                join_toast.set(None);
                            }
                        });
                    }
                }
            });
        };
        fetch(rid.clone());
        let interval = SendWrapper::new(gloo_timers::callback::Interval::new(3_000, move || {
            fetch(rid.clone());
        }));
        on_cleanup(move || drop(interval));
    });

    // BUG FIX #3: on_cleanup agar kamera mati dan room berhenti
    // ketika merchant navigasi pergi tanpa menekan STOP LIVE
    // (misalnya tekan tombol back atau tutup tab).
    on_cleanup(move || {
        if let Some(conn) = pc.get_untracked() {
            conn.set_onicecandidate(None);
            stop_pc_senders(&conn);
            let _ = conn.close();
        }
        ice_closure.set_value(None); // drop closure onicecandidate → memori bebas
        if let Some(stream) = local_stream.get_untracked() {
            stop_all_tracks(&stream);
        }
        // Tutup WS → server memanggil stop_room secara otomatis.
        if let Some(ws) = sig_ws.get_untracked() {
            let _ = ws.close();
        }
        let rid = room_id.get_untracked();
        if !rid.is_empty() {
            wasm_bindgen_futures::spawn_local(async move {
                let _ = api_stop_room(&rid).await;
            });
        }
    });

    let video_ref: NodeRef<leptos::html::Video> = NodeRef::new();

    Effect::new(move |_| {
        if let Some(video) = video_ref.get() {
            // Pasang stream saat ada; bersihkan srcObject saat berhenti supaya
            // elemen video tidak menahan referensi stream (mencegah kebocoran).
            match local_stream.get() {
                Some(stream) => {
                    let _ = video.set_src_object(Some(&stream));
                }
                None => video.set_src_object(None),
            }
        }
    });

    view! {
        <div class="page merchant-live-page" class:is-streaming=move || is_live.get()>
            <header class="mlive-header">
                <div class="mlive-header-left">
                    <A href="/merchant" attr:class="mlive-back-btn" attr:aria-label="Back">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                            <polyline points="15 18 9 12 15 6"/>
                        </svg>
                    </A>
                    <span class="mlive-header-title">"Go Live"</span>
                </div>
                {move || if is_live.get() {
                    view! {
                        <div class="mlive-live-badge">
                            <span class="mlive-live-dot"></span>
                            "LIVE"
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }}
            </header>

            <div class="mlive-preview-area">
                <video
                    node_ref=video_ref
                    class="mlive-video"
                    autoplay=true
                    muted=true
                    playsinline=true
                    poster="/live-poster.svg"
                />
                {move || if !is_live.get() {
                    view! {
                        <div class="mlive-overlay">
                            <svg width="48" height="48" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                <polygon points="23 7 16 12 23 17 23 7"/>
                                <rect x="1" y="5" width="15" height="14" rx="2" ry="2"/>
                            </svg>
                            <p class="mlive-overlay-text">"Preview akan muncul di sini"</p>
                        </div>
                    }.into_any()
                } else {
                    // Overlay gaya streaming app: badge LIVE + jumlah viewer di atas video.
                    view! {
                        <div class="mlive-stream-overlay">
                            <span class="mlive-stream-live">
                                <span class="mlive-live-dot"></span>
                                "LIVE"
                            </span>
                            <span class="mlive-stream-viewers">
                                <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                    <path d="M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2"/>
                                    <circle cx="9" cy="7" r="4"/>
                                    <path d="M23 21v-2a4 4 0 00-3-3.87"/>
                                    <path d="M16 3.13a4 4 0 010 7.75"/>
                                </svg>
                                {move || viewer_count.get()}
                            </span>
                        </div>
                    }.into_any()
                }}

                // Toast sementara: "{nama} telah join" (muncul sekali, 5 detik).
                {move || join_toast.get().map(|msg| view! {
                    <div class="mlive-join-toast">{msg}</div>
                })}
            </div>

            <div class="mlive-stats-row">
                <div class="mlive-stat">
                    <span class="mlive-stat-label">"VIEWERS"</span>
                    <span class="mlive-stat-value">{move || viewer_count.get()}</span>
                </div>
                <div class="mlive-stat">
                    <span class="mlive-stat-label">"STATUS"</span>
                    <span class="mlive-stat-value">{move || status_text.get()}</span>
                </div>
            </div>

            {move || error_msg.get().map(|e| view! {
                <div class="mlive-error">{e}</div>
            })}

            <div class="mlive-actions">
                {move || if is_live.get() {
                    view! {
                        <button
                            class="mlive-btn mlive-btn--stop"
                            on:click=move |_| { stop_live.dispatch(()); }>
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
                                <rect x="6" y="6" width="12" height="12" rx="1"/>
                            </svg>
                            "STOP LIVE"
                        </button>
                    }.into_any()
                } else {
                    view! {
                        <button
                            class="mlive-btn mlive-btn--start"
                            on:click=move |_| { start_live.dispatch(()); }>
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                <polygon points="5 3 19 12 5 21 5 3"/>
                            </svg>
                            "GO LIVE"
                        </button>
                    }.into_any()
                }}
            </div>

            <div class="mlive-tips">
                <h4 class="mlive-tips-title">"Tips Live Selling"</h4>
                <ul class="mlive-tips-list">
                    <li>"Pastikan pencahayaan cukup terang"</li>
                    <li>"Gunakan koneksi WiFi yang stabil"</li>
                    <li>"Interaksi langsung dengan viewer meningkatkan konversi"</li>
                    <li>"Tampilkan QRIS selama live untuk impulse buying"</li>
                </ul>
            </div>
        </div>
        <BottomNav active="merchant" />
    }
}

/// Minta izin kamera/mic via sumber tunggal `rtc::request_camera_mic`. Error
/// dikembalikan sebagai pesan siap-tampil + panduan izin (lihat `MediaError`).
async fn get_user_media() -> Result<web_sys::MediaStream, String> {
    crate::web::rtc::request_camera_mic()
        .await
        .map_err(|e| e.user_message())
}

/// Buat RTCPeerConnection sebagai publisher via WS signaling.
///
/// Perubahan dari HTTP signaling:
/// - Tidak ada `wait_ice_gathering_complete` polling — kandidat ICE dikirim
///   satu per satu saat tersedia (trickle ICE sejati via WS).
/// - Setup koneksi 300–500 ms lebih cepat.
/// - Server otomatis menghentikan room saat WS ditutup (kamera mati / tab ditutup).
///
/// Mengembalikan (RtcPeerConnection, WebSocket) — keduanya harus disimpan oleh
/// pemanggil agar kamera tetap nyala dan WS signaling tetap aktif.
async fn create_publisher(
    room_id: &str,
    stream: Option<web_sys::MediaStream>,
) -> Result<
    (
        web_sys::RtcPeerConnection,
        web_sys::WebSocket,
        Closure<dyn FnMut(web_sys::RtcPeerConnectionIceEvent)>,
    ),
    String,
> {
    let config = web_sys::RtcConfiguration::new();
    config.set_ice_servers(crate::web::rtc::fetch_ice_servers().await.as_ref());

    let pc = web_sys::RtcPeerConnection::new_with_configuration(&config)
        .map_err(|e| format!("Failed to create RTCPeerConnection: {:?}", e))?;

    // Lampirkan tiap track lewat `add_track` (API modern). `add_stream` sudah
    // usang dan di browser baru tidak selalu memicu negosiasi.
    if let Some(stream) = stream {
        let tracks = stream.get_tracks();
        for i in 0..tracks.length() {
            let track: web_sys::MediaStreamTrack = tracks.get(i).unchecked_into();
            let _ = pc.add_track(&track, &stream, &js_sys::Array::new());
        }
    }

    // ── Buka WS signaling SEBELUM createOffer ────────────────────────────────
    // setLocalDescription memulai ICE gathering → kandidat bisa datang segera.
    // Dengan WS sudah terbuka, kandidat langsung terkirim (trickle ICE sejati).
    let ws_url = build_ws_url(&format!("/ws/live/publish/{}", room_id));
    let ws = web_sys::WebSocket::new(&ws_url)
        .map_err(|e| format!("WS gagal dibuka: {:?}", e))?;

    // ── Closures onice/onmessage/onerror: DIPEGANG (bukan .forget()) agar bisa
    // di-detach lalu di-drop → tak bocor per sesi go-live. `on_ice` dikembalikan
    // ke pemanggil (hidup selama sesi); dua closure one-shot dilepas setelah
    // answer diterima. `DetachGuard` melepas SEMUA callback pada jalur error
    // SEBELUM closure di-drop di akhir fungsi (cegah "closure invoked after
    // drop" → panic).
    let ws_ref = ws.clone();
    let on_ice = Closure::<dyn FnMut(web_sys::RtcPeerConnectionIceEvent)>::new(
        move |product: web_sys::RtcPeerConnectionIceEvent| {
            if let Some(candidate) = product.candidate() {
                let msg = serde_json::json!({
                    "type": "candidate",
                    "candidate": candidate.candidate(),
                    "sdp_mid": candidate.sdp_mid().unwrap_or_default(),
                    "sdp_mline_index": candidate.sdp_m_line_index().unwrap_or(0),
                });
                let _ = ws_ref.send_with_str(&msg.to_string());
            }
        },
    );
    pc.set_onicecandidate(Some(on_ice.as_ref().unchecked_ref()));

    // ── Channel untuk menerima answer (pola Rc<Cell<Option<Sender>>> — WASM safe) ─
    let (answer_tx, answer_rx) =
        futures::channel::oneshot::channel::<Result<String, String>>();
    let answer_tx = Rc::new(Cell::new(Some(answer_tx)));

    let on_msg = {
        let tx = answer_tx.clone();
        Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let s: String = txt.into();
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                    let result = if v["type"].as_str() == Some("error") {
                        Err(v["message"].as_str().unwrap_or("Error dari server").to_string())
                    } else {
                        v["sdp"]
                            .as_str()
                            .map(String::from)
                            .ok_or_else(|| "Tidak ada SDP dalam answer".to_string())
                    };
                    if let Some(tx) = tx.take() {
                        let _ = tx.send(result);
                    }
                }
            }
        })
    };
    ws.set_onmessage(Some(on_msg.as_ref().unchecked_ref()));

    let on_err = {
        let tx = answer_tx.clone();
        Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            if let Some(tx) = tx.take() {
                let _ = tx.send(Err("WS error".to_string()));
            }
        })
    };
    ws.set_onerror(Some(on_err.as_ref().unchecked_ref()));

    // Guard: bila fungsi keluar lewat error, lepas SEMUA callback dari pc/ws
    // sebelum closure (on_ice/on_msg/on_err) di-drop di akhir fungsi.
    struct DetachGuard<'a> {
        pc: &'a web_sys::RtcPeerConnection,
        ws: &'a web_sys::WebSocket,
        armed: bool,
    }
    impl Drop for DetachGuard<'_> {
        fn drop(&mut self) {
            if self.armed {
                self.pc.set_onicecandidate(None);
                self.ws.set_onmessage(None);
                self.ws.set_onerror(None);
            }
        }
    }
    let mut detach = DetachGuard {
        pc: &pc,
        ws: &ws,
        armed: true,
    };

    // ── Tunggu WS terbuka (maks 5 detik) ─────────────────────────────────────
    {
        use std::time::Duration;
        for _ in 0..50u8 {
            match ws.ready_state() {
                1 => break,       // OPEN
                2 | 3 => return Err("WS ditutup sebelum terbuka".to_string()),
                _ => gloo_timers::future::sleep(Duration::from_millis(100)).await,
            }
        }
        if ws.ready_state() != 1 {
            return Err("WS koneksi timeout".to_string());
        }
    }

    // ── createOffer → setLocalDescription (ICE gathering dimulai) ────────────
    let offer = wasm_bindgen_futures::JsFuture::from(pc.create_offer())
        .await
        .map_err(|e| format!("createOffer failed: {:?}", e))?;
    let sdp_val = js_sys::Reflect::get(&offer, &JsValue::from_str("sdp"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();

    let desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Offer);
    desc.set_sdp(&sdp_val);
    wasm_bindgen_futures::JsFuture::from(pc.set_local_description(&desc))
        .await
        .map_err(|e| format!("setLocalDescription failed: {:?}", e))?;

    // Ambil SDP lokal saat ini (dengan kandidat yang sudah terkumpul sebelum yield).
    let local_sdp = pc
        .local_description()
        .map(|d| d.sdp())
        .filter(|s| !s.is_empty())
        .unwrap_or(sdp_val);

    // ── Kirim offer via WS SINKRON ────────────────────────────────────────────
    // JavaScript single-thread: onicecandidate tidak bisa menyela sebelum kita await.
    // Mengirim offer sinkron di sini menjamin server menerima offer lebih dulu.
    let offer_msg = serde_json::json!({ "type": "publish_offer", "sdp": local_sdp });
    ws.send_with_str(&offer_msg.to_string())
        .map_err(|e| format!("WS send gagal: {:?}", e))?;

    // ── Tunggu answer dari server (maks 15 detik) ─────────────────────────────
    let answer_sdp = match futures::future::select(
        Box::pin(answer_rx),
        Box::pin(gloo_timers::future::TimeoutFuture::new(15_000)),
    )
    .await
    {
        futures::future::Either::Left((Ok(v), _)) => v?,
        futures::future::Either::Left((Err(_), _)) => {
            return Err("WS channel ditutup sebelum answer".to_string())
        }
        futures::future::Either::Right(_) => {
            return Err("Server tidak merespons (timeout 15 s)".to_string())
        }
    };

    // ── setRemoteDescription dengan answer SDP ────────────────────────────────
    set_remote_answer(&pc, &answer_sdp).await?;

    // Sukses: `on_ice` tetap terpasang (dikembalikan → hidup selama sesi). Lepas
    // HANYA callback one-shot: answer sudah diterima, dan viewer count datang via
    // HTTP polling /api/live/rooms (bukan onmessage WS ini).
    detach.armed = false;
    drop(detach); // akhiri borrow &pc/&ws sebelum pindah kepemilikan
    ws.set_onmessage(None);
    ws.set_onerror(None);
    // on_msg & on_err di-drop di akhir fungsi (sudah detach) — aman.
    Ok((pc, ws, on_ice))
}

async fn set_remote_answer(
    pc: &web_sys::RtcPeerConnection,
    answer: &str,
) -> Result<(), String> {
    let desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Answer);
    desc.set_sdp(answer);
    wasm_bindgen_futures::JsFuture::from(pc.set_remote_description(&desc))
        .await
        .map_err(|e| format!("setRemoteDescription failed: {:?}", e))?;
    Ok(())
}

fn stop_all_tracks(stream: &web_sys::MediaStream) {
    let tracks = stream.get_tracks();
    for i in 0..tracks.length() {
        let track: web_sys::MediaStreamTrack = tracks.get(i).unchecked_into();
        track.stop();
    }
}

/// Hentikan semua track yang sedang dikirim lewat peer connection. Track sender
/// adalah track kamera/mic aktif; jika tidak di-stop, perangkat tetap menyala
/// (lampu kamera tetap hidup) walau PC sudah ditutup.
fn stop_pc_senders(pc: &web_sys::RtcPeerConnection) {
    let senders = pc.get_senders();
    for i in 0..senders.length() {
        let sender: web_sys::RtcRtpSender = senders.get(i).unchecked_into();
        if let Some(track) = sender.track() {
            track.stop();
        }
    }
}
