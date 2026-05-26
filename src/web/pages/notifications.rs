//! notifications.rs — Halaman Notifikasi (SSR).

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::get_notifications;
use crate::web::app::AuthResource;

#[component]
pub fn NotificationsPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let notifs = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in { get_notifications().await } else { Ok(vec![]) }
        },
    );

    view! {
        <div class="page-header">
            <div class="container">
                <p class="page-header__eyebrow">"// pemberitahuan"</p>
                <h1 class="page-header__title">"Notifikasi"</h1>
                <p class="page-header__sub">"Update terbaru tentang tiket, promo, dan event"</p>
            </div>
        </div>

        <div class="container" style="padding-bottom:4rem;max-width:720px">
            <Suspense fallback=|| view! {
                <div class="loading">
                    <div class="loading__spinner"/>
                    <span>"Memuat notifikasi..."</span>
                </div>
            }>
                {move || {
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <div style="text-align:center;padding:4rem 0">
                                <p style="color:var(--clr-muted);margin-bottom:1.5rem">
                                    "Kamu harus masuk untuk melihat notifikasi."
                                </p>
                                <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                            </div>
                        }.into_any();
                    }

                    notifs.get().map(|res| match res {
                        Err(_) => view! {
                            <div class="alert alert--error">"Gagal memuat notifikasi."</div>
                        }.into_any(),
                        Ok(list) if list.is_empty() => view! {
                            <div class="empty">
                                <div class="empty__icon">"🔔"</div>
                                <div class="empty__title">"Semua Tenang"</div>
                                <div class="empty__sub">
                                    "Belum ada notifikasi. Notifikasi tentang tiket, promo, dan event akan muncul di sini."
                                </div>
                                <A href="/explore" attr:class="btn btn--accent" attr:style="margin-top:1.5rem">"Jelajahi Event"</A>
                            </div>
                        }.into_any(),
                        Ok(list) => view! {
                            <div style="display:flex;flex-direction:column;gap:.5rem">
                                {list.into_iter().map(|n| {
                                    let icon = match n.kind.as_str() {
                                        "payment_success" | "order_paid" => "✅",
                                        "promo"                          => "🏷",
                                        "event_reminder"                 => "📅",
                                        _                                => "🔔",
                                    };
                                    let href = n.order_id.as_ref().map(|id| format!("/orders/{id}"))
                                        .or_else(|| n.ticket_id.as_ref().map(|id| format!("/tickets/{id}")))
                                        .unwrap_or_else(|| format!("/notifications/{}", n.id));
                                    let bg = if n.is_read {
                                        "background:var(--clr-surface);border:1px solid var(--clr-border)"
                                    } else {
                                        "background:var(--clr-surface);border:1px solid var(--clr-accent);border-left-width:3px"
                                    };

                                    view! {
                                        <A href=href attr:class="fade-in" attr:style=format!("{bg};display:block;border-radius:12px;padding:1.25rem;text-decoration:none;color:inherit")>
                                            <div style="display:flex;gap:.875rem;align-items:start">
                                                <span style="font-size:1.25rem">{icon}</span>
                                                <div style="flex:1;min-width:0">
                                                    <div style="font-weight:700;margin-bottom:.2rem">{n.title}</div>
                                                    <div style="font-size:.85rem;color:var(--clr-muted);line-height:1.4">{n.body}</div>
                                                </div>
                                                {if !n.is_read {
                                                    view! { <span style="width:8px;height:8px;border-radius:50%;background:var(--clr-accent);flex-shrink:0;margin-top:4px"/> }.into_any()
                                                } else {
                                                    view! { <span/> }.into_any()
                                                }}
                                            </div>
                                        </A>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any(),
                    }).unwrap_or_else(|| view! { <div/> }.into_any())
                }}
            </Suspense>
        </div>
    }
}
