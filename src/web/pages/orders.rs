//! orders.rs — Halaman Riwayat Order (SSR).

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::get_my_orders;
use crate::web::app::AuthResource;
use crate::web::models::{format_date, format_price};

#[component]
pub fn OrdersPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let orders = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in { get_my_orders().await } else { Ok(vec![]) }
        },
    );

    let filter = RwSignal::new("all".to_string());
    let search = RwSignal::new(String::new());

    view! {
        <div class="page-header">
            <div class="container">
                <p class="page-header__eyebrow">"// riwayat transaksi"</p>
                <h1 class="page-header__title">"Order History"</h1>
                <p class="page-header__sub">"Semua pesanan tiket yang pernah kamu buat"</p>
            </div>
        </div>

        <div class="container" style="padding-bottom:4rem">
            // ── Filter & Search ──────────────────────────────────────────────
            <div style="display:flex;gap:.75rem;margin-bottom:1.5rem;flex-wrap:wrap">
                <input
                    type="search"
                    class="filter-bar__input"
                    placeholder="Cari event atau kode order..."
                    prop:value=search
                    on:input=move |ev| search.set(event_target_value(&ev))
                />
                <select
                    class="filter-bar__select"
                    on:change=move |ev| filter.set(event_target_value(&ev))
                >
                    <option value="all">"Semua"</option>
                    <option value="pending">"Menunggu Bayar"</option>
                    <option value="paid">"Lunas"</option>
                    <option value="cancelled">"Dibatalkan"</option>
                </select>
            </div>

            <Suspense fallback=|| view! {
                <div class="loading">
                    <div class="loading__spinner"/>
                    <span>"Memuat order..."</span>
                </div>
            }>
                {move || {
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <div class="container" style="padding:4rem 0;text-align:center">
                                <p style="color:var(--clr-muted);margin-bottom:1.5rem">
                                    "Kamu harus masuk untuk melihat riwayat order."
                                </p>
                                <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                            </div>
                        }.into_any();
                    }

                    orders.get().map(|res| match res {
                        Ok(list) => {
                            let q = search.get().to_lowercase();
                            let f = filter.get();
                            let filtered: Vec<_> = list.into_iter().filter(|o| {
                                let status_match = match f.as_str() {
                                    "pending" => o.status.to_lowercase().contains("pending") || o.status.to_lowercase().contains("waiting"),
                                    "paid" => o.status.to_lowercase() == "paid",
                                    "cancelled" => o.status.to_lowercase() == "cancelled",
                                    _ => true,
                                };
                                let search_match = q.is_empty()
                                    || o.order_code.to_lowercase().contains(&q)
                                    || o.event_name.as_deref().unwrap_or("").to_lowercase().contains(&q);
                                status_match && search_match
                            }).collect();

                            if filtered.is_empty() {
                                view! {
                                    <div class="empty">
                                        <div class="empty__icon">"🛒"</div>
                                        <div class="empty__title">"Belum ada order"</div>
                                        <div class="empty__sub">"Pesanan Anda akan muncul di sini setelah melakukan pembelian."</div>
                                        <A href="/explore" attr:class="btn btn--accent" attr:style="margin-top:1.5rem">"Jelajahi Event"</A>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div style="display:flex;flex-direction:column;gap:.75rem">
                                        {filtered.into_iter().map(|o| {
                                            let is_pending = o.status.to_lowercase().contains("pending") || o.status.to_lowercase().contains("waiting");
                                            let is_paid = o.status.to_lowercase() == "paid";
                                            let status_cls = if is_paid {
                                                "badge badge--success"
                                            } else if is_pending {
                                                "badge badge--accent"
                                            } else {
                                                "badge badge--muted"
                                            };
                                            let action_href = if is_pending {
                                                format!("/orders/{}", o.id)
                                            } else if is_paid {
                                                format!("/orders/{}/tickets", o.id)
                                            } else {
                                                String::new()
                                            };
                                            let event_name = o.event_name.clone().unwrap_or_else(|| "Event".into());
                                            let date_str = o.event_date.as_ref().map(|d| format_date(d)).unwrap_or_default();
                                            let price = format_price(o.total_amount);
                                            let venue = o.venue.clone().unwrap_or_default();
                                            let cover = o.cover_url.clone();
                                            let code = o.order_code.clone();
                                            let status_label = o.status.clone();

                                            view! {
                                                <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:12px;overflow:hidden" class="fade-in">
                                                    <div style="display:flex;gap:1rem;padding:1.25rem;flex-wrap:wrap">
                                                        // Thumbnail
                                                        <div style="width:64px;height:64px;border-radius:8px;overflow:hidden;flex-shrink:0;background:var(--clr-border)">
                                                            {match cover {
                                                                Some(url) => view! { <img src=url alt=event_name.clone() style="width:100%;height:100%;object-fit:cover"/> }.into_any(),
                                                                None => view! { <div style="width:100%;height:100%;display:flex;align-items:center;justify-content:center;font-size:1.5rem">"🎪"</div> }.into_any(),
                                                            }}
                                                        </div>

                                                        // Info
                                                        <div style="flex:1;min-width:150px">
                                                            <div style="font-weight:700;margin-bottom:.25rem">{event_name}</div>
                                                            <div style="font-size:.8rem;color:var(--clr-muted);margin-bottom:.25rem">
                                                                {if !date_str.is_empty() { format!("📅 {date_str}") } else { String::new() }}
                                                                {if !venue.is_empty() { format!("  ·  📍 {venue}") } else { String::new() }}
                                                            </div>
                                                            <div style="font-size:.75rem;color:var(--clr-muted)">{"#"}{code}</div>
                                                        </div>

                                                        // Price + status + action
                                                        <div style="display:flex;flex-direction:column;align-items:flex-end;gap:.5rem">
                                                            <div style="font-family:var(--font-display);font-weight:700;color:var(--clr-accent)">{price}</div>
                                                            <span class=status_cls>{status_label}</span>
                                                            {if !action_href.is_empty() {
                                                                view! {
                                                                    <A href=action_href attr:class="btn btn--ghost btn--sm">
                                                                        {if is_pending { "Bayar Sekarang" } else { "Lihat Tiket" }}
                                                                    </A>
                                                                }.into_any()
                                                            } else {
                                                                view! { <span/> }.into_any()
                                                            }}
                                                        </div>
                                                    </div>
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }
                        }
                        Err(_) => view! {
                            <div class="alert alert--error">"Gagal memuat order. Coba login ulang."</div>
                        }.into_any(),
                    }).unwrap_or_else(|| view! { <div/> }.into_any())
                }}
            </Suspense>
        </div>
    }
}
