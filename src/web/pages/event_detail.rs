//! event_detail.rs — Halaman Detail Event dengan SSR.
//!
//! - Event data di-fetch server-side (SSR) untuk SEO optimal
//! - Tombol beli tiket memanggil create_order (server function dengan cookie auth)
//! - Tidak ada localStorage

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::web::api::{create_order, get_event_detail};
use crate::web::app::AuthResource;
use crate::web::models::{format_date, format_price};

#[component]
pub fn EventDetailPage() -> impl IntoView {
    let params = use_params_map();
    let slug = move || params.read().get("slug").unwrap_or_default();

    // Event detail di-fetch SSR (blocking untuk SEO)
    let event = Resource::new(slug, |s| get_event_detail(s));

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    // State untuk booking
    let selected_variant = RwSignal::new(Option::<String>::None);
    let qty = RwSignal::new(1i32);
    let ordering = RwSignal::new(false);
    let order_err = RwSignal::new(Option::<String>::None);
    let order_ok = RwSignal::new(false);

    // Kirim order — create_order server function membaca token dari cookie
    let do_order = move |_| {
        let Some(variant_id) = selected_variant.get() else {
            order_err.set(Some("Pilih tipe tiket dahulu.".into()));
            return;
        };

        if !is_logged_in() {
            // Redirect ke login
            #[cfg(target_arch = "wasm32")]
            {
                if let Some(win) = web_sys::window() {
                    let _ = win.location().replace("/login");
                }
            }
            return;
        }

        ordering.set(true);
        order_err.set(None);

        leptos::task::spawn_local(async move {
            match create_order(variant_id, qty.get()).await {
                Ok(order_id) => {
                    order_ok.set(true);

                    if let Some(win) = web_sys::window() {
                        let path = if order_id.is_empty() {
                            "/tickets".to_string()
                        } else {
                            format!("/orders/{order_id}")
                        };
                        let _ = win.location().replace(&path);
                    }
                }
                Err(e) => {
                    order_err.set(Some(e.to_string()));
                }
            }
            ordering.set(false);
        });
    };

    view! {
        <div class="container" style="padding-top:2rem;padding-bottom:4rem">
            <Suspense fallback=|| view! {
                <div class="loading" style="min-height:60vh">
                    <div class="loading__spinner"/>
                    <span>"Memuat event..."</span>
                </div>
            }>
                {move || event.get().map(|res| {
                    match res {
                        Err(_) => view! {
                            <div class="empty" style="min-height:50vh">
                                <div class="empty__icon">"😕"</div>
                                <div class="empty__title">"Event tidak ditemukan"</div>
                            </div>
                        }.into_any(),
                        Ok(ev) => {
                            let title    = ev.name.clone();
                            let desc     = ev.description.clone();
                            let cats     = ev.category.clone();
                            let city     = ev.city.clone();
                            let venue    = ev.venue.clone();
                            let date     = format_date(&ev.event_date);
                            let quota    = ev.total_quota - ev.total_sold;
                            let variants = ev.event_variants.clone();
                            let cover    = ev.cover_url.clone();

                            view! {
                                // Cover image
                                {if let Some(url) = cover {
                                    view! {
                                        <img class="event-detail__hero" src=url alt=title.clone() />
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="event-detail__hero-placeholder">"🎪"</div>
                                    }.into_any()
                                }}

                                <div class="event-detail__layout">
                                    // ── Kolom Kiri: Info ───────────────────────────
                                    <div>
                                        <div class="event-detail__cats">
                                            {cats.into_iter().map(|c| {
                                                view! { <span class="event-detail__cat">{c}</span> }
                                            }).collect_view()}
                                        </div>

                                        <h1 class="event-detail__title">{title}</h1>

                                        <ul class="event-detail__meta-list">
                                            <li>"📅  " {date}</li>
                                            {venue.map(|v| view! { <li>"📍  " {v}</li> })}
                                            {city.map(|c| view! { <li>"🏙   " {c}</li> })}
                                            <li>"🎟  " {format!("{quota} kursi tersisa")}</li>
                                        </ul>

                                        {desc.map(|d| view! {
                                            <div>
                                                <h2 style="font-size:.8rem;margin-bottom:0.75rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.06em">
                                                    "Tentang Event"
                                                </h2>
                                                <p class="event-detail__desc">{d}</p>
                                            </div>
                                        })}
                                    </div>

                                    // ── Kolom Kanan: Ticket Panel ──────────────────
                                    <div>
                                        <div class="ticket-panel">
                                            <p class="ticket-panel__title">"Pilih Tiket"</p>

                                            <div class="ticket-panel__variants">
                                                {variants.into_iter().map(|v| {
                                                    let vid  = v.id.clone();
                                                    let vid2 = v.id.clone();
                                                    let name = v.name.clone();
                                                    let desc = v.description.clone();
                                                    let price     = format_price(v.display_price);
                                                    let orig      = v.sale_price.map(|_| format_price(v.price));
                                                    let remaining = v.remaining;
                                                    let max_qty   = v.max_per_order.unwrap_or(10);

                                                    view! {
                                                        <div
                                                            class=move || {
                                                                if selected_variant.get().as_deref() == Some(&vid) {
                                                                    "variant-item selected"
                                                                } else {
                                                                    "variant-item"
                                                                }
                                                            }
                                                            on:click=move |_| {
                                                                selected_variant.set(Some(vid2.clone()));
                                                                qty.set(1);
                                                            }
                                                        >
                                                            <div class="variant-item__top">
                                                                <div class="variant-item__name">{name}</div>
                                                                <div>
                                                                    {orig.map(|o| view! {
                                                                        <div style="font-size:.75rem;color:var(--clr-muted);text-decoration:line-through">{o}</div>
                                                                    })}
                                                                    <div class="variant-item__price">{price}</div>
                                                                </div>
                                                            </div>
                                                            {desc.map(|d| view! {
                                                                <div class="variant-item__desc">{d}</div>
                                                            })}
                                                            <div class="variant-item__stock">
                                                                {format!("{remaining} tersisa • max {max_qty}/order")}
                                                            </div>
                                                        </div>
                                                    }
                                                }).collect_view()}
                                            </div>

                                            // Qty picker
                                            <Show when=move || selected_variant.get().is_some()>
                                                <div style="margin-bottom:.75rem;font-size:.85rem;color:var(--clr-muted)">
                                                    "Jumlah tiket"
                                                </div>
                                                <div class="qty-picker">
                                                    <button
                                                        class="qty-picker__btn"
                                                        on:click=move |_| { if qty.get() > 1 { qty.update(|q| *q -= 1); } }
                                                    >"-"</button>
                                                    <span class="qty-picker__val">{qty}</span>
                                                    <button
                                                        class="qty-picker__btn"
                                                        on:click=move |_| qty.update(|q| *q += 1)
                                                    >"+"</button>
                                                </div>
                                            </Show>

                                            {move || order_err.get().map(|e| view! {
                                                <div class="alert alert--error">{e}</div>
                                            })}

                                            <Show when=move || order_ok.get()>
                                                <div class="alert alert--success">"Order berhasil! Mengalihkan..."</div>
                                            </Show>

                                            // Tombol beli — jika belum login tampilkan "Masuk dulu"
                                            {move || {
                                                if is_logged_in() {
                                                    view! {
                                                        <button
                                                            class="btn btn--accent btn--full btn--lg"
                                                            on:click=do_order
                                                            disabled=move || ordering.get() || selected_variant.get().is_none()
                                                        >
                                                            {move || if ordering.get() { "Memproses..." } else { "Beli Tiket" }}
                                                        </button>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <a href="/login" class="btn btn--accent btn--full btn--lg">
                                                            "Masuk untuk Beli Tiket"
                                                        </a>
                                                    }.into_any()
                                                }
                                            }}

                                            <p style="font-size:.75rem;color:var(--clr-muted);text-align:center;margin-top:.75rem">
                                                "Tiket digital dikirim langsung ke akunmu"
                                            </p>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        }
                    }
                })}
            </Suspense>
        </div>
    }
}
