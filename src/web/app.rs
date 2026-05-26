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

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{FlatRoutes, Redirect, Route, Router},
    path,
};

use crate::web::api::get_session;
use crate::web::components::Navbar;
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
///    override data-theme ke "light" jika user memilihnya. Tanpa ini: dark flash.
/// 3. Inline `tokens.css` + `base.css` langsung di `<head>` — tidak ada
///    network round-trip, CSS tersedia sebelum first paint.
/// 4. File CSS lain di-link eksternal (bisa async/cached oleh browser).
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
                // Tidak pakai defer/async agar tidak ada theme flash.
                <script inner_html=r#"(function(){try{var t=localStorage.getItem('kinetic.theme');if(t==='light'||t==='dark'){document.documentElement.setAttribute('data-theme',t);}}catch(e){}})();"# />

                // ── Fix FOUC #2: Inline critical CSS ────────────────────────
                // tokens.css & base.css di-inline agar tersedia sebelum first paint.
                // Tanpa ini, browser perlu round-trip fetch sebelum bisa styling HTML.
                <style inner_html=include_str!("../../styles/tokens.css") />
                <style inner_html=include_str!("../../styles/base.css") />

                // ── Fonts ────────────────────────────────────────────────────
                <link rel="preconnect" href="https://fonts.googleapis.com" />
                <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="" />
                <link
                    href="https://fonts.googleapis.com/css2?family=Bebas+Neue&family=Space+Mono:ital,wght@0,400;0,700;1,400&display=swap"
                    rel="stylesheet"
                />

                // ── Non-critical CSS (async/cached) ─────────────────────────
                // tokens.css & base.css TIDAK di-link — sudah inline di atas.
                <link rel="stylesheet" href="/styles/components.css" />
                <link rel="stylesheet" href="/styles/page-home.css" />
                <link rel="stylesheet" href="/styles/page-explore.css" />
                <link rel="stylesheet" href="/styles/page-event-detail.css" />
                <link rel="stylesheet" href="/styles/page-auth.css" />
                <link rel="stylesheet" href="/styles/page-tickets.css" />
                <link rel="stylesheet" href="/styles/page-ticket-detail.css" />
                <link rel="stylesheet" href="/styles/page-orders.css" />
                <link rel="stylesheet" href="/styles/page-order-detail.css" />
                <link rel="stylesheet" href="/styles/page-order-tickets.css" />
                <link rel="stylesheet" href="/styles/page-profile.css" />
                <link rel="stylesheet" href="/styles/page-merchant.css" />
                <link rel="stylesheet" href="/styles/page-merchant-event.css" />
                <link rel="stylesheet" href="/styles/page-merchant-landing.css" />
                <link rel="stylesheet" href="/styles/page-admin.css" />
                <link rel="stylesheet" href="/styles/page-messages.css" />
                <link rel="stylesheet" href="/styles/page-notifications.css" />
                <link rel="stylesheet" href="/styles/notifications.css" />
                <link rel="stylesheet" href="/styles/page-cart.css" />
                <link rel="stylesheet" href="/styles/page-scan.css" />
                <link rel="stylesheet" href="/styles/page-story.css" />
                <link rel="stylesheet" href="/styles/page-misc.css" />
                <link rel="stylesheet" href="/styles/subscription.css" />
                <link rel="stylesheet" href="/styles/profile_premium.css" />
                <link rel="stylesheet" href="/styles/message_stories.css" />

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
///
/// Ini menggantikan ProtectedRoute di CSR yang berbasis localStorage.
/// Keunggulan: satu path code, SSR + CSR konsisten, tidak ada mismatch.
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
                                // auth.get() → None saat masih loading, Some(result) saat selesai.
                                // Pada SSR dengan new_blocking: selalu langsung Some.
                                // User terautentikasi → render halaman
                                // Belum login atau error → redirect ke /login
                                view! { <Redirect path="/login" /> }
                                    .into_any()
                            }
                        }
                    })
            }}
        </Suspense>
    }
}

// ── Root App Component ────────────────────────────────────────────────────────

