//! subscription.rs — Halaman Premium Subscription (SSR).

use leptos::prelude::*;

use crate::web::api::create_subscription_order;
use crate::web::app::AuthResource;

struct Benefit { emoji: &'static str, title: &'static str, desc: &'static str }

const BENEFITS: &[Benefit] = &[
    Benefit { emoji: "✨", title: "Story Tanpa Batas", desc: "Upload story sebanyak mungkin setiap hari — tanpa batas." },
    Benefit { emoji: "⚡", title: "Prioritas Antrian Tiket", desc: "Masuk antrian prioritas saat event sold-out." },
    Benefit { emoji: "🎯", title: "Early Access Event", desc: "Akses pembelian tiket 24 jam sebelum dibuka ke publik." },
    Benefit { emoji: "🏷️", title: "Diskon Tiket Eksklusif", desc: "Potongan harga khusus subscriber di event-event pilihan." },
    Benefit { emoji: "🔔", title: "Notifikasi Real-Time", desc: "Push notification instan saat tiket mulai dijual." },
    Benefit { emoji: "💬", title: "Grup Chat VIP", desc: "Ruang obrolan eksklusif sesama premium member." },
];

#[component]
pub fn SubscriptionPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let selected = RwSignal::new("yearly".to_string());
    let loading  = RwSignal::new(false);
    let error    = RwSignal::new(Option::<String>::None);

    let on_subscribe = move |_| {
        if !is_logged_in() {
            #[cfg(target_arch = "wasm32")]
            if let Some(win) = web_sys::window() {
                let _ = win.location().replace("/login");
            }
            return;
        }
        loading.set(true);
        error.set(None);
        let plan = selected.get();
        leptos::task::spawn_local(async move {
            match create_subscription_order(plan).await {
                Ok(order_id) => {
                    let _ = &order_id; // used inside wasm32 cfg block below
                    #[cfg(target_arch = "wasm32")]
                    if let Some(win) = web_sys::window() {
                        let path = if order_id.is_empty() { "/orders".to_string() }
                            else { format!("/orders/{order_id}") };
                        let _ = win.location().replace(&path);
                    }
                }
                Err(e) => { error.set(Some(e.to_string())); loading.set(false); }
            }
        });
    };

    view! {
        <div class="page-header">
            <div class="container">
                <p class="page-header__eyebrow">"// tingkatkan pengalaman"</p>
                <h1 class="page-header__title">"PULSE <span>Premium</span>"</h1>
                <p class="page-header__sub">"Nikmati semua fitur eksklusif tanpa batas"</p>
            </div>
        </div>

        <div class="container" style="padding-bottom:5rem;max-width:640px">

            // ── Benefits ─────────────────────────────────────────────────────
            <div style="margin-bottom:2rem">
                <h2 style="font-size:.75rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.08em;margin-bottom:1rem">"Yang kamu dapatkan"</h2>
                <div style="display:flex;flex-direction:column;gap:.625rem">
                    {BENEFITS.iter().map(|b| view! {
                        <div style="display:flex;gap:.875rem;align-items:start;padding:.875rem;background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:10px">
                            <span style="font-size:1.25rem;flex-shrink:0">{b.emoji}</span>
                            <div>
                                <div style="font-weight:700;font-size:.9rem;margin-bottom:.2rem">{b.title}</div>
                                <div style="font-size:.8rem;color:var(--clr-muted);line-height:1.4">{b.desc}</div>
                            </div>
                        </div>
                    }).collect_view()}
                </div>
            </div>

            // ── Plans ─────────────────────────────────────────────────────────
            <h2 style="font-size:.75rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.08em;margin-bottom:1rem">"Pilih paket"</h2>
            <div style="display:flex;flex-direction:column;gap:.75rem;margin-bottom:1.75rem">
                // Monthly
                <div
                    style=move || {
                        let border = if selected.get() == "monthly" {
                            "border-color:var(--clr-accent)"
                        } else { "" };
                        format!("cursor:pointer;padding:1.25rem;background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:12px;{border}")
                    }
                    on:click=move |_| selected.set("monthly".to_string())
                >
                    <div style="display:flex;justify-content:space-between;align-items:center">
                        <div>
                            <div style="font-weight:700">"Bulanan"</div>
                            <div style="font-size:.8rem;color:var(--clr-muted)">"Rp 29.000/bln"</div>
                        </div>
                        <div style="font-family:var(--font-display);font-size:1.25rem;color:var(--clr-accent)">"Rp 29.000"</div>
                    </div>
                </div>
                // Yearly
                <div
                    style=move || {
                        let border = if selected.get() == "yearly" {
                            "border-color:var(--clr-accent)"
                        } else { "" };
                        format!("cursor:pointer;padding:1.25rem;background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:12px;{border}")
                    }
                    on:click=move |_| selected.set("yearly".to_string())
                >
                    <div style="display:flex;justify-content:space-between;align-items:center">
                        <div>
                            <div style="display:flex;align-items:center;gap:.5rem">
                                <span style="font-weight:700">"Tahunan"</span>
                                <span style="font-size:.7rem;background:var(--clr-accent);color:#000;padding:.15rem .4rem;border-radius:4px;font-weight:700">"TERBAIK"</span>
                            </div>
                            <div style="font-size:.8rem;color:var(--clr-muted)">"Rp 16.583/bln · Hemat 43%"</div>
                        </div>
                        <div style="font-family:var(--font-display);font-size:1.25rem;color:var(--clr-accent)">"Rp 199.000"</div>
                    </div>
                </div>
            </div>

            {move || error.get().map(|e| view! { <div class="alert alert--error" style="margin-bottom:1rem">{e}</div> })}

            <button
                class="btn btn--accent btn--full btn--lg"
                on:click=on_subscribe
                disabled=move || loading.get()
            >
                {move || if loading.get() { "Memproses..." } else { "Berlangganan Sekarang" }}
            </button>

            <p style="font-size:.75rem;color:var(--clr-muted);text-align:center;margin-top:.75rem">
                "Pembayaran aman. Batalkan kapan saja."
            </p>
        </div>
    }
}
