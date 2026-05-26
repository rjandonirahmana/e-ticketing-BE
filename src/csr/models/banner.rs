// use leptos::{component, view, IntoView};
// use leptos::{ev, html, prelude::*};
// use leptos_router::components::A;
// use leptos::wasm_bindgen::JsCast;

// use crate::csr::state::use_banners_store;

// /// Minimum px untuk dianggap swipe (bukan tap)
// const SWIPE_THRESHOLD: f64 = 50.0;
// /// Velocity (px/ms) — flick cepat langsung pindah slide walau jaraknya kecil
// const VELOCITY_THRESHOLD: f64 = 0.45;
// /// Resistensi tarik di tepi pertama/terakhir (0..1)
// const EDGE_RESISTANCE: f64 = 0.32;
// /// Zona mati untuk membedakan tap vs drag
// const TAP_SLOP: f64 = 8.0;
// /// Interval autoplay (ms)
// const AUTOPLAY_MS: u32 = 4_500;

// fn now_ms() -> f64 {
//     web_sys::window()
//         .and_then(|w| w.performance())
//         .map(|p| p.now())
//         .unwrap_or(0.0)
// }

// #[component]
// pub fn BannerSlider() -> impl IntoView {
//     // ── State ─────────────────────────────────────────────────────────────
//     let current = RwSignal::new(0usize);
//     let drag_start_x = RwSignal::new(0.0_f64);
//     let drag_start_y = RwSignal::new(0.0_f64);
//     let drag_delta = RwSignal::new(0.0_f64);
//     let is_dragging = RwSignal::new(false);
//     let axis_locked = RwSignal::new(false);
//     let is_horizontal = RwSignal::new(false);
//     let last_move_x = RwSignal::new(0.0_f64);
//     let last_move_t = RwSignal::new(0.0_f64);
//     let velocity = RwSignal::new(0.0_f64);
//     let moved_enough = RwSignal::new(false);
//     // Pixel step antar slide — diukur dari DOM setelah mount
//     let slide_step_px = RwSignal::new(0.0_f64);
//     // Waktu terakhir interaksi manual (untuk cooldown autoplay)
//     let last_manual_ms = RwSignal::new(0.0_f64);

//     let track_outer_ref = NodeRef::<html::Div>::new();

//     let store = use_banners_store();
//     let banners = store.items.get_untracked();
//     let n = banners.len().max(1);

//     // ── Ukur step slide setelah DOM siap ──────────────────────────────────
//     //
//     // step_px = container_w - 28px (margin first-child 20px vs 6px)
//     //
//     // Fallback ke window.innerWidth jika elemen belum dirender (client_width == 0),
//     // sehingga slider tetap dapat bergerak meski Effect berjalan sebelum paint.
//     Effect::new(move |_| {
//         let container_w = track_outer_ref
//             .get()
//             .map(|el| el.client_width() as f64)
//             .filter(|&w| w > 0.0)
//             .or_else(|| {
//                 web_sys::window()
//                     .and_then(|win| win.inner_width().ok())
//                     .and_then(|v| v.as_f64())
//             })
//             .unwrap_or(375.0);
//         slide_step_px.set(container_w - 28.0);
//     });

//     // ── Autoplay — dibersihkan saat komponen unmount ───────────────────────
//     //
//     // Leptos 0.7 menjalankan reactive runtime di multi-thread sehingga semua
//     // closure yang dipakai di `on_cleanup` harus `Send + Sync`. `gloo_timers`
//     // `Interval` menyimpan `Box<dyn FnMut()>` (tidak Send/Sync) sehingga tidak
//     // bisa dipakai di sini. Pakai `set_interval_with_handle` bawaan Leptos —
//     // handle-nya cuma `i32` JS interval id, jadi Send+Sync dan aman dipindah
//     // ke closure cleanup.
//     let handle = set_interval_with_handle(
//         move || {
//             if is_dragging.get_untracked() {
//                 return;
//             }
//             // Jangan auto-advance jika pengguna baru saja berinteraksi manual
//             let since_manual = now_ms() - last_manual_ms.get_untracked();
//             if since_manual < AUTOPLAY_MS as f64 {
//                 return;
//             }
//             current.update(|c| *c = (*c + 1) % n);
//         },
//         std::time::Duration::from_millis(AUTOPLAY_MS as u64),
//     )
//     .ok();
//     on_cleanup(move || {
//         if let Some(h) = handle {
//             h.clear();
//         }
//     });

