use leptos::prelude::*;
use send_wrapper::SendWrapper;
use serde::Deserialize;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

#[derive(Debug, Clone, Deserialize)]
struct RoomInfo {
    room_id: String,
    merchant_name: String,
    viewer_count: usize,
    started_at: i64,
}

async fn api_get_room(room_id: &str) -> Result<RoomInfo, String> {
    let url = format!("/api/live/rooms/{}", room_id);
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    serde_json::from_value(json["data"].clone()).map_err(|e| e.to_string())
}

async fn api_subscribe_sdp(room_id: &str, sdp: &str) -> Result<String, String> {
    let url = format!("/api/live/rooms/{}/subscribe/sdp", room_id);
    let resp = gloo_net::http::Request::post(&url)
        .json(&serde_json::json!({ "sdp": sdp }))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    json["data"]["sdp"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| "No SDP answer".to_string())
}

#[component]
pub fn LiveStreamViewer(room_id: String) -> impl IntoView {
    let is_playing = RwSignal::new(false);
    let viewer_count = RwSignal::new(0u32);
    let merchant_name = RwSignal::new(String::new());
    let error_msg = RwSignal::new(None::<String>);
    let pc: RwSignal<Option<SendWrapper<web_sys::RtcPeerConnection>>> = RwSignal::new(None);
    let video_ref: NodeRef<leptos::html::Video> = NodeRef::new();

    let connect = Action::new_local(move |_: &()| {
        let room_id = room_id.clone();
        let is_playing = is_playing;
        let viewer_count = viewer_count;
        let merchant_name = merchant_name;
        let error_msg = error_msg;
        let pc = pc;
        let video_ref = video_ref.clone();

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
            config.set_ice_servers(&serde_wasm_bindgen::to_value(&serde_json::json!([
                { "urls": ["stun:stun.l.google.com:19302", "stun:stun1.l.google.com:19302"] }
            ])).unwrap());

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

            let on_track = {
                let video_ref = video_ref.clone();
                Closure::<dyn FnMut(web_sys::RtcTrackEvent)>::new(move |event: web_sys::RtcTrackEvent| {
                    if let Some(video) = video_ref.get() {
                        let stream: web_sys::MediaStream = event.streams().get(0).unchecked_into();
                        video.set_src_object(Some(&stream));
                    }
                })
            };
            peer_connection.set_ontrack(Some(on_track.as_ref().unchecked_ref()));
            on_track.forget();

            let rid = room_id.clone();
            let on_ice_candidate = Closure::<dyn FnMut(web_sys::RtcIceCandidate)>::new(move |candidate: web_sys::RtcIceCandidate| {
                let rid = rid.clone();
                let cand = candidate.candidate();
                let mid = candidate.sdp_mid().unwrap_or_default();
                let idx = candidate.sdp_m_line_index().unwrap_or(0);

                let url = format!("/api/live/rooms/{}/subscribe/ice", rid);
                wasm_bindgen_futures::spawn_local(async move {
                    let _ = gloo_net::http::Request::post(&url)
                        .json(&serde_json::json!({
                            "candidate": cand,
                            "sdp_mid": mid,
                            "sdp_mline_index": idx
                        }))
                        .ok()
                        .unwrap()
                        .send()
                        .await;
                });
            });
            peer_connection.set_onicecandidate(Some(on_ice_candidate.as_ref().unchecked_ref()));
            on_ice_candidate.forget();

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

            let answer_sdp = match api_subscribe_sdp(&room_id, &sdp_str).await {
                Ok(a) => a,
                Err(e) => {
                    error_msg.set(Some(format!("Subscribe failed: {e}")));
                    return;
                }
            };

            let answer_desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Answer);
            answer_desc.set_sdp(&answer_sdp);

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

    let disconnect = Action::new_local(move |_: &()| {
        let pc = pc;
        let is_playing = is_playing;

        async move {
            if let Some(mut conn) = pc.get_untracked() {
                let _ = conn.close();
            }
            pc.set(None);
            is_playing.set(false);
        }
    });

    on_cleanup(move || {
        if let Some(mut conn) = pc.get_untracked() {
            let _ = conn.close();
        }
    });

    view! {
        <div class="live-viewer">
            <div class="live-viewer-header">
                {move || if !merchant_name.get().is_empty() {
                    view! {
                        <span class="live-viewer-merchant">{move || merchant_name.get()}</span>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
                {move || if is_playing.get() {
                    view! {
                        <span class="live-viewer-badge">
                            <span class="live-viewer-dot"></span>
                            "LIVE"
                        </span>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
            </div>

            <div class="live-viewer-video-wrap">
                <video
                    node_ref=video_ref
                    class="live-viewer-video"
                    autoplay=true
                    playsinline=true
                    poster="/live-poster.svg"
                />
                {move || if !is_playing.get() {
                    view! {
                        <div class="live-viewer-overlay">
                            <p>"Click to join the live stream"</p>
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }}
            </div>

            <div class="live-viewer-controls">
                <span class="live-viewer-viewers">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2"/>
                        <circle cx="9" cy="7" r="4"/>
                        <path d="M23 21v-2a4 4 0 00-3-3.87"/>
                        <path d="M16 3.13a4 4 0 010 7.75"/>
                    </svg>
                    {move || format!("{}", viewer_count.get())}
                </span>

                {move || if is_playing.get() {
                    view! {
                        <button class="live-viewer-btn live-viewer-btn--leave"
                                on:click=move |_| { disconnect.dispatch(()); }>
                            "Leave"
                        </button>
                    }.into_any()
                } else {
                    view! {
                        <button class="live-viewer-btn live-viewer-btn--join"
                                on:click=move |_| { connect.dispatch(()); }>
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                <polygon points="5 3 19 12 5 21 5 3"/>
                            </svg>
                            "Join Live"
                        </button>
                    }.into_any()
                }}
            </div>

            {move || error_msg.get().map(|e| view! {
                <div class="live-viewer-error">{e}</div>
            })}
        </div>
    }
}