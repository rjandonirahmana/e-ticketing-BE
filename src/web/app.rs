//! web/app.rs — Root App universal (SSR + Hydration).
//!
//! Arsitektur Unified SSR + Hydration:
//! - `shell()` dipanggil Axum untuk setiap request; menghasilkan HTML penuh
//! - `App` component di-render server-side SEKALIGUS dipakai sebagai target hydration
//! - Satu App = zero DOM mismatch = no FOUC dari re-render
//!
//! Kenapa unified?
//!   Sebelumnya ada dua App: web::app::App (SSR) dan csr::app::App (CSR).
//!   Leptos detect struct DOM berbeda → clear body + fresh mount → FOUC.
//!   Dengan satu App yang sama, hydrate_body() berjalan mulus tanpa mismatch.
//!
//! Auth flow:
//!   SSR  : get_session() membaca HttpOnly cookie → blocking (render tunggu auth)
//!   Client: get_session() dipanggil sebagai server fn HTTP call → async via Suspense
//!
//! Context providers:
//!   Semua context yang dibutuhkan CSR init_app_providers() juga di-provide di sini
//!   melalui `provide_all_app_contexts()`. Dua fungsi harus selalu sinkron.

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{FlatRoutes, Redirect, Route, Router},
    path,
};

use crate::web::api::get_session;
use crate::web::models::{CartItem, OrderRef, UserResponse};
use crate::web::pages::*;

// ── Context types ─────────────────────────────────────────────────────────────

pub type AuthResource = Resource<Result<Option<UserResponse>, ServerFnError>>;

#[derive(Clone, Debug, Default)]
pub struct SuccessSnapshot {
    pub order_code: String,
    pub event_name: String,
    pub total_amount: i64,
}

#[derive(Clone, Copy)]
pub struct CartContext {
    pub items: RwSignal<Vec<CartItem>>,
}

/// SSR-specific PendingOrderCtx (lebih lengkap dari CSR versi order_created.rs).
/// CSR order_created.rs punya PendingOrderCtx sendiri — keduanya di-provide karena
/// komponen berbeda menggunakan tipe berbeda.
#[derive(Clone, Copy)]
pub struct PendingOrderCtx {
    pub pending_order: RwSignal<Option<OrderRef>>,
    pub success_order: RwSignal<Option<SuccessSnapshot>>,
}

// ── Shell ─────────────────────────────────────────────────────────────────────

