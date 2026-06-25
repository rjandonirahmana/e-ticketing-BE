// src/components/story_viewer.rs — rev 12
// FIXES:
//   P0-a  Video duration race  — RAF tidak start sampai loadedmetadata untuk video
//   P0-b  Touch/click double fire — flag touch_handled mute click setelah swipe/close
//   P1-a  Heart burst memory leak — Closure::once (bukan Fn) + forget hanya utk one-shot
//   P1-b  seg_fill_refs vector — fresh NodeRefs tiap render, bukan resize incremental
//   Refactor: helper cancel_raf / start_raf agar DRY

use crate::web::components::audio_pill::AudioPill;
use leptos::either::Either;
use leptos::html::{Div, Video};
use leptos::prelude::*;
use leptos::wasm_bindgen::prelude::*;
use leptos::wasm_bindgen::JsCast;
use send_wrapper::SendWrapper;
use web_sys::js_sys;

use crate::web::state::stories::{use_stories_store, StoryMediaType};

// ── Konstanta ──────────────────────────────────────────────────────────
const STORY_DURATION_MS: f64 = 5_000.0;
const TAP_THRESHOLD_PX: f64 = 50.0;
const SWIPE_THRESHOLD_PX: f64 = 80.0;
const DOUBLE_TAP_MS: f64 = 300.0;
const DOUBLE_TAP_DISTANCE_PX: f64 = 40.0;
const MAX_CONCURRENT_HEARTS: u32 = 5;

// ── RafState ───────────────────────────────────────────────────────────
#[derive(Clone, Copy)]
struct RafState {
    start_ms: f64,
    duration_ms: f64,
    active_idx: usize,
    cancelled: bool,
    paused_at_ms: f64,
}

impl Default for RafState {
    fn default() -> Self {
        Self {
            start_ms: 0.0,
            duration_ms: STORY_DURATION_MS,
            active_idx: 0,
            cancelled: true,
            paused_at_ms: 0.0,
        }
    }
}

// ── Helper: cancel / start RAF (DRY) ──────────────────────────────────
fn cancel_raf(_raf_id: &StoredValue<i32>) {
    #[cfg(target_arch = "wasm32")]
    {
        let id = _raf_id.get_value();
        if id != 0 {
            if let Some(win) = web_sys::window() {
                let _ = win.cancel_animation_frame(id);
            }
            _raf_id.set_value(0);
        }
    }
}

fn start_raf(
    raf_closure: &StoredValue<Option<SendWrapper<Closure<dyn Fn()>>>>,
    raf_id: &StoredValue<i32>,
) {
    let new_id = raf_closure.with_value(|opt| {
        opt.as_ref().and_then(|sw| {
            web_sys::window().and_then(|win| {
                win.request_animation_frame(sw.as_ref().unchecked_ref())
                    .ok()
            })
        })
    });
    if let Some(id) = new_id {
        raf_id.set_value(id);
    }
}

