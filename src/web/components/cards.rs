use leptos::prelude::*;

use super::common::gambar_cadangan;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::web::state::products::{product_to_explore_pub, ExploreProduct};
use crate::web::utils::format_number;

#[component]
pub fn ProductCard(
    href: String,
    img: String,
    #[prop(into)] alt: String,
    #[prop(into)] badge: String,
    #[prop(into)] title: String,
    #[prop(optional)] date: Option<String>,
    #[prop(into)] venue: String,
    #[prop(into)] price: String,
) -> impl IntoView {
    view! {
        <A href=href attr:class="explore-product-card-v2">
            <div class="explore-ecard-img-wrap">
                <img src=img alt=alt class="explore-ecard-img" on:error=gambar_cadangan />
                <span class="explore-ecard-cat">{badge}</span>
            </div>
            <div class="explore-ecard-body">
                <h3 class="explore-ecard-title">{title}</h3>
                {date
                    .map(|d| {
                        view! {
                            <div class="explore-ecard-meta">
                                <svg
                                    width="11"
                                    height="11"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                >
                                    <rect x="3" y="4" width="18" height="18" rx="2" />
                                    <line x1="16" y1="2" x2="16" y2="6" />
                                    <line x1="8" y1="2" x2="8" y2="6" />
                                    <line x1="3" y1="10" x2="21" y2="10" />
                                </svg>
                                <span>{d}</span>
                            </div>
                        }
                    })}
                <div class="explore-ecard-meta">
                    <svg
                        width="11"
                        height="11"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    >
                        <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0118 0z" />
                        <circle cx="12" cy="10" r="3" />
                    </svg>
                    <span>{venue}</span>
                </div>
                <div class="explore-ecard-footer">
                    <span class="explore-ecard-price">{price}</span>
                    <span class="explore-ecard-arrow">
                        <svg
                            width="14"
                            height="14"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2.5"
                            stroke-linecap="round"
                        >
                            <line x1="5" y1="12" x2="19" y2="12" />
                            <polyline points="12 5 19 12 12 19" />
                        </svg>
                    </span>
                </div>
            </div>
        </A>
    }
}

#[component]
pub fn ProductCardShimmer() -> impl IntoView {
    // ── Memakai KELAS KARTU YANG ASLI, bukan kelas skeleton sendiri ─────────
    //
    // Versi sebelumnya memakai pohon kelas terpisah (`.shim-product-card`,
    // `.shim-img`, `.shim-body`, …) yang meniru bentuk kartu dengan ukurannya
    // sendiri. Meniru berarti menebak, dan tebakannya meleset: kartu yang sudah
    // termuat berukuran lain, sehingga tata letak MELOMPAT tepat pada saat data
    // tiba — pergeseran yang paling terasa justru di jaringan lambat, yaitu
    // saat skeleton paling lama terlihat.
    //
    // Dengan memakai `exp-mkt-*` yang sama persis, ukurannya identik BUKAN
    // karena disamakan dengan hati-hati, melainkan karena tak ada dua ukuran
    // yang bisa berselisih. Kalau kartunya berubah nanti, skeletonnya ikut
    // berubah sendiri.
    //
    // `aria-hidden`: ini hiasan penantian, bukan isi. Pembaca layar tak perlu
    // mengumumkan delapan kartu kosong.
    view! {
        <div class="exp-mkt-card exp-mkt-card--shim" aria-hidden="true">
            <div class="exp-mkt-img-wrap">
                <div class="shim exp-mkt-img"></div>
            </div>
            <div class="exp-mkt-body">
                <span class="exp-mkt-merchant shim shim-line"></span>
                // Dua baris: judul kartu asli boleh membungkus ke baris kedua,
                // dan skeleton satu baris akan selalu lebih pendek daripada
                // kebanyakan kartu sungguhan.
                <h3 class="exp-mkt-title">
                    <span class="shim shim-line"></span>
                    <span class="shim shim-line shim-line--pendek"></span>
                </h3>
                <span class="exp-mkt-org shim shim-line"></span>
                <div class="exp-mkt-meta">
                    <span class="exp-mkt-meta-row shim shim-line"></span>
                    <span class="exp-mkt-meta-row shim shim-line"></span>
                </div>
                <div class="exp-mkt-price-block">
                    <span class="exp-mkt-from shim shim-line shim-line--pendek"></span>
                    <span class="exp-mkt-price shim shim-line"></span>
                </div>
                <div class="exp-mkt-foot">
                    <span class="exp-mkt-sold shim shim-line shim-line--pendek"></span>
                </div>
            </div>
        </div>
    }
}

