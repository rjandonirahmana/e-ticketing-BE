//! merchant_landing.rs — Halaman Landing Merchant / Pulse (SSR).

use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn PulseLandingPage() -> impl IntoView {
    let ticket_price = RwSignal::new(500_000_f64);
    let tickets_sold = RwSignal::new(1000_f64);

    let revenue   = Memo::new(move |_| ticket_price.get() * tickets_sold.get());
    let pulse_fee = Memo::new(move |_| revenue.get() * 0.01);
    let net       = Memo::new(move |_| revenue.get() - pulse_fee.get());

    let fmt = |n: f64| -> String {
        if n >= 1_000_000_000.0 { format!("Rp{:.1}M", n / 1_000_000_000.0) }
        else if n >= 1_000_000.0 { format!("Rp{:.1}Jt", n / 1_000_000.0) }
        else { format!("Rp{:.0}", n) }
    };

    view! {
        // ── Hero ─────────────────────────────────────────────────────────────
        <section class="hero" style="min-height:80vh;display:flex;align-items:center">
            <div class="container">
                <p class="hero__eyebrow">"// platform event terbaik untuk merchant"</p>
                <h1 class="hero__heading">
                    "Jual Tiket Event"<br/>
                    <span style="color:var(--clr-accent)">"Tanpa Ribet."</span>
                </h1>
                <p class="hero__sub">
                    "Platform tiket all-in-one untuk event organizer Indonesia. "
                    "Dashboard lengkap, pembayaran real-time, fee hanya 1%."
                </p>
                <div class="hero__actions">
                    <A href="/register" attr:class="btn btn--accent btn--lg">"Daftar Gratis"</A>
                    <A href="/explore" attr:class="btn btn--ghost btn--lg">"Lihat Demo"</A>
                </div>
            </div>
        </section>

        // ── Stats ─────────────────────────────────────────────────────────────
        <div class="container">
            <div class="merchant-stats" style="margin-bottom:3rem">
                <div class="stat-card">
                    <div class="stat-card__value">"1%"</div>
                    <div class="stat-card__label">"Biaya Platform"</div>
                </div>
                <div class="stat-card">
                    <div class="stat-card__value">"10K+"</div>
                    <div class="stat-card__label">"Event Aktif"</div>
                </div>
                <div class="stat-card">
                    <div class="stat-card__value">"500K+"</div>
                    <div class="stat-card__label">"Tiket Terjual"</div>
                </div>
                <div class="stat-card">
                    <div class="stat-card__value">"50+"</div>
                    <div class="stat-card__label">"Kota Indonesia"</div>
                </div>
            </div>
        </div>

        // ── Savings Calculator ────────────────────────────────────────────────
        <section class="section">
            <div class="container">
                <div style="max-width:640px;margin:0 auto">
                    <h2 class="section__title" style="text-align:center;margin-bottom:1.5rem">"Hitung Pendapatanmu"</h2>

                    <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:16px;padding:2rem">
                        <div style="margin-bottom:1.5rem">
                            <div style="display:flex;justify-content:space-between;margin-bottom:.5rem">
                                <label style="font-size:.875rem;color:var(--clr-muted)">"Harga Tiket (Rp)"</label>
                                <span style="font-weight:700">{move || format!("Rp {:}", ticket_price.get() as i64)}</span>
                            </div>
                            <input type="range" style="width:100%"
                                min="10000" max="5000000" step="10000"
                                prop:value=move || ticket_price.get() as i64
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                        ticket_price.set(v);
                                    }
                                }
                            />
                        </div>

                        <div style="margin-bottom:1.5rem">
                            <div style="display:flex;justify-content:space-between;margin-bottom:.5rem">
                                <label style="font-size:.875rem;color:var(--clr-muted)">"Jumlah Tiket Terjual"</label>
                                <span style="font-weight:700">{move || format!("{}", tickets_sold.get() as i64)}</span>
                            </div>
                            <input type="range" style="width:100%"
                                min="10" max="10000" step="10"
                                prop:value=move || tickets_sold.get() as i64
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                        tickets_sold.set(v);
                                    }
                                }
                            />
                        </div>

                        <div style="border-top:1px solid var(--clr-border);padding-top:1.25rem;display:grid;grid-template-columns:1fr 1fr 1fr;gap:.75rem;text-align:center">
                            <div>
                                <div style="font-size:.7rem;color:var(--clr-muted);text-transform:uppercase;margin-bottom:.25rem">"Total Revenue"</div>
                                <div style="font-family:var(--font-display);font-weight:700;color:white">{move || fmt(revenue.get())}</div>
                            </div>
                            <div>
                                <div style="font-size:.7rem;color:var(--clr-muted);text-transform:uppercase;margin-bottom:.25rem">"Fee PULSE (1%)"</div>
                                <div style="font-family:var(--font-display);font-weight:700;color:var(--clr-muted)">{move || fmt(pulse_fee.get())}</div>
                            </div>
                            <div>
                                <div style="font-size:.7rem;color:var(--clr-muted);text-transform:uppercase;margin-bottom:.25rem">"Kamu Terima"</div>
                                <div style="font-family:var(--font-display);font-weight:700;color:var(--clr-accent)">{move || fmt(net.get())}</div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </section>

        // ── Features ──────────────────────────────────────────────────────────
        <section class="section">
            <div class="container">
                <h2 class="section__title" style="margin-bottom:2rem">"Kenapa PULSE?"</h2>
                <div class="events-grid">
                    {[
                        ("🎪", "Dashboard Lengkap", "Kelola event, tiket, pembayaran, dan analitik dalam satu tampilan."),
                        ("⚡", "Real-time Analytics", "Pantau penjualan tiket secara real-time. Tidak ada delay."),
                        ("🔒", "Aman & Terpercaya", "Data terenkripsi, pembayaran via gateway terpercaya."),
                        ("📱", "Mobile Ready", "Akses dari mana saja, kapan saja lewat browser."),
                        ("🎟", "Digital Ticketing", "Tiket digital dengan QR code unik, anti-fraud."),
                        ("💬", "Chat Event", "Grup chat otomatis untuk setiap event kamu."),
                    ].iter().map(|(icon, title, desc)| view! {
                        <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:12px;padding:1.5rem">
                            <div style="font-size:2rem;margin-bottom:.75rem">{*icon}</div>
                            <h3 style="font-weight:700;margin-bottom:.5rem">{*title}</h3>
                            <p style="font-size:.875rem;color:var(--clr-muted);line-height:1.5">{*desc}</p>
                        </div>
                    }).collect_view()}
                </div>
            </div>
        </section>

        // ── CTA ───────────────────────────────────────────────────────────────
        <div class="container" style="padding-bottom:5rem">
            <div class="cta-banner">
                <h2>"Siap Mulai Jual Tiket?"</h2>
                <p>"Daftar gratis dalam 2 menit. Tidak butuh kartu kredit."</p>
                <A href="/register" attr:class="btn btn--accent btn--lg">"Daftar sebagai Merchant"</A>
            </div>
        </div>
    }
}
