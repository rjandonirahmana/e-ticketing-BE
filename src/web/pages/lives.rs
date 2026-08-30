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

use crate::web::components::{BottomNav, LiveStreamViewer};

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

/// Berapa kartu yang boleh memutar video SERENTAK di daftar `/lives`.
///
/// Tiap kartu yang memutar = SATU peer subscriber di SFU, dan SFU ini satu
/// utas (`live/sfu.rs`). Jadi angka ini bukan preferensi tampilan, melainkan
/// plafon beban: satu penonton yang membuka `/lives` menimbulkan sebanyak ini
/// koneksi, bukan satu. Empat menutupi dua baris pertama grid dua-kolom —
/// yang muat di layar ponsel sebelum digulir — dan berhenti di situ.
const MAX_PRATINJAU: usize = 4;

/// Boleh atau tidak kartu ke-`i` memutar video, mengingat kartu mana saja yang
/// sedang terlihat.
///
/// Fungsi murni, dan sengaja: inilah satu-satunya rem antara "daftar siaran"
/// dan "membuka koneksi WebRTC ke setiap siaran di daftar itu". Kalau ia
/// longgar, satu orang membuka `/lives` bisa menimbulkan dua puluh peer di SFU
/// yang berjalan satu utas — dan kegagalannya tidak muncul sebagai galat,
/// melainkan sebagai siaran semua orang yang tersendat.
///
/// `terlihat` adalah `BTreeSet` supaya urutannya bermakna: saat kartu yang
/// terlihat lebih banyak dari plafon, yang menang adalah yang paling ATAS —
/// bukan yang kebetulan lebih dulu dilaporkan pengamat, yang urutannya tak
/// dijamin oleh IntersectionObserver.
fn boleh_pratinjau(terlihat: &std::collections::BTreeSet<usize>, i: usize) -> bool {
    terlihat.iter().take(MAX_PRATINJAU).any(|&x| x == i)
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
    // Indeks kartu yang sedang berada di layar. `BTreeSet` (bukan `HashSet`)
    // karena urutannya dipakai: saat lebih dari `MAX_PRATINJAU` kartu terlihat
    // sekaligus, yang menang adalah yang paling ATAS — bukan yang kebetulan
    // lebih dulu dilaporkan pengamat, yang urutannya tak dijamin.
    let terlihat: RwSignal<std::collections::BTreeSet<usize>> =
        RwSignal::new(std::collections::BTreeSet::new());
    let boleh_pratinjau = move |i: usize| terlihat.with(|v| boleh_pratinjau(v, i));

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

    // ── Pengamat visibilitas kartu (WASM only) ────────────────────────────────
    // Hanya kartu yang benar-benar ada di layar yang boleh membuka WebRTC.
    // Tanpa ini, membuka `/lives` saat ada 20 siaran akan menyambung ke kedua
    // puluhnya sekaligus — dua puluh peer di SFU satu-utas, untuk satu orang.
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;

        // Pengamat + closure-nya dipegang sepanjang hidup halaman. Kalau
        // di-`forget()`, keduanya bocor tiap kali daftar berubah; kalau
        // di-drop begitu efeknya selesai, callback-nya mati dan tak ada kartu
        // yang pernah dilaporkan terlihat.
        let simpan: StoredValue<Option<send_wrapper::SendWrapper<(
            web_sys::IntersectionObserver,
            Closure<dyn FnMut(js_sys::Array)>,
        )>>> = StoredValue::new(None);

        Effect::new(move |_| {
            // Bergantung pada `rooms` supaya pengamat dipasang ulang saat
            // daftar kartu berubah, dan pada `active` supaya tak menyala saat
            // feed fullscreen sedang menutupi daftar.
            let n = rooms.with(|r| r.len());
            if n == 0 || active.get().is_some() {
                simpan.set_value(None);
                terlihat.set(std::collections::BTreeSet::new());
                return;
            }

            let cb = Closure::<dyn FnMut(js_sys::Array)>::new(
                move |entries: js_sys::Array| {
                    let mut set = terlihat.get_untracked();
                    let mut berubah = false;
                    for e in entries.iter() {
                        let Ok(entry) = e.dyn_into::<web_sys::IntersectionObserverEntry>() else {
                            continue;
                        };
                        let Ok(el) = entry.target().dyn_into::<web_sys::HtmlElement>() else {
                            continue;
                        };
                        // Indeks dibaca dari atribut, bukan ditangkap closure:
                        // satu callback melayani SEMUA kartu, jadi ia tak bisa
                        // memiliki satu indeks tetap.
                        let Some(idx) = el
                            .get_attribute("data-idx")
                            .and_then(|v| v.parse::<usize>().ok())
                        else {
                            continue;
                        };
                        if entry.is_intersecting() {
                            berubah |= set.insert(idx);
                        } else {
                            berubah |= set.remove(&idx);
                        }
                    }
                    if berubah {
                        terlihat.set(set);
                    }
                },
            );

            let Ok(observer) =
                web_sys::IntersectionObserver::new(cb.as_ref().unchecked_ref())
            else {
                return;
            };

            let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
                return;
            };
            let Ok(nodes) = doc.query_selector_all(".lives-card") else {
                return;
            };
            for i in 0..nodes.length() {
                if let Some(el) = nodes.item(i).and_then(|n| n.dyn_into::<web_sys::Element>().ok()) {
                    observer.observe(&el);
                }
            }

            // Mengganti isi `simpan` men-DROP pengamat sebelumnya — itulah yang
            // melepas pengamatan kartu-kartu lama, tanpa perlu `disconnect()`
            // manual yang gampang terlewat di satu cabang keluar.
            simpan.set_value(Some(send_wrapper::SendWrapper::new((observer, cb))));
        });

        on_cleanup(move || simpan.set_value(None));
    }

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
                                                let rid = room.room_id.clone();
                                                view! {
                                                    // `data-idx` dibaca pengamat visibilitas di atas.
                                                    <button
                                                        class="lives-card"
                                                        attr:data-idx=i
                                                        on:click=move |_| active.set(Some(i))
                                                    >
                                                        <div class="lives-card-thumb">
                                                            // Inisial tetap dirender DI BAWAH video, bukan
                                                            // sebagai gantinya: ia jadi latar saat pratinjau
                                                            // belum tersambung, saat kartunya di luar plafon
                                                            // `MAX_PRATINJAU`, dan saat siarannya gagal dimuat.
                                                            // Kartu karena itu tak pernah jadi kotak kosong.
                                                            <span class="lives-card-avatar">{initial}</span>
                                                            {
                                                                let rid = rid.clone();
                                                                move || {
                                                                    boleh_pratinjau(i)
                                                                        .then(|| {
                                                                            view! {
                                                                                <LiveStreamViewer
                                                                                    room_id=rid.clone()
                                                                                    autoplay=true
                                                                                    preview=true
                                                                                />
                                                                            }
                                                                        })
                                                                }
                                                            }
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

// ─── Uji plafon pratinjau ─────────────────────────────────────────────────────
//
// Tiap kartu yang memutar video = SATU peer subscriber di SFU, dan SFU-nya satu
// utas. Jadi aturan di bawah bukan preferensi tampilan melainkan plafon beban:
// ia yang memutuskan berapa koneksi WebRTC yang ditimbulkan SATU orang saat
// membuka halaman ini. Longgar sedikit, dan yang rusak bukan halaman ini —
// melainkan siaran semua orang.
#[cfg(test)]
mod tests_pratinjau {
    use super::*;
    use std::collections::BTreeSet;

    fn terlihat(indeks: &[usize]) -> BTreeSet<usize> {
        indeks.iter().copied().collect()
    }

    /// Tak ada yang terlihat → tak ada koneksi sama sekali. Ini keadaan saat
    /// halaman baru dibuka sebelum pengamat melapor, dan saat feed fullscreen
    /// menutupi daftar.
    #[test]
    fn tak_terlihat_tak_memutar() {
        let t = terlihat(&[]);
        for i in 0..10 {
            assert!(!boleh_pratinjau(&t, i));
        }
    }

    /// Di bawah plafon, semua yang terlihat boleh memutar.
    #[test]
    fn di_bawah_plafon_semua_boleh() {
        let t = terlihat(&[0, 1, 2]);
        for i in [0, 1, 2] {
            assert!(boleh_pratinjau(&t, i), "kartu {i} terlihat dan masih di bawah plafon");
        }
        assert!(!boleh_pratinjau(&t, 3), "kartu 3 tidak terlihat");
    }

    /// Tepat di plafon: keempatnya boleh.
    #[test]
    fn tepat_di_plafon() {
        let t = terlihat(&[0, 1, 2, 3]);
        assert_eq!((0..4).filter(|&i| boleh_pratinjau(&t, i)).count(), MAX_PRATINJAU);
    }

    /// MELEBIHI plafon — inilah kasus yang menjaga server. Sepuluh kartu
    /// terlihat sekaligus (layar lebar), hanya empat yang boleh memutar.
    #[test]
    fn di_atas_plafon_dibatasi() {
        let t = terlihat(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        let boleh: Vec<usize> = (0..10).filter(|&i| boleh_pratinjau(&t, i)).collect();
        assert_eq!(boleh.len(), MAX_PRATINJAU, "tak boleh lebih dari plafon");
        assert_eq!(boleh, vec![0, 1, 2, 3], "yang menang adalah yang paling ATAS");
    }

    /// Setelah digulir ke bawah, yang menang adalah empat teratas dari yang
    /// SEDANG terlihat — bukan empat pertama dari seluruh daftar.
    #[test]
    fn setelah_digulir_mengikuti_yang_terlihat() {
        let t = terlihat(&[6, 7, 8, 9, 10, 11]);
        let boleh: Vec<usize> = (0..12).filter(|&i| boleh_pratinjau(&t, i)).collect();
        assert_eq!(boleh, vec![6, 7, 8, 9]);
        assert!(!boleh_pratinjau(&t, 0), "kartu yang sudah tergulir keluar berhenti memutar");
    }

    /// Urutan pelaporan pengamat tak boleh mengubah hasil — IntersectionObserver
    /// tidak menjamin urutan entri, dan hasil yang bergantung padanya akan
    /// membuat kartu berkedip nyala-mati saat digulir.
    #[test]
    fn urutan_laporan_tak_mempengaruhi() {
        let a = terlihat(&[9, 3, 7, 1, 5]);
        let b = terlihat(&[1, 3, 5, 7, 9]);
        for i in 0..12 {
            assert_eq!(
                boleh_pratinjau(&a, i),
                boleh_pratinjau(&b, i),
                "kartu {i} harus sama apa pun urutan laporannya"
            );
        }
    }
}
