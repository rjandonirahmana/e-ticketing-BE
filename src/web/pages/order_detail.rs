//! order_detail.rs — Halaman Verifikasi / Pembayaran Order (unified SSR + hydration).
//!
//! Port parity dari `csr/pages/order_detail.rs`:
//!   - `spawn_local` + `get_private` → `Resource::new(.., get_order_detail)`.
//!   - Countdown "TIME REMAINING": nilai awal dihitung **server-side** dari
//!     `expired_at` (jadi HTML SSR sudah benar), lalu tick per-detik hanya
//!     aktif setelah hydration (`#[cfg(feature = "hydrate")]`).
//!   - Desain `vp-*` (meta-row, status badge, QR card QRIS, paid card, total,
//!     save-QR, how-to-pay, secure footer) dipertahankan identik.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::csr::hooks::ThemeToggle;
use crate::csr::utils::format_idr;
use crate::web::api::get_order_detail;
use crate::web::app::AuthResource;

fn fmt_countdown(secs: i64) -> String {
    if secs <= 0 {
        return "00:00".to_string();
    }
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

#[component]
pub fn OrderDetailPage() -> impl IntoView {
    let params = use_params_map();
    let order_id = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let order = Resource::new(
        move || (order_id(), is_logged_in()),
        |(id, logged_in)| async move {
            if logged_in && !id.is_empty() {
                get_order_detail(id).await
            } else {
                Err(ServerFnError::ServerError("not_ready".into()))
            }
        },
    );

    view! {
        <div class="page vp-page">
            <header class="page-header vp-header">
                <A href="/orders" attr:class="back-btn">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                        stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </A>
                <span class="vp-header-title">"Verification"</span>
                <div class="header-actions">
                    <ThemeToggle />
                </div>
            </header>

            <Suspense fallback=|| {
                view! {
                    <div class="vp-loading">
                        <div class="vp-shimmer vp-shimmer--meta"></div>
                        <div class="vp-shimmer vp-shimmer--qr"></div>
                        <div class="vp-shimmer vp-shimmer--btn"></div>
                    </div>
                }
            }>
                {move || {
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <div class="vp-error">
                                <span class="vp-error-icon">"🔒"</span>
                                <p class="vp-error-msg">
                                    "Kamu harus masuk untuk melihat order."
                                </p>
                                <A href="/login" attr:class="vp-back-link">"← Masuk"</A>
                            </div>
                        }
                            .into_any();
                    }

                    order
                        .get()
                        .map(|res| {
                            match res {
                                Err(e) if e.to_string().contains("not_ready") => {
                                    view! { <div /> }.into_any()
                                }
                                Err(_) => {
                                    view! {
                                        <div class="vp-error">
                                            <span class="vp-error-icon">"⚠️"</span>
                                            <p class="vp-error-msg">"Order tidak ditemukan."</p>
                                            <A href="/orders" attr:class="vp-back-link">
                                                "← Kembali ke Orders"
                                            </A>
                                        </div>
                                    }
                                        .into_any()
                                }
                                Ok(o) => order_view(o).into_any(),
                            }
                        })
                        .unwrap_or_else(|| view! { <div /> }.into_any())
                }}
            </Suspense>
        </div>
    }
}