/// Shell HTML — dipanggil Axum untuk setiap SSR request.
///
/// Fix FOUC yang diterapkan:
/// 1. `data-theme="dark"` pada `<html>` sebagai SSR default.
/// 2. Inline blocking `<script>` — baca localStorage sebelum CSS di-parse,
///    override data-theme ke "light" jika user memilihnya.
/// 3. Inline ALL CSS langsung ke `<head>` via include_str! — tidak ada
///    HTTP round-trip untuk CSS, tidak pernah 404, tidak perlu assets router.
///    Browser tetap bisa cache via ETag pada full-page response.
#[cfg(feature = "ssr")]
pub fn shell(options: leptos::config::LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="id" data-theme="dark">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <meta name="theme-color" content="#050814" />

                // ── Fix FOUC #1: Inline theme script ────────────────────────
                // Synchronous/blocking — eksekusi sebelum CSS apapun di-parse.
                <script inner_html=r#"(function(){try{var t=localStorage.getItem('kinetic.theme');if(t==='light'||t==='dark'){document.documentElement.setAttribute('data-theme',t);}}catch(e){}})();"# />

                // ── Fix FOUC #2: Semua CSS di-inline ────────────────────────
                // Tidak ada <link> = tidak ada round-trip = tidak ada 404.
                // Path relatif dari src/web/app.rs ke styles/ di root proyek.
                <style inner_html=include_str!("../../styles/tokens.css") />
                <style inner_html=include_str!("../../styles/base.css") />
                <style inner_html=include_str!("../../styles/components.css") />
                <style inner_html=include_str!("../../styles/page-home.css") />
                <style inner_html=include_str!("../../styles/page-explore.css") />
                <style inner_html=include_str!("../../styles/page-event-detail.css") />
                <style inner_html=include_str!("../../styles/page-auth.css") />
                <style inner_html=include_str!("../../styles/page-tickets.css") />
                <style inner_html=include_str!("../../styles/page-ticket-detail.css") />
                <style inner_html=include_str!("../../styles/page-orders.css") />
                <style inner_html=include_str!("../../styles/page-order-detail.css") />
                <style inner_html=include_str!("../../styles/page-order-tickets.css") />
                <style inner_html=include_str!("../../styles/page-profile.css") />
                <style inner_html=include_str!("../../styles/page-merchant.css") />
                <style inner_html=include_str!("../../styles/page-merchant-event.css") />
                <style inner_html=include_str!("../../styles/page-merchant-landing.css") />
                <style inner_html=include_str!("../../styles/page-admin.css") />
                <style inner_html=include_str!("../../styles/page-messages.css") />
                <style inner_html=include_str!("../../styles/page-notifications.css") />
                <style inner_html=include_str!("../../styles/notifications.css") />
                <style inner_html=include_str!("../../styles/page-cart.css") />
                <style inner_html=include_str!("../../styles/page-scan.css") />
                <style inner_html=include_str!("../../styles/page-story.css") />
                <style inner_html=include_str!("../../styles/page-misc.css") />
                <style inner_html=include_str!("../../styles/subscription.css") />
                <style inner_html=include_str!("../../styles/profile_premium.css") />
                <style inner_html=include_str!("../../styles/message_stories.css") />

                // ── Fonts ────────────────────────────────────────────────────
                <link rel="preconnect" href="https://fonts.googleapis.com" />
                <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="" />
                <link
                    href="https://fonts.googleapis.com/css2?family=Bebas+Neue&family=Space+Mono:ital,wght@0,400;0,700;1,400&display=swap"
                    rel="stylesheet"
                />

                // ── Leptos infrastructure ────────────────────────────────────
                <AutoReload options=options.clone() />
                <HydrationScripts options=options.clone() />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

// ── Auth Guard ────────────────────────────────────────────────────────────────

/// Guard halaman yang membutuhkan login.
///
/// Pada SSR: AuthResource sudah resolved (new_blocking) → langsung redirect
///   atau render children. Tidak ada spinner saat SSR karena data tersedia.
///
/// Pada client setelah hydration: resource di-refetch via server function.
///   Selama fetch → spinner. Setelah fetch → redirect atau children.
#[component]
fn AuthGuard(children: ChildrenFn) -> impl IntoView {
    let auth = use_context::<AuthResource>()
        .expect("AuthResource tidak di-provide — pastikan AuthGuard dipakai di dalam App");
    let children = StoredValue::new(children);

    view! {
        <Suspense fallback=move || {
            view! {
                <div class="auth-guard-loading">
                    <div class="auth-guard-spinner"></div>
                </div>
            }
        }>
            {move || {
                auth.get()
                    .map(|result| {
                        match result {
                            Ok(Some(_user)) => children.with_value(|c| c()).into_any(),
                            _ => {
                                view! { <Redirect path="/login" /> }.into_any()
                            }
                        }
                    })
            }}
        </Suspense>
    }
}

// ── Unified context provider ───────────────────────────────────────────────────

/// Sediakan SEMUA context yang dibutuhkan App, baik di SSR maupun client.
///
/// Fungsi ini harus sinkron dengan CSR `init_app_providers()` di `csr/app.rs`.
/// Setiap kali CSR menambah provider baru, tambahkan juga di sini.
///
/// Perbedaan SSR vs CSR:
///   - Auth   : SSR pakai `Resource::new_blocking` (tunggu cookie); CSR pakai server fn async.
///   - Cart   : SSR pakai `CartContext` kosong (tidak perlu fetch); CSR sama.
///   - Stores : SSR panggil provide_*_store() yang sudah di-guard `if is_server()` di load().
///   - Theme  : Sama persis — `provide_theme()` aman di SSR (web_sys::window() → None).
///   - Premium: Sama persis — `provide_premium_store()` tidak ada spawn_local di provide.
///   - PaySuc : Sama persis — hanya RwSignal kosong.
#[cfg(feature = "ssr")]
fn provide_all_app_contexts() {
    // ── Auth (SSR: blocking; client: async server fn) ──────────────────────
    let auth: AuthResource = Resource::new_blocking(|| (), |_| get_session());
    provide_context(auth);

    // ── UI / theming ────────────────────────────────────────────────────────
    // provide_theme() aman di SSR: web_sys::window() → None, Effect::new aman.
    crate::csr::hooks::provide_theme();

    // ── Cart (ephemeral, tidak perlu fetch) ─────────────────────────────────
    provide_context(CartContext {
        items: RwSignal::new(vec![]),
    });

    // ── Web-specific PendingOrderCtx (dengan success_order) ─────────────────
    provide_context(PendingOrderCtx {
        pending_order: RwSignal::new(None),
        success_order: RwSignal::new(None),
    });

    // ── CSR PendingOrderCtx dari order_created.rs ───────────────────────────
    // Tipe berbeda dari web PendingOrderCtx — komponen berbeda menggunakannya.
    crate::csr::pages::order_created::provide_pending_order();

    // ── Payment success snapshot ────────────────────────────────────────────
    crate::csr::pages::payment_success::provide_payment_success();

    // ── Data stores (web) ───────────────────────────────────────────────────
    // Setiap store sudah di-guard `if is_server() { return; }` di load().
    crate::web::state::provide_all_stores();

    // ── Premium subscription status ─────────────────────────────────────────
    // provide_premium_store() hanya setup signal — tidak ada spawn_local.
    crate::csr::state::premium::provide_premium_store();
}

// ── Root App Component ────────────────────────────────────────────────────────

/// Komponen root PULSE — universal untuk SSR dan hydration.
///
/// Dipakai SAMA untuk:
///   - Server: `shell()` render `<App/>` → HTML lengkap dikirim ke browser
///   - Client: `hydrate_body(App)` → Leptos attach ke SSR DOM (true hydration)
///
/// Tidak ada clear body + fresh mount → tidak ada DOM mismatch → tidak ada FOUC.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    // Semua context disediakan di sini — satu tempat, tidak ada yang terlewat.
    #[cfg(feature = "ssr")]
    provide_all_app_contexts();

    view! {
        <Title text="PULSE — Tiket Event Digital" />
        <Meta name="description" content="Platform tiket event terbaik di Indonesia." />

        <Router>
            <main>
                <FlatRoutes fallback=|| view! { <NotFoundPage /> }>

                    // ── PUBLIC — SSR full content (SEO) ──────────────────────
                    <Route path=path!("/") view=HomePage />
                    <Route path=path!("/explore") view=ExplorePage />
                    <Route path=path!("/events/:slug") view=EventDetailPage />
                    <Route path=path!("/merchant/landing") view=PulseLandingPage />

                    // ── AUTH ─────────────────────────────────────────────────
                    <Route path=path!("/login") view=LoginPage />
                    <Route path=path!("/register") view=RegisterPage />
                    <Route path=path!("/verify-otp") view=VerifyOtpPage />
                    <Route path=path!("/forgot-password") view=ForgotPasswordPage />

                    // ── PRIVATE — AuthGuard memastikan user sudah login ───────
                    <Route
                        path=path!("/tickets")
                        view=|| view! { <AuthGuard><TicketsPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/tickets/:id")
                        view=|| view! { <AuthGuard><TicketDetailPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/profile")
                        view=|| view! { <AuthGuard><ProfilePage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/subscription")
                        view=|| view! { <AuthGuard><SubscriptionPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/story")
                        view=|| view! { <AuthGuard><StoryPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/cart")
                        view=|| view! { <AuthGuard><CartPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/checkout")
                        view=|| view! { <AuthGuard><CheckoutPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/order-created")
                        view=|| view! { <AuthGuard><OrderCreatedPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/payment-success")
                        view=|| view! { <AuthGuard><PaymentSuccessPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/orders")
                        view=|| view! { <AuthGuard><OrdersPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/orders/:id")
                        view=|| view! { <AuthGuard><OrderDetailPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/orders/:id/tickets")
                        view=|| view! { <AuthGuard><OrderTicketsPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/notifications")
                        view=|| view! { <AuthGuard><NotificationsPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/notifications/:id")
                        view=|| view! { <AuthGuard><NotificationDetailPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/messages")
                        view=|| view! { <AuthGuard><MessagesPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/messages/:id")
                        view=|| view! { <AuthGuard><ChatRoomPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/events/:slug/location")
                        view=|| view! { <AuthGuard><VenueLocationPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/scan")
                        view=|| view! { <AuthGuard><ScanPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/merchant")
                        view=|| view! { <AuthGuard><MerchantPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/merchant/events/create")
                        view=|| view! { <AuthGuard><MerchantCreateEventPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/merchant/events/:slug/edit")
                        view=|| view! { <AuthGuard><MerchantEditEventPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/admin")
                        view=|| view! { <AuthGuard><AdminPage /></AuthGuard> }
                    />

                </FlatRoutes>
            </main>
        </Router>
    }
}
