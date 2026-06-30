//! merchant_live_pip.rs — Overlay "live melayang" (picture-in-picture) kecil,
//! BISA DI-DRAG & ditutup.
//!
//! Saat merchant pemilik event sedang siaran (room SFU `live_{merchant_id}`
//! ada), tampilkan video live mini melayang. User bisa **menggeser** posisinya
//! ke mana saja, **tap** untuk buka feed live fullscreen (`/lives`), dan **✕**
//! untuk menutup. Status live dicek periodik dari keberadaan room SFU.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use crate::web::components::LiveStreamViewer;

#[component]
pub fn MerchantLivePip(
    /// Room id SFU, format `live_{merchant_id}`.
    room_id: String,
) -> impl IntoView {
    let room_id = StoredValue::new(room_id);
    let live = RwSignal::new(false);
    let merchant_name = RwSignal::new(String::new());
    let dismissed = RwSignal::new(false);

    // Cek status live merchant berkala (browser-only).
    #[cfg(target_arch = "wasm32")]
    {
        let check = move || {
            let rid = room_id.get_value();
            wasm_bindgen_futures::spawn_local(async move {
                let url = format!("/api/live/rooms/{}", rid);
                match gloo_net::http::Request::get(&url).send().await {
                    Ok(resp) if resp.status() == 200 => {
                        if let Ok(v) = resp.json::<serde_json::Value>().await {
                            if let Some(data) = v.get("data") {
                                if let Some(name) =
                                    data.get("merchant_name").and_then(|n| n.as_str())
                                {
                                    merchant_name.set(name.to_string());
                                }
                                live.set(true);
                                return;
                            }
                        }
                        live.set(false);
                    }
                    _ => live.set(false),
                }
            });
        };
        Effect::new(move |_| {
            check();
            let interval = send_wrapper::SendWrapper::new(
                gloo_timers::callback::Interval::new(15_000, move || check()),
            );
            on_cleanup(move || drop(interval));
        });
    }

    let pip_ref = NodeRef::<leptos::html::Div>::new();

    // ── Drag state (Copy via StoredValue) ──────────────────────────────────────
    let d_active: StoredValue<bool> = StoredValue::new(false);
    let d_moved: StoredValue<bool> = StoredValue::new(false);
    let d_px: StoredValue<f64> = StoredValue::new(0.0);
    let d_py: StoredValue<f64> = StoredValue::new(0.0);
    let d_left: StoredValue<f64> = StoredValue::new(0.0);
    let d_top: StoredValue<f64> = StoredValue::new(0.0);

    let open_lives = move || {
        #[cfg(target_arch = "wasm32")]
        {
            let nav = leptos_router::hooks::use_navigate();
            nav("/lives", Default::default());
        }
    };

    let on_down = move |e: web_sys::PointerEvent| {
        let Some(el) = pip_ref.get() else { return };
        let rect = el.get_bounding_client_rect();
        d_left.set_value(rect.left());
        d_top.set_value(rect.top());
        d_px.set_value(e.client_x() as f64);
        d_py.set_value(e.client_y() as f64);
        d_active.set_value(true);
        d_moved.set_value(false);
        // Pin ke left/top (lepas right/bottom) supaya bisa digeser bebas.
        let s = web_sys::HtmlElement::style(el.unchecked_ref());
        let _ = s.set_property("left", &format!("{}px", rect.left()));
        let _ = s.set_property("top", &format!("{}px", rect.top()));
        let _ = s.set_property("right", "auto");
        let _ = s.set_property("bottom", "auto");
        let _ = el.set_pointer_capture(e.pointer_id());
    };
    let on_move = move |e: web_sys::PointerEvent| {
        if !d_active.get_value() {
            return;
        }
        let dx = e.client_x() as f64 - d_px.get_value();
        let dy = e.client_y() as f64 - d_py.get_value();
        if dx.abs() > 4.0 || dy.abs() > 4.0 {
            d_moved.set_value(true);
        }
        let Some(el) = pip_ref.get() else { return };
        let nl = (d_left.get_value() + dx).max(4.0);
        let nt = (d_top.get_value() + dy).max(4.0);
        let s = web_sys::HtmlElement::style(el.unchecked_ref());
        let _ = s.set_property("left", &format!("{}px", nl));
        let _ = s.set_property("top", &format!("{}px", nt));
    };
    let on_up = move |_e: web_sys::PointerEvent| {
        let was = d_active.get_value();
        d_active.set_value(false);
        // Tap (tanpa geser) = buka feed live. Geser = pindah posisi saja.
        if was && !d_moved.get_value() {
            open_lives();
        }
    };

    view! {
        {move || (live.get() && !dismissed.get()).then(|| {
            let rid = room_id.get_value();
            view! {
                <div
                    class="mlpip"
                    node_ref=pip_ref
                    on:pointerdown=on_down
                    on:pointermove=on_move
                    on:pointerup=on_up
                >
                    <button
                        class="mlpip-close"
                        on:pointerdown=|e: web_sys::PointerEvent| e.stop_propagation()
                        on:click=move |e| { e.stop_propagation(); dismissed.set(true); }
                        aria-label="Tutup"
                    >"✕"</button>
                    <div class="mlpip-video">
                        <LiveStreamViewer room_id=rid autoplay=true />
                    </div>
                    <div class="mlpip-bar">
                        <span class="mlpip-dot"></span>
                        <span class="mlpip-text">
                            "LIVE"
                            {move || {
                                let n = merchant_name.get();
                                (!n.is_empty()).then(|| format!(" · {n}"))
                            }}
                        </span>
                        <span class="mlpip-grip" aria-hidden="true">"⠿"</span>
                    </div>
                </div>
            }
        })}
    }
}
