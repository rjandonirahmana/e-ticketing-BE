//! merchant.rs — Halaman Merchant Hub.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::{
    get_merchant_products, get_merchant_public_products, get_merchant_public_profile,
    update_merchant_profile,
};
use crate::web::app::AuthResource;
use crate::web::components::{
    BottomNav, MerchantProductCardShimmer, SwipeTabBar, TabItem, TabSwipe, ThemeToggle,
};
use crate::web::models::{format_date, format_price, Product, PaginatedProducts};

use super::merchant_public::fmt_count;

// ─── Status badge ─────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum ProductStatus {
    OnSale,
    SoldOut,
    Presale,
}

impl ProductStatus {
    fn from_product(e: &Product) -> Self {
        if e.total_quota > 0 && e.total_sold >= e.total_quota {
            Self::SoldOut
        } else if e.status == "active" {
            Self::OnSale
        } else {
            Self::Presale
        }
    }
    fn css_mod(&self) -> &'static str {
        match self {
            Self::OnSale  => "mhub-product-status mhub-product-status--sale",
            Self::SoldOut => "mhub-product-status mhub-product-status--sold",
            Self::Presale => "mhub-product-status mhub-product-status--presale",
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Self::OnSale  => "On Sale",
            Self::SoldOut => "Sold Out",
            Self::Presale => "Presale",
        }
    }
}

/// Tombol ikon bundar di header Merchant Hub.
///
/// Satu rangkaian dipakai bersama oleh MEET, SCAN, dan lonceng supaya ketiganya
/// tak pernah bergeser ukuran atau warnanya satu sama lain — itu yang membuat
/// deretan tombol sebelumnya terlihat acak. `relative` diperlukan oleh titik
/// notifikasi yang diposisikan absolut di dalam tombol lonceng.
const MHUB_ICON_BTN: &str = "relative inline-flex items-center justify-center w-9 h-9 shrink-0 \
     rounded-full bg-elevated border border-solid border-line text-content \
     transition-colors hover:bg-card-hover active:scale-95";

/// Urutan tab, dan SATU-SATUNYA sumber urutannya.
///
/// Geser kiri/kanan berpindah ke tetangga di larik ini, jadi urutan di bilah tab
/// dan urutan saat digeser mustahil berselisih — kalau keduanya ditulis terpisah,
/// menambah satu tab di kemudian hari akan membuat gesernya melompati tab.
const TABS: [&str; 4] = ["Product", "Analitik", "Keuangan", "Pengaturan"];

// ─── Component ────────────────────────────────────────────────────────────────

