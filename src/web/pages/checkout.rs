//! Halaman checkout: pilih kanal pembayaran, pasang promo, lalu buat order.
//!
//! Semua angka di halaman ini datang dari server. Sebelumnya biaya layanan dan
//! biaya platform adalah dua konstanta Rust (Rp125.000 + Rp25.000) yang sama
//! untuk kanal apa pun, dan daftar kanal ditulis sebagai `const METHODS` —
//! menambah kanal berarti build ulang, dan angka yang dilihat pembeli tak
//! pernah benar-benar terhubung dengan yang ditagihkan. Kini keduanya dibaca
//! dari tabel `payment_methods`, dan totalnya dihitung ulang di server saat
//! order dibuat.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;

use crate::web::api::server_fns::{checkout_cart, get_payment_options};
use crate::web::app::{CartContext, PendingOrderCtx, SuccessSnapshot};
use crate::web::components::ThemeToggle;
use crate::web::models::{OrderRef, PaymentMethodView};
use crate::web::utils::{client_nonce, format_idr};

fn price_label(amount: i64) -> String {
    if amount == 0 {
        "Gratis".to_string()
    } else {
        format_idr(amount)
    }
}

/// Ikon sederhana per jenis kanal — cukup untuk membedakan sekilas tanpa
/// menyeret berkas gambar dari luar (halaman ini dikirim sebagai satu bundel).
fn method_icon(category: &str) -> impl IntoView {
    let path = match category {
        "qris" => view! {
            <rect x="3" y="3" width="7" height="7" rx="1"/>
            <rect x="14" y="3" width="7" height="7" rx="1"/>
            <rect x="3" y="14" width="7" height="7" rx="1"/>
            <path d="M14 14h3v3h-3zM19 19h2v2h-2z"/>
        }
        .into_any(),
        "ewallet" => view! {
            <rect x="2" y="6" width="20" height="13" rx="2"/>
            <path d="M16 12h3"/>
        }
        .into_any(),
        "cash" => view! {
            <circle cx="12" cy="12" r="9"/>
            <path d="M12 7v10M9.5 9.5h5M9.5 14.5h5"/>
        }
        .into_any(),
        _ => view! {
            <rect x="2" y="5" width="20" height="14" rx="2"/>
            <line x1="2" y1="10" x2="22" y2="10"/>
        }
        .into_any(),
    };
    view! {
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
            {path}
        </svg>
    }
}