// ══════════════════════════════════════════════════════════════════════
#[component]
pub fn StoryViewer() -> impl IntoView {
    let ctx = use_stories_store();

    // ── Signals ───────────────────────────────────────────────────────
    let is_paused = RwSignal::new(false);
    let touch_start = RwSignal::new(None::<(f64, f64, f64)>);
    let touch_last = RwSignal::new(None::<(f64, f64, f64)>);
    let is_liked = RwSignal::new(false);
    let pulse_count = RwSignal::new(0_u32);
    let is_muted = RwSignal::new(true);
    let active_hearts = RwSignal::new(0_u32);
    // Instagram-style loading: true while image is loading, false once loaded
    let img_loading = RwSignal::new(false);
    // Tap-to-reveal "klik detail" tag pada frame event — mirip product tag IG
    let show_detail_tag = RwSignal::new(false);

    let last_tap = StoredValue::new(None::<(f64, f64, f64)>);

    // FIX P0-b: suppress click event yang menyusul touchend swipe/close
    let touch_handled: StoredValue<bool> = StoredValue::new(false);

    // ── RAF state ─────────────────────────────────────────────────────
    let raf_id: StoredValue<i32> = StoredValue::new(0);
    let raf_state: StoredValue<RafState> = StoredValue::new(RafState::default());
    let raf_closure: StoredValue<Option<SendWrapper<Closure<dyn Fn()>>>> = StoredValue::new(None);

    // FIX P0-a: flag agar RAF tidak dimulai sebelum loadedmetadata video
    let waiting_for_video: StoredValue<bool> = StoredValue::new(false);
    // Instagram UX: flag agar RAF tidak dimulai sebelum image selesai load
    let waiting_for_image: StoredValue<bool> = StoredValue::new(false);

    // FIX P1-b: NodeRef progress bars — BUKAN di-resize tiap render,
    // melainkan dibuat fresh di view dan disimpan ke sini oleh view closure
    let seg_fill_refs: StoredValue<Vec<NodeRef<Div>>> = StoredValue::new(Vec::new());
    let _transform_buf: StoredValue<String> = StoredValue::new(String::with_capacity(32));

    // Video ref & misc
    let video_ref = NodeRef::<Video>::new();
    let video_duration_ms: StoredValue<f64> = StoredValue::new(STORY_DURATION_MS);
    let media_area_ref = NodeRef::<Div>::new();

    let kb_fn: StoredValue<Option<SendWrapper<Closure<dyn Fn(web_sys::KeyboardEvent)>>>> =
        StoredValue::new(None);
    let vis_closure: StoredValue<Option<SendWrapper<Closure<dyn Fn()>>>> = StoredValue::new(None);

    let preload_img: StoredValue<Option<web_sys::HtmlImageElement>> = StoredValue::new(None);
    let preload_vid: StoredValue<Option<web_sys::HtmlVideoElement>> = StoredValue::new(None);

    // ── Memos ──────────────────────────────────────────────────────────
    let pulse_label = Memo::new(move |_| {
        let n = pulse_count.get();
        if n >= 1000 {
            format!("{:.1}k", n as f64 / 1000.0)
        } else {
            n.to_string()
        }
    });

    let view_label = Memo::new(move |_| {
        let n = pulse_count.get();
        let v = 200 + (n % 600);
        if v >= 1000 {
            format!("{:.1}k views", v as f64 / 1000.0)
        } else {
            format!("{v} views")
        }
    });

    let now_ms = || -> f64 {
        web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0)
    };

    // ── Keyboard handler factory ───────────────────────────────────────
    let create_kb_handler = {
        let ctx = ctx.clone();
        move || {
            Closure::<dyn Fn(web_sys::KeyboardEvent)>::new({
                let ctx = ctx.clone();
                move |ev: web_sys::KeyboardEvent| match ev.key().as_str() {
                    "ArrowRight" | " " => {
                        ev.prevent_default();
                        ctx.next();
                    }
                    "ArrowLeft" => {
                        ev.prevent_default();
                        ctx.prev();
                    }
                    "Escape" => {
                        ev.prevent_default();
                        ctx.close();
                    }
                    _ => {}
                }
            })
        }
    };

    // ══════════════════════════════════════════════════════════════════
    // INIT: RAF closure permanen (wasm32 only — Closure::new panics on SSR)
    // ══════════════════════════════════════════════════════════════════
    #[cfg(target_arch = "wasm32")]
    if raf_closure.with_value(|o| o.is_none()) {
        let cb = Closure::<dyn Fn()>::new({
            let raf_id = raf_id.clone();
            let raf_state = raf_state.clone();
            let seg_fill_refs = seg_fill_refs.clone();
            let _transform_buf = _transform_buf.clone();
            let raf_closure = raf_closure.clone();
            let ctx = ctx.clone();
            move || {
                let st = raf_state.get_value();
                if st.cancelled {
                    raf_id.set_value(0);
                    return;
                }

                let fills = seg_fill_refs.get_value();
                if fills.is_empty() {
                    raf_id.set_value(0);
                    return;
                }

                let now = web_sys::window()
                    .and_then(|w| w.performance())
                    .map(|p| p.now())
                    .unwrap_or(st.start_ms);

                let p = ((now - st.start_ms) / st.duration_ms).min(1.0);

                // Update progress fill
                if let Some(node_ref) = fills.get(st.active_idx) {
                    if let Some(el) = node_ref.get_untracked() {
                        let html_el: &web_sys::HtmlElement = el.unchecked_ref();
                        _transform_buf.update_value(|buf| {
                            buf.clear();
                            use std::fmt::Write;
                            let _ = write!(buf, "scaleX({:.5})", p);
                            let _ = html_el.style().set_property("transform", buf.as_str());
                        });
                    }
                }

                if p >= 1.0 {
                    // Snap fill to 1 then advance
                    if let Some(nr) = fills.get(st.active_idx) {
                        if let Some(el) = nr.get_untracked() {
                            let _ = el
                                .unchecked_ref::<web_sys::HtmlElement>()
                                .style()
                                .set_property("transform", "scaleX(1)");
                        }
                    }
                    raf_state.update_value(|s| s.cancelled = true);
                    raf_id.set_value(0);
                    ctx.progress.set(1.0);
                    ctx.next();
                } else {
                    // Schedule next frame
                    let new_id = raf_closure.with_value(|opt| {
                        opt.as_ref().and_then(|sw| {
                            web_sys::window().and_then(|win| {
                                win.request_animation_frame(sw.as_ref().unchecked_ref())
                                    .ok()
                            })
                        })
                    });
                    raf_id.set_value(new_id.unwrap_or(0));
                }
            }
        });
        raf_closure.set_value(Some(SendWrapper::new(cb)));
    }

    // ══════════════════════════════════════════════════════════════════
    // EFFECT: Story / Group berubah
    // ══════════════════════════════════════════════════════════════════
    Effect::new(move |_| {
        let si = ctx.active_story_idx.get();
        let open = ctx.active_group.get().is_some();

        if !open {
            raf_state.update_value(|s| s.cancelled = true);
            cancel_raf(&raf_id);
            seg_fill_refs.update_value(|v| v.clear());
            waiting_for_video.set_value(false);
            return;
        }

        is_liked.set(false);

        let ts = web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now() as u32)
            .unwrap_or(12345);
        pulse_count.set(800 + (ts % 1400));
        ctx.progress.set(0.0);

        let is_video = ctx
            .with_current_story(|s| matches!(s.media_type, StoryMediaType::Video))
            .unwrap_or(false);

        cancel_raf(&raf_id);

        // Reset visual fills segmen (bagian sebelum active → penuh, sisanya → kosong)
        let fills = seg_fill_refs.get_value();
        for (i, nr) in fills.iter().enumerate() {
            if let Some(el) = nr.get() {
                let h: &web_sys::HtmlElement = el.unchecked_ref();
                let _ = h.style().set_property("transition", "none");
                let _ = h
                    .style()
                    .set_property("transform", if i < si { "scaleX(1)" } else { "scaleX(0)" });
            }
        }

        if !is_video {
            // ── GAMBAR: tahan RAF sampai image loaded (Instagram UX) ───
            waiting_for_video.set_value(false);
            waiting_for_image.set_value(true);
            video_duration_ms.set_value(STORY_DURATION_MS);
            img_loading.set(true);

            raf_state.set_value(RafState {
                start_ms: 0.0,
                duration_ms: STORY_DURATION_MS,
                active_idx: si,
                cancelled: true, // ditahan sampai image onload
                paused_at_ms: 0.0,
            });
        } else {
            // FIX P0-a: VIDEO — tahan RAF sampai loadedmetadata
            waiting_for_video.set_value(true);
            raf_state.set_value(RafState {
                start_ms: 0.0,
                duration_ms: STORY_DURATION_MS,
                active_idx: si,
                cancelled: true, // ← tetap cancelled sampai metadata tiba
                paused_at_ms: 0.0,
            });
            // Fill visual sudah direset di atas, tidak perlu mulai RAF dulu
        }
    });

    // ══════════════════════════════════════════════════════════════════
    // EFFECT: Pause / Resume
    // ══════════════════════════════════════════════════════════════════
    Effect::new(move |_| {
        let open = ctx.active_group.get().is_some();
        let paused = is_paused.get();

        if !open {
            cancel_raf(&raf_id);
            raf_state.update_value(|s| s.cancelled = true);
            return;
        }

        if paused {
            cancel_raf(&raf_id);
            let now = now_ms();
            raf_state.update_value(|s| {
                if s.paused_at_ms == 0.0 {
                    s.paused_at_ms = now;
                }
            });
            return;
        }

        // Jangan resume jika media belum selesai load — mencegah RAF start
        // dengan start_ms=0 sebelum onload/loadedmetadata tiba, yang menyebabkan
        // progress snap ke akhir dan story langsung skip.
        if waiting_for_image.get_value() || waiting_for_video.get_value() {
            return;
        }

        // Resume
        raf_state.update_value(|s| {
            if s.paused_at_ms > 0.0 {
                s.start_ms += now_ms() - s.paused_at_ms;
                s.paused_at_ms = 0.0;
            }
            s.cancelled = false;
        });

        if seg_fill_refs.with_value(|v| v.is_empty()) {
            return;
        }
        if raf_id.get_value() != 0 {
            return;
        }

        start_raf(&raf_closure, &raf_id);
    });

    // ══════════════════════════════════════════════════════════════════
    // EFFECT: Keyboard
    // ══════════════════════════════════════════════════════════════════
    Effect::new(move |_| {
        let win = match web_sys::window() {
            Some(w) => w,
            None => return,
        };

        if let Some(Some(old)) = kb_fn.try_update_value(|o| o.take()) {
            let _ = win.remove_event_listener_with_callback(
                "keydown",
                old.as_ref().unchecked_ref::<web_sys::js_sys::Function>(),
            );
            drop(old);
        }

        if ctx.active_group.get().is_none() {
            return;
        }

        let new_kb = create_kb_handler();
        let _ = win.add_event_listener_with_callback(
            "keydown",
            new_kb.as_ref().unchecked_ref::<web_sys::js_sys::Function>(),
        );
        kb_fn.set_value(Some(SendWrapper::new(new_kb)));
    });

    // ══════════════════════════════════════════════════════════════════
    // EFFECT: Page Visibility
    // ══════════════════════════════════════════════════════════════════
    Effect::new(move |_| {
        let doc = match web_sys::window().and_then(|w| w.document()) {
            Some(d) => d,
            None => return,
        };

        if let Some(Some(old)) = vis_closure.try_update_value(|o| o.take()) {
            let _ = doc.clone().remove_event_listener_with_callback(
                "visibilitychange",
                old.as_ref().unchecked_ref::<web_sys::js_sys::Function>(),
            );
            drop(old);
        }

        if ctx.active_group.get().is_none() {
            return;
        }

        let new_vis = Closure::<dyn Fn()>::new({
            let doc_c = doc.clone();
            move || is_paused.set(doc_c.hidden())
        });
        let _ = doc.add_event_listener_with_callback(
            "visibilitychange",
            new_vis
                .as_ref()
                .unchecked_ref::<web_sys::js_sys::Function>(),
        );
        vis_closure.set_value(Some(SendWrapper::new(new_vis)));
    });

    // ══════════════════════════════════════════════════════════════════
    // EFFECT: Sync muted ke video element
    // ══════════════════════════════════════════════════════════════════
    Effect::new(move |_| {
        if let Some(vid) = video_ref.get() {
            vid.set_muted(is_muted.get());
        }
    });

    // Sembunyikan tag "klik detail" setiap kali story aktif berganti.
    // Reset is_paused juga — kalau tag sedang tampil (yang pauses RAF),
    // story baru harus mulai dalam kondisi tidak paused.
    Effect::new(move |_| {
        let _si = ctx.active_story_idx.get();
        let _gi = ctx.active_group.get();
        show_detail_tag.set(false);
        is_paused.set(false);
    });

    // ══════════════════════════════════════════════════════════════════
    // EFFECT: Preload story berikutnya (dengan cancellation)
    // ══════════════════════════════════════════════════════════════════
    Effect::new(move |_| {
        let _si = ctx.active_story_idx.get();
        let _gi = ctx.active_group.get();

        // Batalkan preload sebelumnya (FIX: cancellation)
        preload_img.update_value(|o| {
            if let Some(img) = o.as_ref() {
                img.set_src("");
            }
        });
        preload_vid.update_value(|o| {
            if let Some(vid) = o.as_ref() {
                vid.set_src("");
            }
        });
        if let Some(Some(old)) = preload_img.try_update_value(|o| o.take()) {
            drop(old);
        }
        if let Some(Some(old)) = preload_vid.try_update_value(|o| o.take()) {
            drop(old);
        }

        let next_url = {
            let gi = ctx.active_group.get();
            let si = ctx.active_story_idx.get();
            gi.and_then(|gi| {
                ctx.groups.with(|g| {
                    let group = g.get(gi)?;
                    let next = si + 1;
                    if next < group.stories.len() {
                        Some(group.stories[next].media_url.clone())
                    } else if gi + 1 < g.len() {
                        g.get(gi + 1)?.stories.first().map(|s| s.media_url.clone())
                    } else {
                        None
                    }
                })
            })
        };

        let Some(url) = next_url else { return };
        let is_vid = url.ends_with(".mp4") || url.ends_with(".webm") || url.ends_with(".mov");
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };

        if is_vid {
            let Ok(el) = doc.create_element("video") else {
                return;
            };
            let Ok(vid) = el.dyn_into::<web_sys::HtmlVideoElement>() else {
                return;
            };
            vid.set_preload("metadata");
            vid.set_muted(true);
            vid.set_src(&url);
            preload_vid.set_value(Some(vid));
        } else {
            let Ok(img) = web_sys::HtmlImageElement::new() else {
                return;
            };
            let _ = img.set_attribute("decoding", "async");
            let _ = img.set_attribute("fetchpriority", "low");
            img.set_src(&url);
            preload_img.set_value(Some(img));
        }
    });

    // ══════════════════════════════════════════════════════════════════
    // EVENT HANDLERS
    // ══════════════════════════════════════════════════════════════════

    let on_touch_start = move |ev: leptos::ev::TouchEvent| {
        if let Some(t) = ev.touches().get(0) {
            let ts = (t.client_x() as f64, t.client_y() as f64, now_ms());
            touch_start.set(Some(ts));
            touch_last.set(Some(ts));
            is_paused.set(true);
        }
    };

    let on_touch_move = move |ev: leptos::ev::TouchEvent| {
        if let Some(t) = ev.touches().get(0) {
            touch_last.set(Some((t.client_x() as f64, t.client_y() as f64, now_ms())));
        }
    };

    let on_touch_end = move |_ev: leptos::ev::TouchEvent| {
        is_paused.set(false);

        if let (Some((sx, sy, st)), Some((ex, ey, _))) = (touch_start.get(), touch_last.get()) {
            let dx = ex - sx;
            let dy = ey - sy;
            let dt = now_ms() - st;

            if dx.abs() > dy.abs() && dx.abs() > SWIPE_THRESHOLD_PX && dt < 500.0 {
                // Swipe horizontal → prev/next story
                touch_handled.set_value(true);
                show_detail_tag.set(false);
                if dx < 0.0 {
                    ctx.next();
                } else {
                    ctx.prev();
                }
            } else if dy < -(SWIPE_THRESHOLD_PX) && dt < 450.0 {
                // ── Swipe UP — navigasi ke event jika story punya event_slug ──
                // Ini meniru gesture "See More" / swipe-up Instagram.
                // Syarat: swipe ke atas (dy negatif) dan lebih dominan dari horizontal.
                touch_handled.set_value(true);
                let slug = ctx.with_current_story(|s| s.event_slug.clone()).flatten();
                if let Some(slug) = slug {
                    if let Some(win) = web_sys::window() {
                        // Haptic feedback jika tersedia (mobile)
                        if let Ok(vibrate_fn) = js_sys::Reflect::get(
                            &win.navigator(),
                            &wasm_bindgen::JsValue::from_str("vibrate"),
                        ) {
                            if vibrate_fn.is_function() {
                                let f: js_sys::Function = vibrate_fn.unchecked_into();
                                let _ = f.call1(
                                    &win.navigator(),
                                    &wasm_bindgen::JsValue::from_f64(40.0),
                                );
                            }
                        }
                        let _ = win.location().set_href(&format!("/events/{}", slug));
                    }
                }
            } else if dy > TAP_THRESHOLD_PX * 1.6 {
                // Swipe DOWN → tutup story viewer
                touch_handled.set_value(true);
                ctx.close();
            }
            // Tap biasa: biarkan click event menangani (termasuk double-tap logic)
        }

        touch_start.set(None);
        touch_last.set(None);
    };

    let on_media_click = move |ev: leptos::ev::MouseEvent| {
        // FIX P0-b: batalkan jika touch sudah menangani gesture ini
        if touch_handled.get_value() {
            touch_handled.set_value(false);
            return;
        }

        let Some(target) = ev.current_target() else {
            return;
        };
        let rect = target
            .unchecked_into::<web_sys::Element>()
            .get_bounding_client_rect();
        let x = ev.client_x() as f64 - rect.left();
        let y = ev.client_y() as f64 - rect.top();
        let w = rect.width();
        let now = now_ms();

        // Double-tap → like
        if let Some((lx, ly, lt)) = last_tap.get_value() {
            if now - lt < DOUBLE_TAP_MS
                && (x - lx).abs() < DOUBLE_TAP_DISTANCE_PX
                && (y - ly).abs() < DOUBLE_TAP_DISTANCE_PX
            {
                is_liked.set(true);
                spawn_heart_burst(x, y, &media_area_ref, &active_hearts);
                last_tap.set_value(None);
                return;
            }
        }
        last_tap.set_value(Some((x, y, now)));

        if x < w * 0.33 {
            show_detail_tag.set(false);
            ctx.prev();
        } else if x > w * 0.67 {
            show_detail_tag.set(false);
            ctx.next();
        } else {
            // Tap di tengah frame — tampilkan/sembunyikan tag "klik detail"
            // hanya jika story ini punya event_slug untuk dituju.
            // Saat tag muncul, pause RAF agar story tidak auto-advance sebelum
            // user sempat klik. Saat tag disembunyikan, RAF resume kembali.
            let has_slug = ctx
                .with_current_story(|s| s.event_slug.clone())
                .flatten()
                .map(|s| !s.is_empty())
                .unwrap_or(false);
            if has_slug {
                let new_visible = !show_detail_tag.get_untracked();
                show_detail_tag.set(new_visible);
                is_paused.set(new_visible);
            }
        }
    };

    let on_hold_start = move |_: leptos::ev::MouseEvent| is_paused.set(true);
    let on_hold_end = move |_: leptos::ev::MouseEvent| is_paused.set(false);

    // ══════════════════════════════════════════════════════════════════
    // CLEANUP
    // ══════════════════════════════════════════════════════════════════
    on_cleanup(move || {
        cancel_raf(&raf_id);
        raf_state.update_value(|s| s.cancelled = true);

        if let Some(Some(old)) = raf_closure.try_update_value(|o| o.take()) {
            drop(old);
        }

        #[cfg(target_arch = "wasm32")]
        if let Some(win) = web_sys::window() {
            if let Some(Some(old)) = kb_fn.try_update_value(|o| o.take()) {
                let _ = win.remove_event_listener_with_callback(
                    "keydown",
                    old.as_ref().unchecked_ref::<web_sys::js_sys::Function>(),
                );
                drop(old);
            }
            if let Some(doc) = win.document() {
                if let Some(Some(old)) = vis_closure.try_update_value(|o| o.take()) {
                    let _ = doc.remove_event_listener_with_callback(
                        "visibilitychange",
                        old.as_ref().unchecked_ref::<web_sys::js_sys::Function>(),
                    );
                    drop(old);
                }
            }
        }

        seg_fill_refs.update_value(|v| v.clear());

        preload_img.update_value(|o| {
            if let Some(img) = o.as_ref() {
                img.set_src("");
            }
        });
        preload_vid.update_value(|o| {
            if let Some(vid) = o.as_ref() {
                vid.set_src("");
            }
        });
        if let Some(Some(old)) = preload_img.try_update_value(|o| o.take()) {
            drop(old);
        }
        if let Some(Some(old)) = preload_vid.try_update_value(|o| o.take()) {
            drop(old);
        }

        ctx.close();
    });

    // ══════════════════════════════════════════════════════════════════
    // HELPER: Heart burst
    // FIX P1-a: gunakan Closure::once (bukan Fn) → captures dibebaskan
    //           setelah callback dijalankan; leak hanya shell closure kosong
    // ══════════════════════════════════════════════════════════════════
    fn spawn_heart_burst(
        x: f64,
        y: f64,
        container_ref: &NodeRef<Div>,
        active_hearts: &RwSignal<u32>,
    ) {
        if active_hearts.get_untracked() >= MAX_CONCURRENT_HEARTS {
            return;
        }
        active_hearts.update(|v| *v += 1);

        let Some(container) = container_ref.get() else {
            active_hearts.update(|v| *v -= 1);
            return;
        };
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
            active_hearts.update(|v| *v -= 1);
            return;
        };
        let Ok(el) = doc.create_element("div") else {
            active_hearts.update(|v| *v -= 1);
            return;
        };
        let Some(win) = web_sys::window() else {
            active_hearts.update(|v| *v -= 1);
            return;
        };

        let html_el: &web_sys::HtmlElement = el.unchecked_ref();
        let svg = r##"<svg width="80" height="80" viewBox="0 0 24 24" fill="#ff3040" stroke="white" stroke-width="1"><path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/></svg>"##;
        let _ = html_el.set_inner_html(svg);
        let s = html_el.style();
        let _ = s.set_property("position", "absolute");
        let _ = s.set_property("left", &format!("{}px", x - 40.0));
        let _ = s.set_property("top", &format!("{}px", y - 40.0));
        let _ = s.set_property("pointer-events", "none");
        let _ = s.set_property("z-index", "50");
        let _ = s.set_property("transform", "scale(0)");
        let _ = s.set_property("opacity", "1");
        let _ = s.set_property(
            "transition",
            "transform 0.45s cubic-bezier(0.175,0.885,0.32,1.275), opacity 0.45s ease-out",
        );
        let _ = container
            .unchecked_ref::<web_sys::Element>()
            .append_child(&el);

        let active_hearts = *active_hearts;

        // FIX P1-a: Closure::once → FnOnce semantics
        // Setelah callback dipanggil JS, semua captures di-move keluar / drop.
        // Satu-satunya leak adalah shell Closure struct kosong (beberapa byte).
        let anim = Closure::once({
            let el = el.clone();
            let win = win.clone();
            move || {
                let h: &web_sys::HtmlElement = el.unchecked_ref();
                let _ = h.style().set_property("transform", "scale(1.3)");

                // Clone el sebelum move ke fade closure
                let el_fade = el.clone();
                let fade = Closure::once(move || {
                    let h: &web_sys::HtmlElement = el_fade.unchecked_ref();
                    let _ = h.style().set_property("opacity", "0");
                    let _ = h.style().set_property("transform", "scale(1.6)");
                    h.remove();
                    active_hearts.update(|v| *v -= 1);
                    // el_fade, active_hearts di-drop di sini (FnOnce selesai)
                });
                let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(
                    fade.as_ref().unchecked_ref(),
                    450,
                );
                fade.forget(); // one-shot; setelah 450ms JS GC bebaskan captures
            }
        });
        let _ = win.request_animation_frame(anim.as_ref().unchecked_ref());
        anim.forget(); // one-shot; setelah RAF fires JS GC bebaskan captures
    }

    // ══════════════════════════════════════════════════════════════════
    // VIEW
    // ══════════════════════════════════════════════════════════════════
    view! {
        {move || {
            ctx.active_group
                .get()
                .map(|_| {
                    view! {
                        <div class="sv-portal">
                <div class="sv-backdrop" on:click=move |_| ctx.close()></div>

                <div class="sv-container"
                     on:touchstart=on_touch_start
                     on:touchmove=on_touch_move
                     on:touchend=on_touch_end>

                    // ── Progress bar ──────────────────────────────────
                    // FIX P1-b: buat NodeRef BARU tiap kali closure reaktif re-run.
                    // Fresh refs → tidak ada stale ref dari story/group sebelumnya.
                    // NodeRef adalah Rc-based; clone-nya berbagi inner yang sama →
                    // setelah DOM mount, refs di seg_fill_refs otomatis ter-update.
                    <div class="sv-progress-row">
                        {move || {
                            let n = ctx.current_group_len();
                            let fresh: Vec<NodeRef<Div>> =
                                (0..n).map(|_| NodeRef::new()).collect();
                            // Simpan ke StoredValue agar RAF bisa mengaksesnya
                            seg_fill_refs.set_value(fresh.clone());

                            fresh.into_iter().enumerate().map(|(_, node_ref)| {
                                view! {
                                    <div class="sv-seg">
                                        <div class="sv-seg-fill" node_ref=node_ref></div>
                                    </div>
                                }
                            }).collect_view()
                        }}
                    </div>

                    // ── Header ────────────────────────────────────────
                    {move || ctx.with_current_story(|s| {
                        let now = chrono::Utc::now();
                        let hours = (now - s.created_at).num_hours();
                        let mins  = (now - s.created_at).num_minutes().max(1);
                        let time_label = if hours < 1 { format!("{mins}m") } else { format!("{hours}h") };
                        let username   = s.username.clone();
                        let avatar_url = s.avatar_url.clone();
                        view! {
                            <div class="sv-header">
                                <div class="sv-header-left">
                                    <div class="sv-avatar-ring">
                                        <img src=avatar_url class="sv-avatar"
                                             alt=username.clone()
                                             loading="eager" decoding="async" />
                                    </div>
                                    <div class="sv-user-info">
                                        <span class="sv-username">{username}</span>
                                        <span class="sv-meta">{time_label}</span>
                                    </div>
                                </div>
                                <div class="sv-header-right">
                                    <AudioPill is_muted=is_muted />
                                    <button class="sv-close-btn" aria-label="Tutup story"
                                            on:click=move |ev| { ev.stop_propagation(); ctx.close(); }>
                                        <svg width="20" height="20" viewBox="0 0 24 24"
                                             fill="none" stroke="currentColor"
                                             stroke-width="2.5" stroke-linecap="round">
                                            <line x1="18" y1="6"  x2="6"  y2="18"/>
                                            <line x1="6"  y1="6"  x2="18" y2="18"/>
                                        </svg>
                                    </button>
                                </div>
                            </div>
                        }
                    })}

                    // ── Media area ────────────────────────────────────
                    <div class="sv-media-area" node_ref=media_area_ref
                         on:mousedown=on_hold_start
                         on:mouseup=on_hold_end
                         on:mouseleave=on_hold_end
                         on:click=on_media_click>

                        {move || ctx.with_current_story(|s| {
                            let filter_class = match s.filter.as_deref() {
                                Some(f) if !f.is_empty() =>
                                    format!("sv-media filter-{}", f.to_lowercase()),
                                _ => "sv-media".to_string(),
                            };
                            let url    = s.media_url.clone();
                            let poster = s.media_url.clone();

                            let filter_class2 = filter_class.clone();
                            match &s.media_type {
                                StoryMediaType::Image => Either::Left(view! {
                                    // ── Instagram-style loading shell ──────────────
                                    <div class=move || {
                                        if img_loading.get() { "sv-img-shell sv-img-loading" }
                                        else { "sv-img-shell" }
                                    }>
                                        // Shimmer skeleton (visible while loading)
                                        <div class="sv-shimmer"></div>
                                        <img src=url
                                             class=move || {
                                                 let base = filter_class2.clone();
                                                 if img_loading.get() {
                                                     format!("{} sv-img-hidden", base)
                                                 } else {
                                                     format!("{} sv-img-visible", base)
                                                 }
                                             }
                                             draggable="false"
                                             loading="eager"
                                             decoding="async"
                                             on:load=move |_| {
                                                 // Image loaded — hide shimmer, start RAF
                                                 img_loading.set(false);
                                                 if waiting_for_image.get_value() {
                                                     waiting_for_image.set_value(false);
                                                     let current_si = ctx.active_story_idx.get_untracked();
                                                     raf_state.update_value(|st| {
                                                         if st.active_idx == current_si {
                                                             st.start_ms = now_ms();
                                                             st.cancelled = false;
                                                         }
                                                     });
                                                     let rc = raf_closure.clone();
                                                     let ri = raf_id.clone();
                                                     let rs = raf_state.clone();
                                                     // Closure::once → FnOnce: captures freed after first RAF tick,
                                                     // not leaked permanently like Closure::new(Fn) would be.
                                                     let setup = Closure::once(move || {
                                                         let st = rs.get_value();
                                                         if !st.cancelled && st.paused_at_ms == 0.0 && ri.get_value() == 0 {
                                                             start_raf(&rc, &ri);
                                                         }
                                                     });
                                                     if let Some(win) = web_sys::window() {
                                                         let _ = win.request_animation_frame(setup.as_ref().unchecked_ref());
                                                     }
                                                     setup.forget();
                                                 }
                                             }
                                        />
                                    </div>
                                }),

                                StoryMediaType::Video => Either::Right(view! {
                                    <video src=url class=filter_class
                                           muted autoplay playsinline
                                           poster=poster
                                           node_ref=video_ref
                                           on:loadedmetadata=move |ev: leptos::ev::Event| {
                                               // FIX P0-a: update durasi DAN mulai RAF dari sini
                                               // Tidak ada "progress jump" karena RAF belum jalan sebelumnya
                                               let vid = ev.target()
                                                   .and_then(|t| t.dyn_into::<web_sys::HtmlVideoElement>().ok());
                                               if let Some(v) = vid {
                                                   let dur = v.duration() * 1000.0;
                                                   if dur > 0.0 {
                                                       video_duration_ms.set_value(dur);
                                                       let current_si = ctx.active_story_idx.get_untracked();
                                                       raf_state.update_value(|st| {
                                                           if st.active_idx == current_si {
                                                               st.duration_ms = dur;
                                                               // Reset start ke sekarang — progress mulai dari 0
                                                               st.start_ms = web_sys::window()
                                                                   .and_then(|w| w.performance())
                                                                   .map(|p| p.now())
                                                                   .unwrap_or(0.0);
                                                               st.cancelled = false;
                                                           }
                                                       });
                                                       // Mulai RAF hanya jika benar-benar menunggu video ini
                                                       if waiting_for_video.get_value()
                                                           && raf_id.get_value() == 0
                                                       {
                                                           waiting_for_video.set_value(false);
                                                           start_raf(&raf_closure, &raf_id);
                                                       }
                                                   }
                                               }
                                           }
                                    />
                                }),
                            }
                        })}
                    </div>

                    // ── Instagram-style Event Detail Sheet ────────────────────
                    // Muncul saat frame di-tap (di tengah) dan story punya event_slug.
                    // Bottom sheet slide-up dari bawah: cover image, judul event, CTA button.
                    // Tap backdrop (luar sheet) → tutup dan resume RAF.
                    {move || {
                        if !show_detail_tag.get() {
                            return None;
                        }
                        ctx.with_current_story(|s| {
                            let slug = s.event_slug.clone().filter(|e| !e.is_empty())?;
                            let title = s.event_title.clone()
                                .unwrap_or_else(|| "Lihat Event".to_string());
                            let cover = s.media_url.clone();
                            let slug_nav = slug.clone();
                            Some(view! {
                                <div class="sv-detail-overlay"
                                     on:click=move |ev| {
                                         if let (Some(t), Some(c)) = (ev.target(), ev.current_target()) {
                                             if t == c {
                                                 show_detail_tag.set(false);
                                                 is_paused.set(false);
                                             }
                                         }
                                     }>
                                    <div class="sv-detail-sheet"
                                         on:click=move |ev| ev.stop_propagation()>
                                        <div class="sv-detail-handle-bar"></div>
                                        <div class="sv-detail-cover-wrap">
                                            <img src=cover class="sv-detail-cover-img" alt="" />
                                            <div class="sv-detail-cover-grad"></div>
                                        </div>
                                        <div class="sv-detail-body">
                                            <span class="sv-detail-eyebrow">"EVENT"</span>
                                            <h3 class="sv-detail-title">{title}</h3>
                                            <button class="sv-detail-cta"
                                                    on:click=move |ev| {
                                                        ev.stop_propagation();
                                                        if let Some(win) = web_sys::window() {
                                                            let _ = win.location()
                                                                .set_href(&format!("/events/{}", slug_nav));
                                                        }
                                                    }>
                                                "Lihat Event"
                                                <svg width="16" height="16" viewBox="0 0 24 24"
                                                     fill="none" stroke="currentColor"
                                                     stroke-width="2.5" stroke-linecap="round">
                                                    <line x1="5" y1="12" x2="19" y2="12"/>
                                                    <polyline points="12 5 19 12 12 19"/>
                                                </svg>
                                            </button>
                                        </div>
                                    </div>
                                </div>
                            })
                        }).flatten()
                    }}

                    // ── Pulse badge ───────────────────────────────────
                    <div class="sv-pulse-badge">
                        <svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor">
                            <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z"/>
                        </svg>
                        <span>{move || pulse_label.get()}</span>
                        <span>" pulses"</span>
                    </div>

                    // ── Bottom bar ────────────────────────────────────
                    <div class="sv-actions sv-actions--readonly">
                        <div class="sv-viewer-info">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
                                <circle cx="12" cy="12" r="3"/>
                            </svg>
                            <span class="sv-viewer-count">{move || view_label.get()}</span>
                        </div>
                        <button class="sv-action-btn sv-like-btn"
                                class:sv-like-btn--active=move || is_liked.get()
                                aria-label=move || if is_liked.get() { "Batal suka" } else { "Suka" }
                                on:click=move |ev| {
                                    ev.stop_propagation();
                                    is_liked.update(|v| *v = !*v);
                                }>
                            <svg width="22" height="22" viewBox="0 0 24 24"
                                 fill=move || if is_liked.get() { "currentColor" } else { "none" }
                                 stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                <path d="M20.84 4.61a5.5 5.5 0 00-7.78 0L12 5.67l-1.06-1.06a5.5 5.5 0 00-7.78 7.78l1.06 1.06L12 21.23l7.78-7.78 1.06-1.06a5.5 5.5 0 000-7.78z"/>
                            </svg>
                        </button>
                    </div>

                    // ── Tap zones ─────────────────────────────────────
                    <div class="sv-tap-zone sv-tap-zone--left"
                         on:click=move |ev| { ev.stop_propagation(); ctx.prev(); }>
                    </div>
                    <div class="sv-tap-zone sv-tap-zone--right"
                         on:click=move |ev| { ev.stop_propagation(); ctx.next(); }>
                    </div>

                </div>
            </div>
                    }
                })
        }}
    }
}
