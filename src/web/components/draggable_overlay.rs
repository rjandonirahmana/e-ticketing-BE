// src/components/draggable_overlay.rs  — rev 2
//
// FIXES vs rev 1:
//  🔴 RAF id disimpan di StoredValue<i32> → cancel_animation_frame on_cleanup.
//     Sebelumnya hanya JsValue holder di-drop; browser tetap fire ghost callback.
//  🔴 coalesced_points: Vec → stack array [(f64,f64);8]. Zero heap alloc hot path.
//  🟡 Spring snap target: clamp(2,98) → clamp(5,95) — magnetic 5% margin ala IG.

use leptos::prelude::*;
use leptos::wasm_bindgen::prelude::*;
use leptos::wasm_bindgen::JsCast;
use web_sys::HtmlElement;

use crate::web::state::stories::{OverlayType, StoryOverlay};

const FRICTION: f64 = 0.88;
const THRESHOLD: f64 = 0.15;
const VEL_SCALE: f64 = 0.55;
const EMA_ALPHA: f64 = 0.65;
const TAP_THRESHOLD_PX: f64 = 8.0;
const TAP_CTRL_DELAY_MS: i32 = 100;
const SCALE_MIN: f64 = 0.1;
const SCALE_MAX: f64 = 5.0;
const BOUNDARY_MIN: f64 = -5.0;
const BOUNDARY_MAX: f64 = 105.0;
const SPRING_K: f64 = 0.10;
// 🟡 FIX: 5% margin snap (dari 2%) — lebih Instagram-feel
const SNAP_MIN: f64 = 5.0;
const SNAP_MAX: f64 = 95.0;
const TRASH_Y: f64 = 82.0;
const TRASH_X_MIN: f64 = 25.0;
const TRASH_X_MAX: f64 = 75.0;

#[derive(Clone, Copy)]
struct OverlayState {
    x: f64,
    y: f64,
    scale: f64,
    rot: f64,
    ox: f64,
    oy: f64,
    sx: f64,
    sy: f64,
    lx: f64,
    ly: f64,
    vx: f64,
    vy: f64,
    ema_vx: f64,
    ema_vy: f64,
    cw: f64,
    ch: f64,
    actually_dragging: bool,
    cancelled: bool,
    over_trash: bool,
    is_pinching: bool, // 🔴 NEW: untuk badge scale
    is_scaling: bool,  // 🔴 NEW: untuk ghost outline
    lift_active: bool, // 🔴 NEW: lift effect saat drag
}

impl OverlayState {
    fn new(x: f64, y: f64, scale: f64, rot: f64) -> Self {
        Self {
            x,
            y,
            scale,
            rot,
            ox: x,
            oy: y,
            sx: 0.0,
            sy: 0.0,
            lx: 0.0,
            ly: 0.0,
            vx: 0.0,
            vy: 0.0,
            ema_vx: 0.0,
            ema_vy: 0.0,
            cw: 390.0,
            ch: 844.0,
            actually_dragging: false,
            cancelled: false,
            over_trash: false,
            is_pinching: false, // 🔴 NEW: untuk badge scale
            is_scaling: false,  // 🔴 NEW: untuk ghost outline
            lift_active: false, // 🔴 NEW: lift effect saat drag
        }
    }
}

#[inline]
fn write_transform_ig(el_ref: NodeRef<leptos::html::Div>, st: &OverlayState, buf: &mut String) {
    buf.clear();
    // 🔴 Instagram: saat drag, teks "diangkat" 1.05x + shadow lebih tebal
    let display_scale = if st.lift_active {
        st.scale * 1.05
    } else {
        st.scale
    };

    let _ = std::fmt::write(
        buf,
        format_args!(
            "translate3d({:.2}cqw,{:.2}cqh,0) translate(-50%,-50%) scale({:.3}) rotate({:.1}deg)",
            st.x, st.y, display_scale, st.rot
        ),
    );

    if let Some(el) = el_ref.get() {
        let html_el = el.unchecked_ref::<HtmlElement>();
        let style = html_el.style();
        let _ = style.set_property("transform", buf);

        // 🔴 Ghost outline + shadow saat pinch/scale
        if st.is_scaling || st.is_pinching {
            let _ = style.set_property("outline", "1.5px solid rgba(255,255,255,0.6)");
            let _ = style.set_property("outline-offset", "8px");
            let _ = style.set_property("filter", "drop-shadow(0 8px 24px rgba(0,0,0,0.45))");
        } else if st.lift_active {
            let _ = style.set_property("outline", "none");
            let _ = style.set_property("filter", "drop-shadow(0 4px 12px rgba(0,0,0,0.35))");
        } else {
            let _ = style.set_property("outline", "none");
            let _ = style.set_property("filter", "none");
        }
    }
}

