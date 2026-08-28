//! Halaman keranjang belanja — **acuan penulisan Tailwind untuk halaman lain**.
//!
//! Halaman ini tidak memakai satu pun kelas kustom. Semua tampilan lahir dari
//! utility, dan warnanya menarik token yang sama dengan CSS lama (`bg-card` →
//! `var(--bg-card)`), jadi sakelar tema terang/gelap tetap berlaku tanpa satu
//! pun varian `dark:`.
//!
//! ── TIGA POLA YANG DIPAKAI ULANG DI HALAMAN LAIN ───────────────────────────
//!
//! 1. **Kelas kondisional ditulis LENGKAP, bukan dirakit.** Pemindai Tailwind
//!    membaca teks apa adanya di berkas ini. `format!("border-{…}")` lolos dari
//!    pemindaian dan gayanya hilang senyap — hanya di produksi, karena build
//!    dev sering masih memuat kelas dari halaman lain.
//!
//! 2. **Kotak centang tidak memakai `peer-checked:`.** Varian `peer-*` hanya
//!    berlaku untuk SAUDARA sekandung. Tanda centang yang diletakkan di dalam
//!    kotaknya adalah ANAK, bukan saudara, jadi ia tak pernah menyala. Di
//!    Leptos keadaan itu sudah ada di tangan kita — cukup render bersyarat.
//!
//! 3. **Header, kartu, dan tombol memakai rangkaian utility yang sama persis**
//!    di seluruh halaman. Kalau nanti ada tiga halaman memakai rangkaian yang
//!    sama, barulah ia pantas naik jadi kelas komponen di `style/tailwind.css`.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::web::app::CartContext;
use crate::web::components::{CartButton, ThemeToggle};
use crate::web::models::CartItemView;
use crate::web::utils::format_idr;

fn price_label(amount: i64) -> String {
    if amount == 0 {
        "Gratis".to_string()
    } else {
        format_idr(amount)
    }
}

/// Kartu permukaan standar. Dipakai baris keranjang dan kartu ringkasan supaya
/// radius, garis, dan latar tak pernah berbeda antar-blok di halaman yang sama.
const CARD: &str = "bg-card border border-solid border-line-soft rounded-2xl";

/// Tombol utama (pil). Tinggi minimum 44px mengikuti sasaran sentuh yang
/// nyaman di layar sentuh.
const BTN_PRIMARY: &str = "inline-flex items-center justify-center gap-2 \
     min-h-11 px-6 rounded-full cursor-pointer border-0 \
     bg-brand text-on-brand font-sans text-xs font-bold tracking-[0.08em] \
     transition-opacity hover:opacity-90 active:opacity-80";

