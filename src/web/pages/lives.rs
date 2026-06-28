//! lives.rs — halaman "Siaran Langsung".
//!
//! - Menampilkan daftar merchant yang sedang live (`GET /api/live/rooms`).
//! - Klik salah satu kartu → langsung join (feed fullscreen, autoplay).
//! - Di feed, geser ke bawah (scroll-snap vertikal) → live berikutnya.
//!   Hanya slide yang sedang tampil yang menyambung WebRTC; slide lain
//!   menampilkan placeholder agar tidak membuka banyak koneksi sekaligus.

use leptos::prelude::*;
use leptos_router::components::A;
use serde::Deserialize;

use crate::web::components::{BottomNav, GridBackground, LiveStreamViewer};

#[derive(Debug, Clone, Deserialize)]
struct RoomInfo {
    room_id: String,
    #[serde(default)]
    merchant_name: String,
    #[serde(default)]
    viewer_count: usize,
}

async fn api_list_rooms() -> Result<Vec<RoomInfo>, String> {
    let resp = gloo_net::http::Request::get("/api/live/rooms")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if let Some(err) = json.get("error").and_then(|e| e.as_str()) {
        return Err(err.to_string());
    }
    serde_json::from_value(json["data"].clone()).map_err(|e| e.to_string())
}