#[inline]
fn write_transform_buf(el_ref: NodeRef<leptos::html::Div>, st: &OverlayState, buf: &mut String) {
    buf.clear();
    let _ = std::fmt::write(
        buf,
        format_args!(
            "translate3d({:.2}cqw,{:.2}cqh,0) translate(-50%,-50%) scale({:.3}) rotate({:.1}deg)",
            st.x, st.y, st.scale, st.rot
        ),
    );
    if let Some(el) = el_ref.get() {
        let _ = el
            .unchecked_ref::<HtmlElement>()
            .style()
            .set_property("transform", buf);
    }
}

// 🔴 FIX: Stack array — zero heap alloc per PointerMove.
//   Browser coalesced events biasanya ≤ 4; buffer 8 aman.
#[inline]
fn coalesced_points<'a>(
    ev: &leptos::ev::PointerEvent,
    buf: &'a mut [(f64, f64); 8],
) -> &'a [(f64, f64)] {
    let web_ev: &web_sys::PointerEvent = ev.unchecked_ref();
    let arr = web_ev.get_coalesced_events();
    let n = arr.length().min(8) as usize;
    if n == 0 {
        buf[0] = (web_ev.client_x() as f64, web_ev.client_y() as f64);
        return &buf[..1];
    }
    let mut count = 0;
    for i in 0..n {
        if let Ok(e) = arr.get(i as u32).dyn_into::<web_sys::PointerEvent>() {
            buf[count] = (e.client_x() as f64, e.client_y() as f64);
            count += 1;
        }
    }
    &buf[..count.max(1)]
}

