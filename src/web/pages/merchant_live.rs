use leptos::prelude::*;
use leptos_router::components::A;
use send_wrapper::SendWrapper;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures;

use crate::web::app::AuthResource;
use crate::web::components::{BottomNav, GridBackground};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SdpBody {
    sdp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SdpResponse {
    sdp: String,
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

async fn api_publish_sdp(room_id: &str, sdp: &str) -> Result<String, String> {
    let url = format!("/api/live/rooms/{}/publish/sdp", room_id);
    let resp = gloo_net::http::Request::post(&url)
        .json(&SdpBody { sdp: sdp.to_string() })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let data: SdpResponse = parse_api_data(&json)?;
    Ok(data.sdp)
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

    let start_live = Action::new_local(move |_: &()| {
        let is_live = is_live;
        let room_id = room_id;
        let status_text = status_text;
        let error_msg = error_msg;
        let pc = pc;
        let local_stream = local_stream;

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
                    error_msg.set(Some(format!("Camera access denied: {e}")));
                    status_text.set("Camera access denied".to_string());
                    let _ = api_stop_room(&room.room_id).await;
                    return;
                }
            };

            // MediaStream adalah handle JS yang murah di-clone; satu salinan untuk
            // preview lokal (signal) dan satu lagi diserahkan ke publisher.
            local_stream.set(Some(SendWrapper::new(stream.clone())));
            status_text.set("Connecting...".to_string());

            let peer_connection = match create_publisher(&room.room_id, Some(stream)).await {
                Ok(p) => p,
                Err(e) => {
                    error_msg.set(Some(e));
                    status_text.set("Connection failed".to_string());
                    let _ = api_stop_room(&room.room_id).await;
                    return;
                }
            };

            pc.set(Some(SendWrapper::new(peer_connection)));
            is_live.set(true);
            status_text.set("LIVE".to_string());
        }
    });

    let stop_live = Action::new_local(move |_: &()| {
        let is_live = is_live;
        let room_id = room_id;
        let pc = pc;
        let local_stream = local_stream;
        let status_text = status_text;
        let viewer_count = viewer_count;
        let join_toast = join_toast;
        let seen_ids = seen_ids;

        async move {
            // Hentikan track yang menempel di peer connection (track aktif kamera),
            // lalu tutup koneksi. Tanpa menghentikan track sender, lampu kamera
            // tetap menyala meski PC ditutup (penyebab "kamera tidak berhenti").
            if let Some(conn) = pc.get_untracked() {
                stop_pc_senders(&conn);
                let _ = conn.close();
            }
            pc.set(None);

            if let Some(stream) = local_stream.get_untracked() {
                stop_all_tracks(&stream);
            }
            local_stream.set(None);

            let rid = room_id.get_untracked();
            if !rid.is_empty() {
                let _ = api_stop_room(&rid).await;
            }

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
        let fetch = move |rid: String| {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(room) = api_get_room(&rid).await {
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
        <GridBackground />
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
                    <li>"Tampilkan ticket QRIS selama live untuk impulse buying"</li>
                </ul>
            </div>
        </div>
        <BottomNav active="merchant" />
    }
}

async fn get_user_media() -> Result<web_sys::MediaStream, String> {
    let window = web_sys::window().ok_or("No window")?;
    let navigator = window.navigator();

    let constraints = web_sys::MediaStreamConstraints::new();
    let video_constraints = web_sys::MediaTrackConstraints::new();

    video_constraints.set_width(&wasm_bindgen::JsValue::from_f64(1280.0));
    video_constraints.set_height(&wasm_bindgen::JsValue::from_f64(720.0));

    constraints.set_audio(&wasm_bindgen::JsValue::TRUE);
    constraints.set_video(&video_constraints.into());

    let promise = navigator
        .media_devices()
        .map_err(|_| "MediaDevices not supported".to_string())?
        .get_user_media_with_constraints(&constraints)
        .map_err(|_| "getUserMedia failed".to_string())?;

    let js_val = wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("Camera access denied: {:?}", e))?;

    Ok(web_sys::MediaStream::from(js_val))
}

/// Bangun daftar ICE server sebagai array berisi objek JS biasa.
/// `serde_wasm_bindgen` (default) menserialisasi map jadi `Map` JS — bukan
/// object — sehingga `RTCIceServer.urls` tak terbaca dan konstruktor
/// RTCPeerConnection menolak ("urls is required"). Maka dibangun manual.
fn ice_servers() -> js_sys::Array {
    let urls = js_sys::Array::new();
    urls.push(&JsValue::from_str("stun:stun.l.google.com:19302"));
    urls.push(&JsValue::from_str("stun:stun1.l.google.com:19302"));
    let server = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&server, &JsValue::from_str("urls"), &urls);
    let servers = js_sys::Array::new();
    servers.push(&server);
    servers
}

/// Tunggu ICE gathering selesai (maks ~4 dtk) supaya kandidat ikut tertanam di
/// dalam SDP offer. Server mengabaikan trickle ICE (str0m 0.20 tak punya parser
/// kandidat publik), jadi koneksi mengandalkan kandidat yang dikirim non-trickle
/// di dalam SDP.
async fn wait_ice_gathering_complete(pc: &web_sys::RtcPeerConnection) {
    use std::time::Duration;
    for _ in 0..40 {
        if pc.ice_gathering_state() == web_sys::RtcIceGatheringState::Complete {
            return;
        }
        gloo_timers::future::sleep(Duration::from_millis(100)).await;
    }
}

async fn create_publisher(
    room_id: &str,
    stream: Option<web_sys::MediaStream>,
) -> Result<web_sys::RtcPeerConnection, String> {
    let config = web_sys::RtcConfiguration::new();
    config.set_ice_servers(ice_servers().as_ref());

    let pc = web_sys::RtcPeerConnection::new_with_configuration(&config)
        .map_err(|e| format!("Failed to create RTCPeerConnection: {:?}", e))?;

    // Lampirkan tiap track lewat `add_track` (API modern). `add_stream` sudah
    // usang dan di browser baru tidak selalu memicu negosiasi — itulah yang
    // membuat alur lama menggantung di "Connecting".
    if let Some(stream) = stream {
        let tracks = stream.get_tracks();
        for i in 0..tracks.length() {
            let track: web_sys::MediaStreamTrack = tracks.get(i).unchecked_into();
            let _ = pc.add_track(&track, &stream, &js_sys::Array::new());
        }
    }

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

    wait_ice_gathering_complete(&pc).await;

    // `local_description` (= JS `localDescription`) mengembalikan offer yang baru
    // di-set, lengkap dengan kandidat. `current_local_description` masih null
    // sampai answer diterapkan — penyebab bug "Connecting" sebelumnya.
    let local_sdp = pc
        .local_description()
        .map(|d| d.sdp())
        .filter(|s| !s.is_empty())
        .unwrap_or(sdp_val);

    let answer_sdp = api_publish_sdp(room_id, &local_sdp).await?;
    set_remote_answer(&pc, &answer_sdp).await?;

    Ok(pc)
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