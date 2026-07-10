//! user_public.rs — Profil publik user biasa (/u/:id).
//!
//! Hanya menampilkan STORY (tab "Pulses") & ULASAN yang user tulis ke merchant
//! (tab "Reviews"). TANPA bottom navbar. Skema user hanya punya `name` (tak ada
//! username/avatar/bio), jadi avatar = inisial. Diakses dari daftar follower
//! (/m/:id/followers) dan dari penulis ulasan (/m/:id/reviews + panel ULASAN).

use leptos::html::Div;
use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::web::api::{get_merchant_stories, get_user_public, get_user_reviews};
use crate::web::app::AuthResource;
use crate::web::components::story_viewer::StoryViewer;
use crate::web::hooks::ThemeToggle;
use crate::web::state::stories::StoryMediaType;
use crate::web::state::stories::use_stories_store;

use super::merchant_public::{fmt_count, now_ms};

/// Baris bintang statis (ulasan).
#[component]
fn Stars(#[prop(into)] rating: f64) -> impl IntoView {
    view! {
        <span class="mrv-stars" aria-label=format!("{rating:.1} dari 5")>
            {(1..=5)
                .map(|i| {
                    let cls = if (i as f64) <= rating + 0.25 {
                        "mrv-star mrv-star--on"
                    } else {
                        "mrv-star"
                    };
                    view! { <span class=cls>"★"</span> }
                })
                .collect_view()}
        </span>
    }
}