#[component]
pub fn CheckoutPage() -> impl IntoView {
    let nav_redirect = use_navigate();

    let cart = use_context::<CartContext>().expect("CartContext not provided");
    let items_sig = cart.items;
    let summary = cart.summary;

    // Tak ada satu pun barang tercentang = tak ada yang bisa dibayar. Dipakai
    // untuk mengunci tombol bayar sekaligus memantulkan kembali ke /cart.
    let nothing_chosen =
        Memo::new(move |_| items_sig.with(|v| !v.iter().any(|i| i.selected)));

    let pending_ctx = use_context::<PendingOrderCtx>().expect("PendingOrderCtx not provided");
    let pending_order_sig = pending_ctx.pending_order;
    let success_sig = pending_ctx.success_order;
    let toast = crate::web::components::use_toast();

    // Order hasil checkout. Saat terisi, halaman menampilkan panel instruksi
    // pembayaran dari data respons — bukan redirect, sehingga nomor Virtual
    // Account yang baru terbit tidak lenyap karena perpindahan halaman.
    let placed_order: RwSignal<Option<OrderRef>> = RwSignal::new(None);

    // Kunci idempotensi: satu per kunjungan halaman. Dobel-klik mengirim kunci
    // yang sama, dan server mengembalikan order yang sudah ada.
    let idem_key = RwSignal::new(client_nonce());

    // Kembali ke /cart bila dibuka dengan keranjang kosong. `replace: true`
    // membuang /checkout dari riwayat agar tombol back tak memantul bolak-balik.
    Effect::new(move |_| {
        if cart.ready.get()
            && nothing_chosen.get()
            && placed_order.with(|p| p.is_none())
        {
            nav_redirect.clone()(
                "/cart",
                NavigateOptions {
                    replace: true,
                    ..NavigateOptions::default()
                },
            );
        }
    });

    // Kanal pembayaran + biayanya untuk nominal keranjang saat ini. Dimuat
    // ulang setiap kali total keranjang berubah, karena biaya persentase dan
    // batas nominal kanal ikut berubah bersamanya.
    let cart_total = Memo::new(move |_| summary.with(|s| s.total));
    let options = Resource::new(move || cart_total.get(), |_| get_payment_options());

    let selected = RwSignal::new(String::new());
    let paying = RwSignal::new(false);
    let pay_error = RwSignal::new(String::new());

    let promo_input = RwSignal::new(String::new());
    let promo_busy = RwSignal::new(false);

    // Kanal terpilih: pilihan tersimpan di keranjang lebih dulu, kalau tak ada
    // pakai kanal pertama yang tersedia — pembeli tak pernah menghadapi daftar
    // tanpa satu pun yang tersorot.
    Effect::new(move |_| {
        if !selected.get_untracked().is_empty() {
            return;
        }
        if let Some(Ok(opts)) = options.get() {
            let pick = summary
                .with_untracked(|s| s.payment_code.clone())
                .filter(|c| opts.methods.iter().any(|m| &m.code == c))
                .or_else(|| opts.methods.first().map(|m| m.code.clone()));
            if let Some(code) = pick {
                selected.set(code);
            }
        }
    });

    let current_method = Memo::new(move |_| {
        let code = selected.get();
        options
            .get()
            .and_then(|r| r.ok())
            .and_then(|o| o.methods.into_iter().find(|m| m.code == code))
    });

    let charge = Memo::new(move |_| current_method.get().map(|m| m.charge).unwrap_or(0));
    let grand_total = Memo::new(move |_| cart_total.get() + charge.get());

    // ── Promo ───────────────────────────────────────────────────────────────
    let on_apply_promo = move |_| {
        let code = promo_input.get().trim().to_string();
        if code.is_empty() || promo_busy.get_untracked() {
            return;
        }
        promo_busy.set(true);
        cart.set_promo(Some(code));
        promo_busy.set(false);
    };

    let on_clear_promo = move |_| {
        promo_input.set(String::new());
        cart.set_promo(None);
    };

    // ── Checkout ────────────────────────────────────────────────────────────
    let on_confirm = move |_| {
        if paying.get_untracked() {
            return;
        }
        let code = selected.get_untracked();
        if code.is_empty() {
            pay_error.set("Pilih metode pembayaran dulu.".into());
            return;
        }
        if items_sig.with_untracked(|v| v.iter().any(|i| i.exceeds_stock)) {
            pay_error.set("Ada barang yang melebihi sisa stok. Kurangi dulu di keranjang.".into());
            return;
        }

        paying.set(true);
        pay_error.set(String::new());

        let key = idem_key.get_untracked();
        let key = (!key.is_empty()).then_some(key);

        leptos::task::spawn_local(async move {
            match checkout_cart(code, None, key).await {
                Ok(order) => {
                    let order_href = format!("/orders/{}", order.id);
                    let is_paid = order.status.eq_ignore_ascii_case("paid")
                        || order.status.eq_ignore_ascii_case("completed");

                    if is_paid {
                        success_sig.set(Some(SuccessSnapshot {
                            order_code: order.order_code.clone(),
                            event_name: order
                                .items
                                .first()
                                .map(|i| i.event_name.clone())
                                .unwrap_or_default(),
                            total_amount: order.total_amount,
                        }));
                        toast.notify(
                            crate::web::components::ToastKind::Success,
                            "Pembayaran berhasil".into(),
                            Some(format!(
                                "Order #{} lunas. Kode pengambilan sudah terbit.",
                                order.order_code
                            )),
                            Some(order_href),
                        );
                    } else {
                        pending_order_sig.set(Some(order.clone()));
                        toast.notify(
                            crate::web::components::ToastKind::Success,
                            "Pesanan dibuat".into(),
                            Some(format!(
                                "Order #{} menunggu pembayaran.",
                                order.order_code
                            )),
                            Some(order_href),
                        );
                    }

                    // Server sudah menutup keranjangnya; layar tinggal menyusul.
                    cart.reset_after_checkout();
                    placed_order.set(Some(order));
                    paying.set(false);
                }
                Err(e) => {
                    paying.set(false);
                    let msg = e.to_string();
                    pay_error.set(msg.clone());
                    toast.error("Gagal membuat pesanan. Coba lagi.");
                    // Kunci baru untuk percobaan berikutnya: kegagalan ini bukan
                    // dobel-klik, dan memakai ulang kunci lama bisa memantulkan
                    // order yang setengah jadi.
                    idem_key.set(client_nonce());
                }
            }
        });
    };

    let on_confirm2 = on_confirm;

    view! {
        <div class="page">
            <header class="page-header">
                <button class="back-btn" aria-label="Kembali"
                    on:click=move |_| {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(win) = web_sys::window() {
                            let _ = win.history().ok().map(|h| h.back());
                        }
                    }>
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </button>
                <span class="page-logo">"PULSE"</span>
                <div class="header-actions">
                    <ThemeToggle />
                </div>
            </header>

            <div class="checkout-hero">
                <h1 class="checkout-title">"CHECKOUT"</h1>
                <p class="checkout-sub">
                    "Semua harga dalam Rupiah dan sudah termasuk biaya kanal pembayaran yang dipilih."
                </p>
            </div>

            // ── Ringkasan pesanan ────────────────────────────────────────────
            <section class="checkout-section">
                <div class="section-row">
                    <span class="section-head">"RINGKASAN PESANAN"</span>
                    <span class="section-badge">
                        {move || format!("{} BARANG", summary.with(|s| s.total_quantity))}
                    </span>
                </div>
                <div class="order-items">
                    {move || {
                        // Hanya barang tercentang yang dibayar, jadi hanya itu
                        // yang ditampilkan. Menampilkan sisanya di sini akan
                        // membuat ringkasan tak cocok dengan totalnya.
                        let items: Vec<_> = items_sig
                            .get()
                            .into_iter()
                            .filter(|i| i.selected)
                            .collect();
                        if items.is_empty() {
                            return view! {
                                <p class="empty-msg">
                                    "Keranjang kosong. "
                                    <A href="/explore" attr:class="auth-prompt-link">
                                        "Jelajahi product"
                                    </A>
                                </p>
                            }.into_any();
                        }
                        items.iter().map(|item| {
                            let img = if item.event_cover.is_empty() {
                                "https://images.unsplash.com/photo-1470225620780-dba8ba36b745?w=150&q=80".to_string()
                            } else {
                                item.event_cover.clone()
                            };
                            view! {
                                <div class="order-item">
                                    <img src=img alt=item.event_title.clone() class="order-item-img"/>
                                    <div class="order-item-info">
                                        <div class="order-item-name">{item.event_title.clone()}</div>
                                        <div class="order-item-meta">
                                            {format!("{} • {}", item.tier_name, item.venue_name)}
                                        </div>
                                        {(item.quantity > 1).then(|| view! {
                                            <div class="order-item-qty">
                                                {format!("{}× barang", item.quantity)}
                                            </div>
                                        })}
                                        {item.exceeds_stock.then(|| view! {
                                            <div class="order-item-warn">
                                                {format!("Sisa {} stok", item.available.max(0))}
                                            </div>
                                        })}
                                    </div>
                                    <div class="order-item-price">{price_label(item.subtotal)}</div>
                                </div>
                            }
                        }).collect_view().into_any()
                    }}
                </div>
            </section>

            // ── Kanal pembayaran (dari tabel payment_methods) ────────────────
            <section class="checkout-section">
                <span class="section-head">"METODE PEMBAYARAN"</span>
                <div class="method-list">
                    <Suspense fallback=move || view! {
                        <div class="method-card method-card--loading">"Memuat metode pembayaran…"</div>
                    }>
                        {move || Suspend::new(async move {
                            match options.await {
                                Ok(opts) if !opts.methods.is_empty() => opts
                                    .methods
                                    .into_iter()
                                    .map(|m| method_card(m, selected, cart))
                                    .collect_view()
                                    .into_any(),
                                Ok(_) => view! {
                                    <p class="empty-msg">
                                        "Belum ada metode pembayaran yang melayani nominal ini."
                                    </p>
                                }.into_any(),
                                Err(_) => view! {
                                    <p class="empty-msg">"Gagal memuat metode pembayaran."</p>
                                }.into_any(),
                            }
                        })}
                    </Suspense>
                </div>
            </section>

            // ── Kode promo ──────────────────────────────────────────────────
            <section class="checkout-section">
                <div class="promo-wrap">
                    <span class="promo-label">"PUNYA KODE PROMO?"</span>
                    <div class="promo-input-row">
                        <input class="promo-input" type="text" placeholder="Masukkan kode"
                            prop:value=move || promo_input.get()
                            prop:disabled=move || summary.with(|s| s.promo_code.is_some())
                            on:input=move |e| promo_input.set(event_target_value(&e))
                        />
                        {move || {
                            if summary.with(|s| s.promo_code.is_some()) {
                                view! {
                                    <button class="promo-apply-btn" on:click=on_clear_promo>
                                        "HAPUS"
                                    </button>
                                }.into_any()
                            } else {
                                view! {
                                    <button class="promo-apply-btn"
                                        disabled=move || promo_busy.get() || cart.loading.get()
                                        on:click=on_apply_promo>
                                        {move || if cart.loading.get() { "..." } else { "PAKAI" }}
                                    </button>
                                }.into_any()
                            }
                        }}
                    </div>
                </div>
                {move || {
                    let (code, msg, disc) =
                        summary.with(|s| (s.promo_code.clone(), s.promo_message.clone(), s.discount));
                    match (code, msg.is_empty()) {
                        (Some(c), _) => view! {
                            <p class="promo-success">
                                {format!("Promo {c} dipakai: −{}", format_idr(disc))}
                            </p>
                        }.into_any(),
                        // Promo yang gugur menyisakan alasannya di `promo_message`
                        // — pesan itu berasal dari server, bukan tebakan halaman.
                        (None, false) => view! { <p class="promo-error">{msg}</p> }.into_any(),
                        _ => ().into_any(),
                    }
                }}
            </section>

            // ── Rincian harga ───────────────────────────────────────────────
            <section class="checkout-section total-section">
                <div class="total-head">"TOTAL TAGIHAN"</div>
                <div class="total-line">
                    <span>"Subtotal"</span>
                    <span>{move || price_label(summary.with(|s| s.subtotal))}</span>
                </div>
                {move || {
                    let (disc, code) = summary.with(|s| (s.discount, s.promo_code.clone()));
                    (disc > 0).then(|| view! {
                        <div class="total-line total-line--discount">
                            <span>{format!("Promo ({})", code.unwrap_or_default())}</span>
                            <span>{format!("−{}", format_idr(disc))}</span>
                        </div>
                    })
                }}
                {move || {
                    let m = current_method.get();
                    let c = charge.get();
                    (c > 0).then(|| view! {
                        <div class="total-line">
                            <span>
                                {format!("Biaya {}", m.map(|x| x.name).unwrap_or_else(|| "pembayaran".into()))}
                            </span>
                            <span>{format_idr(c)}</span>
                        </div>
                    })
                }}
                <div class="total-final-row">
                    <span class="total-final-label">"TOTAL"</span>
                    <span class="total-final-amt">{move || price_label(grand_total.get())}</span>
                </div>
            </section>

            // ── Konfirmasi ──────────────────────────────────────────────────
            <div class="confirm-section">
                {move || {
                    (!pay_error.get().is_empty())
                        .then(|| view! { <div class="pay-error">{pay_error.get()}</div> })
                }}
                <button class="confirm-btn"
                    disabled=move || paying.get() || nothing_chosen.get()
                    on:click=on_confirm>
                    {move || if paying.get() { "MEMPROSES..." } else { "KONFIRMASI PEMBAYARAN" }}
                </button>
                <p class="terms-note">
                    "DENGAN MENEKAN KONFIRMASI, ANDA MENYETUJUI SYARAT LAYANAN DAN KEBIJAKAN REFUND"
                </p>
                <div class="trust-icons">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                        <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>
                    </svg>
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                        <rect x="3" y="11" width="18" height="11" rx="2"/>
                        <path d="M7 11V7a5 5 0 0110 0v4"/>
                    </svg>
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                        <rect x="2" y="5" width="20" height="14" rx="2"/>
                        <line x1="2" y1="10" x2="22" y2="10"/>
                    </svg>
                </div>
            </div>

            // ── Panel hasil checkout ────────────────────────────────────────
            {move || placed_order.get().map(order_done_panel)}

            // ── Bilah bayar melekat ─────────────────────────────────────────
            <div class="pay-bar">
                <button class="pay-bar-btn"
                    disabled=move || paying.get() || nothing_chosen.get()
                    on:click=on_confirm2>
                    {move || {
                        if paying.get() {
                            "MEMPROSES...".to_string()
                        } else {
                            format!("BAYAR {}", price_label(grand_total.get()))
                        }
                    }}
                </button>
            </div>
        </div>
    }
}

