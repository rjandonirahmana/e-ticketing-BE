//! stories_archive.rs — Halaman /stories: arsip publik semua story user yang
//! pernah ada (termasuk yang sudah lewat 24 jam), terbaru dulu.
//!
//! Terbuka untuk pengunjung anonim (daftar bersifat publik — konsisten dengan
//! StoryBar). MEMBUKA story (lightbox) tetap butuh login → redirect /login.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::web::api::get_all_stories;
use crate::web::app::AuthResource;
use crate::web::hooks::ThemeToggle;
use crate::web::state::stories::{StoryItem, StoryMediaType};

const PER_PAGE: usize = 24;

fn fmt_date(d: &chrono::DateTime<chrono::Utc>) -> String {
    d.format("%d %b %Y").to_string()
}

#[component]
pub fn StoriesArchivePage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();
    let navigate = use_navigate();

    // Daftar inkremental: halaman pertama via Resource (ikut SSR), halaman
    // berikutnya di-append lewat tombol "Muat Lebih Banyak".
    let items: RwSignal<Vec<StoryItem>> = RwSignal::new(Vec::new());
    let page = RwSignal::new(1i64);
    let has_more = RwSignal::new(false);
    let loading = RwSignal::new(false);

    let first_page = Resource::new(|| (), |_| get_all_stories(Some(1)));
    Effect::new(move |_| {
        if let Some(Ok(list)) = first_page.get() {
            has_more.set(list.len() == PER_PAGE);
            page.set(2);
            items.set(list);
        }
    });

    let load_more = move |_| {
        if loading.get_untracked() || !has_more.get_untracked() {
            return;
        }
        loading.set(true);
        let next = page.get_untracked();
        leptos::task::spawn_local(async move {
            if let Ok(list) = get_all_stories(Some(next)).await {
                has_more.set(list.len() == PER_PAGE);
                page.set(next + 1);
                items.update(|v| v.extend(list));
            }
            loading.set(false);
        });
    };

    // Lightbox: story yang sedang dibuka (hanya user login).
    let active: RwSignal<Option<StoryItem>> = RwSignal::new(None);

    view! {
        <div class="page sarc-page">
            <header class="page-header">
                <button class="back-btn" aria-label="Kembali"
                    on:click=move |_| {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(win) = web_sys::window() {
                            let _ = win.history().ok().map(|h| h.back());
                        }
                    }>
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                        stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </button>
                <span class="page-logo">"STORIES"</span>
                <div class="header-actions">
                    <ThemeToggle />
                </div>
            </header>

            <div class="sarc-hero">
                <h1 class="sarc-title">"SEMUA STORY"</h1>
                <p class="sarc-sub">
                    "Arsip cerita publik dari semua pengguna — termasuk yang sudah berakhir."
                </p>
            </div>

            <Suspense fallback=|| {
                view! {
                    <div class="sarc-grid">
                        {(0..9i32)
                            .map(|_| view! { <div class="shim sarc-shim-card"></div> })
                            .collect_view()}
                    </div>
                }
            }>
                {move || {
                    // Sentuh resource agar Suspense menunggu halaman pertama di SSR.
                    let _ = first_page.get();
                    let list = items.get();
                    if list.is_empty() {
                        return view! {
                            <div class="sarc-empty">
                                <div class="sarc-empty-icon">"📷"</div>
                                <p class="sarc-empty-title">"Belum ada story"</p>
                                <p class="sarc-empty-sub">
                                    "Jadilah yang pertama membagikan cerita event!"
                                </p>
                                <A href="/story" attr:class="sarc-empty-cta">"Buat Story"</A>
                            </div>
                        }
                            .into_any();
                    }
                    let nav_grid = navigate.clone();
                    view! {
                        <div class="sarc-grid">
                            {list
                                .into_iter()
                                .map(|s| {
                                    let expired = s.expires_at <= chrono::Utc::now();
                                    let username = s.username.clone();
                                    let avatar = s.avatar_url.clone();
                                    let media = s.media_url.clone();
                                    let is_video = s.media_type == StoryMediaType::Video;
                                    let date_str = fmt_date(&s.created_at);
                                    let nav = nav_grid.clone();
                                    let item = s.clone();
                                    view! {
                                        <button
                                            class="sarc-card"
                                            on:click=move |_| {
                                                if is_logged_in() {
                                                    active.set(Some(item.clone()));
                                                } else {
                                                    nav("/login", Default::default());
                                                }
                                            }
                                        >
                                            {if is_video {
                                                view! {
                                                    <video
                                                        class="sarc-card-media"
                                                        src=media.clone()
                                                        muted=true
                                                        playsinline=true
                                                        preload="metadata"
                                                    ></video>
                                                    <span class="sarc-play-badge">
                                                        <svg width="12" height="12" viewBox="0 0 24 24"
                                                            fill="currentColor">
                                                            <polygon points="5 3 19 12 5 21 5 3" />
                                                        </svg>
                                                    </span>
                                                }
                                                    .into_any()
                                            } else {
                                                view! {
                                                    <img
                                                        class="sarc-card-media"
                                                        src=media.clone()
                                                        alt=username.clone()
                                                        loading="lazy"
                                                    />
                                                }
                                                    .into_any()
                                            }}
                                            <div class="sarc-card-grad"></div>
                                            {(!expired)
                                                .then(|| {
                                                    view! {
                                                        <span class="sarc-live-badge">"AKTIF"</span>
                                                    }
                                                })}
                                            <div class="sarc-card-info">
                                                {(!avatar.is_empty())
                                                    .then(|| {
                                                        view! {
                                                            <img
                                                                class="sarc-card-avatar"
                                                                src=avatar.clone()
                                                                alt=username.clone()
                                                                loading="lazy"
                                                            />
                                                        }
                                                    })}
                                                <div class="sarc-card-meta">
                                                    <span class="sarc-card-user">{username.clone()}</span>
                                                    <span class="sarc-card-date">{date_str}</span>
                                                </div>
                                            </div>
                                        </button>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                        .into_any()
                }}
            </Suspense>

            // ── Muat lebih banyak ─────────────────────────────────────────────
            {move || {
                has_more
                    .get()
                    .then(|| {
                        view! {
                            <div class="sarc-more-wrap">
                                <button
                                    class="sarc-more-btn"
                                    disabled=move || loading.get()
                                    on:click=load_more
                                >
                                    {move || {
                                        if loading.get() { "MEMUAT…" } else { "MUAT LEBIH BANYAK" }
                                    }}
                                </button>
                            </div>
                        }
                    })
            }}

            // ── Lightbox (hanya user login) ───────────────────────────────────
            {move || {
                active
                    .get()
                    .map(|s| {
                        let is_video = s.media_type == StoryMediaType::Video;
                        let media = s.media_url.clone();
                        let username = s.username.clone();
                        let avatar = s.avatar_url.clone();
                        let date_str = fmt_date(&s.created_at);
                        let event_link = s
                            .event_slug
                            .clone()
                            .filter(|slug| !slug.is_empty())
                            .map(|slug| {
                                let title = s
                                    .event_title
                                    .clone()
                                    .unwrap_or_else(|| "Lihat Event".into());
                                view! {
                                    <A
                                        href=format!("/events/{slug}")
                                        attr:class="sarc-lb-event"
                                    >
                                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                                            stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                            <rect x="3" y="4" width="18" height="18" rx="2" />
                                            <line x1="16" y1="2" x2="16" y2="6" />
                                            <line x1="8" y1="2" x2="8" y2="6" />
                                            <line x1="3" y1="10" x2="21" y2="10" />
                                        </svg>
                                        {title}
                                    </A>
                                }
                            });
                        view! {
                            <div class="sarc-lightbox" on:click=move |_| active.set(None)>
                                <div class="sarc-lb-inner" on:click=|e| e.stop_propagation()>
                                    <div class="sarc-lb-head">
                                        {(!avatar.is_empty())
                                            .then(|| {
                                                view! {
                                                    <img
                                                        class="sarc-card-avatar"
                                                        src=avatar.clone()
                                                        alt=username.clone()
                                                    />
                                                }
                                            })}
                                        <div class="sarc-card-meta">
                                            <span class="sarc-card-user">{username.clone()}</span>
                                            <span class="sarc-card-date">{date_str}</span>
                                        </div>
                                        <button
                                            class="sarc-lb-close"
                                            aria-label="Tutup"
                                            on:click=move |_| active.set(None)
                                        >
                                            "✕"
                                        </button>
                                    </div>
                                    {if is_video {
                                        view! {
                                            <video
                                                class="sarc-lb-media"
                                                src=media.clone()
                                                controls=true
                                                autoplay=true
                                                playsinline=true
                                            ></video>
                                        }
                                            .into_any()
                                    } else {
                                        view! {
                                            <img class="sarc-lb-media" src=media.clone() alt=username.clone() />
                                        }
                                            .into_any()
                                    }}
                                    {event_link}
                                </div>
                            </div>
                        }
                    })
            }}
        </div>
    }
}
