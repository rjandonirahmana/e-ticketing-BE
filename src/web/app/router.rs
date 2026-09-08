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

use crate::web::components::{GridBackground, KabarChat, NavProgress, ToastHost};
use crate::web::pages::*;

use super::guards::{AdminGuard, AuthGuard, MerchantGuard};
use super::providers::provide_all_app_contexts;

/// Posisi gulir per halaman — disimpan terus, dipulihkan hanya saat MUNDUR.
///
/// ── KENAPA INI ADA ─────────────────────────────────────────────────────────
/// Sebelumnya di sini duduk `ScrollToTop` yang melompat ke posisi 0 pada SETIAP
/// pergantian path, tanpa membedakan arah. Untuk navigasi maju itu benar:
/// halaman baru harus dibaca dari atas. Untuk MUNDUR itu menghapus pekerjaan
/// orang.
///
/// Yang paling terasa di `/explore`, yang memuat isinya dengan gulir tak
/// berujung: gulir tiga puluh kartu ke bawah, buka satu produk, tekan kembali —
/// dan feed-nya ada di puncak lagi. Kartu yang tadi dilihat masih termuat di
/// memori, hanya posisinya yang hilang; orang harus menggulir ulang melewati
/// semua yang sudah dilewati untuk sampai ke tempat yang tadi. Itu satu-satunya
/// alasan terbesar orang berhenti menelusuri.
#[cfg(target_arch = "wasm32")]
mod gulir {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    thread_local! {
        /// path → posisi gulir terakhir. `thread_local` karena WASM di peramban
        /// berjalan pada satu utas; tak ada yang perlu dikunci.
        static POSISI: RefCell<HashMap<String, f64>> = RefCell::new(HashMap::new());
        /// Penanda agar perekaman menumpang satu bingkai, bukan tiap peristiwa
        /// gulir. Peristiwa gulir datang puluhan kali per detik dan yang kita
        /// butuhkan cuma nilai terakhirnya.
        static TERTUNDA: Cell<bool> = const { Cell::new(false) };
        static TERPASANG: Cell<bool> = const { Cell::new(false) };
    }

    fn path_sekarang() -> Option<String> {
        web_sys::window()?.location().pathname().ok()
    }

    fn rekam() {
        let (Some(win), Some(path)) = (web_sys::window(), path_sekarang()) else {
            return;
        };
        let y = win.scroll_y().unwrap_or(0.0);
        POSISI.with(|p| {
            let mut p = p.borrow_mut();
            // Pagar memori: sesi panjang di aplikasi dengan banyak halaman tak
            // boleh menumbuhkan peta ini tanpa batas. 100 entri jauh melampaui
            // kedalaman riwayat yang bisa ditelusuri seseorang dalam satu sesi;
            // saat penuh, yang paling tak berguna untuk dibuang adalah entri
            // berposisi 0 (halaman yang tak pernah digulir).
            if p.len() >= 100 && !p.contains_key(&path) {
                p.retain(|_, v| *v > 0.0);
                if p.len() >= 100 {
                    p.clear();
                }
            }
            p.insert(path, y);
        });
    }

    /// Pasang perekam sekali seumur sesi.
    pub fn pasang_perekam() {
        if TERPASANG.with(|t| t.replace(true)) {
            return;
        }
        let Some(win) = web_sys::window() else { return };

        // Matikan pemulihan gulir bawaan peramban. Kalau dibiarkan, ia dan kode
        // di bawah ini sama-sama menyetel posisi pada bingkai yang berdekatan,
        // dan yang terlihat adalah layar yang melompat dua kali.
        if let Ok(history) = win.history() {
            let _ = history.set_scroll_restoration(web_sys::ScrollRestoration::Manual);
        }

        let cb = wasm_bindgen::closure::Closure::<dyn Fn()>::new(move || {
            if TERTUNDA.with(|t| t.replace(true)) {
                return;
            }
            leptos::prelude::request_animation_frame(move || {
                TERTUNDA.with(|t| t.set(false));
                rekam();
            });
        });
        let _ = win.add_event_listener_with_callback_and_bool(
            "scroll",
            wasm_bindgen::JsCast::unchecked_ref(cb.as_ref()),
            false,
        );
        // Sengaja `forget`: pendengar ini hidup selama sesi, dipasang tepat
        // sekali dari akar aplikasi. Ia BUKAN kebocoran per-navigasi — tak ada
        // pasangan `on_cleanup` karena tak ada yang pernah membongkarnya.
        cb.forget();
    }

