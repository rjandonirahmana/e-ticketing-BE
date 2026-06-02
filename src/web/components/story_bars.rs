use leptos::prelude::*;

use crate::csr::state::stories::use_stories_store;

fn store_from_page() {
    let from = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_else(|| "/explore".to_string());

    if let Some(storage) = web_sys::window()
        .and_then(|w| w.session_storage().ok())
        .flatten()
    {
        let _ = storage.set_item("story_from_page", &from);
    }
}

#[component]
pub fn StoryBar() -> impl IntoView {
    // use_stories_store() → StoriesCtx
    let ctx = use_stories_store();

    view! {
        <div class="story-bar">

            // ── Tombol "Cerita Anda" ─────────────────────────────────────
            <div class="story-item">
                <a
                    href="/stories/new"
                    class="story-add-btn"
                    aria-label="Tambah cerita"
                    on:click=move |_| store_from_page()
                >
                    <div
                        class="story-avatar-ring story-avatar-ring--add"
                        class:story-avatar-ring--uploading=move || ctx.uploading.get()
                    >
                        <div class="story-avatar-inner">
                            {move || if ctx.uploading.get() {
                                view! {
                                    <div class="story-upload-icon" aria-hidden="true">
                                        <svg
                                            width="18" height="18" viewBox="0 0 24 24"
                                            fill="none" stroke="currentColor"
                                            stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"
                                        >
                                            <polyline points="16 16 12 12 8 16"/>
                                            <line x1="12" y1="12" x2="12" y2="21"/>
                                            <path d="M20.39 18.39A5 5 0 0 0 18 9h-1.26A8 8 0 1 0 3 16.3"/>
                                        </svg>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <svg width="20" height="20" viewBox="0 0 24 24"
                                         fill="none" stroke="currentColor"
                                         stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                                        <line x1="12" y1="5" x2="12" y2="19"/>
                                        <line x1="5"  y1="12" x2="19" y2="12"/>
                                    </svg>
                                }.into_any()
                            }}
                        </div>
                    </div>
                    <span class="story-username story-username--add">
                        {move || if ctx.uploading.get() { "Mengunggah…" } else { "Cerita Anda" }}
                    </span>
                </a>
            </div>

            // ── Loading shimmer ──────────────────────────────────────────
            <Show when=move || ctx.loading.get()>
                {(0..5_usize).map(|_| view! {
                    <div class="story-item">
                        <div class="story-shim-ring shim"></div>
                        <div class="story-shim-name shim"></div>
                    </div>
                }).collect_view()}
            </Show>

            // ── Story groups ─────────────────────────────────────────────
            // StoryGroup { user_id, username, avatar_url, stories, all_viewed }
            {move || {
                let groups = ctx.groups.get();
                if groups.is_empty() {
                    return view! { <></> }.into_any();
                }
                groups
                    .into_iter()
                    .enumerate()
                    .map(|(idx, group)| {
                        let all_viewed = group.all_viewed;
                        let username   = group.username.clone();
                        let avatar_url = group.avatar_url.clone();

                        let ring_class = if all_viewed {
                            "story-avatar-ring story-avatar-ring--viewed"
                        } else {
                            "story-avatar-ring"
                        };
                        let btn_class = if all_viewed {
                            "story-user-btn story-user-btn--viewed"
                        } else {
                            "story-user-btn"
                        };

                        view! {
                            <div class="story-item">
                                <button
                                    class=btn_class
                                    on:click=move |_| ctx.open(idx)
                                    aria-label=format!("Lihat cerita {}", username)
                                >
                                    <div class=ring_class>
                                        <img
                                            src=avatar_url.clone()
                                            alt=username.clone()
                                            class="story-avatar-img"
                                            loading="lazy"
                                        />
                                    </div>
                                    <span class="story-username">{username.clone()}</span>
                                </button>
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </div>
    }
}
