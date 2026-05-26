//! Chat room — /pulse/:room_id
//!
//! Memory-leak fixes:
//!  1. Closure WS disimpan di StoredValue<Option<JsValue>> — TIDAK .forget()
//!     on_cleanup null semua handler lalu set_value(None) → Closure di-drop
//!  2. on_cleanup() null semua handlers lalu close WS — cegah dangling callback
//!  3. StoredValue<Option<WebSocket>> — tidak perlu Rc<RefCell>, aman WASM
//!  4. WS connect di Effect — reaktif terhadap token
//!  5. on_cleanup terpasang DALAM Effect — jalan saat Effect re-run atau unmount

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use leptos::wasm_bindgen::prelude::*;
use leptos::wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

use crate::csr::hooks::use_auth;
use crate::csr::services::chat::{self, get_history, GroupRoom, TicketCard, WsEvent};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn scroll_bottom(el: &web_sys::Element) {
    el.set_scroll_top(el.scroll_height());
}

/// Unix milliseconds → "HH:MM" (UTC).
/// FIX: sent_at sekarang u64 millis (sesuai TimestampMillis di BE proto.rs),
/// bukan ISO string. Tampilan tetap sama: jam:menit UTC.
fn fmt_time(ms: u64) -> String {
    let secs = ms / 1000;
    let hours = (secs / 3600) % 24;
    let mins  = (secs / 60) % 60;
    format!("{:02}:{:02}", hours, mins)
}

// ─── Ticket card component ────────────────────────────────────────────────────

#[component]
fn TicketCardBubble(card: TicketCard) -> impl IntoView {
    view! {
        <div class="chat-ticket-card">
            <div class="chat-ticket-card-header">
                <span class="chat-ticket-badge">"SHARED TICKET"</span>
                <span class="chat-ticket-tier">"1X "{card.tier}</span>
            </div>
            <div class="chat-ticket-body">
                <div>
                    <p class="chat-ticket-event">{card.event_name}</p>
                    <p class="chat-ticket-venue">{card.venue}</p>
                    <p class="chat-ticket-price">{card.price}</p>
                </div>
                <button class="chat-ticket-view-btn">"VIEW"</button>
            </div>
        </div>
    }
}

// ─── component ───────────────────────────────────────────────────────────────

