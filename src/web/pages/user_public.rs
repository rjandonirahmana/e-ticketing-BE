//! user_public.rs — Profil publik user biasa (/u/:id).
//!
//! Hanya menampilkan STORY (tab "Pulses") & ULASAN yang user tulis ke merchant
//! (tab "Reviews"). TANPA bottom navbar. Skema user hanya punya `name` (tak ada
//! username/avatar/bio), jadi avatar = inisial. Diakses dari daftar follower
//! (/m/:id/followers) dan dari penulis ulasan (/m/:id/reviews + panel ULASAN).

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::web::api::{get_merchant_stories, get_user_public, get_user_reviews};
use crate::web::app::AuthResource;
use crate::web::components::story_viewer::StoryViewer;
use crate::web::hooks::ThemeToggle;
use crate::web::state::stories::StoryMediaType;
use crate::web::state::stories::use_stories_store;

use super::merchant_public::fmt_count;

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

    // 0 = Pulses (story), 1 = Reviews.
    let tab = RwSignal::new(0usize);

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

                                    {move || {
                                        if tab.get() == 0 {
                                            let open_story = open_story.clone();
                                            // ── Pulses (story) ──────────────
                                            view! {
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
                                            }
                                                .into_any()
                                        } else {
                                            // ── Ulasan yang ditulis user ────
                                            view! {
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
                                            }
                                                .into_any()
                                        }
                                    }}
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
