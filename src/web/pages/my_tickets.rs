//! my_tickets.rs — Halaman Tiket Ku.
//!
//! Token diambil dari cookie (melalui server function) — tidak ada localStorage.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::get_my_tickets;
use crate::web::app::AuthResource;
use crate::web::models::{format_date, format_price};

#[component]
pub fn MyTicketsPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");

    // Cek login dulu dari context; resource ticket hanya di-fetch kalau login
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    // Resource tiket — server function membaca token dari cookie secara otomatis
    let tickets = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in {
                get_my_tickets().await
            } else {
                Ok(vec![])
            }
        },
    );

    view! {
        <div class="page-header">
            <div class="container">
                <p class="page-header__eyebrow">"// koleksi tiketmu"</p>
                <h1 class="page-header__title">"Tiket Ku"</h1>
                <p class="page-header__sub">"Semua tiket digital yang kamu miliki"</p>
            </div>
        </div>

        <div class="container" style="padding-bottom:4rem">
            <Suspense fallback=|| view! {
                <div class="loading">
                    <div class="loading__spinner"/>
                    <span>"Memuat tiket..."</span>
                </div>
            }>
                {move || {
                    // Kalau belum login
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <div class="container" style="padding:4rem 0;text-align:center">
                                <p style="color:var(--clr-muted);margin-bottom:1.5rem">
                                    "Kamu harus masuk untuk melihat tiket."
                                </p>
                                <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                            </div>
                        }.into_any();
                    }

                    tickets.get().map(|res| {
                        match res {
                            Ok(list) if list.is_empty() => view! {
                                <div class="empty">
                                    <div class="empty__icon">"🎟"</div>
                                    <div class="empty__title">"Belum ada tiket"</div>
                                    <div class="empty__sub">"Beli tiket event favoritmu sekarang!"</div>
                                    <A href="/explore" attr:class="btn btn--accent" attr:style="margin-top:1.5rem">
                                        "Jelajahi Event"
                                    </A>
                                </div>
                            }.into_any(),
                            Ok(list) => view! {
                                <div style="display:flex;flex-direction:column;gap:1rem">
                                    {list.into_iter().map(|t| {
                                        let code   = t.ticket_code.clone();
                                        let event  = t.event_name.clone();
                                        let var    = t.variant_name.clone();
                                        let date   = format_date(&t.event_date);
                                        let venue  = t.event_venue.clone().unwrap_or_default();
                                        let price  = format_price(t.unit_price);
                                        let status = t.status.clone();
                                        let status_cls = if status == "used" {
                                            "ticket-card__status ticket-card__status--used"
                                        } else {
                                            "ticket-card__status ticket-card__status--active"
                                        };

                                        view! {
                                            <div class="ticket-card fade-in">
                                                <div class="ticket-card__accent"/>
                                                <div class="ticket-card__body">
                                                    <div class="ticket-card__event">{event}</div>
                                                    <div class="ticket-card__variant">{var} " · " {price}</div>
                                                    <div class="ticket-card__code">{code}</div>
                                                    <div class="ticket-card__meta">
                                                        "📅 " {date}
                                                        {if !venue.is_empty() {
                                                            format!("  ·  📍 {venue}")
                                                        } else {
                                                            String::new()
                                                        }}
                                                    </div>
                                                    <div style="margin-top:.625rem">
                                                        <span class=status_cls>
                                                            {if status == "used" { "Sudah Digunakan" } else { "Aktif" }}
                                                        </span>
                                                    </div>
                                                </div>
                                            </div>
                                        }
                                    }).collect_view()}
                                </div>
                            }.into_any(),
                            Err(_) => view! {
                                <div class="alert alert--error">
                                    "Gagal memuat tiket. "
                                    <a href="/login" style="color:inherit;text-decoration:underline">
                                        "Coba login ulang."
                                    </a>
                                </div>
                            }.into_any(),
                        }
                    }).unwrap_or_else(|| view! { <div/> }.into_any())
                }}
            </Suspense>
        </div>
    }
}