//     // ── Pointer down ──────────────────────────────────────────────────────
//     let on_pointer_down = move |ev: ev::PointerEvent| {
//         // Hanya proses pointer utama (left-click / touch / pen utama)
//         if ev.button() != 0 && ev.pointer_type() == "mouse" {
//             return;
//         }
//         // Capture pointer agar move/up diterima walau keluar elemen
//         if let Some(target) = ev.target() {
//             if let Ok(el) = target.dyn_into::<web_sys::Element>() {
//                 let _ = el.set_pointer_capture(ev.pointer_id());
//             }
//         }
//         let x = ev.client_x() as f64;
//         let y = ev.client_y() as f64;
//         drag_start_x.set(x);
//         drag_start_y.set(y);
//         drag_delta.set(0.0);
//         is_dragging.set(true);
//         axis_locked.set(false);
//         is_horizontal.set(false);
//         last_move_x.set(x);
//         last_move_t.set(now_ms());
//         velocity.set(0.0);
//         moved_enough.set(false);
//     };

//     // ── Pointer move ──────────────────────────────────────────────────────
//     let on_pointer_move = move |ev: ev::PointerEvent| {
//         if !is_dragging.get_untracked() {
//             return;
//         }
//         let x = ev.client_x() as f64;
//         let y = ev.client_y() as f64;
//         let dx = x - drag_start_x.get_untracked();
//         let dy = y - drag_start_y.get_untracked();

//         // Kunci sumbu pada gerakan pertama yang melewati zona mati
//         if !axis_locked.get_untracked() {
//             let abs_dx = dx.abs();
//             let abs_dy = dy.abs();
//             if abs_dx < TAP_SLOP && abs_dy < TAP_SLOP {
//                 return; // masih di zona mati, belum bisa tentukan arah
//             }
//             let horiz = abs_dx >= abs_dy;
//             axis_locked.set(true);
//             is_horizontal.set(horiz);
//             // BUG FIX: moved_enough hanya di-set jika sumbu terkunci HORIZONTAL.
//             // Sebelumnya, gerakan vertikal dengan abs_dx > TAP_SLOP juga menyebabkan
//             // moved_enough=true sehingga klik link pada slide disupres secara keliru.
//             if horiz && abs_dx > TAP_SLOP {
//                 moved_enough.set(true);
//             }
//         }

//         // Gerakan vertikal → biarkan halaman scroll
//         if !is_horizontal.get_untracked() {
//             return;
//         }

//         // Cegah scroll halaman saat menggeser horizontal
//         ev.prevent_default();

//         if dx.abs() > TAP_SLOP {
//             moved_enough.set(true);
//         }

//         let cur = current.get_untracked();
//         // Resistensi kenyal di slide pertama (tarik kanan) / terakhir (tarik kiri)
//         let adjusted = if (cur == 0 && dx > 0.0) || (cur + 1 == n && dx < 0.0) {
//             dx * EDGE_RESISTANCE
//         } else {
//             dx
//         };
//         drag_delta.set(adjusted);

//         // Velocity tracking dengan exponential smoothing (px/ms)
//         let t = now_ms();
//         let dt = (t - last_move_t.get_untracked()).max(1.0);
//         let dx_step = x - last_move_x.get_untracked();
//         let inst_v = dx_step / dt;
//         let smoothed = velocity.get_untracked() * 0.7 + inst_v * 0.3;
//         velocity.set(smoothed);
//         last_move_x.set(x);
//         last_move_t.set(t);
//     };

//     // ── Commit drag (pointer up / cancel) ────────────────────────────────
//     let commit_drag = move || {
//         if !is_dragging.get_untracked() {
//             return;
//         }
//         let delta = drag_delta.get_untracked();
//         let v = velocity.get_untracked();

//         // Reset drag state — sinyal diupdate atomis sehingga satu rerender
//         // cukup untuk snap kembali ke posisi target dengan easing.
//         is_dragging.set(false);
//         drag_delta.set(0.0);
//         velocity.set(0.0);

//         // Jika sumbu terkunci vertikal, jangan pindah slide
//         if axis_locked.get_untracked() && !is_horizontal.get_untracked() {
//             return;
//         }

//         let advance_next = delta < -SWIPE_THRESHOLD || v < -VELOCITY_THRESHOLD;
//         let advance_prev = delta > SWIPE_THRESHOLD || v > VELOCITY_THRESHOLD;

//         if advance_next || advance_prev {
//             // Catat waktu interaksi manual untuk cooldown autoplay
//             last_manual_ms.set(now_ms());
//         }

//         if advance_next {
//             current.update(|c| {
//                 if *c + 1 < n {
//                     *c += 1;
//                 }
//             });
//         } else if advance_prev {
//             current.update(|c| {
//                 if *c > 0 {
//                     *c -= 1;
//                 }
//             });
//         }
//     };

//     let on_pointer_up = move |_ev: ev::PointerEvent| commit_drag();
//     let on_pointer_cancel = move |_ev: ev::PointerEvent| commit_drag();

