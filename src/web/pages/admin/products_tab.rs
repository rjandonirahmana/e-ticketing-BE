use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::models::{format_date, format_price, Product};

#[derive(Clone, PartialEq)]
pub(super) enum ProductStatus {
    OnSale,
    SoldOut,
    Presale,
}

impl ProductStatus {
    pub(super) fn from_product(e: &Product) -> Self {
        if e.total_quota > 0 && e.total_sold >= e.total_quota {
            Self::SoldOut
        } else if e.status == "active" {
            Self::OnSale
        } else {
            Self::Presale
        }
    }
    pub(super) fn css_mod(&self) -> &'static str {
        match self {
            Self::OnSale  => "mhub-product-status mhub-product-status--sale",
            Self::SoldOut => "mhub-product-status mhub-product-status--sold",
            Self::Presale => "mhub-product-status mhub-product-status--presale",
        }
    }
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::OnSale  => "Dijual",
            Self::SoldOut => "Habis Terjual",
            Self::Presale => "Pre-order",
        }
    }
}

pub(super) fn view_all_products(evs: Vec<Product>) -> impl IntoView {
    view! {
        <section class="mhub-products-section">
            <div class="mhub-products-header">
                <h3 class="mhub-products-title">"Semua Produk Platform"</h3>
                <span class="mhub-live-badge">
                    <span class="mhub-live-dot"></span>
                    "Langsung"
                </span>
            </div>
            {if evs.is_empty() {
                view! {
                    <div class="mhub-empty">
                        <div class="mhub-empty-icon-wrap">
                            <svg width="38" height="38" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                <rect x="1" y="4" width="22" height="16" rx="2" ry="2"/>
                                <line x1="1" y1="10" x2="23" y2="10"/>
                            </svg>
                        </div>
                        <p class="mhub-empty-title">"Belum Ada Produk"</p>
                        <p class="mhub-empty-body">"Belum ada produk yang terdaftar di platform."</p>
                    </div>
                }.into_any()
            } else {
                evs.into_iter().map(|ev| {
                    let cover = ev.cover_url.as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("https://images.unsplash.com/photo-1514525253161-7a46d19cd819?w=600&q=80")
                        .to_string();
                    let status     = ProductStatus::from_product(&ev);
                    let title      = ev.name.clone();
                    let date       = format_date(&ev.event_date);
                    let venue_str  = match (ev.venue.as_deref(), ev.city.as_deref()) {
                        (Some(v), Some(c)) if !c.is_empty() => format!("{v} • {c}"),
                        (Some(v), _) => v.to_string(),
                        _ => String::new(),
                    };
                    let sold  = ev.total_sold;
                    let quota = ev.total_quota;
                    let avail = (quota - sold).max(0);
                    let pct   = if quota > 0 {
                        ((sold as f64 / quota as f64) * 100.0).round() as u32
                    } else { 0 };
                    let fill_style = format!("width:{pct}%");
                    let (val_text, val_cls) = if status == ProductStatus::SoldOut {
                        ("100% Habis Terjual".to_string(), "mhub-product-progress-val mhub-product-progress-val--sold")
                    } else if quota == 0 {
                        ("—".to_string(), "mhub-product-progress-val")
                    } else {
                        (format!("{sold}/{quota} Terjual"), "mhub-product-progress-val")
                    };
                    let remaining_text = if quota == 0 { String::new() } else { format!("{avail} sisa") };
                    let fill_cls = match &status {
                        ProductStatus::SoldOut => "mhub-product-progress-fill mhub-product-progress-fill--sold",
                        ProductStatus::Presale => "mhub-product-progress-fill mhub-product-progress-fill--lime",
                        _                    => "mhub-product-progress-fill",
                    };
                    let price      = format_price(ev.display_price);
                    let slug       = ev.slug.clone();
                    let status_css = status.css_mod();
                    let status_lbl = status.label();

                    view! {
                        <div class="mhub-product-card">
                            <div class="mhub-product-card-img-wrap">
                                <img src=cover alt=title.clone() class="mhub-product-card-img"/>
                                <span class=status_css>{status_lbl}</span>
                            </div>
                            <div class="mhub-product-card-body">
                                <div class="mhub-product-card-top-row">
                                    <p class="mhub-product-card-title">{title}</p>
                                    <div class="mhub-product-card-price-block">
                                        <span class="mhub-product-price-label">"Mulai dari"</span>
                                        <span class="mhub-product-price-value">{price}</span>
                                    </div>
                                </div>
                                <p class="mhub-product-card-meta">{date}" • "{venue_str}</p>
                                <div class="mhub-product-progress-section">
                                    <div class="mhub-product-progress-row">
                                        <span class="mhub-product-progress-key">"Penjualan"</span>
                                        <span class=val_cls>{val_text}</span>
                                    </div>
                                    <div class="mhub-product-progress-bar">
                                        <div class=fill_cls style=fill_style></div>
                                    </div>
                                    {(!remaining_text.is_empty()).then(|| view! {
                                        <div class="mhub-product-remaining-row">
                                            <span class="mhub-product-remaining-badge">
                                                <svg width="10" height="10" viewBox="0 0 24 24" fill="none"
                                                     stroke="currentColor" stroke-width="2.5">
                                                    <circle cx="12" cy="12" r="10"/>
                                                    <line x1="12" y1="8" x2="12" y2="12"/>
                                                    <line x1="12" y1="16" x2="12.01" y2="16"/>
                                                </svg>
                                                {remaining_text}
                                            </span>
                                        </div>
                                    })}
                                </div>
                                <div class="mhub-product-card-actions">
                                    <A href=format!("/admin/products/{slug}/edit")
                                       attr:class="mhub-product-manage-btn">
                                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                            <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/>
                                            <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/>
                                        </svg>
                                        "Sunting Produk"
                                    </A>
                                </div>
                            </div>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </section>
    }
}