#[component]
pub fn MerchantPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");

    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let products = Resource::new(
        move || is_logged_in(),
        |logged_in| async move {
            if logged_in {
                get_merchant_products(Some(1)).await
            } else {
                Ok(PaginatedProducts {
                    data: vec![],
                    total: 0,
                    page: 1,
                    per_page: 20,
                    total_pages: 0,
                })
            }
        },
    );

    // Tab disimpan sebagai INDEKS, bukan kunci string.
    //
    // Versi sebelumnya menyimpan `"tickets"` — sisa masa aplikasi ini menjual
    // tiket — untuk tab yang isinya DAFTAR PRODUK, karena penyapuan istilah
    // waktu itu hanya menyentuh teks yang tampil. Nama internal yang berbohong
    // seperti itu justru yang menuntun ke salah ganti berikutnya. Dengan
    // indeks, urutan bilah tab dan urutan geseran mustahil berselisih: `TABS`
    // adalah satu-satunya sumber keduanya.
    let swipe = TabSwipe::new(TABS.len());
    let tab_items: Vec<TabItem> = TABS.iter().map(|l| TabItem::new(l)).collect();

    let evs_list = move || {
        products
            .get()
            .and_then(|r| r.ok())
            .map(|pg| pg.data)
            .unwrap_or_default()
    };

    let total_sold  = move || evs_list().iter().map(|e| e.total_sold).sum::<i32>();
    let total_quota = move || evs_list().iter().map(|e| e.total_quota).sum::<i32>();
    let capacity_pct = move || {
        let q = total_quota();
        if q == 0 { 0u32 } else { ((total_sold() as f64 / q as f64) * 100.0).round() as u32 }
    };

    view! {
        <div class="page merchant-page mhub-mobile">

            // ── Header ────────────────────────────────────────────────────────
            // Ditulis ulang dengan utility Tailwind, mengikuti `pages/cart.rs`.
            //
            // Yang salah pada versi sebelumnya: LIMA tombol BERLABEL berjejer di
            // kolom yang lebarnya dikunci 480px. Judul "MERCHANT HUB" dan deretan
            // itu berebut ruang yang tak cukup, sehingga judulnya terhimpit dan
            // barisnya meluber. Tiap tombol juga memakai warna penuh yang berbeda
            // — hijau, biru, zaitun, putih, lavender — sehingga tak satu pun
            // terbaca sebagai aksi utama; semuanya berteriak sama keras.
            //
            // Sekarang: SATU aksi utama berlabel (LIVE, karena hanya itu yang
            // mengubah keadaan ke publik), sisanya ikon bundar seragam di atas
            // permukaan netral. Hierarkinya jadi terbaca, dan lebarnya muat
            // dengan sisa ruang untuk judul.
            <header class="sticky top-0 z-[60] flex items-center justify-between gap-3 \
                           px-4 py-3 bg-base border-b border-solid border-line-soft">
                // `min-w-0` + `truncate`: tanpa keduanya, judul menolak mengecil
                // dan justru mendorong deretan tombol keluar dari kolom — persis
                // luberan yang terlihat sebelumnya.
                <span class="min-w-0 truncate font-title text-xl tracking-[0.06em] text-content">
                    "Merchant Hub"
                </span>

                <div class="flex shrink-0 items-center gap-1.5">
                    // Aksi utama: satu-satunya yang berlabel dan berwarna penuh.
                    <A
                        href="/merchant/live"
                        attr:class="inline-flex items-center gap-1.5 h-9 px-3 rounded-full \
                                    bg-brand text-on-brand font-sans text-[11px] font-bold \
                                    tracking-[0.08em] transition-opacity hover:opacity-90 \
                                    active:opacity-80"
                        attr:aria-label="Mulai siaran langsung"
                    >
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
                            <polygon points="5 3 19 12 5 21 5 3"/>
                        </svg>
                        "LIVE"
                    </A>

                    <A href="/meet/host" attr:class=MHUB_ICON_BTN attr:aria-label="Mulai Meet">
                        <svg width="17" height="17" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <polygon points="23 7 16 12 23 17 23 7"/>
                            <rect x="1" y="5" width="15" height="14" rx="2" ry="2"/>
                        </svg>
                    </A>

                    <A href="/scan" attr:class=MHUB_ICON_BTN attr:aria-label="Pindai Ambil">
                        <svg width="17" height="17" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <polyline points="4 7 4 4 7 4"/>
                            <polyline points="20 7 20 4 17 4"/>
                            <polyline points="4 17 4 20 7 20"/>
                            <polyline points="20 17 20 20 17 20"/>
                            <rect x="8" y="8" width="8" height="8" rx="1"/>
                        </svg>
                    </A>

                    <ThemeToggle />

                    <A href="/notifications" attr:class=MHUB_ICON_BTN attr:aria-label="Notifikasi">
                        <svg width="17" height="17" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M18 8a6 6 0 10-12 0c0 7-3 9-3 9h18s-3-2-3-9"/>
                            <path d="M13.73 21a2 2 0 01-3.46 0"/>
                        </svg>
                        // Titik notifikasi: `absolute` di dalam tombol yang
                        // `relative` (lihat MHUB_ICON_BTN).
                        <span class="absolute top-1.5 right-1.5 w-2 h-2 rounded-full \
                                     bg-danger border border-solid border-base"/>
                    </A>
                </div>
            </header>

            // ── Preview profil publik + editor (nama/deskripsi/logo/header) ────
            <MerchantProfileCard />

            // ── Stats strip ───────────────────────────────────────────────────
            <div class="mhub-stats-strip">
                <div class="mhub-stat-cell">
                    <span class="mhub-stat-label">"TOTAL TERJUAL"</span>
                    <span class="mhub-stat-value">
                        {move || {
                            let s = total_sold();
                            if s == 0 { "—".to_string() } else { format!("{s}") }
                        }}
                    </span>
                    <div class="mhub-stat-capacity-bar">
                        <div class="mhub-stat-capacity-fill"
                             style=move || format!("width:{}%", capacity_pct())>
                        </div>
                    </div>
                    <span class="mhub-stat-label">
                        {move || {
                            let q = total_quota();
                            let pct = capacity_pct();
                            if q == 0 { "—".to_string() } else { format!("{pct}% kapasitas") }
                        }}
                    </span>
                </div>
                <div class="mhub-stat-divider"></div>
                <div class="mhub-stat-cell">
                    <span class="mhub-stat-label">"SISA STOK"</span>
                    <span class="mhub-stat-value">
                        {move || format!("{}", (total_quota() - total_sold()).max(0))}
                    </span>
                    <span class="mhub-stat-label">
                        {move || {
                            let q = total_quota();
                            if q == 0 { "—".to_string() } else { format!("dari {q} kuota") }
                        }}
                    </span>
                </div>
            </div>

            // ── Tab bar ───────────────────────────────────────────────────────
            <SwipeTabBar swipe=swipe tabs=tab_items />

            // ── Content ───────────────────────────────────────────────────────
            // Pendengar sentuhan dipasang di PEMBUNGKUS KONTEN, bukan di bilah
            // tab: yang digeser orang adalah isinya, dan bilah tab sendiri
            // terlalu tipis untuk digesek dengan nyaman.
            <div
                class="tabdeck"
                on:touchstart=swipe.on_start()
                on:touchmove=swipe.on_move()
                on:touchend=swipe.on_end()
                on:touchcancel=swipe.on_end()
            >
                <div class="tabdeck-inner" style=move || swipe.gaya_dek()>
                    <Suspense fallback=move || {
                        (0..3).map(|_| view! { <MerchantProductCardShimmer /> }).collect_view()
                    }>
                        {move || {
                            // Kelas panel dibaca DI DALAM penutup ini supaya
                            // pembungkusnya ikut dibangun ulang tiap pindah tab —
                            // itu yang membuat animasi luncurnya diputar lagi.
                            let i = swipe.index();
                            let evs = evs_list();
                            let isi = match i {
                                1 => view_analytics(evs).into_any(),
                                2 => view_finance().into_any(),
                                3 => view_settings().into_any(),
                                _ => view_products(evs).into_any(),
                            };
                            view! { <div class=swipe.kelas_panel()>{isi}</div> }
                        }}
                    </Suspense>
                </div>
            </div>

        </div>
        <BottomNav active="merchant" />

        // ── FAB ───────────────────────────────────────────────────────────────
        <A href="/merchant/products/create" attr:class="mhub-fab" attr:aria-label="Product baru">
            <svg width="22" height="22" viewBox="0 0 24 24" fill="none"
                 stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                <line x1="12" y1="5" x2="12" y2="19"/>
                <line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
        </A>
    }
}

