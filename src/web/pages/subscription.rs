//! subscription.rs — Halaman Premium Subscription (SSR).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::web::api::create_subscription_order;
use crate::web::app::PendingSubCtx;
use crate::web::components::ThemeToggle;
use crate::web::state::premium::use_premium_store;

// ── Data keuntungan premium ───────────────────────────────────────────────────

struct Benefit {
    emoji: &'static str,
    title: &'static str,
    desc: &'static str,
}

const BENEFITS: &[Benefit] = &[
    Benefit {
        emoji: "✨",
        title: "Story Tanpa Batas",
        desc: "Upload story sebanyak mungkin setiap hari — tanpa batas 1x/hari seperti akun free.",
    },
    Benefit {
        emoji: "⚡",
        title: "Prioritas Antrian Tiket",
        desc: "Saat event sold-out, kamu masuk antrian prioritas sehingga peluang dapat tiket jauh lebih besar.",
    },
    Benefit {
        emoji: "🎯",
        title: "Early Access Event",
        desc: "Dapat notifikasi & akses pembelian tiket 24 jam sebelum dibuka ke publik umum.",
    },
    Benefit {
        emoji: "🏷️",
        title: "Diskon Tiket Eksklusif",
        desc: "Nikmati potongan harga khusus subscriber di event-event pilihan partner PULSE.",
    },
    Benefit {
        emoji: "📍",
        title: "Deep-Link Story ke Event",
        desc: "Story yang kamu buat dari halaman event otomatis punya link ke detail event tersebut.",
    },
    Benefit {
        emoji: "🎨",
        title: "Filter & Frame Premium",
        desc: "Akses filter eksklusif dan frame story edisi terbatas yang tidak tersedia di akun free.",
    },
    Benefit {
        emoji: "🔔",
        title: "Notifikasi Real-Time",
        desc: "Push notification instan saat tiket untuk event favoritmu mulai dijual.",
    },
    Benefit {
        emoji: "💬",
        title: "Grup Chat VIP",
        desc: "Bergabung ke ruang obrolan eksklusif sesama premium member dan panitia event.",
    },
];

// ── Paket harga ───────────────────────────────────────────────────────────────

struct Plan {
    id: &'static str,
    label: &'static str,
    price_label: &'static str,
    per_month: &'static str,
    badge: Option<&'static str>,
    savings: Option<&'static str>,
}

const PLANS: &[Plan] = &[
    Plan {
        id: "weekly",
        label: "Mingguan",
        price_label: "Rp 9.900",
        per_month: "~Rp 42.900/bln",
        badge: None,
        savings: None,
    },
    Plan {
        id: "monthly",
        label: "Bulanan",
        price_label: "Rp 29.000",
        per_month: "Rp 29.000/bln",
        badge: None,
        savings: None,
    },
    Plan {
        id: "yearly",
        label: "Tahunan",
        price_label: "Rp 199.000",
        per_month: "Rp 16.583/bln",
        badge: Some("TERBAIK"),
        savings: Some("Hemat 43%"),
    },
    Plan {
        id: "lifetime",
        label: "Seumur Hidup",
        price_label: "Rp 499.000",
        per_month: "Bayar sekali, akses selamanya",
        badge: None,
        savings: Some("Paling hemat"),
    },
];

// ── Komponen utama ────────────────────────────────────────────────────────────

