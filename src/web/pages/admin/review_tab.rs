use leptos::prelude::*;

use crate::web::api::update_event_status_admin;
use crate::web::models::{format_date, Event};

pub(super) fn view_review(
    pending_events: RwSignal<Vec<Event>>,
    all_events: RwSignal<Vec<Event>>,
    toast: RwSignal<Option<(String, bool)>>,
) -> impl IntoView {
    let processing: RwSignal<Option<String>> = RwSignal::new(None);

    let do_update = move |event_id: String, status: &'static str| {
        let id = event_id.clone();
        processing.set(Some(id.clone()));
        leptos::task::spawn_local(async move {
            match update_event_status_admin(id.clone(), status.to_string()).await {
                Ok(_) => {
                    pending_events.update(|v| v.retain(|e| e.id != id));
                    all_events.update(|v| {
                        if let Some(ev) = v.iter_mut().find(|e| e.id == id) {
                            ev.status = status.to_string();
                        }
                    });
                    let msg = match status {
                        "active"    => "✅ Event diaktifkan.",
                        "cancelled" => "❌ Event dibatalkan.",
                        _           => "Status diperbarui.",
                    };
                    toast.set(Some((msg.to_string(), false)));
                }
                Err(e) => toast.set(Some((format!("Gagal: {e}"), true))),
            }
            processing.set(None);
        });
    };

    view! {
        <section class="mhub-events-section">
            <div class="mhub-events-header">
                <h3 class="mhub-events-title">"Review Event"</h3>
                <span class="mhub-live-badge">
                    <span class="mhub-live-dot"></span>
                    "Menunggu"
                </span>
            </div>
            {move || {
                let evs = pending_events.get();
                if evs.is_empty() {
                    return view! {
                        <div class="mhub-empty">
                            <div class="mhub-empty-icon-wrap">
                                <svg width="38" height="38" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                    <polyline points="20 6 9 17 4 12"/>
                                </svg>
                            </div>
                            <p class="mhub-empty-title">"Semua Bersih ✅"</p>
                            <p class="mhub-empty-body">"Tidak ada event yang menunggu review."</p>
                        </div>
                    }.into_any();
                }
                evs.into_iter().map(|ev| {
                    let id_approve = ev.id.clone();
                    let id_reject  = ev.id.clone();
                    let id_proc_a  = ev.id.clone();
                    let id_proc_b  = ev.id.clone();
                    let id_lbl_a   = ev.id.clone();
                    let id_lbl_b   = ev.id.clone();

                    let cover = ev.cover_url.as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("https://images.unsplash.com/photo-1514525253161-7a46d19cd819?w=600&q=80")
                        .to_string();
                    let title  = ev.name.clone();
                    let date   = format_date(&ev.event_date);
                    let venue  = ev.venue.clone().unwrap_or_default();
                    let status = ev.status.clone();

                    view! {
                        <div class="mhub-event-card" style="border:2px solid var(--accent-lime)">
                            <div class="mhub-event-card-img-wrap">
                                <img src=cover alt=title.clone() class="mhub-event-card-img"/>
                                <span class="mhub-event-status mhub-event-status--presale">
                                    "⏳ Menunggu Review"
                                </span>
                            </div>
                            <div class="mhub-event-card-body">
                                <div class="mhub-event-card-top-row">
                                    <p class="mhub-event-card-title">{title}</p>
                                    <span style="font-size:0.7rem;color:var(--text-muted);padding:3px 7px;\
                                                 background:var(--surface-2);border-radius:6px;font-weight:600">
                                        {status}
                                    </span>
                                </div>
                                <p class="mhub-event-card-meta">{date}" • "{venue}</p>
                                <div style="display:flex;gap:8px;margin-top:10px">
                                    <button
                                        class="mhub-event-manage-btn"
                                        style="flex:1;background:var(--accent-lime);color:#000;font-weight:700"
                                        disabled=move || processing.with(|p| p.as_deref() == Some(&id_proc_a))
                                        on:click=move |_| do_update(id_approve.clone(), "active")>
                                        {move || if processing.with(|p| p.as_deref() == Some(&id_lbl_a)) {
                                            "Memproses..."
                                        } else { "✅ Aktifkan" }}
                                    </button>
                                    <button
                                        class="mhub-event-manage-btn"
                                        style="flex:1;background:var(--surface-2);color:var(--text-muted)"
                                        disabled=move || processing.with(|p| p.as_deref() == Some(&id_proc_b))
                                        on:click=move |_| do_update(id_reject.clone(), "cancelled")>
                                        {move || if processing.with(|p| p.as_deref() == Some(&id_lbl_b)) {
                                            "Memproses..."
                                        } else { "❌ Batalkan" }}
                                    </button>
                                </div>
                            </div>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </section>
    }
}