// ─── Tickets tab ──────────────────────────────────────────────────────────────

fn view_products(evs: Vec<Product>) -> impl IntoView {
    view! {
        <section class="mhub-products-section">
            <div class="mhub-products-header">
                <h3 class="mhub-products-title">"Product Saya"</h3>
                <span class="mhub-live-badge">
                    <span class="mhub-live-dot"></span>
                    "Live"
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
                        <p class="mhub-empty-title">"Belum Ada Product"</p>
                        <p class="mhub-empty-body">
                            "Buat product pertamamu dan mulai berjualan."
                        </p>
                        <A href="/merchant/products/create" attr:class="mhub-empty-cta">
                            <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                <line x1="12" y1="5" x2="12" y2="19"/>
                                <line x1="5" y1="12" x2="19" y2="12"/>
                            </svg>
                            "BUAT PRODUCT PERTAMA"
                        </A>
                    </div>
                }
                .into_any()
            } else {
                evs.into_iter()
                    .map(|ev| {
                        let cover = ev
                            .cover_url
                            .as_deref()
                            .filter(|s| !s.is_empty())
                            .unwrap_or(
                                "https://images.unsplash.com/photo-1514525253161-7a46d19cd819?w=600&q=80",
                            )
                            .to_string();
                        let status = ProductStatus::from_product(&ev);
                        let title  = ev.name.clone();
                        let date   = format_date(&ev.event_date);
                        let venue_str = match (ev.venue.as_deref(), ev.city.as_deref()) {
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
                            (
                                "100% Sold Out".to_string(),
                                "mhub-product-progress-val mhub-product-progress-val--sold",
                            )
                        } else if quota == 0 {
                            ("—".to_string(), "mhub-product-progress-val")
                        } else {
                            (format!("{sold}/{quota} Terjual"), "mhub-product-progress-val")
                        };
                        let remaining_text =
                            if quota == 0 { String::new() } else { format!("{avail} sisa") };
                        let fill_cls = match &status {
                            ProductStatus::SoldOut => {
                                "mhub-product-progress-fill mhub-product-progress-fill--sold"
                            }
                            ProductStatus::Presale => {
                                "mhub-product-progress-fill mhub-product-progress-fill--lime"
                            }
                            _ => "mhub-product-progress-fill",
                        };
                        let price     = format_price(ev.display_price);
                        let slug      = ev.slug.clone();
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
                                        {(!remaining_text.is_empty()).then(|| {
                                            view! {
                                                <div class="mhub-product-remaining-row">
                                                    <span class="mhub-product-remaining-badge">
                                                        <svg width="10" height="10" viewBox="0 0 24 24"
                                                             fill="none" stroke="currentColor" stroke-width="2.5">
                                                            <circle cx="12" cy="12" r="10"/>
                                                            <line x1="12" y1="8" x2="12" y2="12"/>
                                                            <line x1="12" y1="16" x2="12.01" y2="16"/>
                                                        </svg>
                                                        {remaining_text}
                                                    </span>
                                                </div>
                                            }
                                        })}
                                    </div>

                                    <div class="mhub-product-card-actions">
                                        // Pratinjau sisi pembeli. Halaman produk
                                        // memang publik, jadi yang dibutuhkan cuma
                                        // tautannya — tak ada mode khusus, dan
                                        // karenanya tak ada risiko pratinjau
                                        // menampilkan sesuatu yang berbeda dari
                                        // yang benar-benar dilihat pembeli.
                                        //
                                        // `target="_blank"`: merchant biasanya
                                        // memeriksa tampilan lalu kembali menyunting.
                                        // Membuka di tab yang sama membuang posisi
                                        // gulir daftar produknya setiap kali.
                                        <A
                                            href=format!("/products/{slug}")
                                            attr:class="mhub-product-manage-btn"
                                            attr:target="_blank"
                                            attr:rel="noopener"
                                            attr:title="Lihat seperti yang dilihat pembeli">
                                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                                 stroke="currentColor" stroke-width="2" stroke-linecap="round"
                                                 stroke-linejoin="round">
                                                <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
                                                <circle cx="12" cy="12" r="3"/>
                                            </svg>
                                            "Lihat"
                                        </A>
                                        <A
                                            href=format!("/merchant/products/{slug}/edit")
                                            attr:class="mhub-product-manage-btn">
                                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                                 stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                                <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/>
                                                <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/>
                                            </svg>
                                            "Edit Product"
                                        </A>
                                    </div>
                                </div>
                            </div>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </section>
    }
}

// ─── Analytics tab ────────────────────────────────────────────────────────────

fn view_analytics(evs: Vec<Product>) -> impl IntoView {
    let total        = evs.len();
    let active_count = evs.iter().filter(|e| e.status == "active").count();
    let top          = evs.iter().max_by_key(|e| e.total_sold).cloned();

    view! {
        <section class="merchant-stats">
            <div class="merchant-card merchant-velocity" style="margin-bottom:12px">
                <h3 class="merchant-section-title">"Product Terlaris"</h3>
                {if let Some(t) = top {
                    let pct = if t.total_quota > 0 {
                        ((t.total_sold as f64 / t.total_quota as f64) * 100.0).round() as u32
                    } else { 0 };
                    let title = t.name.clone();
                    let sold  = t.total_sold;
                    let quota = t.total_quota;
                    view! {
                        <div style="margin-top:10px">
                            <p style="font-size:13px;font-weight:600;margin-bottom:6px">{title}</p>
                            <div style="display:flex;justify-content:space-between;margin-bottom:4px">
                                <span class="merchant-label">{format!("{sold} terjual")}</span>
                                <span class="merchant-label">{format!("{pct}%")}</span>
                            </div>
                            <div style="background:var(--bg-elevated);height:6px;border-radius:3px;overflow:hidden">
                                <div style=format!(
                                    "width:{pct}%;background:var(--accent-lime);height:6px;border-radius:3px"
                                )></div>
                            </div>
                            <span class="merchant-label" style="margin-top:4px;display:block">
                                {format!("{quota} total kuota")}
                            </span>
                        </div>
                    }
                    .into_any()
                } else {
                    view! {
                        <p class="merchant-label" style="margin-top:8px">"Belum ada data."</p>
                    }
                    .into_any()
                }}
            </div>
            <div class="merchant-tile-row">
                <div class="merchant-tile">
                    <span class="merchant-label">"TOTAL PRODUCT"</span>
                    <span class="merchant-tile-value">{total}</span>
                </div>
                <div class="merchant-tile merchant-tile--accent">
                    <span class="merchant-label">"PRODUCT AKTIF"</span>
                    <span class="merchant-tile-value">{active_count}</span>
                </div>
            </div>
        </section>
    }
}

// ─── Finance tab ──────────────────────────────────────────────────────────────

fn view_finance() -> impl IntoView {
    view! {
        <section class="merchant-stats">
            <div class="merchant-card merchant-card--earnings">
                <span class="merchant-label">"SALDO TERSEDIA"</span>
                <h2 class="merchant-amount">"Rp 2.485.900.000"</h2>
                <div class="merchant-trend-row">
                    <span class="merchant-trend-meta">"Pending: Rp 340.200.000"</span>
                    <span class="merchant-trend-meta merchant-trend-meta--right">
                        "Settlement: 15 Nov"
                    </span>
                </div>
            </div>
        </section>
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Tarik Dana"</h3>
            <div class="mhub-form-row" style="margin-top:12px">
                <label class="mhub-form-label">"JUMLAH (RP)"</label>
                <input type="number" class="mhub-form-input" placeholder="cth. 1000000"/>
            </div>
            <div class="mhub-form-row" style="margin-top:10px">
                <label class="mhub-form-label">"REKENING"</label>
                <select class="mhub-form-select">
                    <option>"BCA — ****2847"</option>
                </select>
            </div>
            <button class="mhub-modal-submit" style="width:100%;margin-top:14px">
                "AJUKAN PENARIKAN"
            </button>
        </section>
    }
}

// ─── Settings tab ─────────────────────────────────────────────────────────────

fn view_settings() -> impl IntoView {
    view! {
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Profil Bisnis"</h3>
            <div class="mhub-form-row" style="margin-top:12px">
                <label class="mhub-form-label">"NAMA BISNIS"</label>
                <input type="text" class="mhub-form-input" value="Stellar Product Indonesia"/>
            </div>
            <div class="mhub-form-row" style="margin-top:10px">
                <label class="mhub-form-label">"EMAIL KONTAK"</label>
                <input type="email" class="mhub-form-input" value="contact@stellar.id"/>
            </div>
            <button class="mhub-modal-submit" style="width:100%;margin-top:14px">
                "Simpan Profil"
            </button>
        </section>
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Notifikasi"</h3>
            {[
                ("Penjualan Baru", true),
                ("Konfirmasi Payout", true),
                ("Laporan Mingguan", false),
            ]
            .iter()
            .map(|(l, c)| {
                view! {
                    <div class="mhub-toggle-row">
                        <span class="mhub-toggle-label">{*l}</span>
                        <label class="mhub-toggle-switch">
                            <input type="checkbox" prop:checked=*c/>
                            <span class="mhub-toggle-track"></span>
                        </label>
                    </div>
                }
            })
            .collect_view()}
        </section>
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Keamanan"</h3>
            <div class="mhub-security-actions">
                <button class="mhub-security-btn">"🔒  Ganti Password"</button>
                <button class="mhub-security-btn">
                    "📱  Aktifkan 2FA  "
                    <span class="mhub-security-badge mhub-security-badge--off">"OFF"</span>
                </button>
            </div>
        </section>
    }
}

// ─── Profil merchant: preview publik + editor (nama/deskripsi/logo/header) ──────

/// Unggah gambar (logo/header) ke POST /upload/merchant-image → balas URL.
/// `pub(crate)`: dipakai ulang admin (tab Spanduk) untuk unggah gambar banner —
/// endpoint menerima role merchant DAN admin.
///
/// Unggah satu gambar merchant sambil MELAPORKAN PERSENTASE yang sudah naik.
///
/// ── KENAPA XMLHttpRequest, BUKAN `fetch` ─────────────────────────────────────
/// Versi sebelumnya memakai `fetch`, dan itulah sebabnya tak ada persentase yang
/// bisa ditampilkan: `fetch` tidak melaporkan progres BADAN PERMINTAAN yang
/// sedang naik sama sekali. Yang bisa dipantaunya hanya respons yang turun
/// (`response.body` sebagai stream) — arah yang salah untuk sebuah unggahan.
/// `ReadableStream` sebagai request body memang ada, tapi butuh HTTP/2, ditolak
/// sebagian proxy, dan masih perlu penghitungan manual.
///
/// `xhr.upload.onprogress` adalah satu-satunya API peramban yang melaporkan
/// byte terkirim secara langsung, dan ia didukung di mana-mana. Itu sebabnya
/// unggahan berpindah ke XHR di sini; perilaku selain pelaporan progresnya
/// sengaja dibuat identik dengan versi `fetch` sebelumnya.
///
/// `on_progress` dipanggil dengan 0–100. Bila server tak mengirim panjang
/// total (`length_computable == false`, mis. di balik proxy yang memotong
/// `Content-Length`), callback TIDAK dipanggil sama sekali — pemanggil harus
/// tetap menampilkan sesuatu yang masuk akal tanpa angka, bukan diam di 0%.
/// Perkecil + kompres gambar DI PERAMBAN sebelum diunggah.
///
/// ── KENAPA DI KLIEN, BUKAN DI SERVER ────────────────────────────────────────
/// Foto dari kamera ponsel lazimnya 3000–4000 piksel dan 4–8 MB. Yang terjadi
/// pada berkas sebesar itu, berurutan: ia dikirim utuh lewat jaringan pengguna,
/// dibaca UTUH ke RAM server (`web/api/upload.rs` memakai `field.bytes()`),
/// didorong ke RustFS/S3, lalu — dan ini yang paling mahal — DIUNDUH ULANG oleh
/// setiap pembeli yang membuka galeri produk. Galeri lima foto bisa berarti
/// 30 MB per pengunjung.
///
/// Mengecilkannya di server berarti menambah crate pengolah gambar dan membakar
/// CPU pada kotak 2 vCPU untuk setiap unggahan. Peramban sudah punya pengurai
/// JPEG dan encoder WebP yang dipercepat perangkat keras, dan pekerjaannya
/// terjadi di mesin yang tidak dibayar per jam.
///
/// Sisi 1600 piksel dipilih dari tempat fotonya dipakai: kolom aplikasi dikunci
/// 480 px, jadi 1600 masih menyisakan margin untuk layar 3× dan untuk zoom.
///
/// Mengembalikan `None` berarti "pakai berkas aslinya" — dan itu keputusan yang
/// benar pada tiga keadaan: GIF (menggambar ulang ke canvas hanya menyalin
/// frame pertama dan animasinya mati), berkas yang memang sudah kecil, dan
/// hasil kompresi yang ternyata TIDAK lebih kecil. Yang terakhir nyata terjadi
/// pada tangkapan layar PNG beraut tajam, dan menukarnya tetap akan membuat
/// berkasnya lebih besar.
#[cfg(target_arch = "wasm32")]
async fn kompres_gambar(file: &web_sys::File) -> Option<web_sys::Blob> {
    use std::cell::RefCell;
    use std::rc::Rc;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    const MAKS_SISI: f64 = 1600.0;
    /// Di bawah ini, menyandi ulang lebih sering memperbesar daripada
    /// memperkecil — dan tetap membayar waktu penguraian gambarnya.
    const AMBANG_BYTE: f64 = 300.0 * 1024.0;

    if file.type_() == "image/gif" {
        return None;
    }
    let ukuran_asal = file.size();

    let win = web_sys::window()?;
    let doc = win.document()?;
    let url = web_sys::Url::create_object_url_with_blob(file).ok()?;

    // RAII kecil: object URL WAJIB dicabut di SETIAP jalur keluar. Kalau tidak,
    // peramban menahan seluruh isi berkas di memori sampai tab ditutup — persis
    // kebocoran yang hendak dihindari fungsi ini.
    struct UrlGuard(String);
    impl Drop for UrlGuard {
        fn drop(&mut self) {
            let _ = web_sys::Url::revoke_object_url(&self.0);
        }
    }
    let _guard = UrlGuard(url.clone());

    let img: web_sys::HtmlImageElement = doc
        .create_element("img")
        .ok()?
        .dyn_into::<web_sys::HtmlImageElement>()
        .ok()?;

    // Tunggu gambar selesai diurai. `onerror` ikut dipasang: berkas rusak atau
    // format yang tak bisa diurai peramban harus JATUH KE ASLINYA, bukan
    // menggantungkan unggahan selamanya.
    let (tx, rx) = futures::channel::oneshot::channel::<bool>();
    let tx = Rc::new(RefCell::new(Some(tx)));
    let kirim = {
        let tx = tx.clone();
        move |ok: bool| {
            if let Some(t) = tx.borrow_mut().take() {
                let _ = t.send(ok);
            }
        }
    };
    let on_load = Closure::<dyn FnMut()>::new({
        let kirim = kirim.clone();
        move || kirim(true)
    });
    let on_error = Closure::<dyn FnMut()>::new(move || kirim(false));
    img.set_onload(Some(on_load.as_ref().unchecked_ref()));
    img.set_onerror(Some(on_error.as_ref().unchecked_ref()));
    img.set_src(&url);

    let termuat = rx.await.unwrap_or(false);
    img.set_onload(None);
    img.set_onerror(None);
    drop(on_load);
    drop(on_error);
    if !termuat {
        return None;
    }

    let lebar = img.natural_width() as f64;
    let tinggi = img.natural_height() as f64;
    if lebar <= 0.0 || tinggi <= 0.0 {
        return None;
    }

    let sisi_terpanjang = lebar.max(tinggi);
    // Sudah kecil DAN sudah ringan → tak ada yang bisa diperbaiki.
    if sisi_terpanjang <= MAKS_SISI && ukuran_asal <= AMBANG_BYTE {
        return None;
    }
    let skala = (MAKS_SISI / sisi_terpanjang).min(1.0);
    let w = (lebar * skala).round().max(1.0);
    let h = (tinggi * skala).round().max(1.0);

    let canvas: web_sys::HtmlCanvasElement = doc
        .create_element("canvas")
        .ok()?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .ok()?;
    canvas.set_width(w as u32);
    canvas.set_height(h as u32);
    let ctx: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .ok()?;
    ctx.draw_image_with_html_image_element_and_dw_and_dh(&img, 0.0, 0.0, w, h)
        .ok()?;

    // WebP: diterima server (`service/storage.rs` mendeteksinya lewat magic
    // bytes dan memasukkannya ke whitelist) dan jauh lebih kecil daripada JPEG
    // pada mutu setara. Peramban yang tak bisa menyandi WebP mengembalikan
    // `null` di sini — jalur itu jatuh ke berkas asli, bukan gagal.
    let (tx2, rx2) = futures::channel::oneshot::channel::<Option<web_sys::Blob>>();
    let tx2 = RefCell::new(Some(tx2));
    let on_blob = Closure::<dyn FnMut(wasm_bindgen::JsValue)>::new(move |v: wasm_bindgen::JsValue| {
        if let Some(t) = tx2.borrow_mut().take() {
            let _ = t.send(v.dyn_into::<web_sys::Blob>().ok());
        }
    });
    canvas
        .to_blob_with_type_and_encoder_options(
            on_blob.as_ref().unchecked_ref(),
            "image/webp",
            &wasm_bindgen::JsValue::from_f64(0.82),
        )
        .ok()?;
    let hasil = rx2.await.ok().flatten();
    drop(on_blob);

    let blob = hasil?;
    // Menukar hanya bila memang lebih kecil.
    if blob.size() >= ukuran_asal {
        return None;
    }
    Some(blob)
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn upload_merchant_image_with_progress(
    file: &web_sys::File,
    on_progress: impl Fn(u8) + 'static,
) -> Result<String, String> {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let form = web_sys::FormData::new().map_err(|e| format!("{:?}", e))?;

    // ── Kompres SEBELUM mengirim ────────────────────────────────────────────
    // Ini juga yang menjawab "kok lama padahal sudah 100%": bar progres mencapai
    // 100% begitu byte masuk buffer soket, sedangkan sisa waktunya dihabiskan
    // server untuk mendorong berkas itu ke storage. Memperkecil berkasnya
    // memangkas KEDUA sisi sekaligus — yang dikirim dan yang didorong — bukan
    // sekadar memindahkan tunggunya.
    //
    // `None` = pakai aslinya (GIF, berkas yang sudah kecil, atau hasil kompresi
    // yang ternyata lebih besar). Lihat `kompres_gambar`.
    let hasil = match kompres_gambar(file).await {
        Some(kecil) => form.append_with_blob_and_filename("file", &kecil, "unggahan.webp"),
        None => form.append_with_blob("file", file),
    };
    hasil.map_err(|e| format!("{:?}", e))?;

    let xhr = web_sys::XmlHttpRequest::new().map_err(|e| format!("{:?}", e))?;
    xhr.open_with_async("POST", "/upload/merchant-image", true)
        .map_err(|e| format!("{:?}", e))?;

    // ── Pelapor progres ─────────────────────────────────────────────────────
    // Dipasang pada `xhr.upload`, BUKAN pada `xhr` sendiri: peristiwa progres di
    // objek XHR menghitung respons yang TURUN, sedangkan yang ingin ditampilkan
    // adalah berkas yang NAIK.
    let upload = xhr.upload().map_err(|e| format!("{:?}", e))?;
    let cb_progress = Closure::<dyn FnMut(web_sys::ProgressEvent)>::new(
        move |e: web_sys::ProgressEvent| {
            if !e.length_computable() {
                return;
            }
            let total = e.total();
            if total <= 0.0 {
                return;
            }
            let persen = ((e.loaded() / total) * 100.0).round().clamp(0.0, 100.0);
            on_progress(persen as u8);
        },
    );
    upload.set_onprogress(Some(cb_progress.as_ref().unchecked_ref()));

    // ── Penanda selesai ─────────────────────────────────────────────────────
    // `loadend` menyala untuk SEMUA akhir — sukses, galat jaringan, maupun
    // dibatalkan. Memakai `load` saja akan menggantung selamanya ketika
    // koneksinya putus, dan halaman edit menahan tombol SIMPAN selama unggahan
    // dianggap masih berjalan — jadi satu unggahan yang mati diam-diam akan
    // mengunci seluruh form tanpa satu pun pesan.
    let (tx, rx) = futures::channel::oneshot::channel::<()>();
    let tx = std::cell::RefCell::new(Some(tx));
    let cb_selesai = Closure::<dyn FnMut()>::new(move || {
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.send(());
        }
    });
    xhr.set_onloadend(Some(cb_selesai.as_ref().unchecked_ref()));

    xhr.send_with_opt_form_data(Some(&form))
        .map_err(|e| format!("{:?}", e))?;

    let _ = rx.await;

    // Closure dilepas SESUDAH permintaan selesai. Kalau di-`forget()` seperti
    // pola yang lazim disalin, tiap unggahan meninggalkan closure yang tak
    // pernah dibebaskan — pada halaman yang mengunggah belasan foto detail,
    // kebocoran itu menumpuk sepanjang sesi.
    upload.set_onprogress(None);
    xhr.set_onloadend(None);
    drop(cb_progress);
    drop(cb_selesai);

    let status = xhr.status().map_err(|e| format!("{:?}", e))?;
    if status == 0 {
        return Err("Koneksi terputus saat mengunggah.".to_string());
    }
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }

    let teks = xhr
        .response_text()
        .map_err(|e| format!("{:?}", e))?
        .unwrap_or_default();
    let json = js_sys::JSON::parse(&teks).map_err(|_| "Jawaban server bukan JSON".to_string())?;
    js_sys::Reflect::get(&json, &wasm_bindgen::JsValue::from_str("url"))
        .ok()
        .and_then(|v| v.as_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "URL kosong dari server".to_string())
}