/// Satu kartu kanal pembayaran. Biaya adminnya ditampilkan di kartu, bukan
/// hanya di rincian bawah — pembeli membandingkan kanal di sini, dan biaya yang
/// baru muncul setelah memilih adalah kejutan yang tak perlu.
fn method_card(m: PaymentMethodView, selected: RwSignal<String>, cart: CartContext) -> impl IntoView {
    let code = m.code.clone();
    let code_click = m.code.clone();
    let is_active = Memo::new(move |_| selected.get() == code);
    let charge_label = if m.charge > 0 {
        format!("+{}", format_idr(m.charge))
    } else {
        "Tanpa biaya".to_string()
    };

    view! {
        <button
            class="method-card"
            class:method-card--active=is_active
            type="button"
            on:click=move |_| {
                selected.set(code_click.clone());
                // Disimpan di keranjang supaya pilihan bertahan saat berpindah
                // halaman atau perangkat.
                cart.set_payment(code_click.clone());
            }
        >
            <span class="method-icon">{method_icon(&m.category)}</span>
            <div class="method-info">
                <div class="method-label">{m.name.clone()}</div>
                <div class="method-sub">{m.description.clone()}</div>
            </div>
            <div class="method-charge">{charge_label}</div>
            {move || is_active.get().then(|| view! {
                <div class="method-check">
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                         stroke="#080810" stroke-width="3" stroke-linecap="round">
                        <polyline points="20 6 9 17 4 12"/>
                    </svg>
                </div>
            })}
        </button>
    }
}