fn order_view(o: crate::web::models::OrderDetail) -> impl IntoView {
    let status = o.status.to_lowercase();
    let is_pending = status == "pending" || status == "waiting";
    let is_paid = matches!(status.as_str(), "paid" | "completed");
    let order_code = o.order_code.clone();
    let total = o.total_amount;

    // Sisa waktu awal dihitung server-side dari expired_at → SSR render benar.
    let initial_secs: i64 = o
        .expired_at
        .map(|exp| (exp - chrono::Utc::now()).num_seconds().max(0))
        .unwrap_or(0);
    let remaining = RwSignal::new(initial_secs);

    // Tick per-detik hanya setelah hydration.
    #[cfg(feature = "hydrate")]
    {
        if is_pending {
            let timer: StoredValue<Option<leptos::prelude::IntervalHandle>> =
                StoredValue::new(None);
            timer.set_value(
                set_interval_with_handle(
                    move || remaining.update(|v| *v = (*v - 1).max(0)),
                    std::time::Duration::from_secs(1),
                )
                .ok(),
            );
            on_cleanup(move || {
                if let Some(Some(h)) = timer.try_update_value(|o| o.take()) {
                    h.clear();
                }
            });
        }
    }

    let pm = o.payment_method.clone().unwrap_or_default();
    let paid_str = o
        .paid_at
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "—".into());

    view! {
        <div class="vp-content">
            // ── ORDER ID + TIME REMAINING ────────────────────────────
            <div class="vp-meta-row">
                <div class="vp-meta-block">
                    <span class="vp-meta-label">"ORDER ID"</span>
                    <span class="vp-meta-val vp-order-code">{"#"}{order_code}</span>
                </div>
                {is_pending
                    .then(move || {
                        view! {
                            <div class="vp-meta-block vp-meta-block--right">
                                <span class="vp-meta-label">"TIME REMAINING"</span>
                                <span class="vp-timer">
                                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                        stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                        <circle cx="12" cy="12" r="10" />
                                        <polyline points="12 6 12 12 16 14" />
                                    </svg>
                                    {move || fmt_countdown(remaining.get())}
                                </span>
                            </div>
                        }
                    })}
            </div>

            // ── STATUS BADGE ─────────────────────────────────────────
            <div class=if is_pending {
                "vp-status-badge vp-status-badge--pending"
            } else if is_paid {
                "vp-status-badge vp-status-badge--paid"
            } else {
                "vp-status-badge vp-status-badge--cancelled"
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

            // ── QR CARD (pending) ─────────────────────────────────────
            {is_pending
                .then(|| {
                    view! {
                        <div class="vp-qr-card">
                            <div class="vp-qr-card-header">
                                <div class="vp-qr-logo">
                                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                        stroke="var(--accent-blue)" stroke-width="2.5" stroke-linecap="round">
                                        <polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2" />
                                    </svg>
                                </div>
                                <span class="vp-gpn-label">"GPN NETWORK"</span>
                            </div>
                            <div class="vp-qr-container">
                                <div class="vp-qr-glow"></div>
                                <div class="vp-qr-inner">
                                    <svg viewBox="0 0 160 160" width="148" height="148"
                                        xmlns="http://www.w3.org/2000/svg">
                                        <rect width="160" height="160" fill="white" rx="8" />
                                        <rect x="10" y="10" width="50" height="50" fill="#0d0d1a" />
                                        <rect x="16" y="16" width="38" height="38" fill="white" />
                                        <rect x="22" y="22" width="26" height="26" fill="#0d0d1a" />
                                        <rect x="100" y="10" width="50" height="50" fill="#0d0d1a" />
                                        <rect x="106" y="16" width="38" height="38" fill="white" />
                                        <rect x="112" y="22" width="26" height="26" fill="#0d0d1a" />
                                        <rect x="10" y="100" width="50" height="50" fill="#0d0d1a" />
                                        <rect x="16" y="106" width="38" height="38" fill="white" />
                                        <rect x="22" y="112" width="26" height="26" fill="#0d0d1a" />
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
                                        <rect x="58" y="68" width="44" height="24" rx="4" fill="#4f6bff" />
                                        <text x="80" y="85" text-anchor="middle" fill="white"
                                            font-size="10" font-family="monospace" font-weight="bold">
                                            "QRIS"
                                        </text>
                                    </svg>
                                </div>
                            </div>
                            <p class="vp-scan-label">"SCAN THIS CODE TO PAY"</p>
                        </div>
                    }
                })}

            // ── PAID card ─────────────────────────────────────────────
            {is_paid
                .then(move || {
                    view! {
                        <div class="vp-paid-card">
                            <div class="vp-paid-icon">
                                <svg width="40" height="40" viewBox="0 0 24 24" fill="none"
                                    stroke="var(--accent-lime)" stroke-width="2.5" stroke-linecap="round">
                                    <path d="M22 11.08V12a10 10 0 11-5.93-9.14" />
                                    <polyline points="22 4 12 14.01 9 11.01" />
                                </svg>
                            </div>
                            <h3 class="vp-paid-title">"Pembayaran Berhasil"</h3>
                            {(!pm.is_empty())
                                .then(move || {
                                    view! { <p class="vp-paid-method">"via "{pm.to_uppercase()}</p> }
                                })}
                            <p class="vp-paid-date">{paid_str}</p>
                            <A href="/tickets" attr:class="vp-view-ticket-btn">"Lihat E-Ticket"</A>
                        </div>
                    }
                })}

            // ── Total ─────────────────────────────────────────────────
            <div class="vp-total-section">
                <span class="vp-total-label">"Total Payment"</span>
                <span class="vp-total-amount">{format_idr(total as i64)}</span>
            </div>

            // ── Save QR (pending) ─────────────────────────────────────
            {is_pending
                .then(|| {
                    view! {
                        <button
                            class="vp-save-btn"
                            on:click=move |_| {
                                #[cfg(feature = "hydrate")]
                                {
                                    use leptos::wasm_bindgen::JsCast;
                                    if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
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
                            }
                        >
                            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                                stroke-width="2.2" stroke-linecap="round">
                                <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
                                <polyline points="7 10 12 15 17 10" />
                                <line x1="12" y1="15" x2="12" y2="3" />
                            </svg>
                            "Save QR to Gallery"
                        </button>
                    }
                })}

            // ── How to Pay (pending) ──────────────────────────────────
            {is_pending
                .then(|| {
                    view! {
                        <div class="vp-howto-card">
                            <div class="vp-howto-header">
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                                    stroke-width="2" stroke-linecap="round">
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
                                        <span class="vp-step-italic">"(Gojek, OVO, Dana, etc)."</span>
                                    </p>
                                </div>
                                <div class="vp-step">
                                    <span class="vp-step-num">"2"</span>
                                    <p class="vp-step-text">
                                        "Select the "<strong>"Scan"</strong>" or "<strong>"Pay"</strong>
                                        " option."
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

            // ── Secure footer ─────────────────────────────────────────
            <div class="vp-secure-footer">
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                    stroke-width="2" stroke-linecap="round">
                    <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
                    <path d="M7 11V7a5 5 0 0110 0v4" />
                </svg>
                <span>"SECURE ENCRYPTED PAYMENT"</span>
            </div>
        </div>
    }
}
