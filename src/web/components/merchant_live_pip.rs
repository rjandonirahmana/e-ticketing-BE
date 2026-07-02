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

    // ── Drag state ─────────────────────────────────────────────────────────────
    // Pakai `transform: translate3d` (GPU-composited, tanpa reflow) → geser mulus.
    // `tx/ty` = offset terakumulasi (persist antar-drag); `bx/by` = offset saat
    // pointerdown; `px/py` = posisi pointer saat pointerdown.
    let d_active: StoredValue<bool> = StoredValue::new(false);
    let d_moved: StoredValue<bool> = StoredValue::new(false);
    let d_px: StoredValue<f64> = StoredValue::new(0.0);
    let d_py: StoredValue<f64> = StoredValue::new(0.0);
    let d_bx: StoredValue<f64> = StoredValue::new(0.0);
    let d_by: StoredValue<f64> = StoredValue::new(0.0);
    let d_tx: StoredValue<f64> = StoredValue::new(0.0);
    let d_ty: StoredValue<f64> = StoredValue::new(0.0);

    let open_lives = move || {
        #[cfg(target_arch = "wasm32")]
        {
            let nav = leptos_router::hooks::use_navigate();
            nav("/lives", Default::default());
        }
    };

    let apply_transform = move |el: &web_sys::HtmlDivElement, x: f64, y: f64| {
        let s = web_sys::HtmlElement::style(el.unchecked_ref());
        let _ = s.set_property("transform", &format!("translate3d({x}px,{y}px,0)"));
    };

    let on_down = move |e: web_sys::PointerEvent| {
        let Some(el) = pip_ref.get() else { return };
        d_px.set_value(e.client_x() as f64);
        d_py.set_value(e.client_y() as f64);
        d_bx.set_value(d_tx.get_value());
        d_by.set_value(d_ty.get_value());
        d_active.set_value(true);
        d_moved.set_value(false);
        // Nonaktifkan transition saat menyeret → ikut jari 1:1.
        let s = web_sys::HtmlElement::style(el.unchecked_ref());
        let _ = s.set_property("transition", "none");
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
        let nx = d_bx.get_value() + dx;
        let ny = d_by.get_value() + dy;
        d_tx.set_value(nx);
        d_ty.set_value(ny);
        if let Some(el) = pip_ref.get() {
            apply_transform(&el, nx, ny);
        }
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
