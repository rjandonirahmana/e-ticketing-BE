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
    components::{FlatRoutes, Route, Router},
    hooks::use_location,
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

impl CartContext {
    pub fn get_qty(&self, tier_id: &str) -> i32 {
        self.items.with(|v| {
            v.iter().find(|i| i.tier_id == tier_id).map(|i| i.quantity).unwrap_or(0)
        })
    }

    pub fn add_item(&self, item: CartItem) {
        self.items.update(|v| {
            if let Some(existing) = v.iter_mut().find(|i| i.tier_id == item.tier_id) {
                existing.quantity += item.quantity;
            } else {
                v.push(item);
            }
        });
        self.persist();
    }

    pub fn update_qty(&self, tier_id: &str, qty: i32) {
        if qty <= 0 {
            let t = tier_id.to_string();
            self.items.update(|v| v.retain(|i| i.tier_id != t));
        } else {
            let t = tier_id.to_string();
            self.items.update(|v| {
                if let Some(it) = v.iter_mut().find(|i| i.tier_id == t) {
                    it.quantity = qty;
                }
            });
        }
        self.persist();
    }

    fn persist(&self) {
        #[cfg(target_arch = "wasm32")]
        self.items.with(|v| {
            if let Some(win) = web_sys::window() {
                if let Ok(Some(storage)) = win.local_storage() {
                    if v.is_empty() {
                        let _ = storage.remove_item("pulse_cart");
                    } else if let Ok(json) = serde_json::to_string(v) {
                        let _ = storage.set_item("pulse_cart", &json);
                    }
                }
            }
        });
    }
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
                <style inner_html=include_str!("../../styles/page-pulse-apply.css") />

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

                // ── Hydration loading indicator ──────────────────────────────
                // Hilang secara otomatis setelah WASM hydration selesai karena
                // Leptos menggantikan/update DOM. Script inline ini jauh lebih
                // cepat dari polling JS — tidak ada delay tambahan.
                <style inner_html=r#"
                    #hydration-loader{
                        position:fixed;top:0;left:0;right:0;height:2px;
                        background:linear-gradient(90deg,#c8ff5e,#4f6bff);
                        z-index:9999;animation:hloader 1.4s ease-in-out infinite;
                        transform-origin:left;
                    }
                    @keyframes hloader{
                        0%{transform:scaleX(0) translateX(0)}
                        50%{transform:scaleX(0.7) translateX(40%)}
                        100%{transform:scaleX(0) translateX(100%)}
                    }
                "# />
                <script inner_html=r#"
                    (function(){
                        var bar = document.createElement('div');
                        bar.id = 'hydration-loader';
                        document.head.appendChild(bar);
                        // Leptos fires 'leptos:hydrated' atau kita poll sampai klik jalan
                        var rm = function(){ var b=document.getElementById('hydration-loader'); if(b) b.remove(); };
                        document.addEventListener('leptos:hydrated', rm, {once:true});
                        // Fallback: hapus setelah 8 detik walau event tidak fire
                        setTimeout(rm, 8000);
                    })();
                "# />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

// ── Auth Guards ───────────────────────────────────────────────────────────────

fn guard_skeleton() -> impl IntoView {
    view! {
        <div class="page">
            <div style="display:flex;align-items:center;justify-content:space-between;
                        padding:14px 16px;border-bottom:1px solid var(--border-soft);
                        background:var(--bg-page);position:sticky;top:0;z-index:40">
                <div class="shim" style="width:36px;height:36px;border-radius:50%"></div>
                <div class="shim" style="width:72px;height:18px;border-radius:4px"></div>
                <div class="shim" style="width:36px;height:36px;border-radius:50%"></div>
            </div>
            <div style="padding:20px 16px;display:flex;flex-direction:column;gap:16px;flex:1">
                {(0..6u32).map(|_| view! {
                    <div style="display:flex;align-items:center;gap:12px">
                        <div class="shim" style="width:56px;height:56px;border-radius:12px;flex-shrink:0"></div>
                        <div style="flex:1;display:flex;flex-direction:column;gap:8px">
                            <div class="shim" style="height:15px;border-radius:6px;width:75%"></div>
                            <div class="shim" style="height:12px;border-radius:6px;width:50%"></div>
                        </div>
                    </div>
                }).collect_view()}
            </div>
        </div>
    }
}

/// Guard: user harus login.
#[component]
fn AuthGuard(children: ChildrenFn) -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource not provided");
    let children = StoredValue::new(children);
    view! {
        <Suspense fallback=guard_skeleton>
            {move || auth.get().map(|result| match result {
                Ok(Some(_)) => children.with_value(|c| c()).into_any(),
                _ => view! { <leptos_router::components::Redirect path="/login" /> }.into_any(),
            })}
        </Suspense>
    }
}

/// Guard: user harus punya role "admin".
#[component]
fn AdminGuard(children: ChildrenFn) -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource not provided");
    let children = StoredValue::new(children);
    view! {
        <Suspense fallback=guard_skeleton>
            {move || auth.get().map(|result| match result {
                Ok(Some(user)) if user.role == "admin" => children.with_value(|c| c()).into_any(),
                Ok(Some(_)) => view! { <leptos_router::components::Redirect path="/explore" /> }.into_any(),
                _ => view! { <leptos_router::components::Redirect path="/login" /> }.into_any(),
            })}
        </Suspense>
    }
}