/// Kartu product "marketplace" — SATU sumber tunggal untuk daftar product di
/// Explore, Product Detail, dan /m/:id. Menerima `ExploreProduct` (model tampil
/// yang sudah berisi tanggal/harga terformat, is_live, dll). `index` dipakai
/// untuk delay animasi cascade (`--i`).
#[component]
pub fn ProductCardPub(ev: ExploreProduct, #[prop(default = 0)] index: usize) -> impl IntoView {
    let href = format!("/products/{}", ev.slug);
    let loc = if !ev.city.is_empty() {
        ev.city.clone()
    } else {
        ev.venue.clone()
    };
    let cat = ev.category.first().cloned().unwrap_or_default();
    let sold = ev.total_sold.max(0);
    let sold_label = if sold > 0 {
        format!("{} Terjual", format_number(sold as i64))
    } else {
        "Baru".to_string()
    };
    let price_disp = if ev.price <= 0 {
        "Gratis".to_string()
    } else {
        ev.price_str.clone()
    };
    let org_href = format!("/m/{}", ev.merchant_id);
    // Navigasi SPA, bukan `location().assign()`.
    //
    // Chip ini ada di SETIAP kartu product di beranda. Dengan `assign`, menekannya
    // memicu MUAT ULANG DOKUMEN PENUH: seluruh HTML diambil lagi, WASM dimuat
    // dan dihidrasi lagi dari nol — beberapa detik untuk perpindahan yang
    // seharusnya seketika, dan persis terasa seperti "aplikasi nge-refresh
    // sendiri". Router sudah ada di halaman ini; tinggal dipakai.
    let ke_merchant = use_navigate();
    // Nama toko penyelenggara di chip; product lama tanpa nama → fallback generik.
    let org_label = if ev.merchant_name.is_empty() {
        "PENYELENGGARA \u{2192}".to_string()
    } else {
        format!("{} \u{2192}", ev.merchant_name)
    };
    view! {
        <a
            href=href
            class="exp-mkt-card exp-cascade"
            style=format!("--i:{}", (index % 20).min(5))
        >
            <div class="exp-mkt-img-wrap">
                <img
                    src=ev.cover.clone()
                    alt=ev.title.clone()
                    class="exp-mkt-img"
                    loading="lazy"
                    on:error=gambar_cadangan
                />
                {ev
                    .is_live
                    .then(|| {
                        view! {
                            <span class="exp-mkt-live">
                                <span class="exp-mkt-live-dot"></span>
                                "LIVE"
                            </span>
                        }
                    })}
            </div>
            <div class="exp-mkt-body">
                {(!cat.is_empty())
                    .then(|| {
                        view! { <span class="exp-mkt-merchant">{cat.clone()}</span> }
                    })} <h3 class="exp-mkt-title">{ev.title.clone()}</h3>
                // Chip penyelenggara → profil merchant publik. <span> ber-click
                // (bukan <a>) karena kartu ini sendiri <a> — anchor bersarang invalid.
                <span
                    class="exp-mkt-org"
                    on:click={
                        move |e: leptos::ev::MouseEvent| {
                            // `stop_propagation` tetap perlu: tanpa itu klik ikut
                            // naik ke pendengar router di window, yang akan
                            // membaca anchor kartu (induknya) dan justru membuka
                            // halaman product — bukan profil penyelenggara.
                            e.prevent_default();
                            e.stop_propagation();
                            ke_merchant(&org_href, Default::default());
                        }
                    }
                >
                    {org_label}
                </span>
                <div class="exp-mkt-meta">
                    <span class="exp-mkt-meta-row">
                        <svg
                            width="12"
                            height="12"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                            stroke-linejoin="round"
                        >
                            <rect x="3" y="4" width="18" height="18" rx="2" />
                            <line x1="16" y1="2" x2="16" y2="6" />
                            <line x1="8" y1="2" x2="8" y2="6" />
                            <line x1="3" y1="10" x2="21" y2="10" />
                        </svg>
                        {ev.date.clone()}
                    </span>
                    {(!loc.is_empty())
                        .then(|| {
                            view! {
                                <span class="exp-mkt-meta-row">
                                    <svg
                                        width="12"
                                        height="12"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                        stroke-linejoin="round"
                                    >
                                        <path d="M21 10c0 7-9 12-9 12s-9-5-9-12a9 9 0 0118 0z" />
                                        <circle cx="12" cy="10" r="3" />
                                    </svg>
                                    {loc.clone()}
                                </span>
                            }
                        })}
                </div>
                <div class="exp-mkt-price-block">
                    <span class="exp-mkt-from">"Mulai Dari"</span>
                    <span class="exp-mkt-price">{price_disp}</span>
                </div>
                <div class="exp-mkt-foot">
                    <svg
                        class="exp-mkt-star"
                        width="13"
                        height="13"
                        viewBox="0 0 24 24"
                        fill="currentColor"
                        stroke="none"
                    >
                        <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z" />
                    </svg>
                    <span class="exp-mkt-sold">{sold_label}</span>
                </div>
            </div>
        </a>
    }
}