//     // Supres klik link setelah drag (anti navigasi tidak sengaja)
//     let on_click_capture = move |ev: ev::MouseEvent| {
//         if moved_enough.get_untracked() {
//             ev.prevent_default();
//             ev.stop_propagation();
//             moved_enough.set(false);
//         }
//     };

//     // ── Style track (pixel-based) ─────────────────────────────────────────
//     //
//     // BUG FIX: kode lama menggunakan translate3d(-i * 100%, 0, 0) di mana
//     // 100% = lebar track (= lebar container). Namun step antar slide bukan
//     // container_w, melainkan container_w - 28px (akibat margin first-child
//     // 20px vs 6px). Ini menyebabkan slide "overshoot" 28px per langkah.
//     //
//     // Perbaikan: gunakan px berdasarkan step yang diukur dari DOM.
//     let track_style = move || {
//         let step = slide_step_px.get();
//         let cur = current.get();
//         let base = -(cur as f64) * step;

//         if is_dragging.get() && is_horizontal.get() {
//             format!(
//                 "transform: translate3d({:.2}px, 0, 0); \
//                  transition: none; \
//                  will-change: transform;",
//                 base + drag_delta.get()
//             )
//         } else {
//             // Snap dengan easing iOS-like saat lepas
//             format!(
//                 "transform: translate3d({:.2}px, 0, 0); \
//                  transition: transform 460ms cubic-bezier(0.22, 1, 0.36, 1); \
//                  will-change: transform;",
//                 base
//             )
//         }
//     };

//     // Cursor grabbing saat drag aktif
//     let outer_style = move || {
//         let grabbing = is_dragging.get() && is_horizontal.get();
//         format!(
//             "touch-action: pan-y; \
//              user-select: none; \
//              -webkit-user-select: none; \
//              overflow: hidden; \
//              cursor: {};",
//             if grabbing { "grabbing" } else { "grab" }
//         )
//     };

//     // ── View ──────────────────────────────────────────────────────────────
//     view! {
//         <div class="banner-slider-wrap">
//             <div
//                 class="banner-track-outer"
//                 node_ref=track_outer_ref
//                 style=outer_style
//                 on:pointerdown=on_pointer_down
//                 on:pointermove=on_pointer_move
//                 on:pointerup=on_pointer_up
//                 on:pointercancel=on_pointer_cancel
//                 on:click=on_click_capture
//                 on:dragstart=move |ev| ev.prevent_default()
//             >
//                 <div class="banner-track" style=track_style>
//                     {banners.iter().cloned().map(|b| {
//                         let href = format!("/events/{}", b.id);

//                         let badge_cls = match b.badge.as_str() {
//                             "LIVE"     => "slide-badge slide-badge--live",
//                             "SALE"     => "slide-badge slide-badge--sale",
//                             "FEATURED" => "slide-badge slide-badge--featured",
//                             _          => "slide-badge slide-badge--upcoming",
//                         };
//                         let badge_text = match b.badge.as_str() {
//                             "LIVE"     => "● LIVE NOW",
//                             "SALE"     => "⚡ FLASH SALE",
//                             "FEATURED" => "FEATURED EVENT",
//                             _          => "UPCOMING",
//                         };

//                         let subtitle = b.subtitle.clone();
//                         view! {
//                             <div class="banner-slide">
//                                 <A href=href attr:class="banner-slide-inner">
//                                     <img
//                                         src=b.img.clone()
//                                         alt=b.title.clone()
//                                         class="banner-slide-img"
//                                         draggable="false"
//                                     />
//                                     <div class="banner-overlay">
//                                         <span class=badge_cls>{badge_text}</span>
//                                         <h2 class="banner-title">{b.title.clone()}</h2>
//                                         <p class="banner-subtitle">{subtitle}</p>
//                                         <div class="banner-meta">
//                                             <span>{b.date.clone()}</span>
//                                             <span>{b.price.clone()}</span>
//                                         </div>
//                                         <span class="banner-cta-btn">"SECURE TICKETS →"</span>
//                                     </div>
//                                 </A>
//                             </div>
//                         }
//                     }).collect_view()}
//                 </div>
//             </div>

//             // Dots navigasi
//             <div class="banner-dots">
//                 {(0..n).map(|i| {
//                     let cls = move || {
//                         if current.get() == i { "bdot bdot--active" } else { "bdot" }
//                     };
//                     let on_dot_click = move |_| {
//                         current.set(i);
//                         // Reset cooldown agar autoplay tidak langsung override
//                         last_manual_ms.set(now_ms());
//                     };
//                     view! {
//                         <button class=cls on:click=on_dot_click />
//                     }
//                 }).collect_view()}
//             </div>
//         </div>
//     }
// }
