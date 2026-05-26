//! Merchant Ticket Scanner — /merchant/scan
//!
//! Arsitektur worker-based:
//!   Main thread  → capture ImageBitmap dari <video>, postMessage ke Worker
//!   Worker       → BarcodeDetector.detect(ImageBitmap), kirim hasil kembali
//!   Main thread  → terima hasil, panggil /tickets/validate
//!
//! Keunggulan:
//!   - UI Leptos bebas dari CPU spike saat decoding barcode
//!   - FPS kamera stabil di device low-end
//!   - Fallback ke input manual jika Worker / BarcodeDetector tidak tersedia
//!
//! Perbaikan vs versi awal (lihat komentar [FIX #N]):
//!   [FIX #1] Lock protocol dua arah — kirim { type: "lock" } ke Worker
//!            segera setelah result diterima, { type: "unlock" } setelah
//!            validate selesai. Deterministic, tidak race-prone.
//!   [FIX #7] interval_cb_store — Closure interval disimpan di StoredValue,
//!            bukan .forget(). Cleanup eksplisit, tidak orphan saat navigasi cepat.
//!   [FIX #8] show_cooldown — overlay visual 1.5 detik setelah scan VALID,
//!            kamera tetap hidup, operator tidak perlu klik SCAN LAGI tiap tiket.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos::wasm_bindgen::prelude::*;
use leptos::wasm_bindgen::JsCast;
use leptos_router::components::A;
use wasm_bindgen_futures::JsFuture;

use crate::csr::services::scan::{self, ValidateResponse};

// ─── Kamera — murni web_sys, tanpa inline_js ─────────────────────────────────
//
// getUserMedia, MediaStream, srcObject, play(), dan createImageBitmap
// semuanya tersedia via web_sys. Tidak ada inline JS diperlukan.
// Akses kamera tetap native browser — ini memang harus native (WebRTC/getUserMedia).

/// Buka kamera environment, set ke <video>, return MediaStream.
/// Ini adalah satu-satunya fungsi yang boleh akses browser native kamera.
async fn start_camera(video: &web_sys::HtmlVideoElement) -> Result<JsValue, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let navigator = window.navigator();
    let media_devices = navigator.media_devices()?;

    // Buat constraints: video environment, 1280×720 ideal
    let constraints = web_sys::MediaStreamConstraints::new();
    let video_constraints = web_sys::js_sys::Object::new();
    web_sys::js_sys::Reflect::set(
        &video_constraints,
        &"facingMode".into(),
        &"environment".into(),
    )?;

    let ideal_width = web_sys::js_sys::Object::new();
    web_sys::js_sys::Reflect::set(&ideal_width, &"ideal".into(), &1280_f64.into())?;
    web_sys::js_sys::Reflect::set(&video_constraints, &"width".into(), &ideal_width)?;

    let ideal_height = web_sys::js_sys::Object::new();
    web_sys::js_sys::Reflect::set(&ideal_height, &"ideal".into(), &720_f64.into())?;
    web_sys::js_sys::Reflect::set(&video_constraints, &"height".into(), &ideal_height)?;

    constraints.set_video(&video_constraints.into());

    let stream_promise = media_devices.get_user_media_with_constraints(&constraints)?;
    let stream_val = JsFuture::from(stream_promise).await?;
    let stream: web_sys::MediaStream = stream_val.clone().unchecked_into();

    // srcObject tidak ada setter di web_sys stable; pakai Reflect
    web_sys::js_sys::Reflect::set(video, &"srcObject".into(), &stream)?;

    // play() → Promise
    let play_promise = video.play()?;
    JsFuture::from(play_promise).await?;

    Ok(stream_val)
}

/// Hentikan semua track di MediaStream (mematikan kamera).
fn stop_camera(stream: &JsValue) {
    if stream.is_null() || stream.is_undefined() {
        return;
    }
    let ms: web_sys::MediaStream = stream.clone().unchecked_into();
    let tracks = ms.get_tracks();
    for i in 0..tracks.length() {
        if let Some(track) = tracks.get(i).dyn_ref::<web_sys::MediaStreamTrack>() {
            track.stop();
        }
    }
}

/// Capture frame dari <video> sebagai ImageBitmap (Promise).
/// createImageBitmap adalah Web API murni — tersedia via web_sys::window().
fn capture_frame(video: &web_sys::HtmlVideoElement) -> Result<web_sys::js_sys::Promise, JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    // createImageBitmap(video) → Promise<ImageBitmap>
    window.create_image_bitmap_with_html_video_element(video)
}

