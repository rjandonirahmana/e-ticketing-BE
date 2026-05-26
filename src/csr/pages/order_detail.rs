/// Order Detail page — redesigned as "Verification" / payment page.
/// Matches the design: ORDER ID, TIME REMAINING countdown, AWAITING PAYMENT badge,
/// QR code card, Total Payment, Save QR to Gallery, How to Pay steps.
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use serde::Deserialize;
use leptos::wasm_bindgen::closure::Closure;
use leptos::wasm_bindgen::JsCast;

use crate::csr::services::client::get_private;
use crate::csr::utils::format_idr;

// ── Deserialize helpers ───────────────────────────────────────────────────────

fn de_f64_or_str<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    use serde::de::{self, Visitor};
    struct F64OrStr;
    impl<'de> Visitor<'de> for F64OrStr {
        type Value = f64;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a number or numeric string")
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> { Ok(v) }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> { Ok(v as f64) }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> { Ok(v as f64) }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
            v.parse::<f64>().map_err(de::Error::custom)
        }
    }
    d.deserialize_any(F64OrStr)
}

#[derive(Debug, Clone, Deserialize)]
struct BeOrderItem {
    #[serde(default)] pub event_name: String,
    #[serde(default)] pub variant_name: String,
    pub quantity: i32,
    #[serde(deserialize_with = "de_f64_or_str")] pub subtotal: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct BeOrderDetail {
    pub id: String,
    #[serde(default)] pub order_code: String,
    #[serde(default)] pub status: String,
    #[serde(deserialize_with = "de_f64_or_str", default)] pub total_amount: f64,
    #[serde(default)] pub payment_method: Option<String>,
    #[serde(default)] pub paid_at: Option<String>,
    #[serde(default)] pub expired_at: Option<String>,
    #[serde(default)] pub created_at: Option<String>,
    #[serde(default)] pub items: Vec<BeOrderItem>,
}

// ── Timer helpers ─────────────────────────────────────────────────────────────

/// Parse ISO datetime string → remaining seconds from now.
fn parse_remaining_secs(expired_at: &str) -> i64 {
    let exp_ms = js_sys::Date::new(&wasm_bindgen::JsValue::from_str(expired_at)).get_time();
    let now_ms = js_sys::Date::now();
    let remaining = ((exp_ms - now_ms) / 1000.0).ceil() as i64;
    remaining.max(0)
}

fn fmt_countdown(secs: i64) -> String {
    if secs <= 0 { return "00:00".to_string(); }
    let m = secs / 60;
    let s = secs % 60;
    format!("{:02}:{:02}", m, s)
}

/// Start a 1-second interval that decrements `remaining` signal.
/// Returns the interval handle so it can be cleared on cleanup.
fn start_interval(remaining: RwSignal<i64>) -> i32 {
    let cb = Closure::<dyn Fn()>::new(move || {
        let v = remaining.get_untracked().saturating_sub(1);
        remaining.set(v);
    });
    let handle = web_sys::window()
        .and_then(|w| {
            w.set_interval_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                1000,
            )
            .ok()
        })
        .unwrap_or(-1);
    cb.forget(); // acceptable leak — component unmounts when user navigates away
    handle
}

// ── Page component ────────────────────────────────────────────────────────────

