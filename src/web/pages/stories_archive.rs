//! stories_archive.rs — Halaman /stories: arsip publik story, SATU kartu per
//! user (termasuk story yang sudah lewat 24 jam), user dengan story terbaru
//! dulu. Klik kartu membuka StoryViewer penuh berisi SEMUA story user itu;
//! tap kanan / swipe di akhir grup otomatis lanjut ke user berikutnya.
//!
//! Terbuka untuk pengunjung anonim (daftar bersifat publik — konsisten dengan
//! StoryBar). MEMBUKA story TIDAK butuh login — siapa pun boleh menonton.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::get_story_archive_groups;
use crate::web::components::story_viewer::StoryViewer;
use crate::web::hooks::ThemeToggle;
use crate::web::state::stories::{use_stories_store, StoryGroup, StoryMediaType};

/// Jumlah USER per halaman (bukan jumlah story).
const PER_PAGE: usize = 24;

fn fmt_date(d: &chrono::DateTime<chrono::Utc>) -> String {
    d.format("%d %b %Y").to_string()
}

#[component]
pub fn StoriesArchivePage() -> impl IntoView {
    let ctx = use_stories_store();

    // Daftar inkremental: halaman pertama via Resource (ikut SSR), halaman
    // berikutnya di-append lewat tombol "Muat Lebih Banyak".
    let groups: RwSignal<Vec<StoryGroup>> = RwSignal::new(Vec::new());
    let page = RwSignal::new(1i64);
    let has_more = RwSignal::new(false);
    let loading = RwSignal::new(false);

    let first_page = Resource::new(|| (), |_| get_story_archive_groups(Some(1)));
    Effect::new(move |_| {
        if let Some(Ok(list)) = first_page.get() {
            has_more.set(list.len() == PER_PAGE);
            page.set(2);
            groups.set(list);
        }
    });

    let load_more = move |_| {
        if loading.get_untracked() || !has_more.get_untracked() {
            return;
        }
        loading.set(true);
        let next = page.get_untracked();
        leptos::task::spawn_local(async move {
            if let Ok(list) = get_story_archive_groups(Some(next)).await {
                has_more.set(list.len() == PER_PAGE);
                page.set(next + 1);
                groups.update(|v| v.extend(list));
            }
            loading.set(false);
        });
    };

    // Buka StoryViewer pada grup ke-idx: sinkronkan seluruh grup arsip yang
    // sudah termuat ke stories store, sehingga next() di akhir grup otomatis
    // lanjut ke user berikutnya (dan prev() ke user sebelumnya).
    let open_group = move |idx: usize| {
        ctx.groups.set(groups.get_untracked());
        ctx.open(idx);
    };

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
                    "Satu kartu per pengguna — buka untuk melihat semua story-nya."
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
                    let list = groups.get();
                    if list.is_empty() {
                        return view! {
                            <div class="sarc-empty">
                                <div class="sarc-empty-icon">"📷"</div>
                                <p class="sarc-empty-title">"Belum ada story"</p>
                                <p class="sarc-empty-sub">
                                    "Jadilah yang pertama membagikan cerita product!"
                                </p>
                                <A href="/story" attr:class="sarc-empty-cta">"Buat Story"</A>
                            </div>
                        }
                            .into_any();
                    }
                    view! {
                        <div class="sarc-grid">
                            {list
                                .into_iter()
                                .enumerate()
                                .map(|(idx, g)| {
                                    // Story dalam grup urut ASC → terbaru = last().
                                    let latest = g.stories.last().cloned();
                                    let (cover, is_video, date_str, any_active) = latest
                                        .map(|s| {
                                            (
                                                s.media_url.clone(),
                                                s.media_type == StoryMediaType::Video,
                                                fmt_date(&s.created_at),
                                                g.stories
                                                    .iter()
                                                    .any(|st| st.expires_at > chrono::Utc::now()),
                                            )
                                        })
                                        .unwrap_or((String::new(), false, String::new(), false));
                                    let count = g.stories.len();
                                    let username = g.username.clone();
                                    let avatar = g.avatar_url.clone();
                                    view! {
                                        <button
                                            class="sarc-card"
                                            on:click=move |_| { open_group(idx); }
                                        >
                                            {if is_video {
                                                view! {
                                                    <video
                                                        class="sarc-card-media"
                                                        src=cover.clone()
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
                                                        src=cover.clone()
                                                        alt=username.clone()
                                                        loading="lazy"
                                                    />
                                                }
                                                    .into_any()
                                            }}
                                            <div class="sarc-card-grad"></div>
                                            {(count > 1)
                                                .then(|| {
                                                    view! {
                                                        <span class="sarc-count-badge">
                                                            <svg width="11" height="11" viewBox="0 0 24 24"
                                                                fill="none" stroke="currentColor"
                                                                stroke-width="2.5" stroke-linecap="round">
                                                                <rect x="7" y="3" width="14" height="14" rx="2" />
                                                                <path d="M3 7v12a2 2 0 002 2h12" />
                                                            </svg>
                                                            {count.to_string()}
                                                        </span>
                                                    }
                                                })}
                                            {any_active
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
                                                    <span class="sarc-card-date">
                                                        {format!("{count} story • {date_str}")}
                                                    </span>
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

            // ── Story viewer fullscreen ───────────────────────────────────────
            // Tap kanan / swipe kiri: story berikutnya, lalu user berikutnya.
            <StoryViewer />
        </div>
    }
}
