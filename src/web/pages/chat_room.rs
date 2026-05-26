//! chat_room.rs — Halaman Grup Chat dengan SSR + Hydration untuk WebSocket.
//!
//! SSR merender riwayat pesan dari REST API.
//! WebSocket real-time aktif setelah hydration di browser.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::{get_chat_history, get_chat_room_detail};
use crate::web::app::AuthResource;

#[component]
pub fn ChatRoomPage() -> impl IntoView {
    let params = use_params_map();
    let room_id = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();
    let current_user_id = move || {
        auth.get().and_then(|r| r.ok()).flatten().map(|u| u.id)
    };

    let room = Resource::new(
        move || (room_id(), is_logged_in()),
        |(id, logged_in)| async move {
            if logged_in && !id.is_empty() {
                get_chat_room_detail(id).await
            } else {
                Err(ServerFnError::ServerError("not_ready".into()))
            }
        },
    );

    let history = Resource::new(
        move || (room_id(), is_logged_in()),
        |(id, logged_in)| async move {
            if logged_in && !id.is_empty() {
                get_chat_history(id).await
            } else {
                Ok(vec![])
            }
        },
    );

    // Message input (aktif setelah hydration)
    let msg_input = RwSignal::new(String::new());
    let messages  = RwSignal::new(Vec::<(String, String, String)>::new()); // (sender, content, is_mine)

    view! {
        <div style="display:flex;flex-direction:column;height:calc(100vh - 64px)">
            // ── Header ──────────────────────────────────────────────────────
            <Suspense fallback=|| view! {
                <div style="padding:1rem;border-bottom:1px solid var(--clr-border);display:flex;align-items:center;gap:.75rem">
                    <div style="width:40px;height:40px;border-radius:50%;background:var(--clr-border)"/>
                    <div style="height:16px;width:120px;background:var(--clr-border);border-radius:4px"/>
                </div>
            }>
                {move || room.get().map(|res| match res {
                    Ok(r) => view! {
                        <div style="padding:1rem;border-bottom:1px solid var(--clr-border);display:flex;align-items:center;gap:.75rem;background:var(--clr-surface)">
                            <A href="/pulse" attr:class="btn btn--ghost btn--sm" attr:style="padding:.375rem">"←"</A>
                            <div style="width:40px;height:40px;border-radius:50%;overflow:hidden;background:var(--clr-border);flex-shrink:0">
                                {match r.cover_url {
                                    Some(url) => view! { <img src=url style="width:100%;height:100%;object-fit:cover"/> }.into_any(),
                                    None => view! { <div style="width:100%;height:100%;display:flex;align-items:center;justify-content:center">"🎪"</div> }.into_any(),
                                }}
                            </div>
                            <div>
                                <div style="font-weight:700">{r.name}</div>
                                <div style="font-size:.75rem;color:var(--clr-muted)">{format!("{} anggota", r.member_count)}</div>
                            </div>
                        </div>
                    }.into_any(),
                    Err(e) if e.to_string().contains("not_ready") => view! { <div/> }.into_any(),
                    Err(_) => view! {
                        <div style="padding:1rem;border-bottom:1px solid var(--clr-border)">
                            <A href="/pulse" attr:class="btn btn--ghost btn--sm">"← Kembali"</A>
                        </div>
                    }.into_any(),
                }).unwrap_or_else(|| view! { <div/> }.into_any())}
            </Suspense>

            // ── Chat Area ────────────────────────────────────────────────────
            <div style="flex:1;overflow-y:auto;padding:1rem;display:flex;flex-direction:column;gap:.625rem" id="chat-messages">
                // SSR: riwayat dari REST API
                <Suspense fallback=|| view! {
                    <div class="loading"><div class="loading__spinner"/></div>
                }>
                    {move || history.get().map(|res| match res {
                        Ok(hist) => view! {
                            <div style="display:flex;flex-direction:column;gap:.625rem">
                                {hist.into_iter().map(|m| {
                                    let is_mine = current_user_id().map(|uid| uid == m.sender_id).unwrap_or(false);
                                    let align = if is_mine { "flex-end" } else { "flex-start" };
                                    let bubble_bg = if is_mine {
                                        "background:var(--clr-accent);color:#000"
                                    } else {
                                        "background:var(--clr-surface);border:1px solid var(--clr-border)"
                                    };
                                    view! {
                                        <div style=format!("display:flex;flex-direction:column;align-items:{align};gap:.2rem")>
                                            {if !is_mine {
                                                view! { <span style="font-size:.7rem;color:var(--clr-muted)">{m.sender_name.clone()}</span> }.into_any()
                                            } else {
                                                view! { <span/> }.into_any()
                                            }}
                                            <div style=format!("max-width:75%;padding:.625rem .875rem;border-radius:12px;font-size:.875rem;line-height:1.4;{bubble_bg}")>
                                                {m.content}
                                            </div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any(),
                        Err(_) => view! { <div/> }.into_any(),
                    }).unwrap_or_else(|| view! { <div/> }.into_any())}
                </Suspense>

                // Hydration: pesan baru dari WebSocket (empty SSR, diisi client)
                {move || messages.get().into_iter().map(|(sender, content, is_mine_str)| {
                    let is_mine = is_mine_str == "true";
                    let align = if is_mine { "flex-end" } else { "flex-start" };
                    let bubble_bg = if is_mine {
                        "background:var(--clr-accent);color:#000"
                    } else {
                        "background:var(--clr-surface);border:1px solid var(--clr-border)"
                    };
                    view! {
                        <div style=format!("display:flex;flex-direction:column;align-items:{align};gap:.2rem")>
                            {if !is_mine {
                                view! { <span style="font-size:.7rem;color:var(--clr-muted)">{sender}</span> }.into_any()
                            } else {
                                view! { <span/> }.into_any()
                            }}
                            <div style=format!("max-width:75%;padding:.625rem .875rem;border-radius:12px;font-size:.875rem;line-height:1.4;{bubble_bg}")>
                                {content}
                            </div>
                        </div>
                    }
                }).collect_view()}
            </div>

            // ── Input Bar ────────────────────────────────────────────────────
            <div style="padding:.875rem;border-top:1px solid var(--clr-border);background:var(--clr-surface);display:flex;gap:.625rem">
                <input
                    type="text"
                    style="flex:1;background:var(--clr-bg);border:1px solid var(--clr-border);border-radius:8px;padding:.625rem .875rem;font-size:.875rem;color:inherit"
                    placeholder="Tulis pesan..."
                    prop:value=msg_input
                    on:input=move |ev| msg_input.set(event_target_value(&ev))
                    on:keydown=move |ev| {
                        if ev.key() == "Enter" {
                            let content = msg_input.get();
                            if !content.is_empty() {
                                messages.update(|v| v.push(("Kamu".into(), content.clone(), "true".into())));
                                msg_input.set(String::new());
                                // TODO: kirim via WebSocket setelah hydration
                            }
                        }
                    }
                />
                <button
                    class="btn btn--accent btn--sm"
                    on:click=move |_| {
                        let content = msg_input.get();
                        if !content.is_empty() {
                            messages.update(|v| v.push(("Kamu".into(), content.clone(), "true".into())));
                            msg_input.set(String::new());
                        }
                    }
                >"Kirim"</button>
            </div>
        </div>
    }
}