/// Grid daftar product reusable (profil merchant, dsb). Menerima `Vec<Product>`,
/// mengonversi ke `ExploreProduct`, dan merender kartu `ProductCardPub` yang sama
/// dengan Explore. Kosong → teks `empty`.
#[component]
pub fn ProductGrid(
    products: Vec<crate::web::models::Product>,
    #[prop(optional, into)] empty: Option<String>,
) -> impl IntoView {
    if products.is_empty() {
        let msg = empty.unwrap_or_else(|| "Belum ada product.".into());
        return view! { <p class="product-grid-empty">{msg}</p> }.into_any();
    }
    view! {
        <div class="exp-mkt-grid">
            {products
                .into_iter()
                .enumerate()
                .map(|(i, e)| {
                    let ev = product_to_explore_pub(&e);
                    view! { <ProductCardPub ev=ev index=i /> }
                })
                .collect_view()}
        </div>
    }
        .into_any()
}

/// Skeleton loading untuk `ProductGrid` — `count` kartu shimmer dalam grid sama.
#[component]
pub fn ProductGridShimmer(#[prop(default = 4)] count: usize) -> impl IntoView {
    view! {
        <div class="exp-mkt-grid">
            {(0..count).map(|_| view! { <ProductCardShimmer /> }).collect_view()}
        </div>
    }
}

