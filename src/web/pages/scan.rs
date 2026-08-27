//! scan.rs — Merchant QR ticket scanner (SSR shell + WASM camera).

use leptos::html::Video;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::web::api::scan_ticket;
use crate::web::hooks::ThemeToggle;
use crate::web::models::ScanValidateResult;

#[cfg(target_arch = "wasm32")]
use send_wrapper::SendWrapper;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::{JsCast, JsValue};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;

// ── Scan result enum ──────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum ScanResult {
    None,
    Valid(ScanValidateResult),
    AlreadyUsed(String),
    Invalid(String),
    Err(String),
}

// ── Main page ─────────────────────────────────────────────────────────────────

#[component]
pub fn ScanPage() -> impl IntoView {
    let manual_input  = RwSignal::new(String::new());
    let validating    = RwSignal::new(false);
    let scan_result   = RwSignal::new(ScanResult::None);
    let camera_active = RwSignal::new(false);
    let video_ref      = NodeRef::<Video>::new();

    // Live MediaStream — kept here so stop_scan() can release camera tracks
    // immediately instead of waiting for the detect loop to notice.
    #[cfg(target_arch = "wasm32")]
    let media_stream: StoredValue<Option<SendWrapper<web_sys::MediaStream>>> =
        StoredValue::new(None);

    #[cfg(target_arch = "wasm32")]
    let release_camera = move || {
        if let Some(stream) = media_stream.get_value() {
            let tracks = stream.get_tracks();
            for i in 0..tracks.length() {
                if let Ok(track) = tracks.get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                    track.stop();
                }
            }
        }
        media_stream.set_value(None);
    };

    // ── Validate ticket code (from manual input or WASM camera) ──────────────
    let do_validate = move |code: String| {
        if code.trim().is_empty() { return; }
        validating.set(true);
        scan_result.set(ScanResult::None);
        spawn_local(async move {
            match scan_ticket(code).await {
                Ok(r) => {
                    let res = if r.status.to_uppercase().contains("USED") {
                        ScanResult::AlreadyUsed(format!("{} — {}", r.event_title, r.tier_name))
                    } else {
                        ScanResult::Valid(r)
                    };
                    scan_result.set(res);
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("not found") || msg.contains("invalid") || msg.contains("INVALID") {
                        scan_result.set(ScanResult::Invalid(msg));
                    } else if msg.contains("already") || msg.contains("USED") {
                        scan_result.set(ScanResult::AlreadyUsed(msg));
                    } else {
                        scan_result.set(ScanResult::Err(msg));
                    }
                }
            }
            validating.set(false);
        });
    };

    let do_validate_manual = {
        let do_validate = do_validate.clone();
        move || { do_validate(manual_input.get_untracked()); }
    };

    let scan_again = move || {
        scan_result.set(ScanResult::None);
        manual_input.set(String::new());
    };

    // ── Start camera (WASM only) ─────────────────────────────────────────────
    // Flow: getUserMedia → attach stream ke <video> → feature-detect window.BarcodeDetector
    // → poll detect(video) setiap 350ms selama camera_active & belum ada hasil.
    #[cfg(target_arch = "wasm32")]
    let start_scan = {
        let do_validate = do_validate.clone();
        move || {
            camera_active.set(true);
            scan_result.set(ScanResult::None);
            let do_validate = do_validate.clone();

            spawn_local(async move {
                let Some(win) = web_sys::window() else { camera_active.set(false); return; };

                let constraints = web_sys::MediaStreamConstraints::new();
                let track_constraints = web_sys::MediaTrackConstraints::new();
                track_constraints.set_facing_mode(&JsValue::from_str("environment"));
                constraints.set_video(&track_constraints);

                let media_devices = match win.navigator().media_devices() {
                    Ok(md) => md,
                    Err(_) => {
                        scan_result.set(ScanResult::Err("Browser tidak mendukung akses kamera.".into()));
                        camera_active.set(false);
                        return;
                    }
                };

                let promise = match media_devices.get_user_media_with_constraints(&constraints) {
                    Ok(p) => p,
                    Err(_) => {
                        scan_result.set(ScanResult::Err("Gagal meminta akses kamera.".into()));
                        camera_active.set(false);
                        return;
                    }
                };

                let stream_val = match JsFuture::from(promise).await {
                    Ok(v) => v,
                    Err(_) => {
                        scan_result.set(ScanResult::Err("Izin kamera ditolak.".into()));
                        camera_active.set(false);
                        return;
                    }
                };
                let stream: web_sys::MediaStream = stream_val.unchecked_into();

                let Some(video) = video_ref.get_untracked() else {
                    for i in 0..stream.get_tracks().length() {
                        if let Ok(t) = stream.get_tracks().get(i).dyn_into::<web_sys::MediaStreamTrack>() {
                            t.stop();
                        }
                    }
                    camera_active.set(false);
                    return;
                };
                video.set_src_object(Some(&stream));
                let _ = video.play();
                media_stream.set_value(Some(SendWrapper::new(stream)));

                // ── Feature-detect BarcodeDetector (Chrome/Edge/Android WebView) ──
                let bd_ctor = js_sys::Reflect::get(&win, &JsValue::from_str("BarcodeDetector"))
                    .ok()
                    .filter(|v| !v.is_undefined());

                let Some(bd_ctor) = bd_ctor else {
                    scan_result.set(ScanResult::Err(
                        "Browser ini belum mendukung pemindaian QR otomatis. \
                         Gunakan input manual di bawah.".into(),
                    ));
                    release_camera();
                    camera_active.set(false);
                    return;
                };

                let ctor_fn: js_sys::Function = bd_ctor.unchecked_into();
                let opts = js_sys::Object::new();
                let formats = js_sys::Array::new();
                formats.push(&JsValue::from_str("qr_code"));
                let _ = js_sys::Reflect::set(&opts, &JsValue::from_str("formats"), &formats);

                let detector = match js_sys::Reflect::construct(&ctor_fn, &js_sys::Array::of1(&opts)) {
                    Ok(d) => d,
                    Err(_) => {
                        scan_result.set(ScanResult::Err("Gagal menginisialisasi pemindai QR.".into()));
                        release_camera();
                        camera_active.set(false);
                        return;
                    }
                };

                let detect_fn: js_sys::Function = match js_sys::Reflect::get(&detector, &JsValue::from_str("detect")) {
                    Ok(f) => f.unchecked_into(),
                    Err(_) => {
                        release_camera();
                        camera_active.set(false);
                        return;
                    }
                };

                // ── Polling loop: detect on the live <video> frame ────────────────
                loop {
                    if !camera_active.get_untracked() {
                        break;
                    }
                    if validating.get_untracked() || scan_result.get_untracked() != ScanResult::None {
                        gloo_timers::future::TimeoutFuture::new(350).await;
                        continue;
                    }

                    if let Ok(promise) = detect_fn.call1(&detector, video.as_ref()) {
                        let promise: js_sys::Promise = promise.unchecked_into();
                        if let Ok(result) = JsFuture::from(promise).await {
                            let arr: js_sys::Array = result.unchecked_into();
                            if arr.length() > 0 {
                                if let Ok(raw) = js_sys::Reflect::get(&arr.get(0), &JsValue::from_str("rawValue")) {
                                    if let Some(code) = raw.as_string() {
                                        if !code.is_empty() {
                                            do_validate(code);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    gloo_timers::future::TimeoutFuture::new(350).await;
                }

                release_camera();
            });
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    let start_scan = move || { camera_active.set(true); };

    #[cfg(target_arch = "wasm32")]
    let stop_scan = move || {
        camera_active.set(false);
        release_camera();
    };

    #[cfg(not(target_arch = "wasm32"))]
    let stop_scan = move || { camera_active.set(false); };

    #[cfg(target_arch = "wasm32")]
    on_cleanup(move || {
        if camera_active.get_untracked() {
            release_camera();
        }
    });

    view! {
        <div class="page scan-page">
            <header class="scan-header">
                <A href="/merchant" attr:class="chat-back-btn" attr:aria-label="Kembali">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <div class="scan-header-info">
                    <span class="scan-header-title">"PINDAI KODE AMBIL"</span>
                    <span class="scan-header-sub">"Pindai kode pengambilan milik pembeli"</span>
                </div>
                <ThemeToggle/>
            </header>

            <div class="scan-body">

                // ── Camera viewfinder ─────────────────────────────────────────
                <div class="scan-viewfinder-wrap">
                    <div class="scan-viewfinder"
                         class:scan-viewfinder--active=move || camera_active.get()>

                        <video class="scan-video"
                               class:scan-video--active=move || camera_active.get()
                               node_ref=video_ref
                               playsinline=true
                               muted=true></video>

                        {move || if camera_active.get() {
                            view! {
                                <div class="scan-frame">
                                    <div class="scan-frame-corner scan-frame-corner--tl"></div>
                                    <div class="scan-frame-corner scan-frame-corner--tr"></div>
                                    <div class="scan-frame-corner scan-frame-corner--bl"></div>
                                    <div class="scan-frame-corner scan-frame-corner--br"></div>
                                    {move || validating.get().then(|| view! {
                                        <div class="scan-line"></div>
                                    })}
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="scan-placeholder">
                                    <svg width="64" height="64" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="1.2"
                                         stroke-linecap="round" opacity="0.35">
                                        <rect x="3"  y="3"  width="7" height="7" rx="1"/>
                                        <rect x="14" y="3"  width="7" height="7" rx="1"/>
                                        <rect x="3"  y="14" width="7" height="7" rx="1"/>
                                        <circle cx="17.5" cy="17.5" r="2.5"/>
                                    </svg>
                                    <p class="scan-placeholder-text">
                                        "Tekan MULAI SCAN untuk membuka kamera"
                                    </p>
                                </div>
                            }.into_any()
                        }}

                        {move || validating.get().then(|| view! {
                            <div class="scan-processing">
                                <div class="scan-spinner"></div>
                                <span>"Memvalidasi..."</span>
                            </div>
                        })}
                    </div>

                    <div class="scan-controls">
                        {move || if !camera_active.get() {
                            view! {
                                <button class="scan-start-btn" on:click=move |_| start_scan()>
                                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2.5"
                                         stroke-linecap="round">
                                        <path d="M23 7l-7 5 7 5V7z"/>
                                        <rect x="1" y="5" width="15" height="14" rx="2" ry="2"/>
                                    </svg>
                                    "MULAI SCAN"
                                </button>
                            }.into_any()
                        } else {
                            view! {
                                <button class="scan-stop-btn" on:click=move |_| stop_scan()>
                                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2.5"
                                         stroke-linecap="round">
                                        <rect x="3" y="3" width="18" height="18" rx="2"/>
                                    </svg>
                                    "STOP"
                                </button>
                            }.into_any()
                        }}
                    </div>
                </div>

                // ── Result card ───────────────────────────────────────────────
                {move || {
                    let result = scan_result.get();
                    match result {
                        ScanResult::None => view! { <span></span> }.into_any(),
                        ScanResult::Valid(r) => view! {
                            <div class="scan-result scan-result--valid">
                                <span class="scan-result-icon">"✅"</span>
                                <div class="scan-result-body">
                                    <p class="scan-result-title">"BOLEH DISERAHKAN"</p>
                                    <p class="scan-result-detail">
                                        {format!("{}\n{}", r.event_title, r.tier_name)}
                                    </p>
                                </div>
                                <button class="scan-again-btn" on:click=move |_| scan_again()>
                                    "SCAN LAGI"
                                </button>
                            </div>
                        }.into_any(),
                        ScanResult::AlreadyUsed(msg) => view! {
                            <div class="scan-result scan-result--used">
                                <span class="scan-result-icon">"⚠️"</span>
                                <div class="scan-result-body">
                                    <p class="scan-result-title">"SUDAH DIGUNAKAN"</p>
                                    <p class="scan-result-detail">{msg}</p>
                                </div>
                                <button class="scan-again-btn" on:click=move |_| scan_again()>
                                    "SCAN LAGI"
                                </button>
                            </div>
                        }.into_any(),
                        ScanResult::Invalid(msg) => view! {
                            <div class="scan-result scan-result--invalid">
                                <span class="scan-result-icon">"❌"</span>
                                <div class="scan-result-body">
                                    <p class="scan-result-title">"KODE TIDAK BERLAKU"</p>
                                    <p class="scan-result-detail">{msg}</p>
                                </div>
                                <button class="scan-again-btn" on:click=move |_| scan_again()>
                                    "SCAN LAGI"
                                </button>
                            </div>
                        }.into_any(),
                        ScanResult::Err(msg) => view! {
                            <div class="scan-result scan-result--invalid">
                                <span class="scan-result-icon">"⚠️"</span>
                                <div class="scan-result-body">
                                    <p class="scan-result-title">"ERROR"</p>
                                    <p class="scan-result-detail">{msg}</p>
                                </div>
                                <button class="scan-again-btn" on:click=move |_| scan_again()>
                                    "COBA LAGI"
                                </button>
                            </div>
                        }.into_any(),
                    }
                }}

                // ── Manual input ──────────────────────────────────────────────
                <div class="scan-manual-wrap">
                    <p class="scan-manual-label">
                        "Atau masukkan kode ambil secara manual"
                    </p>
                    <div class="scan-manual-row">
                        <input
                            type="text"
                            class="scan-manual-input"
                            placeholder="Kode ambil / isi QR..."
                            prop:value=move || manual_input.get()
                            on:input=move |e| manual_input.set(event_target_value(&e))
                            on:keydown=move |e| {
                                if e.key() == "Enter" {
                                    e.prevent_default();
                                    do_validate_manual();
                                }
                            }
                        />
                        <button
                            class="scan-manual-btn"
                            disabled=move || validating.get() || manual_input.get().trim().is_empty()
                            on:click=move |_| do_validate_manual()>
                            {move || if validating.get() { "..." } else { "CEK" }}
                        </button>
                    </div>
                </div>

            </div>
        </div>
    }
}
