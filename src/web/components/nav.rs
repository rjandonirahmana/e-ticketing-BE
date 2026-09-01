use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::hooks::{use_auth, AuthCtx};
pub use crate::web::hooks::ThemeToggle;

/// Tab tambahan di bilah navigasi ditentukan LANGSUNG oleh `users.role`.
///
/// Sebelumnya tab merchant diputuskan oleh `membership_tier == "MERCHANT"` — dan
/// medan itu tidak datang dari server sama sekali. Ia dikarang di klien
/// (`hooks::to_profile`) dari `role` yang sama, lalu dibaca kembali seolah data
/// tersendiri. Satu peran, dua nama, dua tempat untuk salah.
///
/// Yang tersisa: satu peran, satu pembacaan.
///   * `merchant` → tab MERCHANT
///   * `admin`    → tab ADMIN
///   * selain itu → tak ada tab tambahan
///
/// Admin sengaja TIDAK mendapat tab merchant: ia tak punya toko, dan tautan ke
/// dasbor toko yang bukan miliknya hanya berakhir sebagai halaman kosong.
fn punya_peran(auth: AuthCtx, peran: &'static str) -> bool {
    auth.user.with(|u| {
        u.as_ref()
            .map(|p| p.role.eq_ignore_ascii_case(peran))
            .unwrap_or(false)
    })
}

fn is_merchant(auth: AuthCtx) -> bool {
    punya_peran(auth, "merchant")
}

fn is_admin(auth: AuthCtx) -> bool {
    punya_peran(auth, "admin")
}

#[allow(dead_code)]
#[component]
pub fn TopNav(#[prop(optional)] back_href: Option<&'static str>) -> impl IntoView {
    view! {
        <header class="page-header">
            {match back_href {
                Some(href) => {
                    view! {
                        <A href=href attr:class="back-btn">
                            <svg
                                width="22"
                                height="22"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2.5"
                                stroke-linecap="round"
                            >
                                <polyline points="15 18 9 12 15 6" />
                            </svg>
                        </A>
                    }
                        .into_any()
                }
                None => {
                    view! {
                        <button class="icon-btn" aria-label="Menu">
                            <svg
                                width="20"
                                height="20"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2"
                                stroke-linecap="round"
                            >
                                <line x1="3" y1="6" x2="21" y2="6" />
                                <line x1="3" y1="12" x2="21" y2="12" />
                                <line x1="3" y1="18" x2="21" y2="18" />
                            </svg>
                        </button>
                    }
                        .into_any()
                }
            }} <span class="page-logo">"PULSE"</span> <div class="header-actions">
                <CartButton />
                <ThemeToggle />
            </div>
        </header>
    }
}

/// Ikon keranjang beserta lencana jumlah barang.
///
/// Lencana membaca `CartSummary.cart_quantity` — SELURUH isi keranjang, bukan
/// hanya yang dicentang. Angka di ikon harus menjawab "ada berapa barang saya
/// simpan", bukan "berapa yang sedang saya bayar"; kalau ia ikut turun saat
/// pembeli melepas centang, ia terbaca seolah barangnya hilang.
#[component]
pub fn CartButton() -> impl IntoView {
    let cart = use_context::<crate::web::app::CartContext>();

    view! {
        <A href="/cart" attr:class="nav-cart" attr:aria-label="Keranjang">
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                 stroke="currentColor" stroke-width="2" stroke-linecap="round"
                 stroke-linejoin="round">
                <circle cx="9" cy="21" r="1"/>
                <circle cx="20" cy="21" r="1"/>
                <path d="M1 1h4l2.68 13.39a2 2 0 002 1.61h9.72a2 2 0 002-1.61L23 6H6"/>
            </svg>
            {move || {
                let n = cart.map(|c| c.count()).unwrap_or(0);
                (n > 0).then(|| view! {
                    <span class="nav-cart-badge">
                        {if n > 99 { "99+".to_string() } else { n.to_string() }}
                    </span>
                })
            }}
        </A>
    }
}

