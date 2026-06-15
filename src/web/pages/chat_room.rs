//! chat_room.rs — Grup Chat Room (SSR + WASM WebSocket).
//!
//! SSR: room info + message history via Resource/Suspense.
//! WASM: WebSocket real-time connection in #[cfg(target_arch = "wasm32")] Effect.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::{get_chat_history, get_chat_room_detail};
use crate::web::app::AuthResource;
use crate::web::models::ChatMessage;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fmt_time_ms(ms: u64) -> String {
    let secs = ms / 1000 + 7 * 3600; // WIB offset
    let hours = (secs / 3600) % 24;
    let mins  = (secs / 60) % 60;
    format!("{:02}:{:02}", hours, mins)
}

// ── Component ─────────────────────────────────────────────────────────────────

#[component]
pub fn ChatRoomPage() -> impl IntoView {
    let params  = use_params_map();
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

    let text_input  = RwSignal::new(String::new());
    let ws_ready    = RwSignal::new(false);
    let error_msg   = RwSignal::new(String::new());
    let live_msgs: RwSignal<Vec<ChatMessage>> = RwSignal::new(vec![]);

    let msg_list_ref = NodeRef::<leptos::html::Div>::new();

    // ── WebSocket (WASM only) ─────────────────────────────────────────────────
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;
        use web_sys::WebSocket;

        let ws_store: StoredValue<Option<WebSocket>> = StoredValue::new(None);
        let cb_onmessage: StoredValue<Option<JsValue>> = StoredValue::new(None);
        let cb_onopen:    StoredValue<Option<JsValue>> = StoredValue::new(None);
        let cb_onclose:   StoredValue<Option<JsValue>> = StoredValue::new(None);
        let cb_onerror:   StoredValue<Option<JsValue>> = StoredValue::new(None);

        Effect::new(move |_| {
            let token = auth.get().and_then(|r| r.ok()).flatten();
            if token.is_none() { return; }

            let proto  = if web_sys::window().map(|w| w.location().protocol().unwrap_or_default() == "https:").unwrap_or(false) { "wss" } else { "ws" };
            let host   = web_sys::window().and_then(|w| w.location().host().ok()).unwrap_or_default();
            let url    = format!("{}://{}/ws/chat", proto, host);

            let Ok(ws) = WebSocket::new(&url) else {
                error_msg.set("Tidak dapat terhubung ke server.".into());
                return;
            };

            let scroll_ref = msg_list_ref;
            let onmessage = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::MessageEvent)>::new(
                move |e: web_sys::MessageEvent| {
                    let Ok(txt) = e.data().dyn_into::<web_sys::js_sys::JsString>() else { return };
                    let s: String = txt.into();
                    if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&s) {
                        if msg.get("type").and_then(|t| t.as_str()) == Some("new_message") {
                            if let Ok(m) = serde_json::from_value::<ChatMessage>(
                                msg.get("data").cloned().unwrap_or_default()
                            ) {
                                live_msgs.update(|v| {
                                    if !v.iter().any(|x| x.id == m.id) { v.push(m); }
                                });
                                if let Some(el) = scroll_ref.get() {
                                    el.set_scroll_top(el.scroll_height());
                                }
                            }
                        }
                    }
                },
            );
            let onopen  = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
                ws_ready.set(true);
                error_msg.set(String::new());
            });
            let onclose = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(move || {
                ws_ready.set(false);
            });
            let onerror = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                ws_ready.set(false);
            });

            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
            ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
            ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));

            cb_onmessage.set_value(Some(onmessage.into_js_value()));
            cb_onopen.set_value(Some(onopen.into_js_value()));
            cb_onclose.set_value(Some(onclose.into_js_value()));
            cb_onerror.set_value(Some(onerror.into_js_value()));
            ws_store.set_value(Some(ws.clone()));

            on_cleanup(move || {
                ws.set_onmessage(None);
                ws.set_onopen(None);
                ws.set_onclose(None);
                ws.set_onerror(None);
                let _ = ws.close();
                ws_store.set_value(None);
                cb_onmessage.set_value(None);
                cb_onopen.set_value(None);
                cb_onclose.set_value(None);
                cb_onerror.set_value(None);
            });
        });
    }

    let do_send = move || {
        let content = text_input.get_untracked().trim().to_string();
        if content.is_empty() { return; }
        text_input.set(String::new());
        // Optimistic local append — WS will confirm from server
        let me_id = current_user_id().unwrap_or_default();
        let msg = ChatMessage {
            id: format!("local-{}", js_sys_now()),
            room_id: room_id(),
            sender_id: me_id,
            sender_name: "You".into(),
            content,
            sent_at: 0,
            message_type: "text".into(),
        };
        live_msgs.update(|v| v.push(msg));
    };

    view! {
        <div class="chat-page">

            // ── Sticky header ──────────────────────────────────────────────────
            <Suspense fallback=|| view! {
                <header class="chat-header">
                    <div class="shim chat-shimmer-avatar" style="width:36px;height:36px;border-radius:50%"></div>
                    <div style="flex:1;display:flex;flex-direction:column;gap:4px">
                        <div class="shim" style="width:120px;height:14px;border-radius:4px"></div>
                        <div class="shim" style="width:80px;height:10px;border-radius:4px"></div>
                    </div>
                </header>
            }>
                {move || room.get().map(|res| {
                    let (name, cover, count) = match res {
                        Ok(r) => (r.name, r.cover_url, r.member_count),
                        _ => (String::new(), None, 0),
                    };
                    view! {
                        <header class="chat-header">
                            <A href="/pulse" attr:class="chat-back-btn" attr:aria-label="Kembali">
                                <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                    <polyline points="15 18 9 12 15 6"/>
                                </svg>
                            </A>
                            <div class="chat-header-avatar-wrap">
                                {match cover {
                                    Some(url) => view! { <img src=url class="chat-header-avatar" alt=name.clone()/> }.into_any(),
                                    None => view! { <div class="chat-header-avatar-placeholder">"🎪"</div> }.into_any(),
                                }}
                            </div>
                            <div class="chat-header-info">
                                <span class="chat-header-name">{name}</span>
                                <span class="chat-header-sub">
                                    {format!("{count} PULSING  ·  ")}
                                    {move || if ws_ready.get() {
                                        view! { <span class="chat-status-live">"● LIVE"</span> }.into_any()
                                    } else {
                                        view! { <span class="chat-status-connecting">"○ CONNECTING"</span> }.into_any()
                                    }}
                                </span>
                            </div>
                            <div class="chat-header-actions">
                                <button class="chat-icon-btn" aria-label="Search">
                                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                        <circle cx="11" cy="11" r="8"/>
                                        <line x1="21" y1="21" x2="16.65" y2="16.65"/>
                                    </svg>
                                </button>
                            </div>
                        </header>
                    }.into_any()
                }).unwrap_or_else(|| view! { <header class="chat-header"/> }.into_any())}
            </Suspense>

            // ── Messages ───────────────────────────────────────────────────────
            <div class="chat-messages" node_ref=msg_list_ref>
                <Suspense fallback=|| view! {
                    <div class="chat-shimmer-wrap">
                        <div class="chat-shimmer-row chat-shimmer-row--other">
                            <div class="shim chat-shimmer-avatar"></div>
                            <div class="shim chat-shimmer-bubble chat-shimmer-bubble--sm"></div>
                        </div>
                        <div class="chat-shimmer-row chat-shimmer-row--self">
                            <div class="shim chat-shimmer-bubble chat-shimmer-bubble--md"></div>
                        </div>
                        <div class="chat-shimmer-row chat-shimmer-row--other">
                            <div class="shim chat-shimmer-avatar"></div>
                            <div class="shim chat-shimmer-bubble chat-shimmer-bubble--lg"></div>
                        </div>
                    </div>
                }>
                    {move || history.get().map(|res| match res {
                        Ok(hist) if hist.is_empty() => view! {
                            <div class="chat-empty-state">
                                <span class="chat-empty-icon">"💬"</span>
                                <p class="chat-empty-title">"BELUM ADA PESAN"</p>
                                <p class="chat-empty-body">"Jadilah yang pertama memulai percakapan!"</p>
                            </div>
                        }.into_any(),
                        Ok(hist) => {
                            let me = current_user_id().unwrap_or_default();
                            hist.into_iter().map(|msg| message_bubble(msg, &me)).collect_view().into_any()
                        }
                        _ => view! { <div/> }.into_any(),
                    }).unwrap_or_else(|| view! { <div/> }.into_any())}
                </Suspense>

                // Live WS messages appended client-side
                {move || {
                    let me = current_user_id().unwrap_or_default();
                    live_msgs.get().into_iter().map(|msg| message_bubble(msg, &me)).collect_view()
                }}
            </div>

            // ── Error toast ────────────────────────────────────────────────────
            {move || (!error_msg.get().is_empty()).then(|| view! {
                <div class="chat-error-toast">{error_msg.get()}</div>
            })}

            // ── Input bar ──────────────────────────────────────────────────────
            <div class="chat-input-bar">
                <button class="chat-input-icon-btn" aria-label="Emoji">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <circle cx="12" cy="12" r="10"/>
                        <path d="M8 14s1.5 2 4 2 4-2 4-2"/>
                        <line x1="9" y1="9" x2="9.01" y2="9"/>
                        <line x1="15" y1="9" x2="15.01" y2="9"/>
                    </svg>
                </button>
                <input
                    type="text"
                    class="chat-input"
                    placeholder="Pulse your message..."
                    prop:value=move || text_input.get()
                    on:input=move |e| text_input.set(event_target_value(&e))
                    on:keydown=move |e| {
                        if e.key() == "Enter" { e.prevent_default(); do_send(); }
                    }
                />
                <button
                    class="chat-send-btn"
                    disabled=move || text_input.get().trim().is_empty()
                    on:click=move |_| do_send()
                >
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <line x1="22" y1="2" x2="11" y2="13"/>
                        <polygon points="22 2 15 22 11 13 2 9 22 2"/>
                    </svg>
                </button>
            </div>
        </div>
    }
}

