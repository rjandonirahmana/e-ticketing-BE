// src/pages/subscription.rs
//
// Halaman Premium Subscription — pembelian paket tahunan.
// Route: /subscription
//
// Alur pembelian:
//   - Tiket event biasa  → CartItem normal → /cart → /checkout → /orders/:id
//   - Premium subscription → CartItem dengan event_id="__premium__" → /cart → /checkout
//     Backend membedakan item premium dari event_id sentinel "__premium__"
//     dan mengaktifkan subscription setelah pembayaran sukses.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::csr::hooks::{use_cart, use_nav, ThemeToggle};
use crate::csr::models::CartItem;
use crate::csr::state::premium::use_premium_store;

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
        desc: "Nikmati potongan harga khusus subscriber di event-event pilihan partner Kinetic.",
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
    /// Harga dalam IDR (satuan rupiah, bukan sen)
    price_idr: i64,
    badge: Option<&'static str>,
    savings: Option<&'static str>,
}

const PLANS: &[Plan] = &[
    Plan {
        id: "monthly",
        label: "Bulanan",
        price_label: "Rp 29.000",
        per_month: "Rp 29.000/bln",
        price_idr: 29_000,
        badge: None,
        savings: None,
    },
    Plan {
        id: "yearly",
        label: "Tahunan",
        price_label: "Rp 199.000",
        per_month: "Rp 16.583/bln",
        price_idr: 199_000,
        badge: Some("TERBAIK"),
        savings: Some("Hemat 43%"),
    },
];

// ── Komponen utama ────────────────────────────────────────────────────────────

#[component]
pub fn SubscriptionPage() -> impl IntoView {
    let premium = use_premium_store();
    let cart = use_cart();
    let nav = use_nav();
    let nav_sv = StoredValue::new(nav);

    let selected_plan = RwSignal::new("yearly");

    // Load premium status saat masuk halaman
    Effect::new(move |_| {
        premium.load();
    });

    // ── Alur pembelian premium (berbeda dari tiket event biasa) ───────────────
    // Tiket event: CartItem dengan event_id = ID event asli dari backend.
    // Premium:     CartItem dengan event_id = "__premium__" (sentinel).
    //              Checkout & backend membedakan item ini dan mengaktifkan
    //              subscription setelah pembayaran sukses, bukan menambah tiket.
    let on_subscribe = move || {
        let plan = match PLANS.iter().find(|p| p.id == selected_plan.get_untracked()) {
            Some(p) => p,
            None => return,
        };

        // Kosongkan cart terlebih dahulu agar tidak tercampur dengan tiket event
        // (premium adalah transaksi terpisah)
        cart.items.set(vec![]);

        let item = CartItem {
            // Sentinel ID yang dikenali backend sebagai item premium subscription
            event_id: "__premium__".to_string(),
            // tier_id menyimpan plan: "premium_monthly" atau "premium_yearly"
            tier_id: format!("premium_{}", plan.id),
            event_title: "Kinetic Premium".to_string(),
            tier_name: format!("{} — {}", plan.label, plan.price_label),
            venue_name: String::new(),
            event_cover: String::new(),
            quantity: 1,
            unit_price: plan.price_idr,
        };

        cart.add_item(item);
        nav_sv.get_value()("/cart", Default::default());
    };

    view! {
        <div class="page sub-page">

            // ── Header — sama dengan halaman lain ────────────────────────────
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
                <span class="page-title page-title--premium">"KINETIC PREMIUM"</span>
                <div class="header-actions">
                    <ThemeToggle />
                </div>
            </header>

            // ── Hero ──────────────────────────────────────────────────────────
            <div class="sub-hero">
                <div class="sub-hero-crown" aria-hidden="true">
                    "👑"
                </div>
                <h1 class="sub-hero-title">
                    "Unlock " <span class="sub-hero-accent">"pengalaman penuh"</span> " concert."
                </h1>
                <p class="sub-hero-sub">
                    "Story tanpa batas, tiket prioritas, dan eksklusivitas yang cuma \
                     dimiliki subscriber Kinetic Premium."
                </p>

                // Premium badge jika sudah aktif
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

            // ── Benefit list ──────────────────────────────────────────────────
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

            // ── Pilih paket ───────────────────────────────────────────────────
            <section class="sub-plans" aria-label="Pilih paket">
                <h2 class="sub-section-title">"Pilih paket"</h2>
                <div class="sub-plan-cards">
                    {PLANS
                        .iter()
                        .map(|plan| {
                            let pid = plan.id;
                            let is_selected = move || selected_plan.get() == pid;
                            view! {
                                <button
                                class="sub-plan-card"
                                class:sub-plan-card--selected=is_selected
                                on:click=move |_| selected_plan.set(pid)
                                aria-pressed=move || is_selected().to_string()
                                aria-label=format!("Pilih paket {}", plan.label)
                            >
                                <div class="sub-plan-header">
                                    <span class="sub-plan-label">{plan.label}</span>
                                    {plan.badge.map(|b| view! {
                                        <span class="sub-plan-badge">{b}</span>
                                    })}
                                </div>
                                <div class="sub-plan-price">{plan.price_label}</div>
                                <div class="sub-plan-per-month">{plan.per_month}</div>
                                {plan.savings.map(|s| view! {
                                    <div class="sub-plan-savings">{s}</div>
                                })}
                                <div class="sub-plan-radio" aria-hidden="true">
                                    <div class="sub-plan-radio-inner"
                                         class:sub-plan-radio-inner--checked=is_selected />
                                </div>
                            </button>
                            }
                        })
                        .collect_view()}
                </div>
            </section>

            // ── Perbandingan Free vs Premium ──────────────────────────────────
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

            // ── Testimonial ───────────────────────────────────────────────────
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

            // ── CTA sticky ────────────────────────────────────────────────────
            <div class="sub-cta-sticky">
                {move || {
                    if premium.is_premium.get() {
                        view! {
                            <div class="sub-cta-already">
                                <span>"Kamu sudah Premium"</span>
                                <A href="/explore" attr:class="sub-cta-explore-link">
                                    "Explore Event →"
                                </A>
                            </div>
                        }
                            .into_any()
                    } else {
                        let plan_label = move || {
                            PLANS
                                .iter()
                                .find(|p| p.id == selected_plan.get())
                                .map(|p| format!("Lanjut Bayar — {}", p.price_label))
                                .unwrap_or_else(|| "Lanjut Bayar".to_string())
                        };
                        // Tombol: masukkan ke cart lalu arahkan ke /cart
                        view! {
                            <button
                                class="sub-cta-btn"
                                on:click=move |_| on_subscribe()
                                aria-label="Tambah ke keranjang dan lanjut ke pembayaran"
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
                                {plan_label}
                            </button>
                        }
                            .into_any()
                    }
                }}
                <p class="sub-cta-terms">
                    "Dengan berlangganan, kamu menyetujui " <a href="/terms" class="sub-cta-link">
                        "Syarat & Ketentuan"
                    </a> " dan " <a href="/privacy" class="sub-cta-link">
                        "Kebijakan Privasi"
                    </a> " Kinetic."
                </p>
            </div>

        </div>
    }
}
