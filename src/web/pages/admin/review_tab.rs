use leptos::prelude::*;

use crate::web::api::update_product_status_admin;
use crate::web::models::{format_date, Product};

pub(super) fn view_review(
    pending_products: RwSignal<Vec<Product>>,
    all_products: RwSignal<Vec<Product>>,
    toast: RwSignal<Option<(String, bool)>>,
) -> impl IntoView {
    let processing: RwSignal<Option<String>> = RwSignal::new(None);

    let do_update = move |event_id: String, status: &'static str| {
        let id = event_id.clone();
        processing.set(Some(id.clone()));
        leptos::task::spawn_local(async move {
            match update_product_status_admin(id.clone(), status.to_string()).await {
                Ok(_) => {
                    pending_products.update(|v| v.retain(|e| e.id != id));
                    all_products.update(|v| {
                        if let Some(ev) = v.iter_mut().find(|e| e.id == id) {
                            ev.status = status.to_string();
                        }
                    });
                    let msg = match status {
                        "active"    => "✅ Produk diaktifkan.",
                        "cancelled" => "❌ Produk dibatalkan.",
                        _           => "Status diperbarui.",
                    };
                    toast.set(Some((msg.to_string(), false)));
                }
                Err(e) => toast.set(Some((format!("Gagal: {e}"), true))),
            }
            processing.set(None);
        });
    };

    view! {
        <section class="mhub-products-section">
            <div class="mhub-products-header">
                <h3 class="mhub-products-title">"Review Produk"</h3>
                <span class="mhub-live-badge">
                    <span class="mhub-live-dot"></span>
                    "Menunggu"
                </span>
            </div>
            {move || {
                let evs = pending_products.get();
                if evs.is_empty() {
                    return view! {
                        <div class="mhub-empty">
                            <div class="mhub-empty-icon-wrap">
                                <svg width="38" height="38" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                    <polyline points="20 6 9 17 4 12"/>
                                </svg>
                            </div>
                            <p class="mhub-empty-title">"Semua Bersih ✅"</p>
                            <p class="mhub-empty-body">"Tidak ada produk yang menunggu review."</p>
                        </div>
                    }.into_any();
                }
                evs.into_iter().map(|ev| {
                    let id_approve = ev.id.clone();
                    let id_reject  = ev.id.clone();
                    let id_proc_a  = ev.id.clone();
                    let id_proc_b  = ev.id.clone();
                    let id_lbl_a   = ev.id.clone();
                    let id_lbl_b   = ev.id.clone();

                    let cover = ev.cover_url.as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("https://images.unsplash.com/photo-1514525253161-7a46d19cd819?w=600&q=80")
                        .to_string();
                    let title  = ev.name.clone();
                    let date   = format_date(&ev.event_date);
                    let venue  = ev.venue.clone().unwrap_or_default();
                    let status = ev.status.clone();

                    view! {
                        <div class="mhub-product-card" style="border:2px solid var(--accent-lime)">
                            <div class="mhub-product-card-img-wrap">
                                <img src=cover alt=title.clone() class="mhub-product-card-img"/>
                                <span class="mhub-product-status mhub-product-status--presale">
                                    "⏳ Menunggu Review"
                                </span>
                            </div>
                            <div class="mhub-product-card-body">
                                <div class="mhub-product-card-top-row">
                                    <p class="mhub-product-card-title">{title}</p>
                                    <span style="font-size:0.7rem;color:var(--text-muted);padding:3px 7px;\
                                                 background:var(--surface-2);border-radius:6px;font-weight:600">
                                        {status}
                                    </span>
                                </div>
                                <p class="mhub-product-card-meta">{date}" • "{venue}</p>
                                <div style="display:flex;gap:8px;margin-top:10px">
                                    <button
                                        class="mhub-product-manage-btn"
                                        style="flex:1;background:var(--accent-lime);color:#000;font-weight:700"
                                        disabled=move || processing.with(|p| p.as_deref() == Some(&id_proc_a))
                                        on:click=move |_| do_update(id_approve.clone(), "active")>
                                        {move || if processing.with(|p| p.as_deref() == Some(&id_lbl_a)) {
                                            "Memproses..."
                                        } else { "✅ Aktifkan" }}
                                    </button>
                                    <button
                                        class="mhub-product-manage-btn"
                                        style="flex:1;background:var(--surface-2);color:var(--text-muted)"
                                        disabled=move || processing.with(|p| p.as_deref() == Some(&id_proc_b))
                                        on:click=move |_| do_update(id_reject.clone(), "cancelled")>
                                        {move || if processing.with(|p| p.as_deref() == Some(&id_lbl_b)) {
                                            "Memproses..."
                                        } else { "❌ Batalkan" }}
                                    </button>
                                </div>
                            </div>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </section>
    }
}
