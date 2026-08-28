use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::{get_banners, get_products};
use crate::web::components::ProductCard;
use crate::web::models::{format_date, format_price};

#[component]
pub fn HomePage() -> impl IntoView {
    let banners = Resource::new(|| (), |_| get_banners());
    let featured = Resource::new(|| (), |_| get_products(Some(1), None, None, None, None));

    view! {
        // ── Hero ─────────────────────────────────────────────────────────────
        <section class="hero">
            <div class="container">
                <p class="hero__eyebrow">"// produk platform terbaik di Indonesia"</p>
                <h1 class="hero__heading">
                    "Temukan Product\nImpianmu, "
                    <span style="color:var(--color-primary)">"Sekarang."</span>
                </h1>
                <p class="hero__sub">
                    "Dari konser hingga festival, ribuan product tersedia di satu tempat.\n\
                    Pesan tiket dalam hitungan detik, langsung ke genggamanmu."
                </p>
                <div class="hero__actions">
                    <A href="/explore" attr:class="btn btn--accent btn--lg">
                        "Jelajahi Product"
                    </A>
                    <A href="/register" attr:class="btn btn--ghost btn--lg">
                        "Daftar Gratis"
                    </A>
                </div>
                <div class="hero__stats">
                    <div>
                        <span class="hero__stat-num">"10K+"</span>
                        <span class="hero__stat-label">"Produk Aktif"</span>
                    </div>
                    <div>
                        <span class="hero__stat-num">"500K+"</span>
                        <span class="hero__stat-label">"Pengguna"</span>
                    </div>
                    <div>
                        <span class="hero__stat-num">"50+"</span>
                        <span class="hero__stat-label">"Kota Indonesia"</span>
                    </div>
                </div>
            </div>
        </section>

        // ── Banners ──────────────────────────────────────────────────────────
        <div class="container">
            <Suspense fallback=|| {
                view! { <div style="height:120px" /> }
            }>
                {move || {
                    banners
                        .get()
                        .map(|res| {
                            if let Ok(list) = res {
                                if list.is_empty() {
                                    return ().into_any();
                                }
                                view! {
                                    <div class="banners">
                                        {list
                                            .into_iter()
                                            .map(|b| {
                                                if let Some(link) = b.link_url {
                                                    view! {
                                                        <a href=link class="banner-slide">
                                                            <img src=b.image_url alt=b.title.unwrap_or_default() />
                                                        </a>
                                                    }
                                                        .into_any()
                                                } else {
                                                    view! {
                                                        <div class="banner-slide">
                                                            <img src=b.image_url alt=b.title.unwrap_or_default() />
                                                        </div>
                                                    }
                                                        .into_any()
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            } else {
                                ().into_any()
                            }
                        })
                }}
            </Suspense>
        </div>

        // ── Featured Products ───────────────────────────────────────────────────
        <section class="section">
            <div class="container">
                <div class="section__header">
                    <h2 class="section__title">"Produk Pilihan"</h2>
                    <A href="/explore" attr:class="section__link">
                        "Lihat semua →"
                    </A>
                </div>

                <Suspense fallback=|| {
                    view! {
                        <div class="products-grid">
                            {(0..4)
                                .map(|_| {
                                    view! {
                                        <div style="display:flex;flex-direction:column;gap:8px">
                                            <div
                                                class="shimmer-bg"
                                                style="width:100%;aspect-ratio:16/10;border-radius:12px;"
                                            ></div>
                                            <div
                                                class="shimmer-bg"
                                                style="width:82%;height:14px;border-radius:6px;"
                                            ></div>
                                            <div
                                                class="shimmer-bg"
                                                style="width:52%;height:12px;border-radius:6px;"
                                            ></div>
                                        </div>
                                    }
                                })
                                .collect_view()}
                        </div>
                    }
                }>
                    {move || {
                        featured
                            .get()
                            .map(|res| {
                                match res {
                                    Ok(pg) if pg.data.is_empty() => {
                                        view! {
                                            <div class="empty">
                                                <div class="empty__icon">"🎪"</div>
                                                <div class="empty__title">"Belum ada produk"</div>
                                                <div class="empty__sub">
                                                    "Nantikan produk menarik yang akan hadir segera!"
                                                </div>
                                            </div>
                                        }
                                            .into_any()
                                    }
                                    Ok(pg) => {
                                        view! {
                                            <div class="products-grid">
                                                {pg
                                                    .data
                                                    .into_iter()
                                                    .map(|e| {
                                                        let href = format!("/products/{}", e.slug);
                                                        let img = e.cover_url.clone().unwrap_or_default();
                                                        let badge = e
                                                            .category
                                                            .first()
                                                            .cloned()
                                                            .unwrap_or_else(|| e.status.clone());
                                                        let date = Some(format_date(&e.event_date));
                                                        let venue = e.venue.clone().unwrap_or_default();
                                                        let price = format_price(e.display_price);
                                                        view! {
                                                            <ProductCard
                                                                href=href
                                                                img=img
                                                                alt=e.name.clone()
                                                                badge=badge
                                                                title=e.name.clone()
                                                                date=date.unwrap_or_default()
                                                                venue=venue
                                                                price=price
                                                            />
                                                        }
                                                    })
                                                    .collect_view()}
                                            </div>
                                        }
                                            .into_any()
                                    }
                                    Err(_) => {
                                        view! {
                                            <div class="alert alert--error">"Gagal memuat produk."</div>
                                        }
                                            .into_any()
                                    }
                                }
                            })
                    }}
                </Suspense>
            </div>
        </section>

        // ── CTA Banner ────────────────────────────────────────────────────────
        <div class="container">
            <div class="cta-banner">
                <h2>"Punya Produk? Jualan di PULSE."</h2>
                <p>
                    "Kelola product, upload tiket, dan raih pembeli dari seluruh Indonesia.\n\
                    Dashboard merchant lengkap, realtime analytics, pembayaran otomatis."
                </p>
                <A href="/register" attr:class="btn btn--accent btn--lg">
                    "Daftar sebagai Merchant"
                </A>
            </div>
        </div>
    }
}
