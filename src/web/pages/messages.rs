//! messages.rs — Halaman Daftar Grup Chat / Pulse (SSR).

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::get_chat_rooms;
use crate::web::app::AuthResource;

#[component]
pub fn MessagesPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let rooms = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in { get_chat_rooms().await } else { Ok(vec![]) }
        },
    );

    let search = RwSignal::new(String::new());

    view! {
        <div class="page-header">
            <div class="container">
                <p class="page-header__eyebrow">"// obrolan event"</p>
                <h1 class="page-header__title">"Messages"</h1>
                <p class="page-header__sub">"Grup chat untuk setiap event yang kamu ikuti"</p>
            </div>
        </div>

        <div class="container" style="padding-bottom:4rem;max-width:720px">
            <div style="margin-bottom:1.25rem">
                <input
                    type="search"
                    class="filter-bar__input"
                    placeholder="Cari grup..."
                    prop:value=search
                    on:input=move |ev| search.set(event_target_value(&ev))
                />
            </div>

            <Suspense fallback=|| view! {
                <div class="loading">
                    <div class="loading__spinner"/>
                    <span>"Memuat obrolan..."</span>
                </div>
            }>
                {move || {
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <div style="text-align:center;padding:4rem 0">
                                <p style="color:var(--clr-muted);margin-bottom:1.5rem">
                                    "Kamu harus masuk untuk melihat pesan."
                                </p>
                                <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                            </div>
                        }.into_any();
                    }

                    rooms.get().map(|res| match res {
                        Err(_) => view! {
                            <div class="alert alert--error">"Gagal memuat obrolan."</div>
                        }.into_any(),
                        Ok(list) if list.is_empty() => view! {
                            <div class="empty">
                                <div class="empty__icon">"💬"</div>
                                <div class="empty__title">"Belum ada grup chat"</div>
                                <div class="empty__sub">
                                    "Beli tiket event untuk bergabung ke grup chatnya."
                                </div>
                                <A href="/explore" attr:class="btn btn--accent" attr:style="margin-top:1.5rem">"Jelajahi Event"</A>
                            </div>
                        }.into_any(),
                        Ok(list) => {
                            let q = search.get().to_lowercase();
                            let filtered: Vec<_> = list.into_iter().filter(|r| {
                                q.is_empty() || r.name.to_lowercase().contains(&q)
                            }).collect();

                            view! {
                                <div style="display:flex;flex-direction:column;gap:.625rem">
                                    {filtered.into_iter().map(|room| {
                                        let href = format!("/pulse/{}", room.id);
                                        let name = room.name.clone();
                                        let last = room.last_message.clone().unwrap_or_default();
                                        let cover = room.cover_url.clone();
                                        let unread = room.unread_count;
                                        let members = room.member_count;

                                        view! {
                                            <A href=href attr:class="fade-in" attr:style="display:flex;align-items:center;gap:1rem;padding:1rem;background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:12px;text-decoration:none;color:inherit">
                                                // Avatar
                                                <div style="width:52px;height:52px;border-radius:50%;overflow:hidden;flex-shrink:0;background:var(--clr-border)">
                                                    {match cover {
                                                        Some(url) => view! { <img src=url alt=name.clone() style="width:100%;height:100%;object-fit:cover"/> }.into_any(),
                                                        None => view! { <div style="width:100%;height:100%;display:flex;align-items:center;justify-content:center;font-size:1.25rem">"🎪"</div> }.into_any(),
                                                    }}
                                                </div>
                                                // Info
                                                <div style="flex:1;min-width:0">
                                                    <div style="font-weight:700;margin-bottom:.2rem">{name}</div>
                                                    <div style="font-size:.8rem;color:var(--clr-muted);overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{last}</div>
                                                    <div style="font-size:.75rem;color:var(--clr-muted);margin-top:.15rem">
                                                        {format!("{members} anggota")}
                                                    </div>
                                                </div>
                                                // Unread badge
                                                {if unread > 0 {
                                                    view! {
                                                        <span style="background:var(--clr-accent);color:#000;font-size:.75rem;font-weight:700;width:22px;height:22px;border-radius:50%;display:flex;align-items:center;justify-content:center;flex-shrink:0">
                                                            {unread}
                                                        </span>
                                                    }.into_any()
                                                } else {
                                                    view! { <span/> }.into_any()
                                                }}
                                            </A>
                                        }
                                    }).collect_view()}
                                </div>
                            }.into_any()
                        }
                    }).unwrap_or_else(|| view! { <div/> }.into_any())
                }}
            </Suspense>
        </div>
    }
}