/// Cek apakah BarcodeDetector API tersedia di browser ini.
/// Implementasi murni via web_sys Reflect — tidak butuh inline JS.
fn is_barcode_detector_supported() -> bool {
    web_sys::window()
        .map(|win| {
            web_sys::js_sys::Reflect::has(&win.into(), &"BarcodeDetector".into()).unwrap_or(false)
        })
        .unwrap_or(false)
}

// ─── JS interop — Worker ─────────────────────────────────────────────────────

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = Worker)]
    type JsWorker;

    #[wasm_bindgen(constructor, js_class = "Worker")]
    fn new(script: &str) -> JsWorker;

    #[wasm_bindgen(method, js_name = postMessage)]
    fn post_message(this: &JsWorker, data: &JsValue);

    #[wasm_bindgen(method, js_name = postMessage)]
    fn post_message_with_transfer(this: &JsWorker, data: &JsValue, transfer: &JsValue);

    #[wasm_bindgen(method, setter, js_name = onmessage)]
    fn set_onmessage(this: &JsWorker, cb: &Closure<dyn FnMut(web_sys::MessageEvent)>);

    #[wasm_bindgen(method, setter, js_name = onerror)]
    fn set_onerror(this: &JsWorker, cb: &Closure<dyn FnMut(web_sys::ErrorEvent)>);

    #[wasm_bindgen(method)]
    fn terminate(this: &JsWorker);
}

// ─── Helper: post pesan { type: T } ke Worker ────────────────────────────────

fn worker_send_type(worker_val: &JsValue, msg_type: &str) {
    let w: &JsWorker = worker_val.unchecked_ref();
    let msg = web_sys::js_sys::Object::new();
    web_sys::js_sys::Reflect::set(&msg, &"type".into(), &msg_type.into()).ok();
    w.post_message(&msg.into());
}

// ─── Result state ────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum ScanResult {
    None,
    Valid(ValidateResponse),
    AlreadyUsed(ValidateResponse),
    Invalid(String),
    Error(String),
}

impl ScanResult {
    fn from_response(r: ValidateResponse) -> Self {
        match r.status.as_str() {
            "VALID" => ScanResult::Valid(r),
            "ALREADY_USED" => ScanResult::AlreadyUsed(r),
            _ => ScanResult::Invalid(r.message.clone()),
        }
    }
}

// ─── Helper: baca field string dari MessageEvent.data ────────────────────────

fn msg_field(data: &JsValue, key: &str) -> Option<String> {
    web_sys::js_sys::Reflect::get(data, &key.into())
        .ok()
        .and_then(|v| v.as_string())
}

// ─── Komponen ────────────────────────────────────────────────────────────────