/// Unggah tanpa pelaporan progres — untuk pemanggil yang tak menampilkannya.
/// Jalur kodenya sama persis, jadi tak ada dua perilaku unggah yang harus dijaga.
///
/// `cfg` wasm32 SAMA dengan fungsi di atas: keduanya menyentuh API peramban dan
/// tak ada satu pun pemanggilnya di sisi server. Tanpa penjaga yang sama,
/// build SSR gagal mencari fungsi yang memang tak pernah ada di sana.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn upload_merchant_image(file: &web_sys::File) -> Result<String, String> {
    upload_merchant_image_with_progress(file, |_| {}).await
}

#[component]
fn MerchantProfileCard() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let my_id = move || {
        auth.get()
            .and_then(|r| r.ok())
            .flatten()
            .map(|u| u.id)
            .unwrap_or_default()
    };

    let profile = Resource::new(my_id, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("not_ready".into()));
        }
        // Merchant SELALU punya merchant_details (dijamin trigger migrasi 016),
        // jadi profil sendiri tak pernah NotFound → tak perlu fallback kosong.
        get_merchant_public_profile(id).await
    });
    let products = Resource::new(my_id, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("not_ready".into()));
        }
        get_merchant_public_products(id, Some(1), None, None).await
    });

    // Editor state (di-seed sekali dari profile).
    let editing = RwSignal::new(false);
    let f_name = RwSignal::new(String::new());
    let f_desc = RwSignal::new(String::new());
    let logo_url = RwSignal::new(String::new());
    let header_url = RwSignal::new(String::new());
    let logo_prev = RwSignal::new(String::new());
    let header_prev = RwSignal::new(String::new());
    let uploading = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let msg = RwSignal::new(String::new());
    let seeded = RwSignal::new(false);

    Effect::new(move |_| {
        if let Some(Ok(p)) = profile.get() {
            if !seeded.get_untracked() {
                f_name.set(p.store_name.clone());
                f_desc.set(p.description.clone().unwrap_or_default());
                logo_url.set(p.logo_url.clone().unwrap_or_default());
                header_url.set(p.header_url.clone().unwrap_or_default());
                seeded.set(true);
            }
        }
    });

    let event_cover = move || {
        products
            .get()
            .and_then(|r| r.ok())
            .and_then(|pe| pe.data.first().and_then(|e| e.cover_url.clone()))
            .filter(|c| !c.is_empty())
    };
    let city = move || {
        products
            .get()
            .and_then(|r| r.ok())
            .and_then(|pe| pe.data.first().and_then(|e| e.city.clone()))
            .filter(|c| !c.is_empty())
    };
    let hero_src = move || {
        let h = if !header_prev.get().is_empty() {
            header_prev.get()
        } else {
            header_url.get()
        };
        if !h.is_empty() {
            Some(h)
        } else {
            event_cover()
        }
    };
    let avatar_src = move || {
        let l = if !logo_prev.get().is_empty() {
            logo_prev.get()
        } else {
            logo_url.get()
        };
        (!l.is_empty()).then_some(l)
    };

    // Pilih file → preview instan + unggah → simpan URL server ke `url_sig`.
    let make_pick = move |url_sig: RwSignal<String>, prev_sig: RwSignal<String>| {
        move |ev: leptos::ev::Event| {
            #[cfg(target_arch = "wasm32")]
            {
                use wasm_bindgen::JsCast;
                let Some(input) = ev
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                else {
                    return;
                };
                let Some(file) = input.files().and_then(|f| f.get(0)) else {
                    return;
                };
                if let Ok(obj) = web_sys::Url::create_object_url_with_blob(&file) {
                    prev_sig.set(obj);
                }
                uploading.set(true);
                msg.set(String::new());
                leptos::task::spawn_local(async move {
                    match upload_merchant_image(&file).await {
                        Ok(u) => url_sig.set(u),
                        Err(e) => msg.set(format!("Upload gagal: {e}")),
                    }
                    uploading.set(false);
                });
            }
            let _ = ev;
            let _ = (url_sig, prev_sig);
        }
    };
    let on_pick_header = make_pick(header_url, header_prev);
    let on_pick_logo = make_pick(logo_url, logo_prev);

    let do_save = move |_| {
        if saving.get_untracked() {
            return;
        }
        let name = f_name.get_untracked().trim().to_string();
        if name.len() < 2 {
            msg.set("Nama bisnis minimal 2 karakter.".into());
            return;
        }
        let desc = f_desc.get_untracked();
        let logo = logo_url.get_untracked();
        let header = header_url.get_untracked();
        saving.set(true);
        msg.set(String::new());
        leptos::task::spawn_local(async move {
            match update_merchant_profile(name, desc, logo, header).await {
                Ok(()) => {
                    msg.set("Profil tersimpan!".into());
                    profile.refetch();
                    editing.set(false);
                }
                Err(e) => msg.set(format!("Gagal menyimpan: {e}")),
            }
            saving.set(false);
        });
    };

    view! {
        <section class="mhub-profile-card">
            <div class="mp-hero mhub-phero">
                {move || { hero_src().map(|src| view! { <img src=src alt="" loading="lazy" /> }) }}
                <div class="mp-hero-grad"></div>
                <button
                    class="mhub-edit-toggle"
                    on:click=move |_| editing.update(|e| *e = !*e)
                >
                    {move || if editing.get() { "Tutup" } else { "Edit Profil" }}
                </button>
            </div>

            <div class="mp-head">
                <div class="mp-avatar-wrap">
                    {move || match avatar_src() {
                        Some(src) => view! { <img class="mp-avatar" src=src alt="Logo" /> }.into_any(),
                        None => {
                            let initial = f_name
                                .get()
                                .chars()
                                .next()
                                .unwrap_or('P')
                                .to_uppercase()
                                .to_string();
                            view! { <div class="mp-avatar mp-avatar-fallback">{initial}</div> }
                                .into_any()
                        }
                    }}
                    {move || {
                        profile
                            .get()
                            .and_then(|r| r.ok())
                            .map(|p| p.verified)
                            .unwrap_or(false)
                            .then(|| {
                                view! {
                                    <span class="mp-avatar-badge" title="Terverifikasi">
                                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                            stroke="currentColor" stroke-width="3" stroke-linecap="round"
                                            stroke-linejoin="round">
                                            <polyline points="20 6 9 17 4 12" />
                                        </svg>
                                    </span>
                                }
                            })
                    }}
                </div>
            </div>

            <div class="mp-container">
                <h1 class="mp-name">{move || f_name.get()}</h1>
                {move || {
                    city()
                        .map(|c| {
                            view! {
                                <p class="mp-loc">
                                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                        stroke="currentColor" stroke-width="2" stroke-linecap="round"
                                        stroke-linejoin="round">
                                        <path d="M21 10c0 7-9 12-9 12s-9-5-9-12a9 9 0 0 1 18 0z" />
                                        <circle cx="12" cy="10" r="3" />
                                    </svg>
                                    {c}
                                </p>
                            }
                        })
                }}

                <div class="mp-stats">
                    {move || {
                        let p = profile.get().and_then(|r| r.ok());
                        let (f, e, r) = p
                            .map(|p| (p.followers, p.products_count, p.rating_avg))
                            .unwrap_or((0, 0, 0.0));
                        let followers_href = format!("/m/{}/followers", my_id());
                        view! {
                            <a class="mp-stat mp-stat-link" href=followers_href>
                                <span class="mp-stat-num">{fmt_count(f)}</span>
                                <span class="mp-stat-label">"FOLLOWERS"</span>
                            </a>
                            <div class="mp-stat">
                                <span class="mp-stat-num">{fmt_count(e)}</span>
                                <span class="mp-stat-label">"PRODUCT"</span>
                            </div>
                            <div class="mp-stat">
                                <span class="mp-stat-num">
                                    {format!("{:.1}", r)}<span class="mp-stat-star">"★"</span>
                                </span>
                                <span class="mp-stat-label">"RATING"</span>
                            </div>
                        }
                    }}
                </div>

                <Show when=move || editing.get()>
                    <div class="mhub-edit-form">
                        <label class="mhub-form-label">"HEADER"</label>
                        <label class="mhub-upload-tile">
                            <input type="file" accept="image/*" on:change=on_pick_header />
                            {move || {
                                let src = if !header_prev.get().is_empty() {
                                    header_prev.get()
                                } else {
                                    header_url.get()
                                };
                                if src.is_empty() {
                                    view! { <span class="mhub-upload-hint">"+ Unggah header"</span> }
                                        .into_any()
                                } else {
                                    view! { <img class="mhub-upload-prev" src=src alt="" /> }.into_any()
                                }
                            }}
                        </label>

                        <label class="mhub-form-label" style="margin-top:12px">"LOGO"</label>
                        <label class="mhub-upload-tile mhub-upload-tile--logo">
                            <input type="file" accept="image/*" on:change=on_pick_logo />
                            {move || {
                                let src = if !logo_prev.get().is_empty() {
                                    logo_prev.get()
                                } else {
                                    logo_url.get()
                                };
                                if src.is_empty() {
                                    view! { <span class="mhub-upload-hint">"+ Logo"</span> }.into_any()
                                } else {
                                    view! { <img class="mhub-upload-prev" src=src alt="" /> }.into_any()
                                }
                            }}
                        </label>

                        <label class="mhub-form-label" style="margin-top:12px">"NAMA BISNIS"</label>
                        <input
                            class="mhub-form-input"
                            type="text"
                            prop:value=move || f_name.get()
                            on:input=move |e| f_name.set(event_target_value(&e))
                        />

                        <label class="mhub-form-label" style="margin-top:10px">"DESKRIPSI"</label>
                        <textarea
                            class="mhub-form-input"
                            rows="4"
                            prop:value=move || f_desc.get()
                            on:input=move |e| f_desc.set(event_target_value(&e))
                        ></textarea>

                        {move || {
                            (!msg.get().is_empty())
                                .then(|| view! { <p class="mhub-form-msg">{msg.get()}</p> })
                        }}

                        <button
                            class="mhub-modal-submit"
                            style="width:100%;margin-top:12px"
                            disabled=move || saving.get() || uploading.get()
                            on:click=do_save
                        >
                            {move || {
                                if saving.get() {
                                    "Menyimpan…"
                                } else if uploading.get() {
                                    "Mengunggah…"
                                } else {
                                    "Simpan Profil"
                                }
                            }}
                        </button>
                    </div>
                </Show>
            </div>
        </section>
    }
}
