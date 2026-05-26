//! order_tickets.rs — Tiket dalam satu order (SSR). Route: /orders/:id/tickets

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::get_order_tickets;
use crate::web::app::AuthResource;
use crate::web::models::{format_date, format_price};

#[component]
pub fn OrderTicketsPage() -> impl IntoView {
    let params = use_params_map();
    let order_id = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let tickets = Resource::new(
        move || (order_id(), is_logged_in()),
        |(id, logged_in)| async move {
            if logged_in && !id.is_empty() {
                get_order_tickets(id).await
            } else {
                Ok(vec![])
            }
        },
    );

    view! {
        <div class="page-header">
            <div class="container">
                <p class="page-header__eyebrow">"// tiket kamu"</p>
                <h1 class="page-header__title">"Tiket Order"</h1>
            </div>
        </div>

        <div class="container" style="padding-bottom:4rem">
            <div style="margin-bottom:1.25rem">
                <A href="/orders" attr:class="btn btn--ghost btn--sm">"← Kembali ke Order"</A>
            </div>

            <Suspense fallback=|| view! {
                <div class="loading">
                    <div class="loading__spinner"/>
                    <span>"Memuat tiket..."</span>
                </div>
            }>
                {move || {
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <div style="text-align:center;padding:4rem 0">
                                <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                            </div>
                        }.into_any();
                    }

                    tickets.get().map(|res| match res {
                        Err(_) => view! {
                            <div class="alert alert--error">"Gagal memuat tiket."</div>
                        }.into_any(),
                        Ok(list) if list.is_empty() => view! {
                            <div class="empty">
                                <div class="empty__icon">"🎟"</div>
                                <div class="empty__title">"Tidak ada tiket"</div>
                            </div>
                        }.into_any(),
                        Ok(list) => {
                            // Show event header from first ticket
                            let event_name = list.first().map(|t| t.event_name.clone()).unwrap_or_default();
                            let event_date = list.first().and_then(|t| Some(format_date(&t.event_date))).unwrap_or_default();
                            view! {
                                <div>
                                    <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:12px;padding:1.25rem;margin-bottom:1.5rem">
                                        <h2 style="font-size:1.1rem;font-weight:700;margin-bottom:.25rem">{event_name}</h2>
                                        <p style="font-size:.85rem;color:var(--clr-muted)">"📅 "{event_date}</p>
                                    </div>

                                    <div style="display:flex;flex-direction:column;gap:1rem">
                                        {list.into_iter().enumerate().map(|(idx, t)| {
                                            let code   = t.ticket_code.clone();
                                            let var    = t.variant_name.clone();
                                            let price  = format_price(t.unit_price);
                                            let venue  = t.event_venue.clone().unwrap_or_default();
                                            let status = t.status.clone();
                                            let status_cls = if status == "used" {
                                                "badge badge--muted"
                                            } else {
                                                "badge badge--success"
                                            };
                                            let ticket_href = format!("/tickets/{}", t.id);

                                            view! {
                                                <div class="ticket-card fade-in">
                                                    <div class="ticket-card__accent"/>
                                                    <div class="ticket-card__body">
                                                        <div style="display:flex;justify-content:space-between;align-items:start;margin-bottom:.75rem">
                                                            <span style="font-size:.75rem;color:var(--clr-muted)">
                                                                {format!("Tiket #{}", idx + 1)}
                                                            </span>
                                                            <span class=status_cls>
                                                                {if status == "used" { "Sudah Digunakan" } else { "Aktif" }}
                                                            </span>
                                                        </div>
                                                        <div class="ticket-card__variant">{var} " · " {price}</div>
                                                        <div class="ticket-card__code">{code}</div>
                                                        {if !venue.is_empty() {
                                                            view! { <div class="ticket-card__meta">"📍 "{venue}</div> }.into_any()
                                                        } else {
                                                            view! { <span/> }.into_any()
                                                        }}
                                                        <div style="margin-top:.75rem">
                                                            <A href=ticket_href attr:class="btn btn--ghost btn--sm">"Lihat Detail"</A>
                                                        </div>
                                                    </div>
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>
                                </div>
                            }.into_any()
                        }
                    }).unwrap_or_else(|| view! { <div/> }.into_any())
                }}
            </Suspense>
        </div>
    }
}
