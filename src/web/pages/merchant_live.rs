use leptos::prelude::*;
use leptos_router::components::A;
use send_wrapper::SendWrapper;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures;

use crate::web::app::AuthResource;
use crate::web::components::{BottomNav, GridBackground};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RoomInfo {
    room_id: String,
    merchant_id: String,
    merchant_name: String,
    event_slug: Option<String>,
    viewer_count: usize,
    started_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SdpBody {
    sdp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SdpResponse {
    sdp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IceBody {
    candidate: String,
    sdp_mid: String,
    sdp_mline_index: u32,
}

async fn api_create_room() -> Result<RoomInfo, String> {
    let resp = gloo_net::http::Request::post("/api/live/rooms")
        .json(&serde_json::json!({}))
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    serde_json::from_value(json["data"].clone()).map_err(|e| e.to_string())
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
    let data: SdpResponse = serde_json::from_value(json["data"].clone()).map_err(|e| e.to_string())?;
    Ok(data.sdp)
}

async fn api_publish_ice(room_id: &str, candidate: &str, sdp_mid: &str, sdp_mline_index: u32) -> Result<(), String> {
    let url = format!("/api/live/rooms/{}/publish/ice", room_id);
    let resp = gloo_net::http::Request::post(&url)
        .json(&IceBody {
            candidate: candidate.to_string(),
            sdp_mid: sdp_mid.to_string(),
            sdp_mline_index,
        })
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status() == 200 {
        Ok(())
    } else {
        Err(format!("ICE send failed: {}", resp.status()))
    }
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

        async move {
            if let Some(mut conn) = pc.get_untracked() {
                let _ = conn.close();
            }
            pc.set(None);

            if let Some(mut stream) = local_stream.get_untracked() {
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
            status_text.set("Ready to go live".to_string());
        }
    });

    let video_ref: NodeRef<leptos::html::Video> = NodeRef::new();

    Effect::new(move |_| {
        if let Some(video) = video_ref.get() {
            if let Some(stream) = local_stream.get() {
                let _ = video.set_src_object(Some(&stream));
            }
        }
    });

    view! {
        <GridBackground />
        <div class="page merchant-live-page">
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
                    view! { <div></div> }.into_any()
                }}
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

    let mut constraints = web_sys::MediaStreamConstraints::new();
    let mut video_constraints = web_sys::MediaTrackConstraints::new();

    video_constraints.width(&wasm_bindgen::JsValue::from_f64(1280.0));
    video_constraints.height(&wasm_bindgen::JsValue::from_f64(720.0));

    constraints.audio(&wasm_bindgen::JsValue::TRUE);
    constraints.video(&video_constraints.into());

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

async fn create_publisher(
    room_id: &str,
    stream: Option<web_sys::MediaStream>,
) -> Result<web_sys::RtcPeerConnection, String> {
    let config = web_sys::RtcConfiguration::new();
    config.set_ice_servers(&serde_wasm_bindgen::to_value(&serde_json::json!([
        { "urls": ["stun:stun.l.google.com:19302", "stun:stun1.l.google.com:19302"] }
    ])).unwrap());

    let pc = web_sys::RtcPeerConnection::new_with_configuration(&config)
        .map_err(|e| format!("Failed to create RTCPeerConnection: {:?}", e))?;

    if let Some(stream) = stream {
        let _ = pc.add_stream(&stream);
    }

    let (offer_tx, offer_rx) = futures::channel::oneshot::channel();
    // oneshot::Sender::send mengonsumsi self, sedangkan Closure WASM bertipe FnMut
    // (bisa dipanggil berkali-kali) — bungkus dengan Option agar bisa di-take sekali.
    let mut offer_tx = Some(offer_tx);

    let on_negotiation_needed = Closure::<dyn FnMut()>::new(move || {
        if let Some(tx) = offer_tx.take() {
            let _ = tx.send(());
        }
    });
    pc.set_onnegotiationneeded(Some(on_negotiation_needed.as_ref().unchecked_ref()));
    on_negotiation_needed.forget();

    let room_id_owned = room_id.to_string();
    let pc_clone = pc.clone();
    let on_ice_candidate = Closure::<dyn FnMut(web_sys::RtcIceCandidate)>::new(move |candidate: web_sys::RtcIceCandidate| {
        let rid = room_id_owned.clone();
        let cand = candidate.candidate();
        let mid = candidate.sdp_mid().unwrap_or_default();
        let idx = candidate.sdp_m_line_index().unwrap_or(0) as u32;

        wasm_bindgen_futures::spawn_local(async move {
            let _ = api_publish_ice(&rid, &cand, &mid, idx).await;
        });
    });
    pc_clone.set_onicecandidate(Some(on_ice_candidate.as_ref().unchecked_ref()));
    on_ice_candidate.forget();

    // Wait for ICE gathering to complete, then send the full SDP
    let create_and_send = async {
        let offer_promise = pc.create_offer_with_rtc_offer_options(
            &web_sys::RtcOfferOptions::new()
        );
        let offer = wasm_bindgen_futures::JsFuture::from(offer_promise)
            .await
            .map_err(|e| format!("createOffer failed: {:?}", e))?;

        let sdp_val = js_sys::Reflect::get(&offer, &wasm_bindgen::JsValue::from_str("sdp"))
            .unwrap()
            .as_string()
            .unwrap_or_default();
        let desc = web_sys::RtcSessionDescriptionInit::new(web_sys::RtcSdpType::Offer);
        desc.set_sdp(&sdp_val);
        let set_local = pc.set_local_description(&desc);
        wasm_bindgen_futures::JsFuture::from(set_local)
            .await
            .map_err(|e| format!("setLocalDescription failed: {:?}", e))?;

        let local_sdp = pc.current_local_description()
            .map(|d| d.sdp())
            .unwrap_or_default();

        if local_sdp.is_empty() {
            let _ = offer_rx.await;
            let final_sdp = pc.current_local_description()
                .map(|d| d.sdp())
                .unwrap_or_default();
            let answer_sdp = api_publish_sdp(room_id, &final_sdp).await?;
            set_remote_answer(&pc, &answer_sdp).await?;
        } else {
            let answer_sdp = api_publish_sdp(room_id, &local_sdp).await?;
            set_remote_answer(&pc, &answer_sdp).await?;
        }

        Ok(pc)
    };

    create_and_send.await
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