#[component]
pub fn ScanPage() -> impl IntoView {
    let video_ref = NodeRef::<leptos::html::Video>::new();
    let camera_active = RwSignal::new(false);
    let scanning = RwSignal::new(false);
    let scan_result = RwSignal::new(ScanResult::None);
    let manual_input = RwSignal::new(String::new());
    let validating = RwSignal::new(false);

    // [FIX #8] Overlay "tiket valid" selama cooldown 1.5 detik
    let show_cooldown = RwSignal::new(false);

    // ── Penyimpanan resource antar-closure ────────────────────────────────────
    let stream_store: StoredValue<Option<JsValue>> = StoredValue::new(None);
    let interval_store: StoredValue<Option<i32>> = StoredValue::new(None);
    // [FIX #7] Simpan Closure interval secara eksplisit — tidak ada .forget()
    let interval_cb_store: StoredValue<Option<JsValue>> = StoredValue::new(None);
    let worker_store: StoredValue<Option<JsValue>> = StoredValue::new(None);
    let worker_msg_cb: StoredValue<Option<JsValue>> = StoredValue::new(None);
    let worker_err_cb: StoredValue<Option<JsValue>> = StoredValue::new(None);

    let supported = is_barcode_detector_supported();

    // ── Helper: hapus interval polling ───────────────────────────────────────
    // [FIX #7] Drop Closure lebih dulu — tidak ada callback ke closure dangling.
    let clear_interval = move || {
        interval_cb_store.set_value(None); // drop Closure → JS GC bisa klaim
        if let Some(id) = interval_store.get_value() {
            web_sys::window().unwrap().clear_interval_with_handle(id);
            interval_store.set_value(None);
        }
    };

    // ── Helper: hentikan stream kamera ───────────────────────────────────────
    let release_stream = move || {
        if let Some(stream) = stream_store.get_value() {
            stop_camera(&stream);
            stream_store.set_value(None);
        }
    };

    // ── Helper: bersihkan Worker ──────────────────────────────────────────────
    let cleanup_worker = move || {
        if let Some(w_val) = worker_store.get_value() {
            worker_send_type(&w_val, "terminate");
            let w: &JsWorker = w_val.unchecked_ref();
            w.terminate();
            worker_store.set_value(None);
        }
        worker_msg_cb.set_value(None);
        worker_err_cb.set_value(None);
    };

    // ── Full stop ─────────────────────────────────────────────────────────────
    let stop_scan_full = move || {
        clear_interval();
        release_stream();
        cleanup_worker();
        show_cooldown.set(false);
        camera_active.set(false);
        scanning.set(false);
    };

    // ── Inisialisasi Worker ───────────────────────────────────────────────────
    let init_worker = move || {
        let worker = JsWorker::new("/scan_worker.js");

        let msg_cb =
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |ev: web_sys::MessageEvent| {
                let data = ev.data();
                let Some(typ) = msg_field(&data, "type") else {
                    return;
                };

                match typ.as_str() {
                    "ready" => {
                        let native = web_sys::js_sys::Reflect::get(&data, &"nativeSupport".into())
                            .ok()
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        web_sys::console::log_1(
                            &format!("[scan_worker] ready, nativeSupport={native}").into(),
                        );
                    }

                    "result" => {
                        let Some(code) = msg_field(&data, "value") else {
                            return;
                        };
                        if validating.get_untracked() {
                            return;
                        }

                        // [FIX #1] Kirim "lock" ke Worker sebelum mulai validasi.
                        // Worker sudah set locked=true dari sisinya sendiri,
                        // ini adalah konfirmasi resmi dari main thread.
                        if let Some(w) = worker_store.get_value() {
                            worker_send_type(&w, "lock");
                        }

                        clear_interval();
                        validating.set(true);

                        spawn_local(async move {
                            match scan::validate_ticket(&code).await {
                                Ok(r) => {
                                    let result = ScanResult::from_response(r);
                                    let is_valid = matches!(result, ScanResult::Valid(_));
                                    scan_result.set(result);

                                    // [FIX #1] Kirim unlock terlepas dari apapun hasilnya
                                    if let Some(w) = worker_store.get_value() {
                                        worker_send_type(&w, "unlock");
                                    }

                                    // [FIX #8] Tiket VALID + kamera masih hidup:
                                    // tampilkan cooldown 1.5 detik lalu reset untuk scan berikutnya
                                    if is_valid && camera_active.get_untracked() {
                                        show_cooldown.set(true);
                                        let win = web_sys::window().unwrap();
                                        // Simpan Closure di StoredValue sementara (cooldown 1.5s)
                                        // JsValue impl Send+Sync → bisa masuk StoredValue
                                        let cooldown_jv: StoredValue<Option<JsValue>> =
                                            StoredValue::new(None);
                                        let cjv = cooldown_jv;
                                        let cb = Closure::<dyn Fn()>::new(move || {
                                            cjv.set_value(None); // drop JsValue → GC klaim
                                            show_cooldown.set(false);
                                            scan_result.set(ScanResult::None);
                                            scanning.set(true);
                                        });
                                        win.set_timeout_with_callback_and_timeout_and_arguments_0(
                                            cb.as_ref().unchecked_ref(),
                                            1500,
                                        )
                                        .ok();
                                        cooldown_jv.set_value(Some(cb.into_js_value()));
                                    } else {
                                        scanning.set(false);
                                    }
                                }
                                Err(e) => {
                                    scan_result.set(ScanResult::Error(e.message));
                                    // [FIX #1] Worker tidak boleh stuck meski validate error
                                    if let Some(w) = worker_store.get_value() {
                                        worker_send_type(&w, "unlock");
                                    }
                                    scanning.set(false);
                                }
                            }
                            validating.set(false);
                        });
                    }

                    "error" => {
                        let msg = msg_field(&data, "message")
                            .unwrap_or_else(|| "Worker scan error".into());
                        web_sys::console::warn_1(&msg.into());
                    }

                    "idle" => { /* tidak ada barcode di frame ini; lanjut polling */ }

                    _ => {}
                }
            });

        let err_cb =
            Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |ev: web_sys::ErrorEvent| {
                web_sys::console::error_1(&ev.message().into());
                scan_result.set(ScanResult::Error(format!("Worker error: {}", ev.message())));
                stop_scan_full();
            });

        worker.set_onmessage(&msg_cb);
        worker.set_onerror(&err_cb);

        worker_msg_cb.set_value(Some(msg_cb.into_js_value()));
        worker_err_cb.set_value(Some(err_cb.into_js_value()));
        worker_store.set_value(Some(worker.into()));

        if let Some(w) = worker_store.get_value() {
            worker_send_type(&w, "init");
        }
    };

    // ── Capture frame dan kirim ke Worker ────────────────────────────────────
    // Satu-satunya throttle: setInterval 250ms di main thread.
    // Worker adalah stateless processor — tidak ada throttle di sisinya.
    let send_frame = move || {
        if validating.get_untracked() {
            return;
        }
        let Some(video) = video_ref.get() else { return };
        let Some(worker_val) = worker_store.get_value() else {
            return;
        };
        let worker_val2 = worker_val.clone();

        let Ok(bitmap_promise) = capture_frame(&video) else {
            return;
        };

        spawn_local(async move {
            match JsFuture::from(bitmap_promise).await {
                Ok(bitmap) => {
                    let w: &JsWorker = worker_val2.unchecked_ref();
                    let msg = web_sys::js_sys::Object::new();
                    web_sys::js_sys::Reflect::set(&msg, &"type".into(), &"scan".into()).ok();
                    web_sys::js_sys::Reflect::set(&msg, &"frame".into(), &bitmap).ok();
                    let transfer = web_sys::js_sys::Array::new();
                    transfer.push(&bitmap);
                    w.post_message_with_transfer(&msg.into(), &transfer.into());
                }
                Err(e) => {
                    // createImageBitmap gagal (video belum siap) — lewati saja
                    web_sys::console::warn_1(&e);
                }
            }
        });
    };

    // ── Start scan ────────────────────────────────────────────────────────────
    let start_scan = move || {
        if camera_active.get_untracked() {
            return;
        }

        scan_result.set(ScanResult::None);
        camera_active.set(true);
        scanning.set(true);

        spawn_local(async move {
            let Some(video) = video_ref.get() else { return };

            match start_camera(&video).await {
                Ok(stream) => {
                    stream_store.set_value(Some(stream));
                    init_worker();

                    // [FIX #7] Simpan Closure di interval_cb_store, bukan .forget()
                    let cb = Closure::<dyn Fn()>::new(move || send_frame());
                    let id = web_sys::window()
                        .unwrap()
                        .set_interval_with_callback_and_timeout_and_arguments_0(
                            cb.as_ref().unchecked_ref(),
                            250,
                        )
                        .unwrap_or(0);

                    // Simpan closure sebelum cek id, agar tidak di-drop prematur
                    interval_cb_store.set_value(Some(cb.into_js_value()));

                    if id != 0 {
                        interval_store.set_value(Some(id));
                    } else {
                        interval_cb_store.set_value(None);
                        release_stream();
                        cleanup_worker();
                        camera_active.set(false);
                        scanning.set(false);
                        scan_result.set(ScanResult::Error("Gagal memulai polling kamera.".into()));
                    }
                }
                Err(_) => {
                    scan_result.set(ScanResult::Error(
                        "Tidak dapat mengakses kamera. Periksa izin browser.".into(),
                    ));
                    camera_active.set(false);
                    scanning.set(false);
                }
            }
        });
    };

    // ── Scan lagi ─────────────────────────────────────────────────────────────
    let scan_again = move || {
        stop_scan_full();
        scan_result.set(ScanResult::None);
        manual_input.set(String::new());
    };

    on_cleanup(move || stop_scan_full());

    // ── Validate manual input ─────────────────────────────────────────────────
    let do_validate_manual = move || {
        let code = manual_input.get_untracked();
        if code.trim().is_empty() || validating.get_untracked() {
            return;
        }

        validating.set(true);
        scan_result.set(ScanResult::None);
        spawn_local(async move {
            match scan::validate_ticket(code.trim()).await {
                Ok(r) => scan_result.set(ScanResult::from_response(r)),
                Err(e) => scan_result.set(ScanResult::Error(e.message)),
            }
            validating.set(false);
        });
    };

    // ── View ──────────────────────────────────────────────────────────────────
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
                    <span class="scan-header-title">"SCAN TIKET"</span>
                    <span class="scan-header-sub">"Scan QR code tiket peserta"</span>
                </div>
            </header>

            <div class="scan-body">

                <div class="scan-viewfinder-wrap">
                    <div class="scan-viewfinder"
                         class:scan-viewfinder--active=move || camera_active.get()>
                        <video
                            node_ref=video_ref
                            class="scan-video"
                            autoplay=true
                            muted=true
                            playsinline=true
                            style=move || if camera_active.get() { "" } else { "display:none" }
                        />

                        {move || if camera_active.get() {
                            view! {
                                <div class="scan-frame">
                                    <div class="scan-frame-corner scan-frame-corner--tl"></div>
                                    <div class="scan-frame-corner scan-frame-corner--tr"></div>
                                    <div class="scan-frame-corner scan-frame-corner--bl"></div>
                                    <div class="scan-frame-corner scan-frame-corner--br"></div>
                                    {move || scanning.get().then(|| view! {
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

                        // Overlay: sedang memvalidasi
                        {move || validating.get().then(|| view! {
                            <div class="scan-processing">
                                <div class="scan-spinner"></div>
                                <span>"Memvalidasi..."</span>
                            </div>
                        })}

                        // [FIX #8] Overlay cooldown — 1.5 detik setelah scan VALID
                        // Kamera tetap hidup; operator langsung arahkan ke tiket berikutnya
                        {move || show_cooldown.get().then(|| view! {
                            <div class="scan-cooldown">
                                <span class="scan-cooldown-icon">"✓"</span>
                                <span class="scan-cooldown-text">"VALID — siap tiket berikutnya"</span>
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
                                <button class="scan-stop-btn" on:click=move |_| stop_scan_full()>
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
                    if result == ScanResult::None {
                        return view! { <span></span> }.into_any();
                    }
                    let (cls, icon, title, detail) = match &result {
                        ScanResult::Valid(r) => (
                            "scan-result scan-result--valid",
                            "✅",
                            "TIKET VALID",
                            format!("{}\n{}\n{}", r.attendee, r.event_title, r.tier_name),
                        ),
                        ScanResult::AlreadyUsed(r) => (
                            "scan-result scan-result--used",
                            "⚠️",
                            "SUDAH DIGUNAKAN",
                            format!("{}\n{}", r.attendee, r.event_title),
                        ),
                        ScanResult::Invalid(msg) => (
                            "scan-result scan-result--invalid",
                            "❌",
                            "TIKET TIDAK VALID",
                            msg.clone(),
                        ),
                        ScanResult::Error(msg) => (
                            "scan-result scan-result--invalid",
                            "⚠️",
                            "ERROR",
                            msg.clone(),
                        ),
                        ScanResult::None => unreachable!(),
                    };
                    view! {
                        <div class=cls>
                            <span class="scan-result-icon">{icon}</span>
                            <div class="scan-result-body">
                                <p class="scan-result-title">{title}</p>
                                <p class="scan-result-detail">{detail}</p>
                            </div>
                            <button class="scan-again-btn" on:click=move |_| scan_again()>
                                "SCAN LAGI"
                            </button>
                        </div>
                    }.into_any()
                }}

                // ── Manual input fallback ─────────────────────────────────────
                <div class="scan-manual-wrap">
                    <p class="scan-manual-label">
                        {if supported {
                            "Atau masukkan kode tiket secara manual"
                        } else {
                            "Browser tidak mendukung scan otomatis — masukkan kode tiket:"
                        }}
                    </p>
                    <div class="scan-manual-row">
                        <input
                            type="text"
                            class="scan-manual-input"
                            placeholder="Kode tiket / QR value..."
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
                            disabled=move || {
                                validating.get() || manual_input.get().trim().is_empty()
                            }
                            on:click=move |_| do_validate_manual()>
                            {move || if validating.get() { "..." } else { "CEK" }}
                        </button>
                    </div>
                </div>

            </div>
        </div>
    }
}
