use leptos::prelude::*;
use leptos_router::components::A;

// ── Savings Calculator ─────────────────────────────────────────────────────────
#[component]
fn SavingsCalculator() -> impl IntoView {
    let ticket_price = RwSignal::new(500_000_f64);
    let tickets_sold = RwSignal::new(1_000_f64);

    // String buffers untuk input ketik — sync dua arah dengan signal
    let price_str = RwSignal::new("500000".to_string());
    let sold_str = RwSignal::new("1000".to_string());

    let revenue = Memo::new(move |_| ticket_price.get() * tickets_sold.get());
    let pulse_fee = Memo::new(move |_| revenue.get() * 0.01);
    let net_profit = Memo::new(move |_| revenue.get() - pulse_fee.get());

    let fmt_idr = |n: f64| -> String {
        if n >= 1_000_000_000.0 {
            format!("Rp{:.1}M", n / 1_000_000_000.0)
        } else if n >= 1_000_000.0 {
            format!("Rp{:.1}Jt", n / 1_000_000.0)
        } else {
            format!("Rp{:.0}", n)
        }
    };

    view! {
        <section class="pl-calc">
            <div class="pl-calc-header">
                <span class="pl-section-label">"Estimate Your Savings"</span>
                <p class="pl-calc-sub">"See how much you take home with PULSE."</p>
            </div>

            <div class="pl-calc-body">
                <div class="pl-calc-badge">"1% FLAT FEE"</div>

                // ── Ticket Price ───────────────────────────────────────────
                <div class="pl-slider-row">
                    <div class="pl-slider-label-row">
                        <span>"Harga Tiket"</span>
                        // Prefix Rp + input angka bisa diketik
                        <div class="pl-num-input-wrap">
                            <span class="pl-num-prefix">"Rp"</span>
                            <input
                                type="number"
                                class="pl-num-input"
                                min="10000"
                                max="5000000"
                                step="10000"
                                prop:value=move || price_str.get()
                                on:input=move |ev| {
                                    let raw = event_target_value(&ev);
                                    price_str.set(raw.clone());
                                    if let Ok(v) = raw.parse::<f64>() {
                                        let clamped = v.clamp(10_000.0, 5_000_000.0);
                                        ticket_price.set(clamped);
                                    }
                                }
                                on:blur=move |_| {
                                    // Normalise buffer saat keluar dari field
                                    price_str.set(format!("{:.0}", ticket_price.get()));
                                }
                            />
                        </div>
                    </div>
                    <input
                        type="range"
                        class="pl-slider"
                        min="10000"
                        max="5000000"
                        step="10000"
                        prop:value=move || ticket_price.get() as i64
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                ticket_price.set(v);
                                price_str.set(format!("{:.0}", v));
                            }
                        }
                    />
                    <div class="pl-slider-range">
                        <span>"Rp10K"</span>
                        <span>"Rp5Jt"</span>
                    </div>
                </div>

                // ── Tickets Sold ───────────────────────────────────────────
                <div class="pl-slider-row">
                    <div class="pl-slider-label-row">
                        <span>"Total Tiket Terjual"</span>
                        <div class="pl-num-input-wrap">
                            <input
                                type="number"
                                class="pl-num-input"
                                min="10"
                                max="10000"
                                step="10"
                                prop:value=move || sold_str.get()
                                on:input=move |ev| {
                                    let raw = event_target_value(&ev);
                                    sold_str.set(raw.clone());
                                    if let Ok(v) = raw.parse::<f64>() {
                                        let clamped = v.clamp(10.0, 10_000.0);
                                        tickets_sold.set(clamped);
                                    }
                                }
                                on:blur=move |_| {
                                    sold_str.set(format!("{:.0}", tickets_sold.get()));
                                }
                            />
                        </div>
                    </div>
                    <input
                        type="range"
                        class="pl-slider"
                        min="10"
                        max="10000"
                        step="10"
                        prop:value=move || tickets_sold.get() as i64
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                tickets_sold.set(v);
                                sold_str.set(format!("{:.0}", v));
                            }
                        }
                    />
                    <div class="pl-slider-range">
                        <span>"10"</span>
                        <span>"10.000"</span>
                    </div>
                </div>

                // ── Result card ────────────────────────────────────────────
                <div class="pl-result-card">
                    <span class="pl-result-label">"ESTIMASI PENDAPATAN KAMU"</span>
                    <div class="pl-result-amount">
                        {move || fmt_idr(revenue.get())}
                    </div>
                    <div class="pl-result-fees">
                        <div class="pl-fee-row">
                            <span>"PULSE Fee (1%)"</span>
                            <span class="pl-fee-val pl-fee-neg">
                                {move || format!("-{}", fmt_idr(pulse_fee.get()))}
                            </span>
                        </div>
                        <div class="pl-fee-row">
                            <span>"Net Profit"</span>
                            <span class="pl-fee-val">
                                {move || fmt_idr(net_profit.get())}
                            </span>
                        </div>
                    </div>
                </div>

                <A href="/register" attr:class="pl-cta-btn pl-cta-btn--lime">
                    "Mulai Jual Tiket"
                </A>
            </div>
        </section>
    }
}