#[component]
pub fn ChatRoomPage() -> impl IntoView {
    let params = use_params_map();
    let auth = use_auth();
    let room_id = move || params.with(|p| p.get("id").unwrap_or_default());

    let my_id = move || {
        auth.user
            .with(|u| u.as_ref().map(|p| p.id.clone()).unwrap_or_default())
    };

    let messages = RwSignal::new(Vec::<crate::csr::services::chat::WsGroupMessage>::new());
    let room_info = RwSignal::new(Option::<GroupRoom>::None);
    let text_input = RwSignal::new(String::new());
    let loading = RwSignal::new(true);
    let ws_ready = RwSignal::new(false);
    let sending = RwSignal::new(false);
    let error_msg = RwSignal::new(String::new());

    let msg_ref = NodeRef::<leptos::html::Div>::new();
    let ws_store: StoredValue<Option<WebSocket>> = StoredValue::new(None);
    // Simpan setiap Closure WS sebagai JsValue agar bisa di-drop eksplisit di on_cleanup.
    // JsValue: Send + Sync → aman untuk StoredValue. set_value(None) → GC klaim memory.
    let cb_onmessage: StoredValue<Option<JsValue>> = StoredValue::new(None);
    let cb_onopen:    StoredValue<Option<JsValue>> = StoredValue::new(None);
    let cb_onclose:   StoredValue<Option<JsValue>> = StoredValue::new(None);
    let cb_onerror:   StoredValue<Option<JsValue>> = StoredValue::new(None);

    // ── Load history & room metadata ─────────────────────────────────────────
    Effect::new(move |_| {
        let rid = room_id();
        if rid.is_empty() {
            return;
        }
        spawn_local(async move {
            if let Ok(rooms) = chat::get_my_rooms().await {
                if let Some(r) = rooms.into_iter().find(|r| r.id == rid) {
                    room_info.set(Some(r));
                }
            }
            if let Ok((msgs, _)) = get_history(&rid, 60, None).await {
                messages.set(msgs);
            }
            loading.set(false);
            if let Some(el) = msg_ref.get() {
                scroll_bottom(&el);
            }
        });
    });

    // ── WebSocket ─────────────────────────────────────────────────────────────
    Effect::new(move |_| {
        let token = auth.access_token.get();
        if token.is_none() {
            return;
        }

        let Some(url) = chat::ws_url() else { return };
        let Ok(ws) = WebSocket::new(&url) else {
            error_msg.set("Tidak dapat terhubung ke server.".into());
            return;
        };

        let ws_ping = ws.clone();
        let scroll_r = msg_ref;

        let onmessage = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            let Ok(txt) = e.data().dyn_into::<web_sys::js_sys::JsString>() else {
                return;
            };
            let s: String = txt.into();
            let Ok(ev) = serde_json::from_str::<WsEvent>(&s) else {
                return;
            };
            match ev {
                WsEvent::Hello { .. } => {
                    ws_ready.set(true);
                }
                WsEvent::NewMessage(msg) => {
                    // FIX P1: Dedup by msg_id — cegah duplikat jika ada optimistic
                    // update di masa depan, atau jika reconnect kirim ulang.
                    messages.update(|v| {
                        if !v.iter().any(|m| m.id == msg.id) {
                            v.push(msg);
                        }
                    });
                    if let Some(el) = scroll_r.get() {
                        scroll_bottom(&el);
                    }
                }
                WsEvent::Ack { .. } => {}
                WsEvent::History { messages: hist, .. } => {
                    messages.set(hist);
                    if let Some(el) = scroll_r.get() {
                        scroll_bottom(&el);
                    }
                }
                WsEvent::Ping => {
                    if ws_ping.ready_state() == WebSocket::OPEN {
                        let _ = ws_ping.send_with_str(r#"{"type":"ping"}"#);
                    }
                }
                WsEvent::Error { message, .. } => {
                    error_msg.set(message);
                }
                _ => {}
            }
        });

        let onopen = Closure::<dyn FnMut()>::new(move || {
            ws_ready.set(true);
            error_msg.set(String::new());
        });
        let onclose = Closure::<dyn FnMut()>::new(move || {
            ws_ready.set(false);
        });
        let onerror = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            ws_ready.set(false);
        });

        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));

        // Simpan Closure sebagai JsValue — TIDAK .forget().
        // into_js_value() menjaga Closure hidup di JS heap;
        // set_value(None) di on_cleanup → Rust drop → JS GC bebaskan memori.
        cb_onmessage.set_value(Some(onmessage.into_js_value()));
        cb_onopen.set_value(Some(onopen.into_js_value()));
        cb_onclose.set_value(Some(onclose.into_js_value()));
        cb_onerror.set_value(Some(onerror.into_js_value()));

        ws_store.set_value(Some(ws.clone()));

        on_cleanup(move || {
            // 1. Lepas semua handler dari WebSocket agar tidak ada callback ke Rust
            ws.set_onmessage(None);
            ws.set_onopen(None);
            ws.set_onclose(None);
            ws.set_onerror(None);
            let _ = ws.close();
            ws_store.set_value(None);
            // 2. Drop Closure → JS GC bebaskan memori Closure
            cb_onmessage.set_value(None);
            cb_onopen.set_value(None);
            cb_onclose.set_value(None);
            cb_onerror.set_value(None);
        });
    });

    // ── Send ─────────────────────────────────────────────────────────────────
    let do_send = move || {
        let content = text_input.get_untracked();
        if content.trim().is_empty() || sending.get_untracked() {
            return;
        }
        let Some(ws) = ws_store.get_value() else {
            return;
        };
        if ws.ready_state() != WebSocket::OPEN {
            error_msg.set("Koneksi terputus, menghubungkan ulang...".into());
            return;
        }
        sending.set(true);
        error_msg.set(String::new());
        let rid = room_id();
        let json = format!(
            r#"{{"type":"send_text","room_id":{},"content":{}}}"#,
            serde_json::to_string(&rid).unwrap_or_default(),
            serde_json::to_string(&content).unwrap_or_default(),
        );
        if ws.send_with_str(&json).is_ok() {
            text_input.set(String::new());
        } else {
            error_msg.set("Gagal mengirim, coba lagi.".into());
        }
        sending.set(false);
    };

    // ── View ─────────────────────────────────────────────────────────────────
    view! {
        <div class="chat-page">

            // ── Sticky header ────────────────────────────────────────────────
            <header class="chat-header">
                <A href="/pulse" attr:class="chat-back-btn" attr:aria-label="Kembali">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>

                {move || room_info.with(|r| {
                    let name  = r.as_ref().map(|x| x.name.clone()).unwrap_or_default();
                    let cover = r.as_ref()
                        .and_then(|x| x.cover_url.clone())
                        .unwrap_or_else(|| "https://images.unsplash.com/photo-1501386761578-eac5c94b800a?w=80&q=80".into());
                    let count = r.as_ref().map(|x| x.member_count).unwrap_or(0);
                    view! {
                        <div class="chat-header-avatar-wrap">
                            <img src=cover class="chat-header-avatar" alt=name.clone()/>
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
                    }
                })}

                <div class="chat-header-actions">
                    <button class="chat-icon-btn" aria-label="Search">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <circle cx="11" cy="11" r="8"/>
                            <line x1="21" y1="21" x2="16.65" y2="16.65"/>
                        </svg>
                    </button>
                    <button class="chat-icon-btn" aria-label="Info">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <circle cx="12" cy="12" r="10"/>
                            <line x1="12" y1="8" x2="12" y2="12"/>
                            <line x1="12" y1="16" x2="12.01" y2="16"/>
                        </svg>
                    </button>
                </div>
            </header>

            // ── Messages scroll area ─────────────────────────────────────────
            <div class="chat-messages" node_ref=msg_ref>
                {move || if loading.get() {
                    view! {
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
                            <div class="chat-shimmer-row chat-shimmer-row--self">
                                <div class="shim chat-shimmer-bubble chat-shimmer-bubble--sm"></div>
                            </div>
                        </div>
                    }.into_any()
                } else {
                    messages.with(|msgs| {
                        if msgs.is_empty() {
                            view! {
                                <div class="chat-empty-state">
                                    <span class="chat-empty-icon">"💬"</span>
                                    <p class="chat-empty-title">"BELUM ADA PESAN"</p>
                                    <p class="chat-empty-body">"Jadilah yang pertama memulai percakapan!"</p>
                                </div>
                            }.into_any()
                        } else {
                            msgs.iter().map(|msg| {
                                let is_me     = msg.sender_id == my_id();
                                let is_system = msg.is_system;
                                let name      = msg.sender_name.clone();
                                let text      = msg.content.clone();
                                let time      = fmt_time(msg.sent_at);
                                let id        = msg.id.clone();
                                let ticket    = msg.ticket_card.clone();
                                let initial   = name.chars().next()
                                    .unwrap_or('?').to_uppercase().next()
                                    .unwrap_or('?').to_string();

                                // System message
                                if is_system {
                                    return view! {
                                        <div class="chat-system-msg" data-id=id>
                                            <span class="chat-system-label">"SISTEM"</span>
                                            <div class="chat-system-bubble">{text}</div>
                                        </div>
                                    }.into_any();
                                }

                                let row_cls    = if is_me { "chat-row chat-row--self" } else { "chat-row chat-row--other" };
                                let wrap_cls   = if is_me { "chat-bubble-wrap chat-bubble-wrap--self" } else { "chat-bubble-wrap" };
                                let bubble_cls = if is_me { "chat-bubble chat-bubble--self" } else { "chat-bubble chat-bubble--other" };

                                view! {
                                    <div class=row_cls data-id=id>
                                        {(!is_me).then(|| view! {
                                            <div class="chat-other-avatar-wrap">
                                                <div class="chat-other-avatar">{initial}</div>
                                            </div>
                                        })}
                                        <div class=wrap_cls>
                                            {(!is_me).then(|| view! {
                                                <span class="chat-sender-name">{name}</span>
                                            })}
                                            // Ticket card atau bubble biasa
                                            {if let Some(card) = ticket {
                                                view! {
                                                    <div class=bubble_cls>
                                                        {(!text.is_empty()).then(|| view! {
                                                            <p class="chat-bubble-text">{text}</p>
                                                        })}
                                                        <TicketCardBubble card=card />
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class=bubble_cls>{text}</div>
                                                }.into_any()
                                            }}
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
                                }.into_any()
                            }).collect_view().into_any()
                        }
                    })
                }}
            </div>

            // ── Error toast ───────────────────────────────────────────────────
            {move || (!error_msg.get().is_empty()).then(|| view! {
                <div class="chat-error-toast">{error_msg.get()}</div>
            })}

            // ── Input bar ─────────────────────────────────────────────────────
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
                    prop:disabled=move || !ws_ready.get() || sending.get()
                    on:input=move |e| text_input.set(event_target_value(&e))
                    on:keydown=move |e| {
                        if e.key() == "Enter" { e.prevent_default(); do_send(); }
                    }
                />
                <button class="chat-input-icon-btn" aria-label="Lampiran">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48"/>
                    </svg>
                </button>
                <button class="chat-input-icon-btn" aria-label="Kamera">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M23 19a2 2 0 01-2 2H3a2 2 0 01-2-2V8a2 2 0 012-2h4l2-3h6l2 3h4a2 2 0 012 2z"/>
                        <circle cx="12" cy="13" r="4"/>
                    </svg>
                </button>
                <button
                    class="chat-send-btn"
                    disabled=move || !ws_ready.get() || sending.get() || text_input.get().trim().is_empty()
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