    /// Matikan/hidupkan `scroll-behavior: smooth` untuk gulir programatik.
    ///
    /// ── KENAPA INI PERLU ───────────────────────────────────────────────────
    /// `01-base.css` menyetel `html { scroll-behavior: smooth }` untuk SELURUH
    /// dokumen. Itu benar untuk tautan jangkar, tapi ia juga berlaku pada
    /// setiap `window.scrollTo` — termasuk yang dipanggil di sini.
    ///
    /// Dua akibatnya berbeda dan keduanya buruk. Untuk reset ke atas: halaman
    /// baru tidak muncul dari atas, melainkan MELUNCUR ke atas setelah muncul,
    /// dan bertabrakan dengan transisi geser yang sedang berjalan. Untuk
    /// pemulihan: `pulihkan` memanggil `scrollTo` tiap bingkai, dan tiap
    /// panggilan MEMULAI ULANG animasi halus dari posisi saat itu — sasarannya
    /// tak pernah tercapai, layar hanya merayap.
    ///
    /// Kelasnya dipasang tepat selama gulir programatik berlangsung, jadi
    /// tautan jangkar di dalam halaman tetap halus seperti semula.
    fn gulir_instan(aktif: bool) {
        let Some(el) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.document_element())
        else {
            return;
        };
        let daftar = el.class_list();
        let _ = if aktif {
            daftar.add_1("gulir-instan")
        } else {
            daftar.remove_1("gulir-instan")
        };
    }

    pub fn ke_atas() {
        gulir_instan(true);
        if let Some(win) = web_sys::window() {
            win.scroll_to_with_x_and_y(0.0, 0.0);
        }
        leptos::prelude::request_animation_frame(|| gulir_instan(false));
    }

    pub fn tersimpan(path: &str) -> Option<f64> {
        POSISI.with(|p| p.borrow().get(path).copied()).filter(|y| *y > 0.0)
    }

    /// Pulihkan ke `target`, menunggu isinya cukup tinggi lebih dulu.
    ///
    /// Ini bagian yang tak bisa dikerjakan sekali jalan. Saat efek navigasi
    /// berjalan, halaman tujuan kerap masih kerangka shimmer setinggi satu
    /// layar — menyetel gulir ke 4000 px di dokumen setinggi 800 px tidak
    /// menghasilkan apa-apa, peramban menjepitnya ke bawah. Jadi kita mencoba
    /// tiap bingkai sampai dokumennya benar-benar sanggup menampung posisi itu.
    ///
    /// Batas 40 bingkai (±650 ms) disengaja. Kalau isinya tak kunjung setinggi
    /// itu, berarti halamannya memang berubah — hasil pencarian menyusut, item
    /// terhapus — dan memaksa terus hanya akan mengunci layar pada percobaan
    /// yang tak akan pernah berhasil. Berhenti di posisi terjauh yang bisa
    /// dicapai lebih baik daripada itu.
    pub fn pulihkan(target: f64) {
        fn coba(target: f64, sisa: u32) {
            let Some(win) = web_sys::window() else {
                gulir_instan(false);
                return;
            };
            let tinggi_isi = win
                .document()
                .and_then(|d| d.document_element())
                .map(|e| e.scroll_height() as f64)
                .unwrap_or(0.0);
            let tinggi_layar = win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
            let maks = (tinggi_isi - tinggi_layar).max(0.0);

            win.scroll_to_with_x_and_y(0.0, target.min(maks));

            if maks + 1.0 < target && sisa > 0 {
                leptos::prelude::request_animation_frame(move || coba(target, sisa - 1));
            } else {
                // Selesai — entah karena sasarannya tercapai atau karena
                // isinya memang tak akan setinggi itu. Kembalikan gulir halus
                // supaya tautan jangkar di dalam halaman berperilaku normal.
                gulir_instan(false);
            }
        }
        gulir_instan(true);
        coba(target, 40);
    }
}

/// Atur posisi gulir tiap kali route berganti: MAJU ke atas, MUNDUR ke tempat
/// semula. Lihat catatan panjang di modul `gulir`.
#[component]
fn PerilakuGulir() -> impl IntoView {
    let location = use_location();
    let pathname = location.pathname;

    Effect::new(move |prev: Option<String>| {
        let current = pathname.get();
        let berpindah = prev.as_ref().map(|p| p != &current).unwrap_or(false);

        #[cfg(target_arch = "wasm32")]
        {
            gulir::pasang_perekam();

            if berpindah {
                // Arah navigasi datang dari router, bukan ditebak di sini.
                // `leptos_router` melacaknya lewat tumpukan path-nya sendiri
                // dan menyetelnya JUGA pada `popstate` — jadi gesture
                // geser-kembali di ponsel dan tombol kembali peramban ikut
                // terbaca sebagai mundur, bukan hanya tombol kembali di dalam
                // aplikasi.
                use leptos_router::location::{BrowserUrl, LocationProvider};
                let mundur = use_context::<BrowserUrl>()
                    .map(|nav| nav.is_back().get_untracked())
                    .unwrap_or(false);

                match (mundur, gulir::tersimpan(&current)) {
                    (true, Some(y)) => gulir::pulihkan(y),
                    _ => gulir::ke_atas(),
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = berpindah;

        current
    });
    view! {}
}

/// Komponen root PULSE — universal untuk SSR dan hydration.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    // Dipakai <Router set_is_routing> di bawah + <NavProgress>.
    let (sedang_pindah, set_sedang_pindah) = signal(false);

    // Semua context disediakan di sini — berjalan di SSR maupun setelah hydration.
    provide_all_app_contexts();

    view! {
        <Title text="PULSE — Marketplace" />
        <Meta name="description" content="Marketplace terbaik di Indonesia." />

        // `set_is_routing` dinyalakan router sebelum memuat rute dan dimatikan
        // setelah selesai — itulah satu-satunya sumber yang benar-benar tahu
        // kapan sebuah perpindahan sedang menunggu data, jadi garis progres di
        // bawah tak perlu menebak-nebak sendiri.
        <Router set_is_routing=set_sedang_pindah>
            <PerilakuGulir />
            <NavProgress sedang_pindah=sedang_pindah />
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

                    // `transition=true` menyalakan View Transitions API bawaan
                    // leptos_router: ia memasang `.router-outlet-0` di <html>
                    // selama perpindahan dan `.router-back` bila arahnya mundur.
                    // Animasinya sendiri ada di `styles/parts/58-page-transition.css`;
                    // di peramban tanpa dukungan, router memanggil rebuild-nya
                    // langsung dan tak ada yang berubah dari perilaku lama.
                    <FlatRoutes transition=true fallback=|| view! { <NotFoundPage /> }>

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