/// Panel setelah order dibuat: bukti pesanan, dan — untuk kanal yang menunggu —
/// nomor Virtual Account beserta cara membayarnya.
fn order_done_panel(o: OrderRef) -> impl IntoView {
    let is_paid =
        o.status.eq_ignore_ascii_case("paid") || o.status.eq_ignore_ascii_case("completed");
    let oid = o.id.clone();

    view! {
        <div class="co-done-overlay">
            <div class="co-done-card">
                <div class=if is_paid { "co-done-icon co-done-icon--paid" } else { "co-done-icon" }>
                    {if is_paid {
                        view! {
                            <svg width="28" height="28" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                <path d="M22 11.08V12a10 10 0 11-5.93-9.14"/>
                                <polyline points="22 4 12 14.01 9 11.01"/>
                            </svg>
                        }.into_any()
                    } else {
                        view! {
                            <svg width="28" height="28" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                <circle cx="12" cy="12" r="10"/>
                                <polyline points="12 6 12 12 16 14"/>
                            </svg>
                        }.into_any()
                    }}
                </div>

                <h2 class="co-done-title">
                    {if is_paid { "PEMBAYARAN BERHASIL" } else { "MENUNGGU PEMBAYARAN" }}
                </h2>
                <p class="co-done-sub">
                    {if is_paid {
                        "Kode pengambilanmu sudah terbit. Tunjukkan saat mengambil barang di toko.".to_string()
                    } else {
                        o.payment_name
                            .clone()
                            .map(|n| format!("Selesaikan pembayaran lewat {n}."))
                            .unwrap_or_else(|| "Selesaikan pembayaran untuk menerbitkan kode pengambilan.".into())
                    }}
                </p>

                <div class="co-done-row">
                    <span>"KODE ORDER"</span>
                    <span class="co-done-code">{"#"}{o.order_code.clone()}</span>
                </div>

                // Nomor VA / referensi QRIS: yang paling dicari pembeli di layar
                // ini, jadi ia berdiri sendiri dan bukan sebaris kecil di daftar.
                {(!is_paid).then(|| o.payment_reference.clone().map(|r| view! {
                    <div class="co-done-va">
                        <span class="co-done-va-label">
                            {o.payment_name.clone().unwrap_or_else(|| "Nomor pembayaran".into())}
                        </span>
                        <span class="co-done-va-num">{r}</span>
                    </div>
                }))}

                <div class="co-done-items">
                    {o.items.iter().map(|i| view! {
                        <div class="co-done-item">
                            <span>{format!("{} — {} ×{}", i.event_name, i.variant_name, i.quantity)}</span>
                            <span>{format_idr(i.subtotal)}</span>
                        </div>
                    }).collect_view()}
                </div>

                {(o.discount_amount > 0).then(|| view! {
                    <div class="co-done-item">
                        <span>{format!("Promo {}", o.promo_code.clone().unwrap_or_default())}</span>
                        <span>{format!("−{}", format_idr(o.discount_amount))}</span>
                    </div>
                })}
                {(o.payment_charge > 0).then(|| view! {
                    <div class="co-done-item">
                        <span>"Biaya pembayaran"</span>
                        <span>{format_idr(o.payment_charge)}</span>
                    </div>
                })}

                <div class="co-done-row co-done-total">
                    <span>"TOTAL"</span>
                    <span>{format_idr(o.total_amount)}</span>
                </div>

                {(!is_paid).then(|| o.payment_instruction.clone()
                    .filter(|s| !s.is_empty())
                    .map(|s| view! { <p class="co-done-instruction">{s}</p> }))}

                {if is_paid {
                    view! { <A href="/tickets" attr:class="co-done-btn">"LIHAT KODE AMBIL"</A> }.into_any()
                } else {
                    view! {
                        <A href=format!("/orders/{oid}") attr:class="co-done-btn">
                            "LANJUT KE PEMBAYARAN"
                        </A>
                    }.into_any()
                }}
                <A href="/explore" attr:class="co-done-secondary">"Kembali ke Explore"</A>
            </div>
        </div>
    }
}
