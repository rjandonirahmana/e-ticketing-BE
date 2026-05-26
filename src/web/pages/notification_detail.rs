//! notification_detail.rs — Detail notifikasi (SSR).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::{get_notification_detail, mark_notification_read};
use crate::web::app::AuthResource;

#[component]
pub fn NotificationDetailPage() -> impl IntoView {
    let params = use_params_map();
    let notif_id = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let notif = Resource::new(
        move || (notif_id(), is_logged_in()),
        |(id, logged_in)| async move {
            if logged_in && !id.is_empty() {
                let _ = mark_notification_read(id.clone()).await;
                get_notification_detail(id).await
            } else {
                Err(ServerFnError::ServerError("not_ready".into()))
            }
        },
    );

    view! {
        <div class="container" style="padding-top:2rem;padding-bottom:4rem;max-width:680px">
            <div style="margin-bottom:1.5rem">
                <A href="/notifications" attr:class="btn btn--ghost btn--sm">"← Notifikasi"</A>
            </div>

            <Suspense fallback=|| view! {
                <div class="loading">
                    <div class="loading__spinner"/>
                </div>
            }>
                {move || notif.get().map(|res| match res {
                    Err(e) if e.to_string().contains("not_ready") => view! { <div/> }.into_any(),
                    Err(_) => view! {
                        <div class="empty">
                            <div class="empty__icon">"😕"</div>
                            <div class="empty__title">"Notifikasi tidak ditemukan"</div>
                        </div>
                    }.into_any(),
                    Ok(n) => {
                        let icon = match n.kind.as_str() {
                            "payment_success" | "order_paid" => "✅",
                            "promo" => "🏷",
                            "event_reminder" => "📅",
                            _ => "🔔",
                        };
                        let cta_href = n.order_id.as_ref().map(|id| format!("/orders/{id}"))
                            .or_else(|| n.ticket_id.as_ref().map(|id| format!("/tickets/{id}")));

                        view! {
                            <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:16px;padding:2rem">
                                <div style="font-size:2.5rem;margin-bottom:1rem">{icon}</div>
                                <h1 style="font-size:1.25rem;font-weight:700;margin-bottom:.75rem">{n.title}</h1>
                                <p style="color:var(--clr-muted);line-height:1.6;margin-bottom:1.5rem">{n.body}</p>

                                {if let Some(href) = cta_href {
                                    view! {
                                        <A href=href attr:class="btn btn--accent">"Lihat Detail"</A>
                                    }.into_any()
                                } else {
                                    view! { <span/> }.into_any()
                                }}
                            </div>
                        }.into_any()
                    }
                }).unwrap_or_else(|| view! { <div/> }.into_any())}
            </Suspense>
        </div>
    }
}