#[component]
pub fn TicketCardShimmer() -> impl IntoView {
    // Skeleton meniru layout .ticket-card asli di /tickets:
    // cover penuh → judul besar → baris venue•tanggal → meta grid 2 kolom
    // (TIER/PRICE) → QR di tengah → footer (tombol OPEN QR + kode tiket).
    view! {
        <div class="shim-ticket-card">
            <div class="shim shim-tkt-cover"></div>
            <div class="shim-tkt-body">
                <div class="shim shim-tkt-title"></div>
                <div class="shim shim-tkt-venue"></div>
                <div class="shim-tkt-meta">
                    <div class="shim shim-tkt-meta-item"></div>
                    <div class="shim shim-tkt-meta-item"></div>
                </div>
                <div class="shim-tkt-qr-wrap">
                    <div class="shim shim-tkt-qr"></div>
                    <div class="shim shim-tkt-code"></div>
                </div>
                <div class="shim-tkt-foot">
                    <div class="shim shim-tkt-badge"></div>
                    <div class="shim shim-tkt-price"></div>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn OrderCardShimmer() -> impl IntoView {
    view! {
        <div class="shim-order-card">
            <div class="shim-order-card-top">
                <div class="shim shim-ord-thumb"></div>
                <div class="shim-ord-info">
                    <div class="shim shim-ord-name"></div>
                    <div class="shim shim-ord-date"></div>
                </div>
            </div>
            <div class="shim shim-ord-divider"></div>
            <div class="shim-foot">
                <div class="shim shim-ord-amount"></div>
                <div class="shim shim-ord-btn"></div>
            </div>
        </div>
    }
}

#[component]
pub fn MerchantRowShimmer() -> impl IntoView {
    view! {
        <div class="shim-merchant-row">
            <div class="shim-mr-info">
                <div class="shim shim-mr-name"></div>
                <div class="shim shim-mr-venue"></div>
            </div>
            <div class="shim shim-mr-badge"></div>
        </div>
    }
}

/// Skeleton that mirrors the `.mhub-product-card` layout used on the merchant &
/// admin hubs (cover image, title/price row, meta, sales progress, action btn).
/// The previous `MerchantRowShimmer` looked nothing like the real cards, which
/// made the loading state jarring.
#[component]
pub fn MerchantProductCardShimmer() -> impl IntoView {
    view! {
        <div class="shim-mhub-card">
            <div class="shim shim-mhub-img"></div>
            <div class="shim-mhub-body">
                <div class="shim-mhub-row">
                    <div class="shim shim-mhub-title"></div>
                    <div class="shim shim-mhub-price"></div>
                </div>
                <div class="shim shim-mhub-meta"></div>
                <div class="shim-mhub-row">
                    <div class="shim shim-mhub-key"></div>
                    <div class="shim shim-mhub-key"></div>
                </div>
                <div class="shim shim-mhub-bar"></div>
                <div class="shim shim-mhub-btn"></div>
            </div>
        </div>
    }
}

#[component]
pub fn MessageRowShimmer() -> impl IntoView {
    view! {
        <div class="shim-msg-row">
            <div class="shim shim-avatar"></div>
            <div class="shim-msg-info">
                <div class="shim shim-msg-name"></div>
                <div class="shim shim-msg-preview"></div>
            </div>
        </div>
    }
}

// ── Marketplace C2C (Hub) ────────────────────────────────────────────────────

/// Kartu posting marketplace — bentuk visual meniru `ProductCardPub` (foto,
/// judul, meta, harga, footer) tapi field-nya milik `Post`, bukan event/tiket:
/// lencana Jual/Cari, lencana kondisi Baru/Bekas (hanya untuk Jual), harga
/// atau "Nego" bila kosong, jumlah suka+komentar sebagai footer.
#[component]
pub fn PostCard(post: crate::web::models::Post, #[prop(default = 0)] index: usize) -> impl IntoView {
    let href = format!("/marketplace/{}", post.id);
    let cover = post.images.first().cloned().unwrap_or_default();
    let kind_label = if post.kind == "jual" { "JUAL" } else { "CARI" };
    let kind_cls = if post.kind == "jual" {
        "pk-badge pk-badge--jual"
    } else {
        "pk-badge pk-badge--cari"
    };
    let condition_label = post.condition.as_deref().map(|c| {
        if c == "baru" { "Baru" } else { "Bekas" }
    });
    let price_disp = match post.price {
        Some(p) if p > 0 => crate::web::models::format_price(p as f64),
        _ => "Nego".to_string(),
    };
    view! {
        <a
            href=href
            class="pk-card exp-cascade"
            style=format!("--i:{}", (index % 20).min(5))
        >
            <div class="pk-img-wrap">
                {if cover.is_empty() {
                    view! { <div class="pk-img pk-img--kosong"></div> }.into_any()
                } else {
                    view! {
                        <img
                            src=cover
                            alt=post.title.clone()
                            class="pk-img"
                            loading="lazy"
                            on:error=gambar_cadangan
                        />
                    }.into_any()
                }}
                <span class=kind_cls><span class="pk-badge-dot"></span>{kind_label}</span>
                {condition_label.map(|c| view! { <span class="pk-badge pk-badge--kondisi">{c}</span> })}
            </div>
            <div class="pk-body">
                <h3 class="pk-title">{post.title.clone()}</h3>
                {(!post.city.clone().unwrap_or_default().is_empty()).then(|| {
                    let city = post.city.clone().unwrap_or_default();
                    view! {
                        <span class="pk-meta-row">
                            <svg width="11" height="11" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <path d="M21 10c0 7-9 12-9 12s-9-5-9-12a9 9 0 0118 0z" />
                                <circle cx="12" cy="10" r="3" />
                            </svg>
                            {city}
                        </span>
                    }
                })}
                <div class="pk-price-block">
                    <span class="pk-price">{price_disp}</span>
                </div>
                <div class="pk-foot">
                    <span class="pk-foot-item">
                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M20.8 4.6a5.5 5.5 0 00-7.8 0L12 5.6l-1-1a5.5 5.5 0 00-7.8 7.8l1 1L12 21l7.8-7.6 1-1a5.5 5.5 0 000-7.8z" />
                        </svg>
                        {post.like_count.max(0)}
                    </span>
                    <span class="pk-foot-item">
                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M21 11.5a8.38 8.38 0 01-.9 3.8 8.5 8.5 0 01-7.6 4.7 8.38 8.38 0 01-3.8-.9L3 21l1.9-5.7a8.38 8.38 0 01-.9-3.8 8.5 8.5 0 014.7-7.6 8.38 8.38 0 013.8-.9h.5a8.48 8.48 0 018 8v.5z" />
                        </svg>
                        {post.comment_count.max(0)}
                    </span>
                </div>
            </div>
        </a>
    }
}

/// Skeleton `PostCard` — kelas `pk-*` yang SAMA (bukan pohon skeleton
/// terpisah), sama alasannya dengan `ProductCardShimmer`: ukuran identik
/// karena tak ada dua ukuran yang bisa berselisih.
#[component]
pub fn PostCardShimmer() -> impl IntoView {
    view! {
        <div class="pk-card pk-card--shim" aria-hidden="true">
            <div class="pk-img-wrap">
                <div class="shim pk-img"></div>
            </div>
            <div class="pk-body">
                <h3 class="pk-title">
                    <span class="shim shim-line"></span>
                    <span class="shim shim-line shim-line--pendek"></span>
                </h3>
                <span class="pk-meta-row shim shim-line"></span>
                <div class="pk-price-block">
                    <span class="pk-price shim shim-line shim-line--pendek"></span>
                </div>
            </div>
        </div>
    }
}
