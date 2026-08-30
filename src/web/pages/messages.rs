//! messages.rs — Halaman Daftar Grup Chat / Pulse (SSR).

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::get_chat_rooms;
use crate::web::app::AuthResource;
use crate::web::components::story_bars::StoryBar;
use crate::web::components::story_viewer::StoryViewer;
use crate::web::components::{BannerSlider, BottomNav, EmptyState, MessageRowShimmer};

#[component]
pub fn MessagesPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let rooms = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in {
                get_chat_rooms().await
            } else {
                Ok(vec![])
            }
        },
    );

    let search = RwSignal::new(String::new());

    let filtered_rooms = Memo::new(move |_| {
        let q = search.get().to_lowercase();
        match rooms.get() {
            Some(Ok(list)) => list
                .into_iter()
                .filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q))
                .collect::<Vec<_>>(),
            _ => vec![],
        }
    });

    view! {
        <div class="page msg-page">
            <header class="msg-list-header">
                <A href="/explore" attr:class="chat-back-btn" attr:aria-label="Kembali">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                        stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </A>
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
            <div class="msg-search-wrap">
                <svg class="msg-search-icon" width="16" height="16" viewBox="0 0 24 24"
                    fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                    <circle cx="11" cy="11" r="8" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                </svg>
                <input
                    type="text"
                    class="msg-search-input"
                    placeholder="Cari percakapan..."
                    prop:value=move || search.get()
                    on:input=move |e| search.set(event_target_value(&e))
                />
            </div>
            // Banner admin, sama persis dengan yang di /explore (komponen yang
            // sama, `components/banner_slider.rs`). TANPA fallback: di Jelajah
            // kartu "SPONSORED" statis mengisi ruang kosong karena ia memang
            // bagian dari tata letak halaman itu, sedangkan di sini daftar
            // percakapan yang harus terlihat lebih dulu — kalau belum ada
            // banner aktif, yang benar adalah tidak menampilkan apa pun.
            <div class="msg-banner-wrap">
                <BannerSlider />
            </div>

            <Suspense fallback=|| view! {
                <div style="padding: 8px 0">
                    <MessageRowShimmer />
                    <MessageRowShimmer />
                    <MessageRowShimmer />
                    <MessageRowShimmer />
                </div>
            }>
                {move || {
                    rooms.get().map(|res| {
                        view! {
                            <section class="msg-convos-section">
                                <h3 class="msg-convos-label">"GRUP KAMU"</h3>
                                <div class="msg-convos-list">
                                    {match res {
                                        Err(e) => view! {
                                            <EmptyState icon="⚠️" title="GAGAL MEMUAT" body=e.to_string() />
                                        }.into_any(),
                                        Ok(_) => view! {
                                            {move || {
                                                let filtered = filtered_rooms.get();
                                                if filtered.is_empty() {
                                                    view! {
                                                        <EmptyState
                                                            icon="💬"
                                                            title="BELUM ADA PERCAKAPAN"
                                                            body="Buka halaman product, lalu tekan Chat Penjual untuk bertanya soal stok, ukuran, atau pengambilan."
                                                            cta_label="JELAJAHI PRODUCT"
                                                            cta_href="/explore"
                                                        />
                                                    }.into_any()
                                                } else {
                                                    filtered.into_iter().enumerate().map(|(i, room)| {
                                                        let href = format!("/pulse/{}", room.id);
                                                        let cover = room.cover_url.clone().unwrap_or_else(|| {
                                                            "https://images.unsplash.com/photo-1501386761578-eac5c94b800a?w=100&q=80".into()
                                                        });
                                                        let name = room.name.clone();
                                                        let preview = format!("{} anggota", room.member_count);
                                                        let is_live = i == 0;
                                                        view! {
                                                            <A href=href attr:class="msg-convo-row">
                                                                <div class="msg-convo-avatar-wrap">
                                                                    <img src=cover alt=name.clone() class="msg-convo-avatar" />
                                                                    {is_live.then(|| view! {
                                                                        <span class="msg-convo-live-dot"></span>
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
                                                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                                                    stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                                                    <polyline points="9 18 15 12 9 6" />
                                                                </svg>
                                                            </A>
                                                        }
                                                    }).collect_view().into_any()
                                                }
                                            }}
                                        }.into_any(),
                                    }}
                                </div>
                            </section>
                        }.into_any()
                    })
                }}
            </Suspense>
            <StoryViewer />
            <BottomNav active="pulse" />
        </div>
    }
}