// 🔴 FIX: Tambah param `raf_id: StoredValue<i32>`.
//   Simpan return value RAF → cancel_animation_frame di cleanup.
#[allow(clippy::too_many_arguments)]
fn inertia_step(
    ph: StoredValue<OverlayState>,
    el_ref: NodeRef<leptos::html::Div>,
    ov_sv: StoredValue<StoryOverlay>,
    on_update: Callback<StoryOverlay>,
    on_trash: Option<Callback<bool>>,
    raf_holder: StoredValue<Option<JsValue>>,
    raf_id: StoredValue<i32>,
    tf_buf: StoredValue<String>,
    last_ts: f64,
) {
    let Some(win) = web_sys::window() else { return };
    let now = win
        .performance()
        .map(|p| p.now())
        .unwrap_or(last_ts + 16.667);
    let dt = ((now - last_ts) / 16.667).clamp(0.5, 3.0);

    let mut should_stop = false;
    let mut should_commit = false;
    let mut trash_change: Option<bool> = None;

    ph.update_value(|s| {
        if s.cancelled {
            s.vx = 0.0;
            s.vy = 0.0;
            s.ema_vx = 0.0;
            s.ema_vy = 0.0;
            s.cancelled = false;
            s.over_trash = false;
            should_stop = true;
            return;
        }

        // 🟡 FIX: Spring ke SNAP_MIN/MAX (5%) bukan 2%
        let target_x = s.x.clamp(SNAP_MIN, SNAP_MAX);
        let target_y = s.y.clamp(SNAP_MIN, SNAP_MAX);
        let dx = target_x - s.x;
        let dy = target_y - s.y;

        if dx.abs() > 0.05 || dy.abs() > 0.05 {
            s.vx += dx * SPRING_K * dt;
            s.vy += dy * SPRING_K * dt;
        }

        let friction = FRICTION.powf(dt);
        s.vx *= friction;
        s.vy *= friction;

        let speed = s.vx.abs().max(s.vy.abs());
        let dist = dx.abs().max(dy.abs());

        if speed < THRESHOLD && dist < 0.2 {
            s.x = target_x;
            s.y = target_y;
            s.vx = 0.0;
            s.vy = 0.0;
            s.ema_vx = 0.0;
            s.ema_vy = 0.0;
            s.over_trash = false;
            should_stop = true;
            should_commit = true;
            return;
        }

        s.x = (s.x + s.vx).clamp(BOUNDARY_MIN, BOUNDARY_MAX);
        s.y = (s.y + s.vy).clamp(BOUNDARY_MIN, BOUNDARY_MAX);

        let over_trash = s.y > TRASH_Y && s.x > TRASH_X_MIN && s.x < TRASH_X_MAX;
        if over_trash != s.over_trash {
            s.over_trash = over_trash;
            trash_change = Some(over_trash);
        }

        tf_buf.update_value(|buf| write_transform_buf(el_ref, s, buf));
    });

    if let Some(ot) = trash_change {
        if let Some(cb) = on_trash {
            cb.run(ot);
        }
    }

    if should_stop {
        if should_commit {
            let s = ph.get_value();
            let mut updated = ov_sv.get_value();
            updated.x = s.x;
            updated.y = s.y;
            updated.scale = Some(s.scale);
            updated.rotation = Some(s.rot);
            on_update.run(updated);
        }
        if let Some(cb) = on_trash {
            cb.run(false);
        }
        raf_id.set_value(0);
        return;
    }

    let f = Closure::once(move || {
        raf_holder.set_value(None);
        inertia_step(
            ph, el_ref, ov_sv, on_update, on_trash, raf_holder, raf_id, tf_buf, now,
        );
    });
    // 🔴 FIX: Simpan id yang dikembalikan
    match win.request_animation_frame(f.as_ref().unchecked_ref()) {
        Ok(id) => {
            raf_id.set_value(id);
            raf_holder.set_value(Some(f.into_js_value()));
        }
        Err(_) => {
            raf_id.set_value(0);
        }
    }
}