#[component]
pub fn SubscriptionPage() -> impl IntoView {
    let premium = use_premium_store();

    let navigate = StoredValue::new(use_navigate());
    let sub_ctx = use_context::<PendingSubCtx>().expect("PendingSubCtx missing");

    let selected = RwSignal::new("yearly");
    let loading  = RwSignal::new(false);
    let error    = RwSignal::new(Option::<String>::None);

    Effect::new(move |_| {
        premium.load();
    });

    view! {
        <div class="page sub-page">

            // ── Header ───────────────────────────────────────────────────────────
            <header class="page-header">
                <A href="/profile" attr:class="back-btn" attr:aria-label="Kembali">
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
                <span class="page-title page-title--premium">"PULSE PREMIUM"</span>
                <div class="header-actions">
                    <ThemeToggle />
                </div>
            </header>

            // ── Hero ──────────────────────────────────────────────────────────────
            <div class="sub-hero">
                <div class="sub-hero-crown" aria-hidden="true">
                    "👑"
                </div>
                <h1 class="sub-hero-title">
                    "Unlock " <span class="sub-hero-accent">"pengalaman penuh"</span> " concert."
                </h1>
                <p class="sub-hero-sub">
                    "Story tanpa batas, tiket prioritas, dan eksklusivitas yang cuma \
                     dimiliki subscriber PULSE Premium."
                </p>

                <Show when=move || premium.is_premium.get()>
                    <div class="sub-active-badge">
                        <svg
                            width="16"
                            height="16"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2.5"
                        >
                            <polyline points="20 6 9 17 4 12" />
                        </svg>
                        " Kamu sudah Premium!"
                    </div>
                </Show>
            </div>

            // ── Benefit list ──────────────────────────────────────────────────────
            <section class="sub-benefits" aria-label="Keuntungan Premium">
                <h2 class="sub-section-title">"Yang kamu dapatkan"</h2>
                <ul class="sub-benefit-list">
                    {BENEFITS
                        .iter()
                        .map(|b| {
                            view! {
                                <li class="sub-benefit-item">
                                    <span class="sub-benefit-emoji" aria-hidden="true">
                                        {b.emoji}
                                    </span>
                                    <div class="sub-benefit-text">
                                        <span class="sub-benefit-title">{b.title}</span>
                                        <span class="sub-benefit-desc">{b.desc}</span>
                                    </div>
                                    <svg
                                        class="sub-benefit-check"
                                        width="16"
                                        height="16"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2.5"
                                        aria-hidden="true"
                                    >
                                        <polyline points="20 6 9 17 4 12" />
                                    </svg>
                                </li>
                            }
                        })
                        .collect_view()}
                </ul>
            </section>

            // ── Pilih paket ───────────────────────────────────────────────────────
            <section class="sub-plans" aria-label="Pilih paket">
                <h2 class="sub-section-title">"Pilih paket"</h2>
                <div class="sub-plan-cards">
                    {PLANS
                        .iter()
                        .map(|plan| {
                            let pid = plan.id;
                            let is_selected = move || selected.get() == pid;
                            view! {
                                <button
                                    class="sub-plan-card"
                                    class:sub-plan-card--selected=is_selected
                                    on:click=move |_| selected.set(pid)
                                    aria-pressed=move || is_selected().to_string()
                                    aria-label=format!("Pilih paket {}", plan.label)
                                >
                                    <div class="sub-plan-header">
                                        <span class="sub-plan-label">{plan.label}</span>
                                        {plan.badge.map(|b| {
                                            view! { <span class="sub-plan-badge">{b}</span> }
                                        })}
                                    </div>
                                    <div class="sub-plan-price">{plan.price_label}</div>
                                    <div class="sub-plan-per-month">{plan.per_month}</div>
                                    {plan.savings.map(|s| {
                                        view! { <div class="sub-plan-savings">{s}</div> }
                                    })}
                                    <div class="sub-plan-radio" aria-hidden="true">
                                        <div
                                            class="sub-plan-radio-inner"
                                            class:sub-plan-radio-inner--checked=is_selected
                                        ></div>
                                    </div>
                                </button>
                            }
                        })
                        .collect_view()}
                </div>
            </section>

            // ── Perbandingan Free vs Premium ──────────────────────────────────────
            <section class="sub-compare" aria-label="Perbandingan Free vs Premium">
                <h2 class="sub-section-title">"Free vs Premium"</h2>
                <div class="sub-compare-table">
                    <div class="sub-compare-header">
                        <div class="sub-compare-feature-col">"Fitur"</div>
                        <div class="sub-compare-tier-col">"Free"</div>
                        <div class="sub-compare-tier-col sub-compare-tier-col--premium">
                            "Premium"
                        </div>
                    </div>
                    {[
                        ("Story per hari", "1x", "Unlimited"),
                        ("Prioritas tiket", "—", "✓"),
                        ("Early access event", "—", "✓"),
                        ("Diskon tiket", "—", "✓"),
                        ("Deep-link story", "✓", "✓"),
                        ("Filter premium", "—", "✓"),
                        ("Notifikasi real-time", "Standar", "Instan"),
                        ("Grup chat VIP", "—", "✓"),
                    ]
                        .iter()
                        .map(|(feat, free_val, prem_val)| {
                            view! {
                                <div class="sub-compare-row">
                                    <div class="sub-compare-feature">{*feat}</div>
                                    <div class="sub-compare-free">{*free_val}</div>
                                    <div class="sub-compare-premium">{*prem_val}</div>
                                </div>
                            }
                        })
                        .collect_view()}
                </div>
            </section>

            // ── Testimonial ───────────────────────────────────────────────────────
            <section class="sub-testimonials" aria-label="Apa kata mereka">
                <h2 class="sub-section-title">"Apa kata mereka"</h2>
                <div class="sub-testi-list">
                    {[
                        (
                            "@rioazzam",
                            "Udah 3x dapet tiket event sold-out karena antrian prioritas. Worth it banget!",
                        ),
                        (
                            "@ndavidina",
                            "Story unlimited bikin aku bisa dokumentasi setiap moment konser tanpa takut limit.",
                        ),
                        (
                            "@kusmantoro",
                            "Early access-nya beneran ngebantu. Tiket Coldplay kena sebelum habis duluan!",
                        ),
                    ]
                        .iter()
                        .map(|(user, quote)| {
                            view! {
                                <blockquote class="sub-testi-card">
                                    <div class="sub-testi-stars" aria-label="5 bintang">
                                        "★★★★★"
                                    </div>
                                    <p class="sub-testi-quote">{format!("\"{}\"", quote)}</p>
                                    <footer class="sub-testi-user">{*user}</footer>
                                </blockquote>
                            }
                        })
                        .collect_view()}
                </div>
            </section>

            // ── CTA sticky ────────────────────────────────────────────────────────
            <div class="sub-cta-sticky">
                <Show when=move || premium.is_premium.get()>
                    <div class="sub-cta-already">
                        <span>"Kamu sudah Premium"</span>
                        <A href="/explore" attr:class="sub-cta-explore-link">
                            "Explore Event →"
                        </A>
                    </div>
                </Show>
                <Show when=move || !premium.is_premium.get()>
                    <Show when=move || error.get().is_some()>
                        <div class="sub-cta-error">
                            <svg
                                width="14"
                                height="14"
                                viewBox="0 0 24 24"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="2.5"
                                aria-hidden="true"
                            >
                                <circle cx="12" cy="12" r="10" />
                                <line x1="12" y1="8" x2="12" y2="12" />
                                <line x1="12" y1="16" x2="12.01" y2="16" />
                            </svg>
                            {move || error.get().unwrap_or_default()}
                        </div>
                    </Show>
                    <button
                        class="sub-cta-btn"
                        on:click=move |_| {
                            loading.set(true);
                            error.set(None);
                            let plan = selected.get().to_string();
                            let nav = navigate.get_value();
                            leptos::task::spawn_local(async move {
                                match create_subscription_order(plan).await {
                                    Ok(pending) => {
                                        sub_ctx.order.set(Some(pending));
                                        loading.set(false);
                                        nav("/subscription/checkout", Default::default());
                                    }
                                    Err(e) => {
                                        error.set(Some(e.to_string()));
                                        loading.set(false);
                                    }
                                }
                            });
                        }
                        disabled=move || loading.get()
                        aria-label="Berlangganan sekarang"
                    >
                        <svg
                            width="18"
                            height="18"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2.5"
                            aria-hidden="true"
                        >
                            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
                        </svg>
                        " "
                        {move || {
                            if loading.get() {
                                "Memproses...".to_string()
                            } else {
                                PLANS
                                    .iter()
                                    .find(|p| p.id == selected.get())
                                    .map(|p| format!("Lanjut Bayar — {}", p.price_label))
                                    .unwrap_or_else(|| "Lanjut Bayar".to_string())
                            }
                        }}
                    </button>
                </Show>
                <p class="sub-cta-terms">
                    "Dengan berlangganan, kamu menyetujui " <a href="/terms" class="sub-cta-link">
                        "Syarat & Ketentuan"
                    </a> " dan " <a href="/privacy" class="sub-cta-link">
                        "Kebijakan Privasi"
                    </a> " PULSE."
                </p>
            </div>

        </div>
    }
}
