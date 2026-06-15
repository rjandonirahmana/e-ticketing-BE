//! scan.rs — Merchant QR ticket scanner (SSR shell + WASM camera).

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::web::api::scan_ticket;
use crate::web::hooks::ThemeToggle;
use crate::web::models::ScanValidateResult;

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
    #[cfg(target_arch = "wasm32")]
    let start_scan = {
        let do_validate = do_validate.clone();
        move || {
            camera_active.set(true);
            let do_validate = do_validate.clone();
            spawn_local(async move {
                // Try BarcodeDetector via JS eval if available; fall back to manual
                if let Some(window) = web_sys::window() {
                    let _ = window; // camera setup would go here
                }
                let _ = do_validate; // will be called when barcode is detected
            });
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    let start_scan = move || { camera_active.set(true); };

    let stop_scan = move || { camera_active.set(false); };

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
                <ThemeToggle/>
            </header>

            <div class="scan-body">

                // ── Camera viewfinder ─────────────────────────────────────────
                <div class="scan-viewfinder-wrap">
                    <div class="scan-viewfinder"
                         class:scan-viewfinder--active=move || camera_active.get()>

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
                                    <p class="scan-result-title">"TIKET VALID"</p>
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
                                    <p class="scan-result-title">"TIKET TIDAK VALID"</p>
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
                        "Atau masukkan kode tiket secara manual"
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