#[component]
pub fn DraggableOverlay(
    overlay: StoryOverlay,
    #[prop(into)] on_update: Callback<StoryOverlay>,
    #[prop(into)] on_delete: Callback<String>,
    // 🔴 NEW: dipanggil saat pointerdown agar overlay ini naik ke atas
    #[prop(into)] on_bring_to_front: Callback<String>,
    #[prop(into)] is_selected: Signal<bool>,
    #[prop(into)] on_select: Callback<String>,
    #[prop(default = None)] on_trash_hover: Option<Callback<bool>>,
) -> impl IntoView {
    let el_ref = NodeRef::<leptos::html::Div>::new();
    let is_drag = RwSignal::new(false);
    let show_ctrl = is_selected;

    let init_scale = overlay.scale.unwrap_or(1.0);
    let init_rot = overlay.rotation.unwrap_or(0.0);
    // 🔴 z_index dari data overlay, bukan hardcoded
    let init_z = overlay.z_index;

    let ph = StoredValue::new(OverlayState::new(
        overlay.x, overlay.y, init_scale, init_rot,
    ));
    let ov_sv = StoredValue::new(overlay.clone());
    let dims: StoredValue<(f64, f64)> = StoredValue::new((390.0, 844.0));
    let tf_buf: StoredValue<String> = StoredValue::new(String::with_capacity(128));

    let pinch_start_dist = StoredValue::new(0.0_f64);
    let pinch_start_scale = StoredValue::new(1.0_f64);
    let pinch_start_angle = StoredValue::new(0.0_f64);
    let base_rot = StoredValue::new(0.0_f64);

    let raf_holder: StoredValue<Option<JsValue>> = StoredValue::new(None);
    let raf_id: StoredValue<i32> = StoredValue::new(0);

    let pc_did_fire = StoredValue::new(false);
    let wheel_timer: StoredValue<Option<i32>> = StoredValue::new(None);
    let wheel_closure: StoredValue<Option<JsValue>> = StoredValue::new(None);
    let tap_timer: StoredValue<Option<i32>> = StoredValue::new(None);
    let tap_closure: StoredValue<Option<JsValue>> = StoredValue::new(None);
    let this_id = StoredValue::new(overlay.id.clone());
    let del_id = StoredValue::new(overlay.id.clone());

    // ── on_cleanup ────────────────────────────────────────────────────────
    on_cleanup(move || {
        ph.update_value(|p| p.cancelled = true);
        let pending_id = raf_id.get_value();
        if pending_id != 0 {
            if let Some(win) = web_sys::window() {
                let _ = win.cancel_animation_frame(pending_id);
            }
            raf_id.set_value(0);
        }
        raf_holder.set_value(None);
        if let Some(id) = wheel_timer.get_value() {
            if let Some(win) = web_sys::window() {
                win.clear_timeout_with_handle(id);
            }
        }
        wheel_closure.set_value(None);
        if let Some(id) = tap_timer.get_value() {
            if let Some(win) = web_sys::window() {
                win.clear_timeout_with_handle(id);
            }
        }
        tap_closure.set_value(None);
    });

    let release_capture = move |ev: &leptos::ev::PointerEvent| {
        if let Some(el) = el_ref.get() {
            let _ = el.release_pointer_capture(ev.pointer_id());
        }
    };

    // ── on:pointerdown ────────────────────────────────────────────────────
    let on_pd = move |ev: leptos::ev::PointerEvent| {
        ev.prevent_default();
        ev.stop_propagation();

        // 🔴 Bring to front saat finger menyentuh overlay
        on_bring_to_front.run(this_id.get_value());

        // Cancel inertia pending
        let pending = raf_id.get_value();
        if pending != 0 {
            if let Some(win) = web_sys::window() {
                let _ = win.cancel_animation_frame(pending);
            }
            raf_id.set_value(0);
        }
        raf_holder.set_value(None);
        ph.update_value(|p| {
            p.cancelled = true;
        });
        pc_did_fire.set_value(false);

        let cx = ev.client_x() as f64;
        let cy = ev.client_y() as f64;

        let (cw, ch) = dims.get_value();
        if cw <= 1.0 || ch <= 1.0 {
            if let Some(el) = el_ref.get() {
                if let Some(parent) = el
                    .parent_element()
                    .and_then(|p| p.dyn_into::<HtmlElement>().ok())
                {
                    dims.set_value((parent.offset_width() as f64, parent.offset_height() as f64));
                }
            }
        }
        let (cw, ch) = dims.get_value();
        let cur = ph.get_value();
        ph.update_value(|p| {
            p.cancelled = false;
            p.actually_dragging = false;
            p.over_trash = false;
            p.ox = cur.x;
            p.oy = cur.y;
            p.sx = cx;
            p.sy = cy;
            p.lx = cx;
            p.ly = cy;
            p.vx = 0.0;
            p.vy = 0.0;
            p.ema_vx = 0.0;
            p.ema_vy = 0.0;
            p.cw = cw;
            p.ch = ch;
        });
        if let Some(el) = el_ref.get() {
            let _ = el.set_pointer_capture(ev.pointer_id());
        }
    };
    // ── on:pointermove — stack array coalesced, zero alloc ────────────────
    let on_pm = move |ev: leptos::ev::PointerEvent| {
        // FIX: Skip pointer drag saat 2-jari pinch aktif.
        // Browser fire pointermove untuk tiap jari saat touchmove, sehingga
        // drag handler bertabrakan dengan touch-pinch handler → overlay jump.
        if ph.get_value().is_pinching {
            return;
        }
        ev.stop_propagation();
        let mut coal_buf = [(0.0_f64, 0.0_f64); 8];
        let pts = coalesced_points(&ev, &mut coal_buf);
        let mut became_drag = false;
        let mut any_movement = false;

        // 🔴 FIX: Batch semua coalesced events, tulis DOM 1x di akhir
        for &(cx, cy) in pts {
            ph.update_value(|s| {
                if !s.actually_dragging {
                    if (cx - s.sx).abs().max((cy - s.sy).abs()) <= TAP_THRESHOLD_PX {
                        return;
                    }
                    s.actually_dragging = true;
                    s.lift_active = true; // 🔴 Instagram lift
                    became_drag = true;
                }
                any_movement = true;

                let raw_vx = (cx - s.lx) * VEL_SCALE;
                let raw_vy = (cy - s.ly) * VEL_SCALE;
                s.ema_vx = EMA_ALPHA * raw_vx + (1.0 - EMA_ALPHA) * s.ema_vx;
                s.ema_vy = EMA_ALPHA * raw_vy + (1.0 - EMA_ALPHA) * s.ema_vy;
                s.vx = s.ema_vx;
                s.vy = s.ema_vy;
                s.lx = cx;
                s.ly = cy;

                // 🔴 Boundary bounce physics (lebih Instagram daripada hard clamp)
                let nx = s.ox + (cx - s.sx) / s.cw * 100.0;
                let ny = s.oy + (cy - s.sy) / s.ch * 100.0;

                if nx < BOUNDARY_MIN || nx > BOUNDARY_MAX {
                    s.vx *= 0.5; // damping saat mentok
                }
                if ny < BOUNDARY_MIN || ny > BOUNDARY_MAX {
                    s.vy *= 0.5;
                }

                s.x = nx.clamp(BOUNDARY_MIN, BOUNDARY_MAX);
                s.y = ny.clamp(BOUNDARY_MIN, BOUNDARY_MAX);
            });
        }

        if became_drag {
            is_drag.set(true);
            on_select.run(String::new());
        }

        // 🔴 Tulis DOM sekali saja, meski ada 4 coalesced events
        if any_movement {
            tf_buf.update_value(|buf| {
                let s = ph.get_value();
                write_transform_ig(el_ref, &s, buf);
            });
        }
    };
    // ── on:pointerup ─────────────────────────────────────────────────────
    let on_pu = move |ev: leptos::ev::PointerEvent| {
        ev.stop_propagation();
        release_capture(&ev);

        // FIX: Reset lift effect saat pointer dilepas.
        // lift_active = true di-set saat drag mulai; tanpa reset ini overlay
        // tetap scale 1.05x setelah drag selesai.
        ph.update_value(|p| p.lift_active = false);

        if pc_did_fire.get_value() {
            pc_did_fire.set_value(false);
            return;
        }
        let p = ph.get_value();

        if !p.actually_dragging {
            let sel = on_select.clone();
            let id = this_id.get_value();
            if let Some(old) = tap_timer.get_value() {
                if let Some(win) = web_sys::window() {
                    win.clear_timeout_with_handle(old);
                }
            }
            tap_closure.set_value(None);
            let f = Closure::once(move || {
                tap_timer.set_value(None);
                tap_closure.set_value(None);
                sel.run(id);
            });
            if let Some(win) = web_sys::window() {
                if let Ok(tid) = win.set_timeout_with_callback_and_timeout_and_arguments_0(
                    f.as_ref().unchecked_ref(),
                    TAP_CTRL_DELAY_MS,
                ) {
                    tap_timer.set_value(Some(tid));
                }
            }
            tap_closure.set_value(Some(f.into_js_value()));
        } else {
            is_drag.set(false);
            if p.vx.abs() > THRESHOLD || p.vy.abs() > THRESHOLD {
                let ts = web_sys::window()
                    .and_then(|w| w.performance())
                    .map(|perf| perf.now())
                    .unwrap_or(0.0);
                inertia_step(
                    ph,
                    el_ref,
                    ov_sv,
                    on_update,
                    on_trash_hover,
                    raf_holder,
                    raf_id,
                    tf_buf,
                    ts,
                );
            } else {
                // 🟡 FIX: Snap ke 5% margin
                let mut s = ph.get_value();
                s.x = s.x.clamp(SNAP_MIN, SNAP_MAX);
                s.y = s.y.clamp(SNAP_MIN, SNAP_MAX);
                s.vx = 0.0;
                s.vy = 0.0;
                s.ema_vx = 0.0;
                s.ema_vy = 0.0;
                ph.set_value(s);
                // FIX: pakai write_transform_ig agar outline/shadow terhapus
                tf_buf.update_value(|buf| write_transform_ig(el_ref, &s, buf));
                let mut updated = ov_sv.get_value();
                updated.x = s.x;
                updated.y = s.y;
                updated.scale = Some(s.scale);
                updated.rotation = Some(s.rot);
                on_update.run(updated);
            }
        }
    };

    let on_pc = move |ev: leptos::ev::PointerEvent| {
        ev.stop_propagation();
        release_capture(&ev);
        pc_did_fire.set_value(true);
        is_drag.set(false);
        ph.update_value(|p| {
            p.vx = 0.0;
            p.vy = 0.0;
            p.ema_vx = 0.0;
            p.ema_vy = 0.0;
            p.actually_dragging = false;
        });
    };

    let on_wheel = move |ev: leptos::ev::WheelEvent| {
        ev.prevent_default();
        ev.stop_propagation();
        let delta = ev.delta_y() * -0.003;
        ph.update_value(|s| {
            s.scale = (s.scale + delta).clamp(SCALE_MIN, SCALE_MAX);
        });
        tf_buf.update_value(|buf| {
            let s = ph.get_value();
            write_transform_buf(el_ref, &s, buf);
        });
        if let Some(id) = wheel_timer.get_value() {
            if let Some(win) = web_sys::window() {
                win.clear_timeout_with_handle(id);
            }
        }
        wheel_closure.set_value(None);
        let f = Closure::once(move || {
            wheel_timer.set_value(None);
            wheel_closure.set_value(None);
            let s = ph.get_value();
            let mut updated = ov_sv.get_value();
            updated.x = s.x;
            updated.y = s.y;
            updated.scale = Some(s.scale);
            updated.rotation = Some(s.rot);
            on_update.run(updated);
        });
        if let Some(win) = web_sys::window() {
            if let Ok(id) = win.set_timeout_with_callback_and_timeout_and_arguments_0(
                f.as_ref().unchecked_ref(),
                150,
            ) {
                wheel_timer.set_value(Some(id));
            }
        }
        wheel_closure.set_value(Some(f.into_js_value()));
    };

    let on_ts = move |ev: leptos::ev::TouchEvent| {
        let touches = ev.touches();
        if touches.length() == 2 {
            ev.prevent_default();
            ev.stop_propagation();
            let t0 = touches.get(0).unwrap();
            let t1 = touches.get(1).unwrap();
            let dx = (t1.client_x() - t0.client_x()) as f64;
            let dy = (t1.client_y() - t0.client_y()) as f64;
            pinch_start_dist.set_value((dx * dx + dy * dy).sqrt());
            pinch_start_scale.set_value(ph.get_value().scale);
            pinch_start_angle.set_value(dy.atan2(dx));
            base_rot.set_value(ph.get_value().rot);
        }
    };

    // Di on_tm (touchmove 2 jari), aktifkan flag is_pinching
    let on_tm = move |ev: leptos::ev::TouchEvent| {
        let touches = ev.touches();
        if touches.length() == 2 {
            ev.prevent_default();
            ev.stop_propagation();
            let t0 = touches.get(0).unwrap();
            let t1 = touches.get(1).unwrap();
            let dx = (t1.client_x() - t0.client_x()) as f64;
            let dy = (t1.client_y() - t0.client_y()) as f64;
            let dist = (dx * dx + dy * dy).sqrt();
            let angle = dy.atan2(dx);
            let start_dist = pinch_start_dist.get_value();
            if start_dist < 1.0 {
                return;
            }

            let new_scale =
                (pinch_start_scale.get_value() * dist / start_dist).clamp(SCALE_MIN, SCALE_MAX);
            let new_rot =
                base_rot.get_value() + (angle - pinch_start_angle.get_value()).to_degrees();

            ph.update_value(|s| {
                s.scale = new_scale;
                s.rot = new_rot;
                s.is_pinching = true; // 🔴 Aktifkan badge
                s.is_scaling = true; // 🔴 Aktifkan ghost outline
            });

            tf_buf.update_value(|buf| {
                let s = ph.get_value();
                write_transform_ig(el_ref, &s, buf);
            });
        }
    };

    // Di on_te (touchend), matikan flag
    let on_te = move |_ev: leptos::ev::TouchEvent| {
        if pinch_start_dist.get_value() > 1.0 {
            let mut s = ph.get_value();
            s.is_pinching = false;
            s.is_scaling = false;
            ph.set_value(s);

            // FIX: Update DOM segera untuk hapus ghost outline + drop-shadow.
            // Tanpa ini, outline tetap visible sampai frame berikutnya.
            tf_buf.update_value(|buf| write_transform_ig(el_ref, &s, buf));

            // Commit ke state
            let mut updated = ov_sv.get_value();
            updated.x = s.x;
            updated.y = s.y;
            updated.scale = Some(s.scale);
            updated.rotation = Some(s.rot);
            on_update.run(updated);
            pinch_start_dist.set_value(0.0);
        }
    };

    let init_style = format!(
        "transform:translate3d({:.2}cqw,{:.2}cqh,0) translate(-50%,-50%) scale({:.3}) rotate({:.1}deg);\
         touch-action:none;will-change:transform;backface-visibility:hidden;\
         contain:layout style;z-index:{};",
        overlay.x, overlay.y, init_scale, init_rot, init_z
    );

    view! {
        <div
            node_ref=el_ref
            class="drg-overlay"
            class:drg-overlay--dragging=move || is_drag.get()
            class:drg-overlay--selected=move || show_ctrl.get()
            class:drg-overlay--trash=move || ph.get_value().over_trash
            class:drg-overlay--pinching=move || ph.get_value().is_pinching
            style=init_style
            on:pointerdown=on_pd
            on:pointermove=on_pm
            on:pointerup=on_pu
            on:pointercancel=on_pc
            on:wheel=on_wheel
            on:touchstart=on_ts
            on:touchmove=on_tm
            on:touchend=on_te
        >
            {match overlay.overlay_type {
                OverlayType::Text => {
                    let color = overlay.color.as_deref().unwrap_or("#fff").to_string();
                    let fs    = overlay.font_size.unwrap_or(24);
                    let rot   = overlay.rotation.unwrap_or(0.0);
                    let content = overlay.content.clone().unwrap_or_default();
                    let bg_class = overlay.text_style.as_deref().unwrap_or("ig-text-classic");
                    let align = overlay.text_align.as_deref()
                    .unwrap_or("center");

                   view! {
                    <div class=format!("drg-text-content {}", bg_class)
                         style=format!(
                             "color:{};font-size:{}px;rotate:{}deg;text-align:{};",
                             color, fs, rot, align
                         )>
                        {content}
                    </div>
                }.into_any()
                }
                OverlayType::Sticker => {
                    let scale = overlay.scale.unwrap_or(1.2);
                    let rot   = overlay.rotation.unwrap_or(0.0);
                    let emoji = overlay.emoji.clone().unwrap_or_default();
                    view! {
                        <div class="drg-sticker-content"
                             style=format!("scale:{scale};rotate:{rot}deg;")>
                            {emoji}
                        </div>
                    }.into_any()
                }
            }}
                <Show when=move || {
                    let s = ph.get_value();
                    s.is_pinching || (show_ctrl.get() && s.scale != 1.0)
                }>
                <div class="drg-scale-badge">
                <span class="drg-scale-badge__dot"></span>
                <span class="drg-scale-badge__text">
                    {move || format!("{:.1}x", ph.get_value().scale)}
                </span>
            </div>
        </Show>

            <Show when=move || show_ctrl.get()>
                <div class="drg-kontrol">
                    <button
                        class="drg-delete-btn drg-delete-btn--ig"
                        aria-label="Hapus overlay"
                        on:click=move |ev| {
                            ev.stop_propagation();
                            on_delete.run(del_id.get_value());
                        }
                    >
                        <svg width="10" height="10" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="3.5" stroke-linecap="round">
                            <line x1="18" y1="6"  x2="6"  y2="18"/>
                            <line x1="6"  y1="6"  x2="18" y2="18"/>
                        </svg>
                    </button>
                </div>
            </Show>
        </div>
    }
}
