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
        <Title text="PULSE — Tiket Event Digital" />
        <Meta name="description" content="Platform tiket event terbaik di Indonesia." />

        <Router>
            <ScrollToTop />
            <main>
                <ErrorBoundary fallback=|_| {
                    view! {
                        <div
                            class="page"
                            style="display:flex;flex-direction:column;align-items:center;
                            justify-content:center;gap:16px;min-height:60vh;
                            padding:40px 20px;text-align:center"
                        >
                            <p style="color:var(--text-primary);font-size:18px;font-weight:700">
                                "Terjadi kesalahan"
                            </p>
                            <p style="color:var(--text-muted);font-size:13px">
                                "Coba muat ulang halaman."
                            </p>
                            <button
                                onclick="window.location.reload()"
                                style="padding:12px 24px;background:var(--accent-lime);border:none;
                                 border-radius:12px;color:#0a0a14;font-weight:700;cursor:pointer"
                            >
                                "Muat Ulang"
                            </button>
                        </div>
                    }
                }>
                    <FlatRoutes fallback=|| view! { <NotFoundPage /> }>

                        // ── PUBLIC — SSR full content (SEO) ──────────────────────
                        <Route path=path!("/") view=ExplorePage />
                        <Route path=path!("/explore") view=ExplorePage />
                        <Route path=path!("/lives") view=LivesPage />
                        <Route path=path!("/meet/:id") view=MeetPage />
                        <Route path=path!("/events/:slug") view=EventDetailPage />
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
                            path=path!("/merchant/events/create")
                            view=|| {
                                view! {
                                    <MerchantGuard>
                                        <MerchantCreateEventPage />
                                    </MerchantGuard>
                                }
                            }
                        />
                        <Route
                            path=path!("/merchant/events/:slug/edit")
                            view=|| {
                                view! {
                                    <MerchantGuard>
                                        <MerchantEditEventPage />
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

                    </FlatRoutes>
                </ErrorBoundary>
            </main>
        </Router>
    }
}