/// Guard: user harus punya role "merchant" atau "admin".
#[component]
fn MerchantGuard(children: ChildrenFn) -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource not provided");
    let children = StoredValue::new(children);
    view! {
        <Suspense fallback=guard_skeleton>
            {move || auth.get().map(|result| match result {
                Ok(Some(user)) if user.role == "merchant" || user.role == "admin" => {
                    children.with_value(|c| c()).into_any()
                }
                Ok(Some(_)) => view! { <leptos_router::components::Redirect path="/explore" /> }.into_any(),
                _ => view! { <leptos_router::components::Redirect path="/login" /> }.into_any(),
            })}
        </Suspense>
    }
}

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
fn provide_all_app_contexts() {
    // ── Auth ────────────────────────────────────────────────────────────────
    // new_blocking di SEMUA target: SSR blocks render; client baca serialized
    // state dari HTML → langsung resolved, tidak ada Suspense fallback flash.
    let auth: AuthResource = Resource::new_blocking(|| (), |_| get_session());
    provide_context(auth);

    // ── UI / theming ────────────────────────────────────────────────────────
    // provide_theme() aman di SSR: web_sys::window() → None, Effect::new aman.
    crate::web::hooks::provide_theme();

    // ── Cart: init dari localStorage di client, kosong di SSR ──────────────
    let initial_cart: Vec<CartItem> = {
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .and_then(|w| w.local_storage().ok()).flatten()
                .and_then(|s| s.get_item("pulse_cart").ok()).flatten()
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default()
        }
        #[cfg(not(target_arch = "wasm32"))]
        { vec![] }
    };
    let cart_signal = RwSignal::new(initial_cart);
    provide_context(CartContext { items: cart_signal });

    // Cross-tab sync: storage event fires in OTHER tabs when localStorage changes.
    #[cfg(target_arch = "wasm32")]
    if let Some(win) = web_sys::window() {
        let cb = wasm_bindgen::closure::Closure::<dyn Fn(web_sys::StorageEvent)>::new(
            move |e: web_sys::StorageEvent| {
                if e.key().as_deref() == Some("pulse_cart") {
                    let new_items = e.new_value()
                        .and_then(|json| serde_json::from_str::<Vec<CartItem>>(&json).ok())
                        .unwrap_or_default();
                    cart_signal.set(new_items);
                }
            },
        );
        use wasm_bindgen::JsCast;
        let _ = win.add_event_listener_with_callback("storage", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    // ── PendingOrderCtx (dipakai checkout / order_created / payment_success) ──
    provide_context(PendingOrderCtx {
        pending_order: RwSignal::new(None),
        success_order: RwSignal::new(None),
    });

    // ── Data stores (web) ───────────────────────────────────────────────────
    // Setiap store sudah di-guard `if is_server() { return; }` di load().
    crate::web::state::provide_all_stores();

    // ── Premium subscription status ─────────────────────────────────────────
    // provide_premium_store() hanya setup signal — tidak ada spawn_local.
    crate::web::state::premium::provide_premium_store();
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

    // Semua context disediakan di sini — berjalan di SSR maupun setelah hydration.
    provide_all_app_contexts();

    view! {
        <Title text="PULSE — Tiket Event Digital" />
        <Meta name="description" content="Platform tiket event terbaik di Indonesia." />

        <Router>
            <ScrollToTop />
            <main>
                <ErrorBoundary fallback=|_| view! {
                    <div class="page" style="display:flex;flex-direction:column;align-items:center;
                                            justify-content:center;gap:16px;min-height:60vh;
                                            padding:40px 20px;text-align:center">
                        <p style="color:var(--text-primary);font-size:18px;font-weight:700">
                            "Terjadi kesalahan"
                        </p>
                        <p style="color:var(--text-muted);font-size:13px">
                            "Coba muat ulang halaman."
                        </p>
                        <button onclick="window.location.reload()"
                            style="padding:12px 24px;background:var(--accent-lime);border:none;
                                   border-radius:12px;color:#0a0a14;font-weight:700;cursor:pointer">
                            "Muat Ulang"
                        </button>
                    </div>
                }>
                <FlatRoutes fallback=|| view! { <NotFoundPage /> }>

                    // ── PUBLIC — SSR full content (SEO) ──────────────────────
                    <Route path=path!("/") view=ExplorePage />
                    <Route path=path!("/explore") view=ExplorePage />
                    <Route path=path!("/events/:slug") view=EventDetailPage />
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
                        path=path!("/pulse")
                        view=|| view! { <AuthGuard><MessagesPage /></AuthGuard> }
                    />
                    <Route
                        path=path!("/pulse/:id")
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

                    // ── MERCHANT — hanya merchant & admin ─────────────────────
                    <Route
                        path=path!("/merchant")
                        view=|| view! { <MerchantGuard><MerchantPage /></MerchantGuard> }
                    />
                    <Route
                        path=path!("/merchant/events/create")
                        view=|| view! { <MerchantGuard><MerchantCreateEventPage /></MerchantGuard> }
                    />
                    <Route
                        path=path!("/merchant/events/:slug/edit")
                        view=|| view! { <MerchantGuard><MerchantEditEventPage /></MerchantGuard> }
                    />

                    // ── ADMIN — hanya admin ───────────────────────────────────
                    <Route
                        path=path!("/admin")
                        view=|| view! { <AdminGuard><AdminPage /></AdminGuard> }
                    />

                </FlatRoutes>
                </ErrorBoundary>
            </main>
        </Router>
    }
}
