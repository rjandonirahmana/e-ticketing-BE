//! Halaman daftar grup chat — /pulse

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;

use crate::csr::components::story_bars::StoryBar;
use crate::csr::components::story_viewer::StoryViewer;
use crate::csr::components::BottomNav;
use crate::csr::components::{EmptyState, MessageRowShimmer};
use crate::csr::hooks::use_nav;
use crate::csr::services::chat::{self, GroupRoom};

#[component]
pub fn MessagesPage() -> impl IntoView {
    let rooms = RwSignal::new(Vec::<GroupRoom>::new());
    let loading = RwSignal::new(true);
    let error = RwSignal::new(String::new());
    let search_q = RwSignal::new(String::new());

    let navigate = use_nav();

    spawn_local(async move {
        match chat::get_my_rooms().await {
            Ok(r) => {
                rooms.set(r);
                loading.set(false);
            }
            Err(e) => {
                error.set(e.message);
                loading.set(false);
            }
        }
    });

    view! {
        <div class="page msg-page">

            // ── Header — back button, no profile avatar ───────────────────────
            <header class="msg-list-header">
                <button
                    class="chat-back-btn"
                    aria-label="Kembali"
                    on:click=move |_| navigate("/", Default::default())
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
                <h1 class="msg-list-title">"Messages"</h1>
                <button class="msg-icon-btn" aria-label="Opsi">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor">
                        <circle cx="12" cy="5" r="1.5" />
                        <circle cx="12" cy="12" r="1.5" />
                        <circle cx="12" cy="19" r="1.5" />
                    </svg>
                </button>
            </header>

            <StoryBar />
            // ── Search ───────────────────────────────────────────────────────
            <div class="msg-search-wrap">
                <svg
                    class="msg-search-icon"
                    width="16"
                    height="16"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                >
                    <circle cx="11" cy="11" r="8" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                </svg>
                <input
                    type="text"
                    class="msg-search-input"
                    placeholder="Cari percakapan..."
                    prop:value=move || search_q.get()
                    on:input=move |e| search_q.set(event_target_value(&e))
                />
            </div>

            // ── Loading ───────────────────────────────────────────────────────
            {move || {
                loading
                    .get()
                    .then(|| {
                        view! {
                            <div style="padding: 8px 0">
                                <MessageRowShimmer />
                                <MessageRowShimmer />
                                <MessageRowShimmer />
                            </div>
                        }
                    })
            }}

            // ── Error ─────────────────────────────────────────────────────────
            {move || {
                (!error.get().is_empty())
                    .then(|| {
                        view! {
                            <EmptyState
                                icon="⚠️"
                                title="GAGAL MEMUAT"
                                body=error.get()
                            />
                        }
                    })
            }}

            // ── Conversation list ─────────────────────────────────────────────
            {move || {
                if loading.get() {
                    return view! { <span></span> }.into_any();
                }
                let q = search_q.get().to_lowercase();
                let list: Vec<_> = rooms
                    .get()
                    .into_iter()
                    .filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q))
                    .collect();
                if list.is_empty() {
                    return view! {
                        <EmptyState
                            icon="💬"
                            title="BELUM ADA GRUP"
                            body="Beli tiket event untuk bergabung ke grup chat komunitas."
                            cta_label="JELAJAHI EVENT"
                            cta_href="/"
                        />
                    }
                        .into_any();
                }

                view! {
                    <section class="msg-convos-section">
                        <h3 class="msg-convos-label">"GRUP KAMU"</h3>
                        <div class="msg-convos-list">
                            {list
                                .into_iter()
                                .enumerate()
                                .map(|(i, room)| {
                                    let href = format!("/pulse/{}", room.id);
                                    let cover = room
                                        .cover_url
                                        .clone()
                                        .unwrap_or_else(|| {
                                            "https://images.unsplash.com/photo-1501386761578-eac5c94b800a?w=100&q=80"
                                                .into()
                                        });
                                    let name = room.name.clone();
                                    let preview = format!("{} anggota", room.member_count);
                                    let is_live = i == 0;
                                    // first room highlighted
                                    view! {
                                        <A href=href attr:class="msg-convo-row">
                                            <div class="msg-convo-avatar-wrap">
                                                <img src=cover alt=name.clone() class="msg-convo-avatar" />
                                                {is_live
                                                    .then(|| {
                                                        view! { <span class="msg-convo-live-dot"></span> }
                                                    })}
                                            </div>
                                            <div class="msg-convo-body">
                                                <div class="msg-convo-top">
                                                    <span class="msg-convo-name">{name}</span>
                                                </div>
                                                <div class="msg-convo-bottom">
                                                    <span class="msg-convo-preview">{preview}</span>
                                                </div>
                                            </div>
                                            <svg
                                                width="16"
                                                height="16"
                                                viewBox="0 0 24 24"
                                                fill="none"
                                                stroke="currentColor"
                                                stroke-width="2"
                                                stroke-linecap="round"
                                            >
                                                <polyline points="9 18 15 12 9 6" />
                                            </svg>
                                        </A>
                                    }
                                })
                                .collect_view()}
                        </div>
                    </section>
                }
                    .into_any()
            }}
            <StoryViewer />
            <BottomNav active="pulse" />
        </div>
    }
}