fn initial_of(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

#[component]
pub fn LivesPage() -> impl IntoView {
    let rooms = RwSignal::new(Vec::<RoomInfo>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(None::<String>);
    // None = tampilan daftar; Some(i) = feed fullscreen mulai dari index i.
    let active = RwSignal::new(None::<usize>);
    let feed_idx = RwSignal::new(0usize);
    let feed_ref: NodeRef<leptos::html::Div> = NodeRef::new();

    let load = move || {
        loading.set(true);
        wasm_bindgen_futures::spawn_local(async move {
            match api_list_rooms().await {
                Ok(list) => {
                    rooms.set(list);
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    };

    // Muat daftar saat halaman dipasang (fallback awal; WS akan ambil alih).
    Effect::new(move |_| {
        load();
    });

    // ── Realtime via WebSocket /ws/lives (WASM only) ──────────────────────────
    // Server push snapshot daftar room tiap ada perubahan (room baru/berhenti,
    // penonton masuk/keluar) → tidak perlu polling.
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::prelude::*;

        let ws_store: StoredValue<Option<web_sys::WebSocket>> = StoredValue::new(None);
        let cb_msg: StoredValue<Option<JsValue>> = StoredValue::new(None);

        Effect::new(move |_| {
            let proto = if web_sys::window()
                .map(|w| w.location().protocol().unwrap_or_default() == "https:")
                .unwrap_or(false)
            {
                "wss"
            } else {
                "ws"
            };
            let host = web_sys::window()
                .and_then(|w| w.location().host().ok())
                .unwrap_or_default();
            let url = format!("{}://{}/ws/lives", proto, host);

            let Ok(ws) = web_sys::WebSocket::new(&url) else {
                return;
            };

            let onmessage = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
                move |e: web_sys::MessageEvent| {
                    let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() else {
                        return;
                    };
                    let s: String = txt.into();
                    if let Ok(list) = serde_json::from_str::<Vec<RoomInfo>>(&s) {
                        // BUG FIX #2: Jangan update `rooms` saat feed fullscreen aktif.
                        // Jika diupdate, Leptos akan me-render ulang seluruh slide list
                        // dan unmount+remount setiap LiveStreamViewer — yang mematikan
                        // koneksi WebRTC yang sedang aktif tiap kali ada penonton masuk/keluar.
                        if active.get_untracked().is_none() {
                            rooms.set(list);
                            loading.set(false);
                            error.set(None);
                        }
                    }
                },
            );
            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            cb_msg.set_value(Some(onmessage.into_js_value()));
            ws_store.set_value(Some(ws.clone()));

            on_cleanup(move || {
                ws.set_onmessage(None);
                let _ = ws.close();
                ws_store.set_value(None);
                cb_msg.set_value(None);
            });
        });
    }

    // Lacak nilai `active` sebelumnya agar bisa mendeteksi transisi Some→None.
    // Tidak bisa pakai parameter `prev` Effect::new karena closure harus
    // mengembalikan tipe yang sama dengan `prev: Option<T>`, sedangkan kita
    // butuh menyimpan Option<usize> — bukan return value closure.
    let prev_active_store: StoredValue<Option<usize>> = StoredValue::new(None);

    // Saat feed dibuka, lompat ke slide awal lalu samakan feed_idx.
    // Saat feed ditutup (active → None), muat ulang daftar room dari server
    // karena update WS ditahan selama mode feed (Bug Fix #2).
    Effect::new(move |_| {
        let now = active.get();
        let was = prev_active_store.get_value();
        prev_active_store.set_value(now);

        if let Some(el) = feed_ref.get() {
            if let Some(start) = now {
                let h = el.client_height().max(1) as f64;
                el.set_scroll_top((start as f64 * h).round() as i32);
                feed_idx.set(start);
            }
        }
        // Kembali dari feed ke daftar: muat ulang daftar terkini.
        if was.is_some() && now.is_none() {
            load();
        }
    });

    // Saat user men-scroll feed, hitung slide aktif dari posisi scroll.
    let on_feed_scroll = move |_| {
        if let Some(el) = feed_ref.get_untracked() {
            let h = el.client_height().max(1) as f64;
            let idx = (el.scroll_top() as f64 / h).round() as usize;
            if idx != feed_idx.get_untracked() {
                feed_idx.set(idx);
            }
        }
    };

    view! {
        {move || {
            if active.get().is_some() {
                // ── FEED FULLSCREEN (swipe vertikal) ─────────────────────────
                let slides = rooms.get();
                view! {
                    <div class="lives-feed" node_ref=feed_ref on:scroll=on_feed_scroll>
                        <button
                            class="lives-feed-close"
                            on:click=move |_| active.set(None)
                            aria-label="Kembali"
                        >
                            <svg width="22" height="22" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                <polyline points="15 18 9 12 15 6"/>
                            </svg>
                        </button>

                        // Hint swipe: tampil saat masih ada live di bawah (lebih dari satu).
                        {move || {
                            let total = rooms.get().len();
                            (total > 1 && feed_idx.get() + 1 < total).then(|| view! {
                                <div class="lives-swipe-hint">
                                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                        <polyline points="18 15 12 9 6 15"/>
                                    </svg>
                                    <span>"Geser untuk live lainnya"</span>
                                </div>
                            })
                        }}
                        {slides.into_iter().enumerate().map(|(i, room)| {
                            let rid = room.room_id.clone();
                            let name = room.merchant_name.clone();
                            view! {
                                <section class="lives-slide">
                                    {move || if feed_idx.get() == i {
                                        view! {
                                            <LiveStreamViewer room_id=rid.clone() autoplay=true />
                                        }.into_any()
                                    } else {
                                        let n = name.clone();
                                        view! {
                                            <div class="lives-slide-ph">
                                                <span class="lives-ph-avatar">{initial_of(&n)}</span>
                                                <span class="lives-ph-name">{n}</span>
                                                <span class="lives-ph-hint">"Geser untuk menonton"</span>
                                            </div>
                                        }.into_any()
                                    }}
                                </section>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            } else {
                // ── DAFTAR MERCHANT YANG LIVE ────────────────────────────────
                view! {
                    <GridBackground />
                    <div class="page lives-page">
                        <header class="lives-header">
                            <A href="/explore" attr:class="lives-back" attr:aria-label="Kembali">
                                <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                    <polyline points="15 18 9 12 15 6"/>
                                </svg>
                            </A>
                            <span class="lives-title">"Siaran Langsung"</span>
                            <button class="lives-refresh" on:click=move |_| load() aria-label="Muat ulang">
                                <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
                                    <polyline points="23 4 23 10 17 10"/>
                                    <path d="M20.49 15a9 9 0 11-2.12-9.36L23 10"/>
                                </svg>
                            </button>
                        </header>

                        {move || {
                            if loading.get() {
                                view! {
                                    <div class="lives-status"><span class="lives-spinner"></span></div>
                                }.into_any()
                            } else if let Some(e) = error.get() {
                                view! {
                                    <div class="lives-status"><p class="lives-status-text">{e}</p></div>
                                }.into_any()
                            } else {
                                let list = rooms.get();
                                if list.is_empty() {
                                    view! {
                                        <div class="lives-status">
                                            <svg width="44" height="44" viewBox="0 0 24 24" fill="none"
                                                 stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                                <polygon points="23 7 16 12 23 17 23 7"/>
                                                <rect x="1" y="5" width="15" height="14" rx="2" ry="2"/>
                                            </svg>
                                            <p class="lives-status-text">"Belum ada siaran langsung"</p>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="lives-grid">
                                            {list.into_iter().enumerate().map(|(i, room)| {
                                                let name = room.merchant_name.clone();
                                                let initial = initial_of(&name);
                                                let vc = room.viewer_count;
                                                view! {
                                                    <button class="lives-card" on:click=move |_| active.set(Some(i))>
                                                        <div class="lives-card-thumb">
                                                            <span class="lives-card-avatar">{initial}</span>
                                                            <span class="lives-card-badge">
                                                                <span class="lives-card-dot"></span>
                                                                "LIVE"
                                                            </span>
                                                        </div>
                                                        <div class="lives-card-info">
                                                            <span class="lives-card-name">{name}</span>
                                                            <span class="lives-card-viewers">
                                                                {vc}" menonton"
                                                            </span>
                                                        </div>
                                                    </button>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                }
                            }
                        }}

                        <BottomNav active="lives" />
                    </div>
                }.into_any()
            }
        }}
    }
}