// ── Main Page ──────────────────────────────────────────────────────────────────
#[component]
pub fn PulseLandingPage() -> impl IntoView {
    view! {
        <div class="pl-page">

            // ── NAV ──────────────────────────────────────────────────────────
            <nav class="pl-nav">
                <div class="pl-nav-inner">
                    <A href="/explore" attr:class="pl-nav-back" attr:aria-label="Kembali ke Explore">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                            stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                            <polyline points="15 18 9 12 15 6"/>
                        </svg>
                    </A>
                    <div class="pl-nav-logo">
                        <span class="pl-nav-badge">"GROWTH ENGINE"</span>
                        <span class="pl-nav-title">"PULSE"</span>
                    </div>
                    <a href="mailto:hi@pulse.id" class="pl-contact-btn">"Contact Us"</a>
                </div>
            </nav>

            // ── HERO ──────────────────────────────────────────────────────────
            <section class="pl-hero">
                <p class="pl-hero-tagline">"Join the next generation of live entertainment."</p>
                <h1 class="pl-hero-title">
                    "Scale Your Event with "
                    <span class="pl-hero-accent">"PULSE"</span>
                </h1>
                <p class="pl-hero-sub">
                    "Experience the power of our 1% profit-sharing model designed for modern merchants."
                </p>
                <div class="pl-hero-ctas">
                    <A href="/register" attr:class="pl-cta-btn pl-cta-btn--lime">
                        "Join Now "
                        <svg width="13" height="13" viewBox="0 0 24 24" fill="none"
                            stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                            <line x1="5" y1="12" x2="19" y2="12"/>
                            <polyline points="12 5 19 12 12 19"/>
                        </svg>
                    </A>
                    <button class="pl-cta-btn pl-cta-btn--ghost">"Watch Demo"</button>
                </div>

                // Concert visual SVG
                <div class="pl-hero-visual">
                    <div class="pl-hero-img-wrap">
                        <div class="pl-hero-img-glow"></div>
                        <svg class="pl-hero-concert" viewBox="0 0 440 250"
                            xmlns="http://www.w3.org/2000/svg">
                            <defs>
                                <radialGradient id="stageGlow" cx="50%" cy="55%" r="60%">
                                    <stop offset="0%" stop-color="#4f6bff" stop-opacity="0.45"/>
                                    <stop offset="50%" stop-color="#7c4fff" stop-opacity="0.25"/>
                                    <stop offset="100%" stop-color="#080810" stop-opacity="0"/>
                                </radialGradient>
                                <radialGradient id="spotGlow" cx="50%" cy="100%" r="70%">
                                    <stop offset="0%" stop-color="#c8ff5e" stop-opacity="0.2"/>
                                    <stop offset="100%" stop-color="#c8ff5e" stop-opacity="0"/>
                                </radialGradient>
                                <linearGradient id="floorGrad" x1="0%" y1="0%" x2="0%" y2="100%">
                                    <stop offset="0%" stop-color="#1a1a3a"/>
                                    <stop offset="100%" stop-color="#0d0d20"/>
                                </linearGradient>
                            </defs>
                            // Sky
                            <rect width="440" height="250" fill="#08080f"/>
                            <ellipse cx="220" cy="90" rx="220" ry="120" fill="url(#stageGlow)"/>
                            // Curtains
                            <path d="M0,0 Q50,35 38,250 L0,250 Z" fill="#1a1a40"/>
                            <path d="M440,0 Q390,35 402,250 L440,250 Z" fill="#1a1a40"/>
                            // Stage floor
                            <rect x="24" y="185" width="392" height="65" rx="4" fill="url(#floorGrad)"/>
                            <rect x="24" y="183" width="392" height="4" rx="2" fill="#2a2a60" opacity="0.8"/>
                            // Spotlight beams
                            <polygon points="120,0 72,185 168,185" fill="#4f6bff" opacity="0.05"/>
                            <polygon points="220,0 158,185 282,185" fill="#c8ff5e" opacity="0.07"/>
                            <polygon points="320,0 272,185 368,185" fill="#7c4fff" opacity="0.05"/>
                            // Spotlight dots
                            <circle cx="120" cy="5" r="6" fill="#4f6bff" opacity="0.75"/>
                            <circle cx="220" cy="5" r="7" fill="#c8ff5e" opacity="0.9"/>
                            <circle cx="320" cy="5" r="6" fill="#7c4fff" opacity="0.75"/>
                            // Rigging bar
                            <rect x="24" y="12" width="392" height="7" rx="3.5" fill="#1e1e40"/>
                            <line x1="80"  y1="19" x2="80"  y2="36" stroke="#2a2a50" stroke-width="1.5"/>
                            <line x1="170" y1="19" x2="170" y2="36" stroke="#2a2a50" stroke-width="1.5"/>
                            <line x1="270" y1="19" x2="270" y2="36" stroke="#2a2a50" stroke-width="1.5"/>
                            <line x1="360" y1="19" x2="360" y2="36" stroke="#2a2a50" stroke-width="1.5"/>
                            // LED wall
                            <rect x="72" y="38" width="296" height="104" rx="4" fill="#0d0d22"/>
                            {(0..13_i32).flat_map(|row| {
                                (0..24_i32).map(move |col| {
                                    let x = 77 + col * 12;
                                    let y = 43 + row * 7;
                                    let colors = ["#4f6bff","#7c4fff","#c8ff5e","#ff6b9d","#4f6bff","#7da7ff"];
                                    let color = colors[((row * 3 + col * 7) % 6) as usize];
                                    let opacity = if (row + col) % 3 == 0 { "0.9" }
                                                  else if (row + col) % 3 == 1 { "0.5" }
                                                  else { "0.15" };
                                    view! {
                                        <rect x={x} y={y} width="9" height="5" rx="1"
                                            fill={color} opacity={opacity}/>
                                    }
                                }).collect::<Vec<_>>()
                            }).collect::<Vec<_>>()}
                            // Performers
                            <ellipse cx="175" cy="185" rx="9" ry="22" fill="#060614" opacity="0.95"/>
                            <circle cx="175" cy="158" r="8" fill="#060614" opacity="0.95"/>
                            <ellipse cx="220" cy="180" rx="10" ry="28" fill="#060614" opacity="0.95"/>
                            <circle cx="220" cy="146" r="9" fill="#060614" opacity="0.95"/>
                            <ellipse cx="265" cy="183" rx="9" ry="24" fill="#060614" opacity="0.95"/>
                            <circle cx="265" cy="154" r="8" fill="#060614" opacity="0.95"/>
                            // Crowd
                            {(0..48_i32).map(|i| {
                                let cx = 30 + (i % 24) * 16;
                                let cy = 200 + (i / 24) * 13;
                                let opacity = if i % 3 == 0 { "0.45" } else { "0.22" };
                                view! {
                                    <circle cx={cx} cy={cy} r="3.5" fill="#8888aa" opacity={opacity}/>
                                }
                            }).collect::<Vec<_>>()}
                            <ellipse cx="220" cy="188" rx="100" ry="22" fill="url(#spotGlow)"/>
                        </svg>
                    </div>
                    <div class="pl-live-badge">
                        <span class="pl-live-dot"></span>
                        "Live Sales Tracking"
                    </div>
                </div>
            </section>

            // ── FEATURES ──────────────────────────────────────────────────────
            <section class="pl-features">
                <div class="pl-features-header">
                    <span class="pl-section-label">"Merchant-First Features"</span>
                    <p class="pl-features-sub">
                        "Engineered to maximise your margins and engagement."
                    </p>
                </div>
                <div class="pl-feature-list">
                    <div class="pl-feature-card">
                        <div class="pl-feature-icon pl-icon--lime">
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                <line x1="12" y1="1" x2="12" y2="23"/>
                                <path d="M17 5H9.5a3.5 3.5 0 000 7h5a3.5 3.5 0 010 7H6"/>
                            </svg>
                        </div>
                        <div class="pl-feature-body">
                            <h3 class="pl-feature-title">"Low 1% Platform Fee"</h3>
                            <p class="pl-feature-desc">
                                "Keep more of what you earn. Our transparent pricing means no hidden costs or surprises."
                            </p>
                        </div>
                    </div>
                    <div class="pl-feature-card">
                        <div class="pl-feature-icon pl-icon--blue">
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                <polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/>
                            </svg>
                        </div>
                        <div class="pl-feature-body">
                            <h3 class="pl-feature-title">"Real-time Analytics"</h3>
                            <p class="pl-feature-desc">
                                "Monitor sales, audience demographics, and venue density in real-time from our dashboard."
                            </p>
                        </div>
                    </div>
                    <div class="pl-feature-card">
                        <div class="pl-feature-icon pl-icon--violet">
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                <rect x="1" y="4" width="22" height="16" rx="2" ry="2"/>
                                <line x1="1" y1="10" x2="23" y2="10"/>
                            </svg>
                        </div>
                        <div class="pl-feature-body">
                            <h3 class="pl-feature-title">"Secure QRIS Payments"</h3>
                            <p class="pl-feature-desc">
                                "Integrated secure QRIS checkout for fast, secure transactions across all Indonesian banks."
                            </p>
                        </div>
                    </div>
                    <div class="pl-feature-card">
                        <div class="pl-feature-icon pl-icon--pink">
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                                stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                <rect x="2" y="3" width="20" height="14" rx="2"/>
                                <path d="M8 21h8m-4-4v4"/>
                            </svg>
                        </div>
                        <div class="pl-feature-body">
                            <h3 class="pl-feature-title">"Integrated Stories"</h3>
                            <p class="pl-feature-desc">
                                "Let your fans market for you with built-in story sharing directly to major social platforms."
                            </p>
                        </div>
                    </div>
                </div>
            </section>

            // ── CALCULATOR ────────────────────────────────────────────────────
            <SavingsCalculator />

            // ── TESTIMONIAL ───────────────────────────────────────────────────
            <section class="pl-testimonial">
                <div class="pl-testi-card">
                    <div class="pl-testi-avatar">
                        <svg width="34" height="34" viewBox="0 0 24 24" fill="none"
                            stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                            <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/>
                            <circle cx="12" cy="7" r="4"/>
                        </svg>
                    </div>
                    <blockquote class="pl-testi-quote">
                        "\"Switching to PULSE saved us over Rp200.000.000 in platform fees for our last tour. The real-time QRIS reconciliation is a game changer for our financial audit.\""
                    </blockquote>
                    <div class="pl-testi-author">
                        <span class="pl-testi-name">"Andre Pratama"</span>
                        <span class="pl-testi-role">"CEO of Jakarta Groove Events"</span>
                    </div>
                </div>
            </section>

            // ── FOOTER ────────────────────────────────────────────────────────
            <footer class="pl-footer">
                <div class="pl-footer-brand">
                    <span class="pl-footer-logo">"PULSE"</span>
                    <p class="pl-footer-desc">"Powering the next generation of live entertainment commerce."</p>
                </div>
                <nav class="pl-footer-links">
                    <a href="/explore" class="pl-footer-link">"Terms of Service"</a>
                    <a href="/explore" class="pl-footer-link">"Privacy Policy"</a>
                </nav>
                <nav class="pl-footer-links">
                    <a href="/merchant" class="pl-footer-link">"Organisers Portal"</a>
                    <a href="/explore" class="pl-footer-link">"Support"</a>
                </nav>
                <p class="pl-footer-copy">"© 2024 PULSE Hikent Stage. All Rights Reserved."</p>
            </footer>
        </div>
    }
}