#[component]
pub fn OrderDetailPage() -> impl IntoView {
    let params = use_params_map();
    let order_id = Memo::new(move |_| {
        params.with(|p| p.get("id").unwrap_or_default().to_string())
    });

    let order: RwSignal<Option<Result<BeOrderDetail, String>>> = RwSignal::new(None);
    let remaining_secs: RwSignal<i64> = RwSignal::new(0);
    let interval_handle: StoredValue<i32> = StoredValue::new(-1);

    // Fetch order on mount / id change
    Effect::new(move |_| {
        let id = order_id.get();
        if id.is_empty() { return; }
        order.set(None);

        spawn_local(async move {
            let path = format!("/orders/{}", id);
            match get_private::<BeOrderDetail>(&path).await {
                Ok(o) => {
                    // Start timer if pending
                    if o.status.to_lowercase() == "pending" {
                        if let Some(exp) = &o.expired_at {
                            let secs = parse_remaining_secs(exp);
                            remaining_secs.set(secs);
                            let h = start_interval(remaining_secs);
                            interval_handle.set_value(h);
                        }
                    }
                    order.set(Some(Ok(o)));
                }
                Err(e) => order.set(Some(Err(e.message))),
            }
        });
    });

    // Clear interval on page unmount
    on_cleanup(move || {
        let h = interval_handle.get_value();
        if h >= 0 {
            if let Some(win) = web_sys::window() {
                win.clear_interval_with_handle(h);
            }
        }
    });

    view! {
        <div class="page vp-page">
            // ── Header ──────────────────────────────────────────────────────
            <header class="page-header vp-header">
                <A href="/orders" attr:class="back-btn">
                    <svg
                        width="22"
                        height="22"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                    >
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </A>
                <span class="vp-header-title">"Verification"</span>
                <div class="header-actions"></div>
            </header>

            {move || match order.get() {
                None => {
                    view! {
                        // Loading state
                        <div class="vp-loading">
                            <div class="vp-shimmer vp-shimmer--meta"></div>
                            <div class="vp-shimmer vp-shimmer--qr"></div>
                            <div class="vp-shimmer vp-shimmer--btn"></div>
                        </div>
                    }
                        .into_any()
                }
                Some(Err(msg)) => {

                    view! {
                        <div class="vp-error">
                            <span class="vp-error-icon">"⚠️"</span>
                            <p class="vp-error-msg">{msg}</p>
                            <A href="/orders" attr:class="vp-back-link">
                                "← Kembali ke Orders"
                            </A>
                        </div>
                    }
                        .into_any()
                }
                Some(Ok(o)) => {
                    let is_pending = o.status.to_lowercase() == "pending";
                    let is_paid = matches!(o.status.to_lowercase().as_str(), "paid" | "completed");
                    let order_code = o.order_code.clone();
                    let total = o.total_amount;
                    let o2 = o.clone();

                    view! {
                        <div class="vp-content">
                            // ── ORDER ID + TIME REMAINING ────────────────────
                            <div class="vp-meta-row">
                                <div class="vp-meta-block">
                                    <span class="vp-meta-label">"ORDER ID"</span>
                                    <span class="vp-meta-val vp-order-code">
                                        {"#"}{order_code.clone()}
                                    </span>
                                </div>
                                {is_pending
                                    .then(move || {
                                        view! {
                                            <div class="vp-meta-block vp-meta-block--right">
                                                <span class="vp-meta-label">"TIME REMAINING"</span>
                                                <span class="vp-timer">
                                                    <svg
                                                        width="14"
                                                        height="14"
                                                        viewBox="0 0 24 24"
                                                        fill="none"
                                                        stroke="currentColor"
                                                        stroke-width="2"
                                                        stroke-linecap="round"
                                                    >
                                                        <circle cx="12" cy="12" r="10" />
                                                        <polyline points="12 6 12 12 16 14" />
                                                    </svg>
                                                    {move || fmt_countdown(remaining_secs.get())}
                                                </span>
                                            </div>
                                        }
                                    })}
                            </div>

                            // ── STATUS BADGE ─────────────────────────────────
                            <div class=move || {
                                if is_pending {
                                    "vp-status-badge vp-status-badge--pending"
                                } else if is_paid {
                                    "vp-status-badge vp-status-badge--paid"
                                } else {
                                    "vp-status-badge vp-status-badge--cancelled"
                                }
                            }>
                                <span class="vp-status-dot"></span>
                                {if is_pending {
                                    "AWAITING PAYMENT"
                                } else if is_paid {
                                    "PAYMENT CONFIRMED"
                                } else {
                                    "CANCELLED"
                                }}
                            </div>

                            // ── QR CARD (only if pending) ─────────────────────
                            {is_pending
                                .then(|| {
                                    view! {
                                        <div class="vp-qr-card">
                                            // QR card header
                                            <div class="vp-qr-card-header">
                                                <div class="vp-qr-logo">
                                                    <svg
                                                        width="20"
                                                        height="20"
                                                        viewBox="0 0 24 24"
                                                        fill="none"
                                                        stroke="var(--accent-blue)"
                                                        stroke-width="2.5"
                                                        stroke-linecap="round"
                                                    >
                                                        <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
                                                    </svg>
                                                </div>
                                                <span class="vp-gpn-label">"GPN NETWORK"</span>
                                            </div>

                                            // QR code container (dark with glow)
                                            <div class="vp-qr-container">
                                                <div class="vp-qr-glow"></div>
                                                <div class="vp-qr-inner">
                                                    // QR SVG placeholder
                                                    <svg
                                                        viewBox="0 0 160 160"
                                                        width="148"
                                                        height="148"
                                                        xmlns="http://www.w3.org/2000/svg"
                                                    >
                                                        <rect width="160" height="160" fill="white" rx="8" />
                                                        // Finder top-left
                                                        <rect x="10" y="10" width="50" height="50" fill="#0d0d1a" />
                                                        <rect x="16" y="16" width="38" height="38" fill="white" />
                                                        <rect x="22" y="22" width="26" height="26" fill="#0d0d1a" />
                                                        // Finder top-right
                                                        <rect
                                                            x="100"
                                                            y="10"
                                                            width="50"
                                                            height="50"
                                                            fill="#0d0d1a"
                                                        />
                                                        <rect x="106" y="16" width="38" height="38" fill="white" />
                                                        <rect
                                                            x="112"
                                                            y="22"
                                                            width="26"
                                                            height="26"
                                                            fill="#0d0d1a"
                                                        />
                                                        // Finder bottom-left
                                                        <rect
                                                            x="10"
                                                            y="100"
                                                            width="50"
                                                            height="50"
                                                            fill="#0d0d1a"
                                                        />
                                                        <rect x="16" y="106" width="38" height="38" fill="white" />
                                                        <rect
                                                            x="22"
                                                            y="112"
                                                            width="26"
                                                            height="26"
                                                            fill="#0d0d1a"
                                                        />
                                                        // Data modules
                                                        <rect x="70" y="10" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="82" y="10" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="70" y="22" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="82" y="34" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="70" y="46" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="10" y="70" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="22" y="82" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="34" y="70" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="46" y="82" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="70" y="70" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="82" y="82" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="94" y="70" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="106" y="70" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="118" y="82" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="130" y="70" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="82" y="94" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="70" y="106" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="94" y="106" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="106" y="118" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="118" y="106" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="130" y="118" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="142" y="106" width="8" height="8" fill="#0d0d1a" />
                                                        <rect x="142" y="130" width="8" height="8" fill="#0d0d1a" />
                                                        // QRIS label
                                                        <rect
                                                            x="58"
                                                            y="68"
                                                            width="44"
                                                            height="24"
                                                            rx="4"
                                                            fill="#4f6bff"
                                                        />
                                                        <text
                                                            x="80"
                                                            y="85"
                                                            text-anchor="middle"
                                                            fill="white"
                                                            font-size="10"
                                                            font-family="monospace"
                                                            font-weight="bold"
                                                        >
                                                            "QRIS"
                                                        </text>
                                                    </svg>
                                                </div>
                                            </div>

                                            <p class="vp-scan-label">"SCAN THIS CODE TO PAY"</p>
                                        </div>
                                    }
                                })}

                            // ── PAID success card ─────────────────────────────
                            {is_paid
                                .then(|| {
                                    let pm = o2.payment_method.clone().unwrap_or_default();
                                    let pa = o2
                                        .paid_at
                                        .as_deref()
                                        .and_then(|s| s.get(..10))
                                        .unwrap_or("—")
                                        .to_string();
                                    view! {
                                        <div class="vp-paid-card">
                                            <div class="vp-paid-icon">
                                                <svg
                                                    width="40"
                                                    height="40"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="var(--accent-lime)"
                                                    stroke-width="2.5"
                                                    stroke-linecap="round"
                                                >
                                                    <path d="M22 11.08V12a10 10 0 11-5.93-9.14" />
                                                    <polyline points="22 4 12 14.01 9 11.01" />
                                                </svg>
                                            </div>
                                            <h3 class="vp-paid-title">"Pembayaran Berhasil"</h3>
                                            {(!pm.is_empty())
                                                .then(move || {
                                                    view! {
                                                        <p class="vp-paid-method">"via "{pm.to_uppercase()}</p>
                                                    }
                                                })}
                                            <p class="vp-paid-date">{pa}</p>
                                            <A href="/tickets" attr:class="vp-view-ticket-btn">
                                                "Lihat E-Ticket"
                                            </A>
                                        </div>
                                    }
                                })}

                            // ── Total Payment ─────────────────────────────────
                            <div class="vp-total-section">
                                <span class="vp-total-label">"Total Payment"</span>
                                <span class="vp-total-amount">{format_idr(total as i64)}</span>
                            </div>

                            // ── Save QR to Gallery (pending only) ─────────────
                            {is_pending
                                .then(|| {
                                    view! {
                                        <button
                                            class="vp-save-btn"
                                            on:click=move |_| {
                                                if let Some(doc) = web_sys::window()
                                                    .and_then(|w| w.document())
                                                {
                                                    if let Ok(el) = doc.create_element("a") {
                                                        let a = el.unchecked_into::<web_sys::HtmlAnchorElement>();
                                                        a.set_href(
                                                            "data:image/svg+xml;charset=utf-8,<svg xmlns='http://www.w3.org/2000/svg' width='200' height='200'><rect width='200' height='200' fill='white'/><text x='50%' y='50%' text-anchor='middle' dominant-baseline='middle' font-size='14'>QR Code</text></svg>",
                                                        );
                                                        a.set_download("qr-payment.svg");
                                                        a.click();
                                                    }
                                                }
                                            }
                                        >
                                            <svg
                                                width="18"
                                                height="18"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2.2"
                                                stroke-linecap="round"
                                            >
                                                <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
                                                <polyline points="7 10 12 15 17 10" />
                                                <line x1="12" y1="15" x2="12" y2="3" />
                                            </svg>
                                            "Save QR to Gallery"
                                        </button>
                                    }
                                })}

                            // ── How to Pay (pending only) ─────────────────────
                            {is_pending
                                .then(|| {
                                    view! {
                                        <div class="vp-howto-card">
                                            <div class="vp-howto-header">
                                                <svg
                                                    width="16"
                                                    height="16"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="currentColor"
                                                    stroke-width="2"
                                                    stroke-linecap="round"
                                                >
                                                    <circle cx="12" cy="12" r="10" />
                                                    <line x1="12" y1="8" x2="12" y2="12" />
                                                    <line x1="12" y1="16" x2="12.01" y2="16" />
                                                </svg>
                                                <span class="vp-howto-title">"How to Pay"</span>
                                            </div>
                                            <div class="vp-howto-steps">
                                                <div class="vp-step">
                                                    <span class="vp-step-num">"1"</span>
                                                    <p class="vp-step-text">
                                                        "Open your mobile banking or digital wallet app "
                                                        <span class="vp-step-italic">
                                                            "(Gojek, OVO, Dana, etc)."
                                                        </span>
                                                    </p>
                                                </div>
                                                <div class="vp-step">
                                                    <span class="vp-step-num">"2"</span>
                                                    <p class="vp-step-text">
                                                        "Select the " <strong>"Scan"</strong> " or "
                                                        <strong>"Pay"</strong> " option."
                                                    </p>
                                                </div>
                                                <div class="vp-step">
                                                    <span class="vp-step-num">"3"</span>
                                                    <p class="vp-step-text">
                                                        "Scan the QR code shown above or upload the saved image from your gallery."
                                                    </p>
                                                </div>
                                            </div>
                                        </div>
                                    }
                                })}

                            // ── Secure footer ─────────────────────────────────
                            <div class="vp-secure-footer">
                                <svg
                                    width="12"
                                    height="12"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                >
                                    <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
                                    <path d="M7 11V7a5 5 0 0110 0v4" />
                                </svg>
                                <span>"SECURE ENCRYPTED PAYMENT"</span>
                            </div>
                        </div>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}