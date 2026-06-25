use leptos::prelude::*;
use send_wrapper::SendWrapper;
use serde::Deserialize;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen::closure::Closure;

#[derive(Debug, Clone, Deserialize)]
struct RoomInfo {
    room_id: String,
    merchant_name: String,
    viewer_count: usize,
    started_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct SubscribeSdpResponse {
    sdp: String,
    subscriber_id: String,
}

async fn api_get_room(room_id: &str) -> Result<RoomInfo, String> {
    let url = format!("/api/live/rooms/{}", room_id);
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    // Respons error berbentuk `{ "error": ... }`; tanpa cek ini, `json["data"]`
    // bernilai null dan deserialisasi gagal dengan pesan menyesatkan.
    if let Some(err) = json.get("error").and_then(|e| e.as_str()) {
        return Err(err.to_string());
    }
    serde_json::from_value(json["data"].clone()).map_err(|e| e.to_string())
}

async fn api_subscribe_sdp(
    room_id: &str,
    sdp: &str,
    viewer_id: Option<String>,
    viewer_name: Option<String>,
) -> Result<SubscribeSdpResponse, String> {
    let url = format!("/api/live/rooms/{}/subscribe/sdp", room_id);
    let resp = gloo_net::http::Request::post(&url)
        .json(&serde_json::json!({
            "sdp": sdp,
            "viewer_id": viewer_id,
            "viewer_name": viewer_name,
        }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let sdp = json["data"]["sdp"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "No SDP answer".to_string())?;

    let subscriber_id = json["data"]["subscriber_id"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "No subscriber_id".to_string())?;

    Ok(SubscribeSdpResponse { sdp, subscriber_id })
}

async fn api_subscribe_ice(
    room_id: &str,
    subscriber_id: &str,
    candidate: &str,
    sdp_mid: &str,
    sdp_mline_index: u16,
) -> Result<(), String> {
    let url = format!("/api/live/rooms/{}/subscribe/ice", room_id);
    let resp = gloo_net::http::Request::post(&url)
        .json(&serde_json::json!({
            "subscriber_id": subscriber_id,
            "candidate": candidate,
            "sdp_mid": sdp_mid,
            "sdp_mline_index": sdp_mline_index
        }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.ok() {
        Ok(())
    } else {
        Err(format!("ICE send failed: {}", resp.status()))
    }
}

async fn api_leave(room_id: &str, subscriber_id: &str) -> Result<(), String> {
    let url = format!("/api/live/rooms/{}/subscribe/{}", room_id, subscriber_id);
    gloo_net::http::Request::delete(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
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

/// Tunggu ICE gathering selesai (maks ~4 dtk) agar kandidat tertanam di SDP.
/// Server mengabaikan trickle ICE, jadi offer harus dikirim non-trickle —
/// tanpa ini koneksi penonton tak pernah terbentuk (video/suara tidak muncul).
async fn wait_ice_gathering_complete(pc: &web_sys::RtcPeerConnection) {
    use std::time::Duration;
    for _ in 0..40 {
        if pc.ice_gathering_state() == web_sys::RtcIceGatheringState::Complete {
            return;
        }
        gloo_timers::future::sleep(Duration::from_millis(100)).await;
    }
}

/// Helper: add a recvonly transceiver using raw JS
/// web-sys 0.3.99 does NOT have `add_transceiver_with_str_and_init`,
/// so we call the JS method directly via Reflect.
fn add_recvonly_transceiver(
    pc: &web_sys::RtcPeerConnection,
    kind: &str,
) -> Result<(), JsValue> {
    let init = js_sys::Object::new();
    js_sys::Reflect::set(&init, &"direction".into(), &"recvonly".into())?;
    let _ = js_sys::Reflect::get(pc.as_ref(), &"addTransceiver".into())?
        .dyn_into::<js_sys::Function>()?
        .call2(pc.as_ref(), &kind.into(), &init)?;
    Ok(())
}

#[component]
pub fn LiveStreamViewer(
    room_id: String,
    /// Bila true, langsung menyambung tanpa menunggu klik (dipakai di feed lives).
    #[prop(optional)]
    autoplay: bool,
) -> impl IntoView {
    // StoredValue (Copy) supaya bisa dipakai di beberapa closure `move`
    // (polling effect, connect, disconnect, on_cleanup) tanpa konflik move.
    let room_id = StoredValue::new(room_id);
    // Identitas penonton (jika login) dikirim ke server saat subscribe agar
    // merchant bisa melihat siapa saja yang join.
    let auth = crate::web::hooks::use_auth();
    let is_playing = RwSignal::new(false);
    let viewer_count = RwSignal::new(0u32);
    let merchant_name = RwSignal::new(String::new());
    let error_msg = RwSignal::new(None::<String>);
    let pc: RwSignal<Option<SendWrapper<web_sys::RtcPeerConnection>>> = RwSignal::new(None);
    // Stream remote yang dirakit dari track yang masuk (audio + video).
    let remote_stream: RwSignal<Option<SendWrapper<web_sys::MediaStream>>> = RwSignal::new(None);
    let video_ref: NodeRef<leptos::html::Video> = NodeRef::new();
    let subscriber_id: RwSignal<Option<String>> = RwSignal::new(None);

    // ── Polling viewer count ──────────────────────────────────────────────
    Effect::new(move |_| {
        if !is_playing.get() {
            return;
        }
        let rid = room_id.get_value();
        let vc = viewer_count;

        // SendWrapper: `Interval` memegang closure non-Send, sedangkan `on_cleanup`
        // menuntut Send+Sync agar tetap type-check di target native (efek hanya
        // benar-benar dijalankan di klien/WASM, jadi tak ada drop lintas-thread).
        let interval = SendWrapper::new(gloo_timers::callback::Interval::new(5_000, move || {
            let rid = rid.clone();
            let vc = vc;
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(room) = api_get_room(&rid).await {
                    vc.set(room.viewer_count as u32);
                }
            });
        }));

        on_cleanup(move || drop(interval));
    });

    let connect = Action::new_local(move |_: &()| {
        let room_id = room_id.get_value();
        let is_playing = is_playing;
        let viewer_count = viewer_count;
        let merchant_name = merchant_name;
        let error_msg = error_msg;
        let pc = pc;
        let video_ref = video_ref.clone();
        let subscriber_id = subscriber_id;
        let profile = auth.user.get_untracked();

        async move {
            error_msg.set(None);

            let room = match api_get_room(&room_id).await {
                Ok(r) => r,
                Err(e) => {
                    error_msg.set(Some(format!("Stream not found: {e}")));
                    return;
                }
            };

            viewer_count.set(room.viewer_count as u32);
            merchant_name.set(room.merchant_name);

            let config = web_sys::RtcConfiguration::new();
            config.set_ice_servers(ice_servers().as_ref());

            let peer_connection = web_sys::RtcPeerConnection::new_with_configuration(&config)
                .map_err(|e| format!("RTCPeerConnection failed: {:?}", e))
                .ok();

            let peer_connection = match peer_connection {
                Some(p) => p,
                None => {
                    error_msg.set(Some("WebRTC not supported".to_string()));
                    return;
                }
            };

            // ── Add recvonly transceivers BEFORE createOffer ──────────────────
            if let Err(e) = add_recvonly_transceiver(&peer_connection, "video") {
                error_msg.set(Some(format!("addTransceiver(video) failed: {:?}", e)));
                return;
            }
            if let Err(e) = add_recvonly_transceiver(&peer_connection, "audio") {
                error_msg.set(Some(format!("addTransceiver(audio) failed: {:?}", e)));
                return;
            }

            // ── ontrack: rakit track masuk → pasang ke elemen video ─────────
            let on_track = {
                let video_ref = video_ref.clone();
                Closure::<dyn FnMut(web_sys::RtcTrackEvent)>::new(move |event: web_sys::RtcTrackEvent| {
                    let streams = event.streams();
                    let stream: web_sys::MediaStream = if streams.length() > 0 {
                        streams.get(0).unchecked_into()
                    } else {
                        // Answer SDP dari str0m sering tanpa msid → streams kosong.
                        // Rakit track audio & video ke satu MediaStream sendiri.
                        let s = match remote_stream.get_untracked() {
                            Some(s) => (*s).clone(),
                            None => match web_sys::MediaStream::new() {
                                Ok(s) => s,
                                Err(_) => return,
                            },
                        };
                        s.add_track(&event.track());
                        s
                    };
                    remote_stream.set(Some(SendWrapper::new(stream.clone())));
                    if let Some(video) = video_ref.get_untracked() {
                        video.set_src_object(Some(&stream));
                        let _ = video.play();
                    }
                })
            };
            peer_connection.set_ontrack(Some(on_track.as_ref().unchecked_ref()));
            on_track.forget();

            // ── onicecandidate: send local ICE candidates to server ─────────
            let rid = room_id.clone();
            let sub_id_store = subscriber_id;
            let on_ice_candidate =
                Closure::<dyn FnMut(web_sys::RtcPeerConnectionIceEvent)>::new(
                    move |event: web_sys::RtcPeerConnectionIceEvent| {
                        if let Some(candidate) = event.candidate() {
                            let rid = rid.clone();
                            let cand = candidate.candidate();
                            let mid = candidate.sdp_mid().unwrap_or_default();
                            let idx = candidate.sdp_m_line_index().unwrap_or(0);
                            let sub_id = sub_id_store.get_untracked();

                            wasm_bindgen_futures::spawn_local(async move {
                                if let Some(sid) = sub_id {
                                    let _ = api_subscribe_ice(
                                        &rid, &sid, &cand, &mid, idx
                                    ).await;
                                }
                            });
                        }
                    },
                );
            peer_connection.set_onicecandidate(Some(on_ice_candidate.as_ref().unchecked_ref()));
            on_ice_candidate.forget();

            // ── Create offer ──────────────────────────────────────────────────
            let offer_promise = peer_connection.create_offer_with_rtc_offer_options(
                &web_sys::RtcOfferOptions::new()
            );
            let offer = match wasm_bindgen_futures::JsFuture::from(offer_promise).await {
                Ok(o) => o,
                Err(e) => {
                    error_msg.set(Some(format!("createOffer failed: {:?}", e)));
                    return;
                }
            };

            let sdp_str = js_sys::Reflect::get(&offer, &wasm_bindgen::JsValue::from_str("sdp"))
                .unwrap()
                .as_string()
                .unwrap_or_default();

            let desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Offer);
            desc.set_sdp(&sdp_str);

            if let Err(e) = wasm_bindgen_futures::JsFuture::from(
                peer_connection.set_local_description(&desc),
            ).await {
                error_msg.set(Some(format!("setLocalDescription failed: {:?}", e)));
                return;
            }

            // Tunggu kandidat ICE masuk ke SDP sebelum dikirim (non-trickle).
            wait_ice_gathering_complete(&peer_connection).await;
            let offer_sdp = peer_connection
                .local_description()
                .map(|d| d.sdp())
                .filter(|s| !s.is_empty())
                .unwrap_or(sdp_str);

            // ── Send offer, receive answer + subscriber_id ──────────────────
            let (viewer_id, viewer_name) = match &profile {
                Some(p) => (Some(p.id.clone()), Some(p.name.clone())),
                None => (None, None),
            };
            let answer = match api_subscribe_sdp(&room_id, &offer_sdp, viewer_id, viewer_name).await {
                Ok(a) => a,
                Err(e) => {
                    error_msg.set(Some(format!("Subscribe failed: {e}")));
                    return;
                }
            };

            subscriber_id.set(Some(answer.subscriber_id.clone()));

            let answer_desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Answer);
            answer_desc.set_sdp(&answer.sdp);

            if let Err(e) = wasm_bindgen_futures::JsFuture::from(
                peer_connection.set_remote_description(&answer_desc),
            ).await {
                error_msg.set(Some(format!("setRemoteDescription failed: {:?}", e)));
                return;
            }

            pc.set(Some(SendWrapper::new(peer_connection)));
            is_playing.set(true);
        }
    });

    // Auto-join sekali saat dipasang (feed lives): langsung connect tanpa tap.
    if autoplay {
        Effect::new(move |prev: Option<()>| {
            if prev.is_none() {
                connect.dispatch(());
            }
        });
    }

    let disconnect = Action::new_local(move |_: &()| {
        let pc = pc;
        let is_playing = is_playing;
        let subscriber_id = subscriber_id;
        let rid = room_id.get_value();

        async move {
            if let Some(mut conn) = pc.get_untracked() {
                let _ = conn.close();
            }
            pc.set(None);
            // Beri tahu server agar viewer count berkurang.
            if let Some(sid) = subscriber_id.get_untracked() {
                let _ = api_leave(&rid, &sid).await;
            }
            subscriber_id.set(None);
            is_playing.set(false);
        }
    });

    on_cleanup(move || {
        if let Some(mut conn) = pc.get_untracked() {
            let _ = conn.close();
        }
        // Navigasi keluar saat masih menonton: lepas slot viewer di server.
        if let Some(sid) = subscriber_id.get_untracked() {
            let rid = room_id.get_value();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = api_leave(&rid, &sid).await;
            });
        }
    });

    view! {
        <div class="live-viewer">
            <div class="live-viewer-header">
                {move || {
                    if !merchant_name.get().is_empty() {
                        view! {
                            <span class="live-viewer-merchant">{move || merchant_name.get()}</span>
                        }
                            .into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}
                {move || {
                    if is_playing.get() {
                        view! {
                            <span class="live-viewer-badge">
                                <span class="live-viewer-dot"></span>
                                "LIVE"
                            </span>
                        }
                            .into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}
            </div>

            <div class="live-viewer-video-wrap">
                <video
                    node_ref=video_ref
                    class="live-viewer-video"
                    autoplay=true
                    playsinline=true
                    muted=false
                    poster="/live-poster.svg"
                />
                {move || {
                    if is_playing.get() {
                        view! { <div></div> }.into_any()
                    } else if connect.pending().get() {
                        view! {
                            <div class="live-viewer-overlay">
                                <span class="live-viewer-spinner"></span>
                                <p>"Menghubungkan..."</p>
                            </div>
                        }
                            .into_any()
                    } else {
                        // Seluruh overlay bisa diklik untuk bergabung.
                        view! {
                            <button
                                class="live-viewer-overlay live-viewer-overlay--btn"
                                on:click=move |_| { connect.dispatch(()); }
                            >
                                <span class="live-viewer-play">
                                    <svg width="22" height="22" viewBox="0 0 24 24"
                                         fill="currentColor">
                                        <polygon points="6 4 20 12 6 20 6 4" />
                                    </svg>
                                </span>
                                <p>"Ketuk untuk menonton siaran langsung"</p>
                            </button>
                        }
                            .into_any()
                    }
                }}
            </div>

            <div class="live-viewer-controls">
                <span class="live-viewer-viewers">
                    <svg
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    >
                        <path d="M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2" />
                        <circle cx="9" cy="7" r="4" />
                        <path d="M23 21v-2a4 4 0 00-3-3.87" />
                        <path d="M16 3.13a4 4 0 010 7.75" />
                    </svg>
                    {move || format!("{}", viewer_count.get())}
                </span>

                {move || {
                    if is_playing.get() {
                        view! {
                            <button
                                class="live-viewer-btn live-viewer-btn--leave"
                                on:click=move |_| {
                                    disconnect.dispatch(());
                                }
                            >
                                "Keluar"
                            </button>
                        }
                            .into_any()
                    } else {
                        view! {
                            <button
                                class="live-viewer-btn live-viewer-btn--join"
                                prop:disabled=move || connect.pending().get()
                                on:click=move |_| {
                                    connect.dispatch(());
                                }
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
                                    <polygon points="5 3 19 12 5 21 5 3" />
                                </svg>
                                {move || if connect.pending().get() { "Menghubungkan" } else { "Tonton" }}
                            </button>
                        }
                            .into_any()
                    }
                }}
            </div>

            {move || error_msg.get().map(|e| view! { <div class="live-viewer-error">{e}</div> })}
        </div>
    }
}