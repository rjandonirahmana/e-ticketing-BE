//! web/pages/event_detail.rs — Event Detail SSR (production-ready).
//!
//! Fixed vs draft sebelumnya:
//!   1. `order_id` → `created_order_id` signal (tidak undefined lagi).
//!   2. `slug` di-memoize untuk stabilitas SSR/hydration.
//!   3. Hapus import `use_navigate` yang tidak dipakai.
//!   4. `Effect::new` redirect baca dari signal, bukan variable async scope.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_params_map;

use crate::web::api::{create_order, get_event_detail};
use crate::web::app::AuthResource;
use crate::web::models::{format_date, format_price};

#[component]
pub fn EventDetailPage() -> impl IntoView {
    let params = use_params_map();

    // ── Memoized slug: stabilitas dependency tracking SSR/hydration ────────────
    let slug = Memo::new(move |_| {
        params.read().get("slug").unwrap_or_default()
    });

    // ── DATA: blocking resource (SSR menunggu data selesai) ────────────────────
    let event = Resource::new_blocking(move || slug.get(), |s| get_event_detail(s));

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    // Booking state
    let selected_variant = RwSignal::new(Option::<String>::None);
    let qty = RwSignal::new(1i32);
    let ordering = RwSignal::new(false);
    let order_err = RwSignal::new(Option::<String>::None);
    let order_ok = RwSignal::new(false);
    let created_order_id = RwSignal::new(String::new()); // ← FIX: signal untuk order_id

    // ── INTERAKTIVITAS browser: redirect pasca-order (client-only) ─────────────
    Effect::new(move |_| {
        if order_ok.get() {
            #[cfg(target_arch = "wasm32")]
            if let Some(win) = web_sys::window() {
                let path = if created_order_id.get().is_empty() {
                    "/tickets".to_string()
                } else {
                    format!("/orders/{}", created_order_id.get())
                };
                let _ = win.location().replace(&path);
            }
        }
    });

    // ── Order handler (hanya jalan di client via on:click) ────────────────────
    let do_order = move |_| {
        let Some(variant_id) = selected_variant.get() else {
            order_err.set(Some("Pilih tipe tiket dahulu.".into()));
            return;
        };

        if !is_logged_in() {
            #[cfg(target_arch = "wasm32")]
            if let Some(win) = web_sys::window() {
                let _ = win.location().replace("/login");
            }
            return;
        }

        ordering.set(true);
        order_err.set(None);

        spawn_local(async move {
            match create_order(variant_id, qty.get()).await {
                Ok(order_id) => {
                    created_order_id.set(order_id); // ← FIX: simpan ke signal
                    order_ok.set(true);
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
            <Suspense fallback=|| {
                view! {
                    <div class="loading" style="min-height:60vh">
                        <div class="loading__spinner" />
                        <span>"Memuat event..."</span>
                    </div>
                }
            }>
                {move || Suspend::new(async move {
                    let res = event.await;
                    match res {
                        Err(_) => {
                            view! {
                                <div class="empty" style="min-height:50vh">
                                    <div class="empty__icon">"😕"</div>
                                    <div class="empty__title">"Event tidak ditemukan"</div>
                                </div>
                            }
                                .into_any()
                        }
                        Ok(ev) => {
                            let title = ev.name.clone();
                            let desc = ev.description.clone();
                            let cats = ev.category.clone();
                            let city = ev.city.clone();
                            let venue = ev.venue.clone();
                            let date = format_date(&ev.event_date);
                            let quota = ev.total_quota - ev.total_sold;
                            let variants = ev.event_variants.clone();
                            let cover = ev.cover_url.clone();

                            view! {
                                {if let Some(url) = cover {
                                    view! {
                                        <img class="event-detail__hero" src=url alt=title.clone() />
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <div class="event-detail__hero-placeholder">"🎪"</div>
                                    }
                                        .into_any()
                                }}

                                <div class="event-detail__layout">
                                    <div>
                                        <div class="event-detail__cats">
                                            {cats
                                                .into_iter()
                                                .map(|c| {
                                                    view! { <span class="event-detail__cat">{c}</span> }
                                                })
                                                .collect_view()}
                                        </div>

                                        <h1 class="event-detail__title">{title}</h1>

                                        <ul class="event-detail__meta-list">
                                            <li>"📅  " {date}</li>
                                            {venue.map(|v| view! { <li>"📍  " {v}</li> })}
                                            {city.map(|c| view! { <li>"🏙   " {c}</li> })}
                                            <li>"🎟  " {format!("{quota} kursi tersisa")}</li>
                                        </ul>

                                        {desc
                                            .map(|d| {
                                                view! {
                                                    <div>
                                                        <h2 style="font-size:.8rem;margin-bottom:0.75rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.06em">
                                                            "Tentang Event"
                                                        </h2>
                                                        <p class="event-detail__desc">{d}</p>
                                                    </div>
                                                }
                                            })}
                                    </div>

                                    <div>
                                        <div class="ticket-panel">
                                            <p class="ticket-panel__title">"Pilih Tiket"</p>

                                            <div class="ticket-panel__variants">
                                                {variants
                                                    .into_iter()
                                                    .map(|v| {
                                                        let vid = v.id.clone();
                                                        let vid2 = v.id.clone();
                                                        let name = v.name.clone();
                                                        let desc = v.description.clone();
                                                        let price = format_price(v.display_price);
                                                        let orig = v.sale_price.map(|_| format_price(v.price));
                                                        let remaining = v.remaining;
                                                        let max_qty = v.max_per_order.unwrap_or(10);

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
                                                                        {orig
                                                                            .map(|o| {
                                                                                view! {
                                                                                    <div style="font-size:.75rem;color:var(--clr-muted);text-decoration:line-through">
                                                                                        {o}
                                                                                    </div>
                                                                                }
                                                                            })} <div class="variant-item__price">{price}</div>
                                                                    </div>
                                                                </div>
                                                                {desc
                                                                    .map(|d| {
                                                                        view! { <div class="variant-item__desc">{d}</div> }
                                                                    })}
                                                                <div class="variant-item__stock">
                                                                    {format!("{remaining} tersisa • max {max_qty}/order")}
                                                                </div>
                                                            </div>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </div>

                                            <Show when=move || selected_variant.get().is_some()>
                                                <div style="margin-bottom:.75rem;font-size:.85rem;color:var(--clr-muted)">
                                                    "Jumlah tiket"
                                                </div>
                                                <div class="qty-picker">
                                                    <button
                                                        class="qty-picker__btn"
                                                        on:click=move |_| {
                                                            if qty.get() > 1 {
                                                                qty.update(|q| *q -= 1);
                                                            }
                                                        }
                                                    >
                                                        "-"
                                                    </button>
                                                    <span class="qty-picker__val">{qty}</span>
                                                    <button
                                                        class="qty-picker__btn"
                                                        on:click=move |_| qty.update(|q| *q += 1)
                                                    >
                                                        "+"
                                                    </button>
                                                </div>
                                            </Show>

                                            {move || {
                                                order_err
                                                    .get()
                                                    .map(|e| {
                                                        view! { <div class="alert alert--error">{e}</div> }
                                                    })
                                            }}

                                            <Show when=move || order_ok.get()>
                                                <div class="alert alert--success">
                                                    "Order berhasil! Mengalihkan..."
                                                </div>
                                            </Show>

                                            {move || {
                                                if is_logged_in() {
                                                    view! {
                                                        <button
                                                            class="btn btn--accent btn--full btn--lg"
                                                            on:click=do_order
                                                            disabled=move || {
                                                                ordering.get() || selected_variant.get().is_none()
                                                            }
                                                        >
                                                            {move || {
                                                                if ordering.get() { "Memproses..." } else { "Beli Tiket" }
                                                            }}
                                                        </button>
                                                    }
                                                        .into_any()
                                                } else {
                                                    view! {
                                                        <a href="/login" class="btn btn--accent btn--full btn--lg">
                                                            "Masuk untuk Beli Tiket"
                                                        </a>
                                                    }
                                                        .into_any()
                                                }
                                            }}

                                            <p style="font-size:.75rem;color:var(--clr-muted);text-align:center;margin-top:.75rem">
                                                "Tiket digital dikirim langsung ke akunmu"
                                            </p>
                                        </div>
                                    </div>
                                </div>
                            }
                                .into_any()
                        }
                    }
                })}
            </Suspense>
        </div>
    }
}