/// Komponen root PULSE — universal untuk SSR dan hydration.
///
/// Dipakai SAMA untuk:
///   - Server: `shell()` render `<App/>` → HTML lengkap dikirim ke browser
///   - Client: `hydrate_body(App)` → Leptos attach ke SSR DOM (true hydration)
///
/// Tidak ada clear body + fresh mount → tidak ada DOM mismatch → tidak ada FOUC.
///
/// Context yang di-provide:
///   - `AuthResource`     — status login, blocking SSR
///   - `CartContext`      — keranjang belanja (ephemeral)
///   - `PendingOrderCtx`  — order pending & success snapshot
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    // ── Auth ───────────────────────────────────────────────────────────────
    // new_blocking: SSR menunggu auth selesai sebelum render.
    // Client: resource re-run sebagai HTTP call ke /api-fn/get_session.
    let auth: AuthResource = Resource::new_blocking(|| (), |_| get_session());
    provide_context(auth);

    // ── Cart ───────────────────────────────────────────────────────────────
    let cart = CartContext {
        items: RwSignal::new(vec![]),
    };
    provide_context(cart);

    // ── Order context ──────────────────────────────────────────────────────
    let pending = PendingOrderCtx {
        pending_order: RwSignal::new(None),
        success_order: RwSignal::new(None),
    };
    provide_context(pending);

    view! {
        <Title text="PULSE — Tiket Event Digital" />
        <Meta name="description" content="Platform tiket event terbaik di Indonesia." />

        <Router>
            // Navbar membaca AuthResource dari context — bekerja di SSR dan client
            <Navbar />

            <main>
                <FlatRoutes fallback=|| view! { <NotFoundPage /> }>

                    // ── PUBLIC — SSR full content (SEO) ──────────────────────
                    <Route path=path!("/") view=HomePage />
                    <Route path=path!("/explore") view=ExplorePage />
                    <Route path=path!("/events/:slug") view=EventDetailPage />
                    <Route path=path!("/merchant/landing") view=PulseLandingPage />

                    // ── AUTH — redirect ke /explore jika sudah login ──────────
                    <Route path=path!("/login") view=LoginPage />
                    <Route path=path!("/register") view=RegisterPage />
                    <Route path=path!("/verify-otp") view=VerifyOtpPage />
                    <Route path=path!("/forgot-password") view=ForgotPasswordPage />

                    // ── PRIVATE — AuthGuard memastikan user sudah login ───────
                    // SSR: get_session() baca cookie → langsung redirect jika belum login
                    // Client: re-fetch server fn → Suspense spinner → redirect/render

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
                        path=path!("/profile")
                        view=|| {
                            view! {
                                <AuthGuard>
                                    <ProfilePage />
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
                        path=path!("/messages")
                        view=|| {
                            view! {
                                <AuthGuard>
                                    <MessagesPage />
                                </AuthGuard>
                            }
                        }
                    />
                    <Route
                        path=path!("/messages/:id")
                        view=|| {
                            view! {
                                <AuthGuard>
                                    <ChatRoomPage />
                                </AuthGuard>
                            }
                        }
                    />
                    <Route
                        path=path!("/events/:slug/location")
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
                    <Route
                        path=path!("/merchant")
                        view=|| {
                            view! {
                                <AuthGuard>
                                    <MerchantPage />
                                </AuthGuard>
                            }
                        }
                    />
                    <Route
                        path=path!("/merchant/events/create")
                        view=|| {
                            view! {
                                <AuthGuard>
                                    <MerchantCreateEventPage />
                                </AuthGuard>
                            }
                        }
                    />
                    <Route
                        path=path!("/merchant/events/:slug/edit")
                        view=|| {
                            view! {
                                <AuthGuard>
                                    <MerchantEditEventPage />
                                </AuthGuard>
                            }
                        }
                    />
                    <Route
                        path=path!("/admin")
                        view=|| {
                            view! {
                                <AuthGuard>
                                    <AdminPage />
                                </AuthGuard>
                            }
                        }
                    />

                </FlatRoutes>
            </main>
        </Router>
    }
}
