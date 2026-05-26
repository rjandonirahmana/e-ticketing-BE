//! merchant.rs — Halaman Merchant Hub.
//!
//! Token diambil dari cookie (melalui server function) — tidak ada localStorage.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::get_merchant_events;
use crate::web::app::AuthResource;
use crate::web::models::{format_date, format_price, PaginatedEvents};

#[component]
pub fn MerchantPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let page = RwSignal::new(1i64);

    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    // Resource events merchant — server function membaca token dari cookie
    let events = Resource::new(
        move || (is_logged_in(), page.get()),
        |(logged_in, p)| async move {
            if logged_in {
                get_merchant_events(Some(p)).await
            } else {
                Ok(PaginatedEvents {
                    data: vec![],
                    total: 0,
                    page: 1,
                    per_page: 20,
                    total_pages: 0,
                })
            }
        },
    );

    view! {
        <div class="page-header">
            <div class="container">
                <div style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:1rem">
                    <div>
                        <h1 class="page-header__title">"Merchant Hub"</h1>
                        <p class="page-header__sub">"Kelola event dan penjualan tiketmu"</p>
                    </div>
                    <A href="/merchant/events/create" attr:class="btn btn--accent">"+ Buat Event Baru"</A>
                </div>
            </div>
        </div>

        <div class="container" style="padding-bottom:4rem">
            <Suspense fallback=|| view! {
                <div class="loading">
                    <div class="loading__spinner"/>
                    <span>"Memuat data..."</span>
                </div>
            }>
                {move || {
                    // Belum login
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <div class="container" style="padding:4rem 0;text-align:center">
                                <p style="color:var(--clr-muted);margin-bottom:1.5rem">
                                    "Kamu harus masuk sebagai merchant."
                                </p>
                                <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                            </div>
                        }.into_any();
                    }

                    events.get().map(|res| match res {
                        Ok(pg) => {
                            let total       = pg.total;
                            let total_pages = pg.total_pages;

                            view! {
                                // Stats
                                <div class="merchant-stats">
                                    <div class="stat-card">
                                        <div class="stat-card__label">"Total Event"</div>
                                        <div class="stat-card__value">{total}</div>
                                    </div>
                                    <div class="stat-card">
                                        <div class="stat-card__label">"Halaman Ini"</div>
                                        <div class="stat-card__value">{pg.data.len()}</div>
                                    </div>
                                </div>

                                {if pg.data.is_empty() {
                                    view! {
                                        <div class="empty">
                                            <div class="empty__icon">"🎪"</div>
                                            <div class="empty__title">"Belum ada event"</div>
                                            <div class="empty__sub">"Buat event pertamamu dan mulai jual tiket!"</div>
                                            <A href="/merchant/events/create" attr:class="btn btn--accent" attr:style="margin-top:1.5rem">
                                                "Buat Event"
                                            </A>
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div style="display:flex;flex-direction:column;gap:.75rem">
                                            {pg.data.into_iter().map(|e| {
                                                let slug   = e.slug.clone();
                                                let name   = e.name.clone();
                                                let date   = format_date(&e.event_date);
                                                let price  = format_price(e.display_price);
                                                let sold   = e.total_sold;
                                                let quota  = e.total_quota;
                                                let status = e.status.clone();
                                                let status_cls = match status.as_str() {
                                                    "active"    => "badge badge--success",
                                                    "cancelled" => "badge badge--muted",
                                                    "edited"    => "badge badge--blue",
                                                    _           => "badge badge--muted",
                                                };

                                                view! {
                                                    <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:12px;padding:1.25rem;display:flex;align-items:center;gap:1rem;flex-wrap:wrap"
                                                        class="fade-in"
                                                    >
                                                        <div style="flex:1;min-width:200px">
                                                            <div style="font-weight:700;margin-bottom:.25rem">{name}</div>
                                                            <div style="font-size:.8rem;color:var(--clr-muted)">"📅 " {date}</div>
                                                        </div>
                                                        <div style="text-align:right">
                                                            <div style="font-family:var(--font-display);font-weight:700;color:var(--clr-accent)">{price}</div>
                                                            <div style="font-size:.8rem;color:var(--clr-muted)">
                                                                {format!("{sold}/{quota} terjual")}
                                                            </div>
                                                        </div>
                                                        <span class=status_cls>{status}</span>
                                                        <A
                                                            href=format!("/events/{slug}")
                                                            attr:class="btn btn--ghost btn--sm"
                                                        >"Lihat"</A>
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>

                                        // Pagination
                                        <div style="display:flex;justify-content:center;gap:.75rem;padding:2rem 0">
                                            <button
                                                class="btn btn--ghost btn--sm"
                                                on:click=move |_| { if page.get() > 1 { page.update(|p| *p -= 1); } }
                                                disabled=move || page.get() <= 1
                                            >"← Prev"</button>
                                            <span style="display:flex;align-items:center;color:var(--clr-muted);font-size:.875rem">
                                                {move || format!("Hal {} / {}", page.get(), total_pages)}
                                            </span>
                                            <button
                                                class="btn btn--ghost btn--sm"
                                                on:click=move |_| {
                                                    if page.get() < total_pages { page.update(|p| *p += 1); }
                                                }
                                                disabled=move || page.get() >= total_pages
                                            >"Next →"</button>
                                        </div>
                                    }.into_any()
                                }}
                            }.into_any()
                        }
                        Err(_) => view! {
                            <div class="alert alert--error">
                                "Akses ditolak. Pastikan akun kamu terdaftar sebagai merchant."
                            </div>
                        }.into_any(),
                    }).unwrap_or_else(|| view! { <div/> }.into_any())
                }}
            </Suspense>
        </div>
    }
}