#[component]
pub fn CartPage() -> impl IntoView {
    let navigate = use_navigate();
    let cart = use_context::<CartContext>().expect("CartContext not provided");
    let items_sig = cart.items;
    let summary = cart.summary;
    let toast = crate::web::components::use_toast();
    let ready = cart.ready;

    // Pemberitahuan barang yang dibuang otomatis (stok habis / produk tutup).
    // Ditampilkan sekali per pesan, bukan tiap kali sinyal ringkasan berubah.
    let last_notif = RwSignal::new(String::new());
    Effect::new(move |_| {
        let n = summary.with(|s| s.notif.clone());
        if !n.is_empty() && last_notif.get_untracked() != n {
            last_notif.set(n.clone());
            toast.notify(
                crate::web::components::ToastKind::Info,
                "Keranjang diperbarui".into(),
                Some(n),
                None,
            );
        }
    });

    Effect::new(move |prev: Option<()>| {
        if prev.is_none() && cart.authed.get() {
            cart.load();
        }
    });

    let blocked = Memo::new(move |_| {
        items_sig.with(|v| v.iter().any(|i| i.selected && i.exceeds_stock))
    });
    let selected_count = Memo::new(move |_| items_sig.with(|v| v.iter().filter(|i| i.selected).count()));
    let total_count = Memo::new(move |_| items_sig.with(|v| v.len()));
    let all_selected = Memo::new(move |_| {
        items_sig.with(|v| !v.is_empty() && v.iter().all(|i| i.selected))
    });

    let on_proceed = {
        let navigate = navigate.clone();
        move |_| {
            if selected_count.get_untracked() == 0 {
                toast.error("Pilih dulu barang yang ingin dibayar.");
                return;
            }
            if blocked.get_untracked() {
                toast.error("Kurangi jumlah yang melebihi sisa stok.");
                return;
            }
            navigate("/checkout", Default::default());
        }
    };

    view! {
        // `pb-36` menyediakan ruang untuk bilah bayar yang melekat di bawah.
        // Tanpa itu, baris terakhir keranjang tertutup dan tak bisa digulir
        // sampai terlihat.
        <div class="min-h-screen bg-page pb-36">

            // ── Header ──────────────────────────────────────────────────────
            <header class="sticky top-0 z-40 flex items-center justify-between gap-3 \
                           w-full px-4 py-3.5 bg-page border-b border-solid border-line-soft">
                <button
                    class="flex items-center justify-center w-9 h-9 shrink-0 rounded-full \
                           bg-card border border-solid border-line text-content cursor-pointer \
                           transition-colors hover:bg-card-hover active:scale-95"
                    aria-label="Kembali"
                    on:click=move |_| {
                        #[cfg(target_arch = "wasm32")]
                        if let Some(win) = web_sys::window() {
                            let _ = win.history().ok().map(|h| h.back());
                        }
                    }>
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </button>

                <span class="font-title text-lg tracking-[0.12em] text-content">"PULSE"</span>

                <div class="flex items-center gap-2">
                    <CartButton />
                    <ThemeToggle />
                </div>
            </header>

            // ── Judul ───────────────────────────────────────────────────────
            <div class="px-5 pt-7 pb-1">
                <h1 class="font-title text-5xl leading-[0.95] text-content">
                    "ISI"<br/>"KERANJANG"
                </h1>
                <p class="mt-2 text-[13px] text-content-soft">
                    "Periksa pilihan Anda sebelum membayar."
                </p>
            </div>

            {move || {
                if !ready.get() {
                    return view! {
                        <div class="flex flex-col gap-3 px-5 pt-6">
                            {(0..3u32).map(|_| view! {
                                <div class=format!("{CARD} flex items-center gap-3.5 p-4")>
                                    <div class="w-[72px] h-[72px] shrink-0 rounded-xl bg-elevated animate-pulse"/>
                                    <div class="flex-1 flex flex-col gap-2.5">
                                        <div class="h-3.5 w-[70%] rounded-md bg-elevated animate-pulse"/>
                                        <div class="h-3 w-1/2 rounded-md bg-elevated animate-pulse"/>
                                        <div class="h-3 w-[35%] rounded-md bg-elevated animate-pulse"/>
                                    </div>
                                </div>
                            }).collect_view()}
                        </div>
                    }.into_any();
                }

                let items = items_sig.get();

                if items.is_empty() {
                    return view! {
                        <div class="flex flex-col items-center justify-center gap-4 px-5 py-16 text-center">
                            <div class="flex items-center justify-center w-16 h-16 rounded-full bg-card
                                        border border-solid border-line text-content-muted">
                                <svg width="28" height="28" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                    <circle cx="9" cy="21" r="1"/>
                                    <circle cx="20" cy="21" r="1"/>
                                    <path d="M1 1h4l2.68 13.39a2 2 0 002 1.61h9.72a2 2 0 002-1.61L23 6H6"/>
                                </svg>
                            </div>
                            <p class="text-sm text-content-muted">"Belum ada barang yang dipilih."</p>
                            <A href="/explore" attr:class=BTN_PRIMARY>"JELAJAHI PRODUK"</A>
                        </div>
                    }.into_any();
                }

                view! {
                    <div>
                        // ── Pilih semua ─────────────────────────────────────
                        <div class=format!("{CARD} mx-5 mt-6 mb-3 flex items-center justify-between gap-3 px-4 py-3")>
                            <label class="inline-flex items-center gap-2.5 cursor-pointer select-none">
                                <input type="checkbox"
                                    class="sr-only"
                                    prop:checked=move || all_selected.get()
                                    on:change=move |_| cart.select_all(!all_selected.get_untracked()) />
                                {move || check_box(all_selected.get())}
                                <span class="font-sans text-xs font-bold tracking-[0.04em] text-content">
                                    "Pilih semua"
                                </span>
                            </label>
                            <span class="text-[11px] text-content-soft whitespace-nowrap">
                                {move || format!("{} dari {} dipilih", selected_count.get(), total_count.get())}
                            </span>
                        </div>

                        // ── Daftar barang ───────────────────────────────────
                        <div class="flex flex-col gap-3 px-5">
                            {items.iter().map(|item| cart_row(cart, item.clone())).collect_view()}
                        </div>

                        // ── Ringkasan ───────────────────────────────────────
                        <div class=format!("{CARD} m-5 p-5")>
                            <div class="font-sans text-[10px] text-content-muted tracking-[0.12em] mb-4">
                                "RINCIAN"
                            </div>

                            {move || items_sig.get().iter().filter(|i| i.selected).map(|item| view! {
                                <div class="flex justify-between gap-3 text-[13px] text-content-soft mb-2">
                                    <span class="truncate">
                                        {format!("{}× {}", item.quantity, item.tier_name)}
                                    </span>
                                    <span class="text-content shrink-0">{price_label(item.subtotal)}</span>
                                </div>
                            }).collect_view()}

                            <div class="h-px bg-line my-4"></div>

                            <div class="flex justify-between text-[13px] text-content-soft mb-2">
                                <span>"Subtotal"</span>
                                <span class="text-content">
                                    {move || price_label(summary.with(|s| s.subtotal))}
                                </span>
                            </div>

                            {move || {
                                let (disc, code) = summary.with(|s| (s.discount, s.promo_code.clone()));
                                (disc > 0).then(|| view! {
                                    <div class="flex justify-between text-[13px] text-promo mb-2">
                                        <span>{format!("Promo {}", code.unwrap_or_default())}</span>
                                        <span>{format!("−{}", format_idr(disc))}</span>
                                    </div>
                                })
                            }}

                            <div class="h-px bg-line my-4"></div>

                            <div class="flex justify-between items-baseline gap-3">
                                <span class="font-sans text-[11px] text-content-muted tracking-[0.08em]">
                                    {move || format!("TOTAL ({} item)", selected_count.get())}
                                </span>
                                <span class="font-title text-3xl text-brand">
                                    {move || price_label(summary.with(|s| s.total))}
                                </span>
                            </div>

                            <p class="mt-3 text-[11px] leading-relaxed text-content-muted">
                                "Biaya layanan mengikuti metode pembayaran yang dipilih di langkah berikutnya."
                            </p>
                        </div>

                        // ── Bilah bayar ─────────────────────────────────────
                        <div class="fixed bottom-0 left-1/2 -translate-x-1/2 z-50 \
                                    w-full max-w-[480px] flex items-center justify-between gap-4 \
                                    px-5 pt-3.5 pb-[calc(18px+env(safe-area-inset-bottom,8px))] \
                                    bg-overlay backdrop-blur-xl border-t border-solid border-line">
                            <div class="flex-1 min-w-0">
                                <span class="block font-sans text-[9px] text-content-muted tracking-[0.12em]">
                                    "TOTAL"
                                </span>
                                <div class="font-title text-xl text-brand truncate">
                                    {move || price_label(summary.with(|s| s.total))}
                                </div>
                            </div>
                            <button
                                class=move || {
                                    if blocked.get() {
                                        format!("{BTN_PRIMARY} opacity-50")
                                    } else {
                                        BTN_PRIMARY.to_string()
                                    }
                                }
                                on:click=on_proceed.clone()>
                                "LANJUT BAYAR"
                                <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
                                     stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                                    <line x1="5" y1="12" x2="19" y2="12"/>
                                    <polyline points="12 5 19 12 12 19"/>
                                </svg>
                            </button>
                        </div>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

/// Kotak centang.
///
/// `<input>` aslinya TETAP ada di DOM dengan `sr-only` — disembunyikan secara
/// visual tapi tetap bisa difokus keyboard dan terbaca pembaca layar sebagai
/// checkbox. Yang digambar hanyalah tampilannya.
///
/// Tanda centang dirender BERSYARAT dari keadaan, bukan lewat `peer-checked:`.
/// Varian `peer-*` hanya berlaku untuk saudara sekandung, sedangkan tanda
/// centang berada DI DALAM kotaknya — ia tak pernah menyala. Bug itu tak
/// terlihat saat compile dan tak terlihat pula di CSS hasil build.
fn check_box(checked: bool) -> impl IntoView {
    let kelas = if checked {
        "flex items-center justify-center w-5 h-5 shrink-0 rounded-md \
         border-2 border-solid bg-brand border-brand text-on-brand transition-colors"
    } else {
        "flex items-center justify-center w-5 h-5 shrink-0 rounded-md \
         border-2 border-solid bg-surface border-line-strong text-transparent transition-colors"
    };
    view! {
        <span class=kelas>
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                 stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round">
                <polyline points="20 6 9 17 4 12"/>
            </svg>
        </span>
    }
}

/// Satu baris keranjang.
fn cart_row(cart: CartContext, item: CartItemView) -> impl IntoView {
    let tier_remove = item.tier_id.clone();
    let tier_minus = item.tier_id.clone();
    let tier_plus = item.tier_id.clone();
    let tier_check = item.tier_id.clone();
    let qty = item.quantity;
    let available = item.available;
    let selected = item.selected;
    let warn = item.exceeds_stock;

    let img = if item.event_cover.is_empty() {
        "https://images.unsplash.com/photo-1470225620780-dba8ba36b745?w=200&q=80".to_string()
    } else {
        item.event_cover.clone()
    };

    // Batas tombol "+": SISA STOK, titik.
    //
    // Sebelumnya `max_per_order` ikut membatasi, dan itu keliru di dua sisi.
    //
    // Pertama, tak ada yang bisa mengaturnya: formulir merchant selalu
    // mengirim `max_per_order: None`, jadi angkanya hanya bisa datang dari
    // baris lama atau seed. Yang tersisa adalah plafon yang membatasi pembeli
    // tanpa satu pun orang di aplikasi ini bisa melihat atau mengubahnya.
    //
    // Kedua, server tidak pernah menegakkannya. `CartService::add` dan
    // `update_quantity` hanya memeriksa minimal 1 dan ketersediaan stok. Jadi
    // plafon ini murni menahan tombol di layar, bukan menjaga aturan apa pun —
    // pembeli yang lewat REST sudah bebas melampauinya sejak dulu.
    let ceiling = available;

    // Rangkaian kelas ditulis LENGKAP dan literal, lalu dipilih salah satu —
    // bukan dirakit potong-potong. Pemindai Tailwind hanya melihat teks apa
    // adanya di berkas ini.
    let tone = if warn {
        "bg-[color-mix(in_srgb,var(--warning-amber)_7%,var(--bg-card))] border-warning"
    } else {
        "bg-card border-line-soft"
    };
    let dim = if selected { "" } else { "opacity-60" };
    let row_class = format!(
        "flex items-start gap-3 p-4 rounded-2xl border border-solid transition-colors {tone} {dim}"
    );

    let qty_btn = "flex items-center justify-center w-8 h-8 shrink-0 rounded-lg border-0 \
                   cursor-pointer text-base leading-none transition-colors";

    view! {
        <div class=row_class>
            // Kotak centang
            <label class="inline-flex items-center pt-1 cursor-pointer select-none">
                <input type="checkbox"
                    class="sr-only"
                    prop:checked=selected
                    on:change=move |_| cart.toggle_selected(&tier_check, !selected) />
                {check_box(selected)}
            </label>

            <img src=img alt=item.event_title.clone()
                 class="w-[72px] h-[72px] shrink-0 rounded-xl object-cover"/>

            <div class="flex-1 min-w-0">
                <div class="font-title text-[15px] leading-snug text-content tracking-[0.02em] truncate">
                    {item.event_title.clone()}
                </div>
                <div class="mt-0.5 text-[11px] text-content-soft truncate">
                    {item.tier_name.clone()}
                </div>
                <div class="text-[10px] text-content-muted truncate">
                    {item.venue_name.clone()}
                </div>
                <div class="mt-1 font-sans text-sm font-bold text-brand">
                    {price_label(item.unit_price)}
                </div>

                {(item.price_changed && item.unit_price_snapshot > 0).then(|| {
                    let naik = item.unit_price > item.unit_price_snapshot;
                    view! {
                        <div class="mt-1 text-[11px] leading-snug text-content-soft">
                            {if naik { "Harga naik sejak ditambahkan" }
                             else { "Harga turun sejak ditambahkan" }}
                            " ("{format_idr(item.unit_price_snapshot)}")"
                        </div>
                    }
                })}

                {warn.then(|| view! {
                    <div class="mt-1 text-[11px] font-semibold leading-snug text-warning">
                        {format!("Sisa {} stok — kurangi jumlahnya", available.max(0))}
                    </div>
                })}
            </div>

            <div class="flex flex-col items-end gap-2 shrink-0">
                <button
                    class="flex items-center justify-center w-8 h-8 rounded-lg cursor-pointer \
                           text-danger bg-danger/10 border border-solid border-danger/20 \
                           transition-colors hover:bg-danger/20"
                    aria-label="Hapus"
                    on:click=move |_| cart.update_qty(&tier_remove, 0)>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2" stroke-linecap="round">
                        <polyline points="3 6 5 6 21 6"/>
                        <path d="M19 6l-1 14H6L5 6"/>
                        <path d="M10 11v6M14 11v6"/>
                        <path d="M9 6V4h6v2"/>
                    </svg>
                </button>

                // Kontrol jumlah sebagai satu pil, bukan tiga tombol terpisah.
                <div class="flex items-center gap-1 p-1 rounded-xl bg-surface border border-solid border-line">
                    <button class=format!("{qty_btn} bg-elevated text-content hover:bg-elevated-2")
                        on:click=move |_| {
                            let cur = cart.get_qty(&tier_minus);
                            cart.update_qty(&tier_minus, cur - 1);
                        }>"−"</button>
                    <span class="w-7 text-center font-title text-base text-content">{qty}</span>
                    <button class=format!("{qty_btn} bg-brand text-on-brand hover:opacity-90")
                        on:click=move |_| {
                            let cur = cart.get_qty(&tier_plus);
                            if cur < ceiling {
                                cart.update_qty(&tier_plus, cur + 1);
                            }
                        }>"+"</button>
                </div>

                <div class="font-sans text-[13px] font-bold text-content">
                    {price_label(item.subtotal)}
                </div>
            </div>
        </div>
    }
}