/// Bottom navigation bar.
/// Tabs: EXPLORE | LIVES | PULSE | ORDERS | PROFILE | (MERCHANT) | (ADMIN)
#[component]
pub fn BottomNav(#[prop(default = "")] active: &'static str) -> impl IntoView {
    let auth_ctx = use_auth();
    // Lencana pesan belum dibaca. `None` selama patokan dari server belum tiba
    // — dibedakan dari nol supaya lencananya tidak sempat berkedip dengan angka
    // yang belum tentu benar.
    let bus = crate::web::components::use_chat_bus();
    let belum_chat = move || bus.and_then(|b| b.total()).unwrap_or(0);
    let cls = move |key: &str| {
        if key == active {
            "bottom-item bottom-item--active"
        } else {
            "bottom-item"
        }
    };
    view! {
        <nav class="bottom-nav">

            // 1. EXPLORE
            <A href="/explore" attr:class=cls("explore")>
                <svg
                    width="22"
                    height="22"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <circle cx="11" cy="11" r="8" />
                    <line x1="21" y1="21" x2="16.65" y2="16.65" />
                </svg>
                <span class="bottom-label">"EXPLORE"</span>
            </A>

            // 1b. LIVES — daftar merchant yang sedang siaran langsung
            <A href="/lives" attr:class=cls("lives")>
                <svg
                    width="22"
                    height="22"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <polygon points="23 7 16 12 23 17 23 7" />
                    <rect x="1" y="5" width="15" height="14" rx="2" ry="2" />
                </svg>
                <span class="bottom-label">"LIVE"</span>
            </A>


            // 0. PULSE CHAT
            <A href="/pulse" attr:class=cls("pulse")>
                <svg
                    width="22"
                    height="22"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
                </svg>
                // Ditaruh SESUDAH svg tapi diposisikan mutlak ke ikonnya:
                // menaruhnya di dalam <svg> berarti ia ikut aturan koordinat
                // SVG, bukan CSS.
                {move || {
                    let n = belum_chat();
                    (n > 0).then(|| {
                        // Di atas 99 angkanya melebar sampai memakan label di
                        // bawahnya, dan selisih antara 100 dan 340 pesan tak
                        // mengubah apa pun yang akan dilakukan orangnya.
                        let teks = if n > 99 { "99+".to_string() } else { n.to_string() };
                        view! { <span class="bottom-badge">{teks}</span> }
                    })
                }}
                <span class="bottom-label">"CHAT"</span>
            </A>

            // 2. TICKETS
            // ORDERS — riwayat pesanan. Tiket yang sudah terbit dijangkau dari
            // detail pesanannya, jadi tab TICKETS tersendiri tak lagi diperlukan.
            <A href="/orders" attr:class=cls("orders")>
                <svg
                    width="22"
                    height="22"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <path d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2" />
                    <rect x="9" y="3" width="6" height="4" rx="1" />
                    <path d="M9 12h6M9 16h4" />
                </svg>
                <span class="bottom-label">"ORDERS"</span>
            </A>

            // 4. PROFILE
            <A href="/profile" attr:class=cls("profile")>
                <svg
                    width="22"
                    height="22"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="1.8"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                >
                    <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
                    <circle cx="12" cy="7" r="4" />
                </svg>
                <span class="bottom-label">"PROFILE"</span>
            </A>

            // ── TAB BERSYARAT DI DALAM <Transition> ─────────────────────────
            // Membaca Resource sesi HARUS terjadi di dalam batas Suspense atau
            // Transition; di luar itu Leptos memperingatkan "reading resource in
            // hydrate mode" dan hasilnya bisa berbeda antara server dan klien.
            //
            // `Transition`, bukan `Suspense`: saat sesi dimuat ulang, Suspense
            // mengosongkan isinya lebih dulu — dan di sini itu berarti dua tab
            // LENYAP lalu muncul lagi, membuat seluruh bilah navigasi bergeser
            // di bawah jari orang yang sedang mengarah ke salah satunya.
            // Transition mempertahankan yang lama sampai yang baru siap.
            //
            // `fallback=|| ()`: sebelum sesi terbaca, tak ada tab tambahan —
            // bukan kerangka. Kerangka di bilah navigasi hanya akan menggeser
            // tab yang sudah benar posisinya.
            <Transition fallback=|| ()>
            // 5. MERCHANT — only if user role is merchant
            {move || {
                is_merchant(auth_ctx)
                    .then(|| {
                        view! {
                            <A href="/merchant" attr:class=cls("merchant")>
                                <svg
                                    width="22"
                                    height="22"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="1.8"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                >
                                    <path d="M3 7l1.5-3h15L21 7" />
                                    <path d="M3 7v13a1 1 0 001 1h16a1 1 0 001-1V7" />
                                    <path d="M9 11h6" />
                                </svg>
                                <span class="bottom-label">"MERCHANT"</span>
                            </A>
                        }
                    })
            }}

            // ADMIN — only if user role is admin
            {move || {
                is_admin(auth_ctx)
                    .then(|| {
                        view! {
                            <A href="/admin" attr:class=cls("admin")>
                                <svg
                                    width="22"
                                    height="22"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="1.8"
                                    stroke-linecap="round"
                                    stroke-linejoin="round"
                                >
                                    <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
                                </svg>
                                <span class="bottom-label">"ADMIN"</span>
                            </A>
                        }
                    })
            }}
            </Transition>
        </nav>
    }
}
