//! web/app/router.rs — Root `App` component + route table + ScrollToTop.
//!
//! `App` universal untuk SSR dan hydration:
//!   - Server: `shell()` render `<App/>` → HTML lengkap dikirim ke browser
//!   - Client: `hydrate_body(App)` → Leptos attach ke SSR DOM (true hydration)
//! Satu App = zero DOM mismatch = no FOUC.

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{FlatRoutes, Route, Router},
    hooks::use_location,
    path,
};

use crate::web::components::{GridBackground, KabarChat, ToastHost};
use crate::web::pages::*;

use super::guards::{AdminGuard, AuthGuard, MerchantGuard};
use super::providers::provide_all_app_contexts;

/// Scroll ke atas saat navigasi antar-route.
#[component]
fn ScrollToTop() -> impl IntoView {
    let location = use_location();
    let pathname = location.pathname;
    Effect::new(move |prev: Option<String>| {
        let current = pathname.get();
        if prev.as_ref().map(|p| p != &current).unwrap_or(false) {
            #[cfg(target_arch = "wasm32")]
            if let Some(win) = web_sys::window() {
                win.scroll_to_with_x_and_y(0.0, 0.0);
            }
        }
        current
    });
    view! {}
}

/// Komponen root PULSE — universal untuk SSR dan hydration.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    // Semua context disediakan di sini — berjalan di SSR maupun setelah hydration.
    provide_all_app_contexts();

    view! {
        <Title text="PULSE — Marketplace" />
        <Meta name="description" content="Marketplace terbaik di Indonesia." />

        <Router>
            <ScrollToTop />
            // Toast host global — dirender sekali, tampung notifikasi UI dari
            // seluruh app (checkout/order, pesan masuk, dll). Di dalam <Router>
            // agar klik toast bisa navigasi (use_navigate).
            <ToastHost />
            // Kabar pesan masuk di halaman mana pun. Tak membuka koneksi
            // sendiri — menumpang WebSocket tunggal milik bus di root.
            <KabarChat />
            // Latar grid global di belakang SEMUA halaman (fixed, z-index 0).
            // Kolom halaman OPAQUE + ≤480px terpusat → grid hanya terlihat di
            // gutter kiri/kanan pada layar lebar (persis /lives & /login).
            <GridBackground />

            // ── KOLOM MOBILE TUNGGAL ─────────────────────────────────────────
            // Satu tampilan untuk semua ukuran layar: lebar dikunci 480px dan
            // dipusatkan, jadi membuka dari laptop menghasilkan susunan yang
            // persis sama dengan di ponsel.
            //
            // Kolomnya OPAQUE (`bg-page`) di atas `GridBackground` yang fixed —
            // itulah yang membuat grid hanya terlihat di gutter kiri-kanan pada
            // layar lebar, sesuai rancangan semula. Tanpa pembatas ini, isi
            // halaman melar memenuhi layar sementara bilah melayang
            // (bottom-nav, bilah bayar) tetap 480px — dua lebar yang berbeda
            // pada halaman yang sama.
            //
            // `relative z-10` wajib: tanpanya kolom ini berada di bawah latar
            // grid dan seluruh isinya tak bisa diklik.
            //
            // Elemen `position: fixed` di dalam halaman TIDAK terpengaruh
            // pembatas ini — mereka mengacu ke viewport, dan masing-masing
            // sudah memusatkan diri dengan lebar maksimum yang sama.
            <main class="relative z-10 w-full max-w-[480px] mx-auto min-h-screen bg-page \
                         shadow-[0_0_60px_rgba(0,0,0,0.35)]">
                // ── ERRORBOUNDARY DICABUT DARI SEKELILING <FlatRoutes> ──────
                //
                // Dulu seluruh tabel rute dibungkus `<ErrorBoundary>`. Itu
                // tampak seperti jaring pengaman, padahal justru satu-satunya
                // cara membuat navigasi rusak PERMANEN sampai halaman dimuat
                // ulang — dan bentuk kerusakannya persis yang dilaporkan:
                // "diklik tak pindah halaman, di-refresh baru bisa".
                //
                // Mekanismenya: begitu ada SATU galat terdaftar di dalamnya,
                // ErrorBoundary menukar anak-anaknya dengan fallback. Penukaran
                // itu MEMBUANG subtree `FlatRoutes` beserta owner reaktifnya.
                // Sesudah itu:
                //
                //   * pendengar klik router masih terpasang di window, jadi
                //     tautan tetap disadap dan `current_url` tetap berubah —
                //     alamat di bilah URL ikut berganti;
                //   * tetapi efek yang memanggil `rebuild()` sudah ikut dibuang,
                //     jadi DOM tak pernah menyusul.
                //
                // URL berpindah, layar tidak, dan tak ada satu pun pesan galat.
                // Hanya muat ulang yang memulihkan, karena itu membangun
                // seluruh pohon dari nol.
                //
                // Yang hilang dengan mencabutnya kecil: setiap halaman sudah
                // menangani galatnya sendiri lewat `match` atas `Result`
                // resource-nya, jadi boundary ini nyaris tak pernah menampilkan
                // apa pun — ia hanya menunggu untuk merusak router.
                //
                // Bila kelak ingin jaring pengaman lagi, tempatnya DI DALAM
                // view tiap rute, bukan di sekelilingnya: galat satu halaman
                // tak boleh sanggup mematikan router seluruh aplikasi.

                    <FlatRoutes fallback=|| view! { <NotFoundPage /> }>

                        // ── PUBLIC — SSR full content (SEO) ──────────────────────
                        <Route path=path!("/") view=ExplorePage />
                        <Route path=path!("/explore") view=ExplorePage />
                        <Route path=path!("/lives") view=LivesPage />
                        <Route path=path!("/meet/:id") view=MeetPage />
                        <Route path=path!("/products/:slug") view=ProductDetailPage />
                        <Route path=path!("/m/:id") view=MerchantPublicPage />
                        <Route path=path!("/m/:id/reviews") view=MerchantReviewsPage />
                        <Route path=path!("/m/:id/followers") view=MerchantFollowersPage />
                        <Route path=path!("/u/:id") view=UserPublicPage />
                        // Arsip publik semua story (View All di Explore).
                        <Route path=path!("/stories") view=StoriesArchivePage />
                        <Route path=path!("/pulse-landing") view=PulseLandingPage />
                        <Route path=path!("/pulse-apply") view=PulseApplyPage />

                        // ── AUTH ─────────────────────────────────────────────────
                        <Route path=path!("/login") view=LoginPage />
                        <Route path=path!("/register") view=RegisterPage />
                        <Route path=path!("/verify-otp") view=VerifyOtpPage />
                        <Route path=path!("/forgot-password") view=ForgotPasswordPage />

                        // ── PRIVATE — hanya user yang sudah login ─────────────────
                        <Route
                            path=path!("/tickets")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <TicketsPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/tickets/:id")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <TicketDetailPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/profile/edit")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <EditProfilePage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/profile")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <ProfilePage />
                                    </AuthGuard>
                                }
                            }
                        />
                        // Daftar toko yang diikuti — data pribadi, jadi di balik
                        // AuthGuard seperti /profile. Server function-nya juga
                        // menolak anonim, jadi guard ini murni soal pengalaman:
                        // yang belum masuk diarahkan ke /login, bukan disuguhi
                        // halaman kosong yang tak pernah bisa terisi.
                        <Route
                            path=path!("/following")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <FollowingPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/subscription")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <SubscriptionPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/subscription/checkout")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <SubscriptionCheckoutPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/story")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <StoryPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/cart")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <CartPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/checkout")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <CheckoutPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/order-created")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <OrderCreatedPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/payment-success")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <PaymentSuccessPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/orders")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <OrdersPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/orders/:id")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <OrderDetailPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/orders/:id/tickets")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <OrderTicketsPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/notifications")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <NotificationsPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/notifications/:id")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <NotificationDetailPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/pulse")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <MessagesPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        // WAJIB di ATAS `/pulse/:id`. FlatRoutes mencocokkan
                        // berurutan, dan `:id` akan menelan "toko" sebagai
                        // sebuah room id — halaman chat lalu mencari room
                        // bernama "toko" yang tak pernah ada.
                        <Route
                            path=path!("/pulse/toko/:merchant_id")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <ChatNewPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/pulse/:id")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <ChatRoomPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/products/:slug/location")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <VenueLocationPage />
                                    </AuthGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/scan")
                            view=|| {
                                view! {
                                    <AuthGuard>
                                        <ScanPage />
                                    </AuthGuard>
                                }
                            }
                        />

                        // ── MERCHANT — hanya merchant & admin ─────────────────────
                        <Route
                            path=path!("/merchant")
                            view=|| {
                                view! {
                                    <MerchantGuard>
                                        <MerchantPage />
                                    </MerchantGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/merchant/live")
                            view=|| {
                                view! {
                                    <MerchantGuard>
                                        <MerchantLivePage />
                                    </MerchantGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/merchant/products/create")
                            view=|| {
                                view! {
                                    <MerchantGuard>
                                        <MerchantCreateProductPage />
                                    </MerchantGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/merchant/products/:slug/edit")
                            view=|| {
                                view! {
                                    <MerchantGuard>
                                        <MerchantEditProductPage />
                                    </MerchantGuard>
                                }
                            }
                        />

                        // ── ADMIN — hanya admin ───────────────────────────────────
                        <Route
                            path=path!("/admin")
                            view=|| {
                                view! {
                                    <AdminGuard>
                                        <AdminPage />
                                    </AdminGuard>
                                }
                            }
                        />
                        // Sunting produk milik SIAPA PUN, dari panel admin.
                        //
                        // Rutenya sempat tidak ada. Daftar produk di panel admin
                        // sudah lama menautkan ke `/admin/products/{slug}/edit`,
                        // tetapi tak ada `<Route>` yang cocok — jadi setiap klik
                        // `Sunting Produk` di sana mendarat di halaman tidak
                        // ditemukan. Kemampuannya sendiri sudah lengkap sejak
                        // dulu: `get_merchant_product_detail` menerima admin
                        // lewat `get_for_merchant(.., is_admin)`, dan
                        // `update_merchant_product` memakai `require_roles`
                        // (merchant + admin) lalu menyimpan atas nama pemilik
                        // aslinya. Yang hilang hanya pintunya.
                        //
                        // Halaman yang dipakai sengaja SAMA dengan jalur
                        // merchant, bukan salinan khusus admin: satu formulir
                        // berarti satu perilaku simpan, satu validasi, dan satu
                        // tempat yang perlu diperbaiki bila ada yang salah.
                        <Route
                            path=path!("/admin/products/:slug/edit")
                            view=|| {
                                view! {
                                    <AdminGuard>
                                        <MerchantEditProductPage />
                                    </AdminGuard>
                                }
                            }
                        />

                    </FlatRoutes>
            </main>
        </Router>
    }
}