// ── Message bubble renderer ───────────────────────────────────────────────────

fn message_bubble(msg: ChatMessage, my_id: &str) -> impl IntoView {
    let is_me   = msg.sender_id == my_id;
    let name    = msg.sender_name.clone();
    let text    = msg.content.clone();
    let time    = if msg.sent_at > 0 { fmt_time_ms(msg.sent_at) } else { String::new() };
    let initial = name.chars().next().unwrap_or('?').to_uppercase().next().unwrap_or('?').to_string();

    let row_cls    = if is_me { "chat-row chat-row--self" } else { "chat-row chat-row--other" };
    let wrap_cls   = if is_me { "chat-bubble-wrap chat-bubble-wrap--self" } else { "chat-bubble-wrap" };
    let bubble_cls = if is_me { "chat-bubble chat-bubble--self" } else { "chat-bubble chat-bubble--other" };

    view! {
        <div class=row_cls>
            {(!is_me).then(|| view! {
                <div class="chat-other-avatar-wrap">
                    <div class="chat-other-avatar">{initial}</div>
                </div>
            })}
            <div class=wrap_cls>
                {(!is_me).then(|| view! {
                    <span class="chat-sender-name">{name}</span>
                })}
                <div class=bubble_cls>{text}</div>
                <div class="chat-msg-meta">
                    <span class="chat-msg-time">{time}</span>
                    {is_me.then(|| view! {
                        <span class="chat-msg-sent-icon">
                            <svg width="14" height="14" viewBox="0 0 24 24"
                                 fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                <polyline points="20 6 9 17 4 12"/>
                                <polyline points="20 6 9 17 14 17"/>
                            </svg>
                        </span>
                    })}
                </div>
            </div>
        </div>
    }
}

// ── Shim for non-WASM targets ─────────────────────────────────────────────────

#[cfg(not(target_arch = "wasm32"))]
fn js_sys_now() -> u64 { 0 }

#[cfg(target_arch = "wasm32")]
fn js_sys_now() -> u64 { web_sys::js_sys::Date::now() as u64 }
