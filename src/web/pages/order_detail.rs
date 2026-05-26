//! order_detail.rs — Halaman Detail Order / Konfirmasi Pembayaran (SSR).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::get_order_detail;
use crate::web::app::AuthResource;
use crate::web::models::{format_datetime, format_price};

#[component]
pub fn OrderDetailPage() -> impl IntoView {
    let params = use_params_map();
    let order_id = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let order = Resource::new(
        move || (order_id(), is_logged_in()),
        |(id, logged_in)| async move {
            if logged_in && !id.is_empty() {
                get_order_detail(id).await
            } else {
                Err(ServerFnError::ServerError("not_ready".into()))
            }
        },
    );

    view! {
        <div class="container" style="padding-top:2rem;padding-bottom:4rem;max-width:680px">
            <div style="margin-bottom:1.5rem">
                <A href="/orders" attr:class="btn btn--ghost btn--sm">"← Kembali ke Order"</A>
            </div>

            <Suspense fallback=|| view! {
                <div class="loading" style="min-height:60vh">
                    <div class="loading__spinner"/>
                    <span>"Memuat order..."</span>
                </div>
            }>
                {move || {
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <div style="text-align:center;padding:4rem 0">
                                <p style="color:var(--clr-muted);margin-bottom:1.5rem">"Kamu harus masuk untuk melihat detail order."</p>
                                <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                            </div>
                        }.into_any();
                    }

                    order.get().map(|res| match res {
                        Err(e) if e.to_string().contains("not_ready") => view! { <div/> }.into_any(),
                        Err(_) => view! {
                            <div class="empty" style="min-height:40vh">
                                <div class="empty__icon">"😕"</div>
                                <div class="empty__title">"Order tidak ditemukan"</div>
                                <A href="/orders" attr:class="btn btn--ghost" attr:style="margin-top:1rem">"Kembali"</A>
                            </div>
                        }.into_any(),
                        Ok(o) => {
                            let is_pending = o.status.to_lowercase().contains("pending") || o.status.to_lowercase().contains("waiting");
                            let is_paid    = o.status.to_lowercase() == "paid";
                            let status_cls = if is_paid { "badge badge--success" }
                                else if is_pending { "badge badge--accent" }
                                else { "badge badge--muted" };

                            let expired_str = o.expired_at.as_ref().map(|d| format_datetime(d)).unwrap_or_default();
                            let created_str = o.created_at.as_ref().map(|d| format_datetime(d)).unwrap_or_default();
                            let paid_str    = o.paid_at.as_ref().map(|d| format_datetime(d));
                            let order_id_val = o.id.clone();

                            view! {
                                // ── Header ──────────────────────────────────────────
                                <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:16px;padding:1.75rem;margin-bottom:1rem">
                                    <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:1.25rem;flex-wrap:wrap;gap:.75rem">
                                        <div>
                                            <p style="font-size:.75rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.06em;margin-bottom:.25rem">"ORDER ID"</p>
                                            <p style="font-family:var(--font-mono);font-weight:700;font-size:1rem">{"#"}{o.order_code.clone()}</p>
                                        </div>
                                        <span class=status_cls>{o.status.to_uppercase()}</span>
                                    </div>

                                    <div style="display:grid;grid-template-columns:1fr 1fr;gap:1rem;margin-bottom:1.25rem">
                                        <div>
                                            <p style="font-size:.7rem;color:var(--clr-muted);text-transform:uppercase;margin-bottom:.25rem">"Dibuat"</p>
                                            <p style="font-size:.85rem">{created_str}</p>
                                        </div>
                                        {if is_pending && !expired_str.is_empty() {
                                            view! {
                                                <div>
                                                    <p style="font-size:.7rem;color:var(--clr-muted);text-transform:uppercase;margin-bottom:.25rem">"Batas Bayar"</p>
                                                    <p style="font-size:.85rem;color:var(--clr-accent)">{expired_str}</p>
                                                </div>
                                            }.into_any()
                                        } else if let Some(p_str) = paid_str {
                                            view! {
                                                <div>
                                                    <p style="font-size:.7rem;color:var(--clr-muted);text-transform:uppercase;margin-bottom:.25rem">"Dibayar"</p>
                                                    <p style="font-size:.85rem;color:var(--clr-success)">{p_str}</p>
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! { <div/> }.into_any()
                                        }}
                                    </div>

                                    // ── Items ───────────────────────────────────────
                                    <div style="border-top:1px solid var(--clr-border);padding-top:1rem;margin-bottom:1rem">
                                        <p style="font-size:.75rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.06em;margin-bottom:.75rem">"Item Pesanan"</p>
                                        {o.items.into_iter().map(|item| {
                                            view! {
                                                <div style="display:flex;justify-content:space-between;margin-bottom:.5rem;font-size:.875rem">
                                                    <span>{format!("{}× {} — {}", item.quantity, item.variant_name, item.event_name)}</span>
                                                    <span style="color:var(--clr-accent)">{format_price(item.subtotal)}</span>
                                                </div>
                                            }
                                        }).collect_view()}
                                    </div>

                                    // ── Total ───────────────────────────────────────
                                    <div style="display:flex;justify-content:space-between;border-top:1px solid var(--clr-border);padding-top:1rem">
                                        <span style="font-weight:700">"TOTAL PEMBAYARAN"</span>
                                        <span style="font-family:var(--font-display);font-size:1.25rem;color:var(--clr-accent)">
                                            {format_price(o.total_amount)}
                                        </span>
                                    </div>
                                </div>

                                // ── Actions ──────────────────────────────────────────
                                {if is_pending {
                                    view! {
                                        <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:16px;padding:1.75rem">
                                            <p style="font-size:.875rem;color:var(--clr-muted);margin-bottom:1rem">
                                                "Selesaikan pembayaran sebelum batas waktu berakhir. "
                                                "Hubungi tim kami jika butuh bantuan."
                                            </p>
                                            <div style="display:flex;gap:.75rem;flex-wrap:wrap">
                                                <A href=format!("/checkout?order_id={order_id_val}") attr:class="btn btn--accent">"Lanjut Pembayaran"</A>
                                                <A href="/orders" attr:class="btn btn--ghost">"Kembali"</A>
                                            </div>
                                        </div>
                                    }.into_any()
                                } else if is_paid {
                                    view! {
                                        <A href=format!("/orders/{order_id_val}/tickets") attr:class="btn btn--accent btn--full">"🎟 Lihat Tiket"</A>
                                    }.into_any()
                                } else {
                                    view! {
                                        <A href="/orders" attr:class="btn btn--ghost btn--full">"Kembali ke Order"</A>
                                    }.into_any()
                                }}
                            }.into_any()
                        }
                    }).unwrap_or_else(|| view! { <div/> }.into_any())
                }}
            </Suspense>
        </div>
    }
}
