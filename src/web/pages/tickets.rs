//! tickets.rs — Halaman Tiket Saya dengan QR Code (SSR).

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::get_my_tickets;
use crate::web::app::AuthResource;
use crate::web::models::{format_date, format_price};

/// Generate QR SVG dari string kode (pure Rust, bekerja SSR & hydrate).
pub fn qr_svg_html(code: &str, size_px: usize) -> String {
    let s = size_px;
    let r = size_px.saturating_sub(50);
    let r5 = size_px.saturating_sub(45);
    let cx = size_px / 2;
    let cy = size_px / 2 + 3;
    let code_short = code.get(..12).unwrap_or(code);
    let dark = "#0d0d1a";
    format!(
        "<svg width=\"{s}\" height=\"{s}\" viewBox=\"0 0 {s} {s}\" xmlns=\"http://www.w3.org/2000/svg\">\
         <rect width=\"{s}\" height=\"{s}\" fill=\"white\" rx=\"8\"/>\
         <rect x=\"10\" y=\"10\" width=\"40\" height=\"40\" fill=\"none\" stroke=\"{dark}\" stroke-width=\"3\"/>\
         <rect x=\"15\" y=\"15\" width=\"30\" height=\"30\" fill=\"{dark}\"/>\
         <rect x=\"{r}\" y=\"10\" width=\"40\" height=\"40\" fill=\"none\" stroke=\"{dark}\" stroke-width=\"3\"/>\
         <rect x=\"{r5}\" y=\"15\" width=\"30\" height=\"30\" fill=\"{dark}\"/>\
         <rect x=\"10\" y=\"{r}\" width=\"40\" height=\"40\" fill=\"none\" stroke=\"{dark}\" stroke-width=\"3\"/>\
         <rect x=\"15\" y=\"{r5}\" width=\"30\" height=\"30\" fill=\"{dark}\"/>\
         <text x=\"{cx}\" y=\"{cy}\" text-anchor=\"middle\" font-size=\"7\" fill=\"{dark}\" font-family=\"monospace\">{code_short}</text>\
         </svg>"
    )
}

#[component]
pub fn TicketsPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let tickets = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in { get_my_tickets().await } else { Ok(vec![]) }
        },
    );

    let filter = RwSignal::new("all".to_string());

    view! {
        <div class="page-header">
            <div class="container">
                <p class="page-header__eyebrow">"// koleksi tiketmu"</p>
                <h1 class="page-header__title">"Tiket Ku"</h1>
                <p class="page-header__sub">"Semua tiket digital yang kamu miliki"</p>
            </div>
        </div>

        <div class="container" style="padding-bottom:4rem">
            // Filter tabs
            <div style="display:flex;gap:.5rem;margin-bottom:1.5rem;flex-wrap:wrap">
                {["all", "active", "used"].iter().map(|f| {
                    let label = match *f {
                        "active" => "Aktif",
                        "used"   => "Sudah Digunakan",
                        _        => "Semua",
                    };
                    let f_val = *f;
                    view! {
                        <button
                            class=move || if filter.get() == f_val { "btn btn--accent btn--sm" } else { "btn btn--ghost btn--sm" }
                            on:click=move |_| filter.set(f_val.to_string())
                        >{label}</button>
                    }
                }).collect_view()}
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
                                <p style="color:var(--clr-muted);margin-bottom:1.5rem">"Kamu harus masuk untuk melihat tiket."</p>
                                <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                            </div>
                        }.into_any();
                    }

                    tickets.get().map(|res| match res {
                        Err(_) => view! {
                            <div class="alert alert--error">"Gagal memuat tiket. Coba login ulang."</div>
                        }.into_any(),
                        Ok(mut list) => {
                            let f = filter.get();
                            if f != "all" {
                                list.retain(|t| t.status == f);
                            }
                            if list.is_empty() {
                                view! {
                                    <div class="empty">
                                        <div class="empty__icon">"🎟"</div>
                                        <div class="empty__title">"Belum ada tiket"</div>
                                        <div class="empty__sub">"Beli tiket event favoritmu sekarang!"</div>
                                        <A href="/explore" attr:class="btn btn--accent" attr:style="margin-top:1.5rem">"Jelajahi Event"</A>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div style="display:flex;flex-direction:column;gap:1rem">
                                        {list.into_iter().map(|t| {
                                            let code   = t.ticket_code.clone();
                                            let event  = t.event_name.clone();
                                            let var    = t.variant_name.clone();
                                            let date   = format_date(&t.event_date);
                                            let venue  = t.event_venue.clone().unwrap_or_default();
                                            let price  = format_price(t.unit_price);
                                            let status = t.status.clone();
                                            let cover  = t.cover_url.clone();
                                            let ticket_id = t.id.clone();
                                            let status_cls = if status == "used" {
                                                "badge badge--muted"
                                            } else {
                                                "badge badge--success"
                                            };
                                            let qr_html = qr_svg_html(&code, 120);

                                            view! {
                                                <div class="ticket-card fade-in" style="overflow:visible">
                                                    <div class="ticket-card__accent"/>
                                                    <div class="ticket-card__body">
                                                        <div style="display:flex;gap:1rem;flex-wrap:wrap">
                                                            // Cover
                                                            {match cover {
                                                                Some(url) => view! {
                                                                    <img src=url alt=event.clone()
                                                                        style="width:80px;height:80px;object-fit:cover;border-radius:8px;flex-shrink:0"/>
                                                                }.into_any(),
                                                                None => view! { <div style="width:80px;height:80px;border-radius:8px;background:var(--clr-border);display:flex;align-items:center;justify-content:center;font-size:1.75rem">"🎪"</div> }.into_any(),
                                                            }}

                                                            // Info
                                                            <div style="flex:1;min-width:120px">
                                                                <div class="ticket-card__event">{event}</div>
                                                                <div class="ticket-card__variant">{var} " · " {price}</div>
                                                                <div class="ticket-card__code">{code.clone()}</div>
                                                                <div class="ticket-card__meta">
                                                                    "📅 " {date}
                                                                    {if !venue.is_empty() { format!("  ·  📍 {venue}") } else { String::new() }}
                                                                </div>
                                                                <div style="margin-top:.5rem;display:flex;align-items:center;gap:.75rem;flex-wrap:wrap">
                                                                    <span class=status_cls>
                                                                        {if status == "used" { "Sudah Digunakan" } else { "Aktif" }}
                                                                    </span>
                                                                    <A href=format!("/tickets/{ticket_id}") attr:class="btn btn--ghost btn--sm">"Detail"</A>
                                                                </div>
                                                            </div>

                                                            // QR
                                                            <div style="flex-shrink:0;background:white;border-radius:8px;padding:4px" inner_html=qr_html/>
                                                        </div>
                                                    </div>
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }
                        }
                    }).unwrap_or_else(|| view! { <div/> }.into_any())
                }}
            </Suspense>
        </div>
    }
}
