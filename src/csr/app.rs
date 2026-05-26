//! app.rs — Root App component untuk Leptos CSR.
//!
//! Modul ini berisi komponen App dan ProtectedRoute yang merupakan
//! inti dari frontend CSR. Digunakan oleh `src/lib.rs` sebagai
//! entry point WASM hydration saat `feature = "hydrate"`.

use leptos::prelude::*;
use leptos_router::components::{Redirect, Route, Router, Routes};
use leptos_router::hooks::use_location;
use leptos_router::path;

use crate::csr::hooks::use_auth;

// ═══════════════════════════════════════════════════════════════════════════════
// Provider Initialization
// ═══════════════════════════════════════════════════════════════════════════════

/// Inisialisasi semua application context providers di satu tempat.
/// Centralized di sini agar app.rs tidak perlu import provider individual
/// dari modul internal (reduces coupling).
fn init_app_providers() {
    crate::csr::hooks::provide_theme();
    crate::csr::hooks::provide_auth();
    crate::csr::hooks::provide_cart();
    crate::csr::state::provide_all_stores();
    crate::csr::state::provide_stories_store();
    crate::csr::state::premium::provide_premium_store();
    crate::csr::pages::order_created::provide_pending_order();
    crate::csr::pages::payment_success::provide_payment_success();
}

// ═══════════════════════════════════════════════════════════════════════════════
// Auth Guard
// ═══════════════════════════════════════════════════════════════════════════════

/// Guard halaman yang butuh login.
/// - Loading (rehydrasi localStorage) → spinner (stabil, tidak flicker)
/// - Belum login → redirect ke /login dengan return URL preservation
/// - Sudah login → render children
#[component]
fn ProtectedRoute(children: ChildrenFn) -> impl IntoView {
    let auth = use_auth();
    let children = StoredValue::new(children);
    let location = use_location();

    // Bangun return URL dari path saat ini untuk redirect balik setelah login.
    // Hindari redirect loop ke halaman auth itu sendiri.
    let return_url = Memo::new(move |_| {
        let path = location.pathname.get();
        if path.starts_with("/login")
            || path.starts_with("/register")
            || path.starts_with("/verify-otp")
            || path.starts_with("/forgot-password")
        {
            "/explore".to_string()
        } else {
            path
        }
    });

    view! {
        <Suspense fallback=move || {
            view! {
                <div class="auth-guard-loading">
                    <div class="auth-guard-spinner"></div>
                </div>
            }
        }>
            {move || {
                if auth.is_loading.get() {
                    // Masih rehydrasi → spinner
                    view! {
                        <div class="auth-guard-loading">
                            <div class="auth-guard-spinner"></div>
                        </div>
                    }
                        .into_any()
                } else if auth.is_authenticated() {
                    children.with_value(|c| c()).into_any()
                } else {
                    let redirect = format!("/login?redirect={}", return_url.get());
                    view! { <Redirect path=redirect /> }.into_any()
                }
            }}
        </Suspense>
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// App Root
// ═══════════════════════════════════════════════════════════════════════════════

/// Root component seluruh aplikasi CSR.
/// Di-mount ke `<body>` oleh WASM entry point di `src/lib.rs`.
#[component]
pub fn App() -> impl IntoView {
    init_app_providers();

    view! {
        <Router>
            <Routes fallback=|| view! { <crate::csr::pages::not_found::NotFoundPage /> }>
                // ── Publik (tanpa login) ───────────────────────────────────────
                <Route path=path!("/login") view=crate::csr::pages::login::LoginPage />
                <Route path=path!("/register") view=crate::csr::pages::register::RegisterPage />
                <Route
                    path=path!("/verify-otp")
                    view=crate::csr::pages::verify_otp::VerifyOtpPage
                />
                <Route
                    path=path!("/forgot-password")
                    view=crate::csr::pages::forgot_password::ForgotPasswordPage
                />

                // Browse event publik — tidak butuh login
                <Route path=path!("/") view=|| view! { <Redirect path="/explore" /> } />
                <Route path=path!("/explore") view=crate::csr::pages::explore::ExplorePage />
                <Route
                    path=path!("/pulse-landing")
                    view=crate::csr::pages::merchant_landing::PulseLandingPage
                />
                <Route
                    path=path!("/events/:slug")
                    view=crate::csr::pages::event_detail::EventDetailPage
                />
                <Route
                    path=path!("/events/:slug/location")
                    view=crate::csr::pages::venue_location::VenueLocationPage
                />

                // ── Terproteksi (harus login) ──────────────────────────────────
                <Route
                    path=path!("/profile")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::profile::ProfilePage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/orders/:id/tickets")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::order_tickets::OrderTicketsPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/tickets")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::tickets::TicketsPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/subscription")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::subscription::SubscriptionPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/tickets/:id")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::ticket_detail::TicketDetailPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/cart")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::cart::CartPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/checkout")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::checkout::CheckoutPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/order-created")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::order_created::OrderCreatedPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/payment-success")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::payment_success::PaymentSuccessPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/orders")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::orders::OrdersPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/orders/:id")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::order_detail::OrderDetailPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/notifications")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::notifications::NotificationsPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/notifications/:id")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::notification_detail::NotificationDetailPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/admin")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::admin::AdminPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/merchant")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::merchant::MerchantPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/merchant/events/new")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::merchant_create_event::MerchantCreateEventPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/merchant/events/:slug/edit")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::merchant_edit_event::MerchantEditEventPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/merchant/scan")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::scan::ScanPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/stories/new")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::story::StoryCreatorPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/pulse")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::messages::MessagesPage />
                            </ProtectedRoute>
                        }
                    }
                />
                <Route
                    path=path!("/pulse/:id")
                    view=|| {
                        view! {
                            <ProtectedRoute>
                                <crate::csr::pages::chat_room::ChatRoomPage />
                            </ProtectedRoute>
                        }
                    }
                />

                <Route path=path!("/*any") view=crate::csr::pages::not_found::NotFoundPage />
            </Routes>
        </Router>
    }
}