#[component]
pub fn UserPublicPage() -> impl IntoView {
    let params = use_params_map();
    let uid = move || params.read().get("id").unwrap_or_default();
    let auth = use_context::<AuthResource>().expect("AuthResource missing");

    let profile = Resource::new(uid, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("not_ready".into()));
        }
        get_user_public(id).await
    });
    let stories = Resource::new(uid, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("not_ready".into()));
        }
        get_merchant_stories(id).await
    });
    let reviews = Resource::new(uid, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("not_ready".into()));
        }
        get_user_reviews(id, Some(1)).await
    });

    // 0 = Pulses (story), 1 = Reviews. Panel bisa "digeser" (swipe horizontal)
    // antar-tab atau lewat klik tab — mekanisme sama dengan profil merchant.
    const TAB_COUNT: usize = 2;
    let tab = RwSignal::new(0usize);

    // ── Swipe antar-panel (carousel: kedua panel selalu dirender) ───────────────
    // Panel aktif `position:relative` (menentukan tinggi), lainnya `absolute`
    // digeser ±100% via translateX(calc(N% + Dpx)). Ambang 45px ATAU flick;
    // sumbu dikunci di gerak pertama agar tak membajak scroll vertikal.
    const SWIPE_PX: f64 = 45.0;
    const SWIPE_VEL: f64 = 0.4;
    let swipe_ref = NodeRef::<Div>::new();
    let drag_start = RwSignal::new(None::<(f64, f64)>);
    let drag_dx = RwSignal::new(0f64);
    let dragging = RwSignal::new(false);
    let drag_axis = RwSignal::new(0i8); // 0 belum, 1 horizontal, 2 vertikal
    let drag_t0 = RwSignal::new(0f64);

    let on_pointer_down = move |ev: leptos::ev::PointerEvent| {
        drag_start.set(Some((ev.client_x() as f64, ev.client_y() as f64)));
        drag_axis.set(0);
        drag_dx.set(0.0);
        drag_t0.set(now_ms());
    };
    let on_pointer_move = move |ev: leptos::ev::PointerEvent| {
        let Some((sx, sy)) = drag_start.get_untracked() else {
            return;
        };
        let dx = ev.client_x() as f64 - sx;
        let dy = ev.client_y() as f64 - sy;
        if drag_axis.get_untracked() == 0 {
            if dx.abs() > 8.0 || dy.abs() > 8.0 {
                if dx.abs() > dy.abs() {
                    drag_axis.set(1);
                    dragging.set(true);
                    if let Some(el) = swipe_ref.get_untracked() {
                        let _ = el.set_pointer_capture(ev.pointer_id());
                    }
                } else {
                    drag_axis.set(2);
                }
            }
        }
        if drag_axis.get_untracked() == 1 {
            let t = tab.get_untracked();
            // Tahanan di tepi (tak ada panel sebelum 0 / sesudah terakhir).
            let d = if (t == 0 && dx > 0.0) || (t == TAB_COUNT - 1 && dx < 0.0) {
                dx * 0.35
            } else {
                dx
            };
            drag_dx.set(d);
        }
    };
    let on_pointer_up = move |ev: leptos::ev::PointerEvent| {
        let was_h = drag_axis.get_untracked() == 1;
        drag_start.set(None);
        drag_axis.set(0);
        if was_h {
            if let Some(el) = swipe_ref.get_untracked() {
                if el.has_pointer_capture(ev.pointer_id()) {
                    let _ = el.release_pointer_capture(ev.pointer_id());
                }
            }
            let d = drag_dx.get_untracked();
            let dt = (now_ms() - drag_t0.get_untracked()).max(1.0);
            let vel = d / dt; // px/ms, bertanda (negatif = geser kiri)
            let t = tab.get_untracked();
            let go_next = d <= -SWIPE_PX || vel <= -SWIPE_VEL;
            let go_prev = d >= SWIPE_PX || vel >= SWIPE_VEL;
            if go_next && t < TAB_COUNT - 1 {
                tab.set(t + 1);
            } else if go_prev && t > 0 {
                tab.set(t - 1);
            }
        }
        dragging.set(false);
        drag_dx.set(0.0);
    };

    // Transform per panel: (i - tab)*100% + geseran drag (px).
    let panel_tf = move |i: usize| {
        let base = (i as f64 - tab.get() as f64) * 100.0;
        let dx = if dragging.get() { drag_dx.get() } else { 0.0 };
        format!("transform:translateX(calc({base}% + {dx}px))")
    };

    // Buka viewer story user (login required — konsisten StoryBar).
    let ctx = use_stories_store();
    let navigate = use_navigate();
    let open_story = {
        let navigate = navigate.clone();
        move |list: Vec<crate::web::state::stories::StoryGroup>, idx: usize| {
            let logged_in = auth
                .get_untracked()
                .and_then(|r| r.ok())
                .flatten()
                .is_some();
            if !logged_in {
                navigate("/login", Default::default());
                return;
            }
            ctx.groups.set(list);
            ctx.open_at(0, idx);
        }
    };

    view! {
        <div class="mp-page">
            <header class="page-header mp-header">
                <button
                    class="back-btn"
                    aria-label="Kembali"
                    on:click=move |_| {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(win) = web_sys::window() {
                            let _ = win.history().ok().map(|h| h.back());
                        }
                    }
                >
                    <svg
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                    >
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </button>
                <span class="page-logo">"PROFIL"</span>
                <div class="header-actions">
                    <ThemeToggle />
                </div>
            </header>

            <Suspense fallback=|| {
                view! {
                    <p class="mp-empty" style="text-align:center">
                        "Memuat profil…"
                    </p>
                }
            }>
                {move || {
                    match profile.get() {
                        None => {
                            view! {
                                <p class="mp-empty" style="text-align:center">
                                    "Memuat…"
                                </p>
                            }
                                .into_any()
                        }
                        Some(Err(e)) if e.to_string().contains("not_ready") => {
                            view! {
                                <p class="mp-empty" style="text-align:center">
                                    "Memuat…"
                                </p>
                            }
                                .into_any()
                        }
                        Some(Err(_)) => {
                            view! {
                                <div class="mp-container">
                                    <div class="medit-error-banner">"User tidak ditemukan."</div>
                                </div>
                            }
                                .into_any()
                        }
                        Some(Ok(p)) => {
                            let initial: String = p
                                .name
                                .chars()
                                .next()
                                .unwrap_or('P')
                                .to_uppercase()
                                .to_string();
                            let open_story = open_story.clone();
                            view! {
                                // ── Kepala profil (terpusat) ──────────────────
                                <div class="up-head">
                                    <div class="up-avatar">{initial}</div>
                                    <h1 class="up-name">{p.name.clone()}</h1>
                                </div>

                                <div class="mp-container">
                                    <div class="mp-stats">
                                        <div class="mp-stat">
                                            <span class="mp-stat-num">{fmt_count(p.stories)}</span>
                                            <span class="mp-stat-label">"STORY"</span>
                                        </div>
                                        <div class="mp-stat">
                                            <span class="mp-stat-num">{fmt_count(p.following)}</span>
                                            <span class="mp-stat-label">"MENGIKUTI"</span>
                                        </div>
                                        <div class="mp-stat">
                                            <span class="mp-stat-num">{fmt_count(p.reviews)}</span>
                                            <span class="mp-stat-label">"ULASAN"</span>
                                        </div>
                                    </div>

                                    // ── Tabs ──────────────────────────────────
                                    <div class="mp-tabs">
                                        {["PULSES", "ULASAN"]
                                            .into_iter()
                                            .enumerate()
                                            .map(|(i, label)| {
                                                view! {
                                                    <button
                                                        class=move || {
                                                            if tab.get() == i { "mp-tab mp-tab--on" } else { "mp-tab" }
                                                        }
                                                        on:click=move |_| tab.set(i)
                                                    >
                                                        {label}
                                                    </button>
                                                }
                                            })
                                            .collect_view()}
                                    </div>

                                    // ── Panel yang bisa digeser (swipe) ───────
                                    <div
                                        class="mp-swipe"
                                        node_ref=swipe_ref
                                        on:pointerdown=on_pointer_down
                                        on:pointermove=on_pointer_move
                                        on:pointerup=on_pointer_up
                                        on:pointercancel=on_pointer_up
                                    >
                                        <div
                                            class="mp-panel"
                                            class:mp-panel--active=move || tab.get() == 0
                                            class:mp-panel--drag=move || dragging.get()
                                            style=move || panel_tf(0)
                                        >
                                            // ── Pulses (story) ──────────────
                                            <div class="mp-stories">
                                                    <Suspense fallback=|| {
                                                        view! { <p class="mp-empty">"Memuat story…"</p> }
                                                    }>
                                                        {
                                                            let open_story = open_story.clone();
                                                            move || {
                                                                let open_story = open_story.clone();
                                                                stories
                                                                    .get()
                                                                    .map(|r| match r {
                                                                        Ok(list) => {
                                                                            let items = list
                                                                                .first()
                                                                                .map(|g| g.stories.clone())
                                                                                .unwrap_or_default();
                                                                            if items.is_empty() {
                                                                                view! { <p class="mp-empty">"Belum ada story."</p> }
                                                                                    .into_any()
                                                                            } else {
                                                                                view! {
                                                                                    <div class="mp-story-grid">
                                                                                        {items
                                                                                            .iter()
                                                                                            .enumerate()
                                                                                            .map(|(i, s)| {
                                                                                                let is_video = s.media_type == StoryMediaType::Video;
                                                                                                let media = s.media_url.clone();
                                                                                                let list_c = list.clone();
                                                                                                let open_story = open_story.clone();
                                                                                                view! {
                                                                                                    <button
                                                                                                        class="mp-story-cell"
                                                                                                        on:click=move |_| open_story(list_c.clone(), i)
                                                                                                    >
                                                                                                        {if is_video {
                                                                                                            view! {
                                                                                                                <video
                                                                                                                    class="mp-story-media"
                                                                                                                    src=media.clone()
                                                                                                                    muted=true
                                                                                                                    playsinline=true
                                                                                                                    preload="metadata"
                                                                                                                ></video>
                                                                                                                <span class="mp-story-play">
                                                                                                                    <svg
                                                                                                                        width="12"
                                                                                                                        height="12"
                                                                                                                        viewBox="0 0 24 24"
                                                                                                                        fill="currentColor"
                                                                                                                    >
                                                                                                                        <polygon points="5 3 19 12 5 21 5 3" />
                                                                                                                    </svg>
                                                                                                                </span>
                                                                                                            }
                                                                                                                .into_any()
                                                                                                        } else {
                                                                                                            view! {
                                                                                                                <img
                                                                                                                    class="mp-story-media"
                                                                                                                    src=media.clone()
                                                                                                                    alt=""
                                                                                                                    loading="lazy"
                                                                                                                />
                                                                                                            }
                                                                                                                .into_any()
                                                                                                        }}
                                                                                                    </button>
                                                                                                }
                                                                                            })
                                                                                            .collect_view()}
                                                                                    </div>
                                                                                }
                                                                                    .into_any()
                                                                            }
                                                                        }
                                                                        Err(_) => {
                                                                            view! { <p class="mp-empty">"Gagal memuat story."</p> }
                                                                                .into_any()
                                                                        }
                                                                    })
                                                            }
                                                        }
                                                    </Suspense>
                                                </div>
                                        </div>
                                        <div
                                            class="mp-panel"
                                            class:mp-panel--active=move || tab.get() == 1
                                            class:mp-panel--drag=move || dragging.get()
                                            style=move || panel_tf(1)
                                        >
                                            // ── Ulasan yang ditulis user ────
                                            <div class="mp-reviews">
                                                    <Suspense fallback=|| {
                                                        view! { <p class="mp-empty">"Memuat ulasan…"</p> }
                                                    }>
                                                        {move || {
                                                            reviews
                                                                .get()
                                                                .map(|r| match r {
                                                                    Ok(d) => {
                                                                        if d.items.is_empty() {
                                                                            view! { <p class="mp-empty">"Belum menulis ulasan."</p> }
                                                                                .into_any()
                                                                        } else {
                                                                            view! {
                                                                                <div class="mrv-list">
                                                                                    {d
                                                                                        .items
                                                                                        .iter()
                                                                                        .map(|r| {
                                                                                            let initial: String = r
                                                                                                .store_name
                                                                                                .chars()
                                                                                                .next()
                                                                                                .unwrap_or('P')
                                                                                                .to_uppercase()
                                                                                                .to_string();
                                                                                            let date = r.created_at.format("%d %b %Y").to_string();
                                                                                            let href = format!("/m/{}", r.merchant_id);
                                                                                            view! {
                                                                                                <a class="mrv-item up-review-link" href=href>
                                                                                                    <div class="mrv-item-head">
                                                                                                        <span class="mrv-item-avatar">{initial}</span>
                                                                                                        <div class="mrv-item-who">
                                                                                                            <span class="mrv-item-name">{r.store_name.clone()}</span>
                                                                                                            <span class="mrv-item-date">{date}</span>
                                                                                                        </div>
                                                                                                        <Stars rating=r.rating as f64 />
                                                                                                    </div>
                                                                                                    {(!r.comment.is_empty())
                                                                                                        .then(|| {
                                                                                                            view! { <p class="mrv-item-text">{r.comment.clone()}</p> }
                                                                                                        })}
                                                                                                </a>
                                                                                            }
                                                                                        })
                                                                                        .collect_view()}
                                                                                </div>
                                                                            }
                                                                                .into_any()
                                                                        }
                                                                    }
                                                                    Err(_) => {
                                                                        view! { <p class="mp-empty">"Gagal memuat ulasan."</p> }
                                                                            .into_any()
                                                                    }
                                                                })
                                                        }}
                                                    </Suspense>
                                                </div>
                                        </div>
                                    </div>
                                </div>
                            }
                                .into_any()
                        }
                    }
                }}
            </Suspense>

            // Viewer fullscreen story (overlay; buka via tab Pulses).
            <StoryViewer />
        </div>
    }
}
