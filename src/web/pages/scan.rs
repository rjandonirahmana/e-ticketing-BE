//! scan.rs — Halaman QR Scanner (SSR shell + client hydration).
//!
//! SSR: render UI shell + manual input.
//! Hydration: kamera QR scanner aktif di browser setelah WASM load.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::scan_ticket;
use crate::web::app::AuthResource;

#[component]
pub fn ScanPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let manual_code = RwSignal::new(String::new());
    let scanning   = RwSignal::new(false);
    let result     = RwSignal::new(Option::<String>::None);
    let scan_error = RwSignal::new(Option::<String>::None);

    let do_scan = move |code: String| {
        if code.is_empty() { return; }
        scanning.set(true);
        result.set(None);
        scan_error.set(None);
        leptos::task::spawn_local(async move {
            match scan_ticket(code).await {
                Ok(msg) => result.set(Some(msg)),
                Err(e) => scan_error.set(Some(e.to_string())),
            }
            scanning.set(false);
        });
    };

    let on_manual_submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        do_scan(manual_code.get());
    };

    view! {
        <div class="page-header">
            <div class="container">
                <p class="page-header__eyebrow">"// validasi tiket"</p>
                <h1 class="page-header__title">"Scan Tiket"</h1>
                <p class="page-header__sub">"Scan QR code tiket pengunjung di pintu masuk"</p>
            </div>
        </div>

        <div class="container" style="padding-bottom:4rem;max-width:480px">
            {move || {
                if !is_logged_in() && auth.get().is_some() {
                    return view! {
                        <div style="text-align:center;padding:4rem 0">
                            <p style="color:var(--clr-muted);margin-bottom:1.5rem">
                                "Kamu harus masuk sebagai merchant untuk scan tiket."
                            </p>
                            <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                        </div>
                    }.into_any();
                }

                view! {
                    <div>
                        // ── Camera viewfinder (placeholder SSR, aktif saat hydrate) ────
                        <div style="width:100%;aspect-ratio:1;background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:16px;display:flex;align-items:center;justify-content:center;margin-bottom:1.5rem;position:relative;overflow:hidden"
                            id="qr-camera-mount"
                        >
                            <div style="text-align:center;color:var(--clr-muted)">
                                <div style="font-size:3rem;margin-bottom:.75rem">"📷"</div>
                                <p style="font-size:.875rem">"Kamera aktif setelah halaman dimuat"</p>
                                <p style="font-size:.75rem;margin-top:.25rem">"Pastikan JavaScript diaktifkan"</p>
                            </div>
                            // Corner guides
                            <div style="position:absolute;top:20px;left:20px;width:40px;height:40px;border-top:3px solid var(--clr-accent);border-left:3px solid var(--clr-accent);border-radius:4px 0 0 0"/>
                            <div style="position:absolute;top:20px;right:20px;width:40px;height:40px;border-top:3px solid var(--clr-accent);border-right:3px solid var(--clr-accent);border-radius:0 4px 0 0"/>
                            <div style="position:absolute;bottom:20px;left:20px;width:40px;height:40px;border-bottom:3px solid var(--clr-accent);border-left:3px solid var(--clr-accent);border-radius:0 0 0 4px"/>
                            <div style="position:absolute;bottom:20px;right:20px;width:40px;height:40px;border-bottom:3px solid var(--clr-accent);border-right:3px solid var(--clr-accent);border-radius:0 0 4px 0"/>
                        </div>

                        // ── Result ───────────────────────────────────────────────────
                        {move || result.get().map(|msg| view! {
                            <div class="alert alert--success" style="margin-bottom:1rem">
                                "✅ " {msg}
                            </div>
                        })}
                        {move || scan_error.get().map(|e| view! {
                            <div class="alert alert--error" style="margin-bottom:1rem">
                                "❌ " {e}
                            </div>
                        })}

                        // ── Manual input ─────────────────────────────────────────────
                        <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:12px;padding:1.25rem">
                            <h3 style="font-size:.875rem;font-weight:700;margin-bottom:.875rem">"Atau masukkan kode manual"</h3>
                            <form on:submit=on_manual_submit style="display:flex;gap:.625rem">
                                <input
                                    type="text"
                                    class="filter-bar__input"
                                    placeholder="Masukkan kode tiket..."
                                    prop:value=manual_code
                                    on:input=move |ev| manual_code.set(event_target_value(&ev))
                                    style="flex:1"
                                />
                                <button
                                    type="submit"
                                    class="btn btn--accent"
                                    disabled=move || scanning.get()
                                >
                                    {move || if scanning.get() { "..." } else { "Scan" }}
                                </button>
                            </form>
                        </div>

                        <p style="font-size:.75rem;color:var(--clr-muted);text-align:center;margin-top:1rem">
                            "Scan QR code akan otomatis memvalidasi tiket"
                        </p>
                    </div>
                }.into_any()
            }}
        </div>
    }
}
