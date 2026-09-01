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
use crate::web::components::{gambar_cadangan, CartButton, ThemeToggle};
use crate::web::models::CartItemView;
use crate::web::utils::format_idr;

/// Satu toko beserta barangnya di keranjang.
struct KelompokToko {
    merchant_id: String,
    merchant_name: String,
    items: Vec<CartItemView>,
}

/// Kelompokkan isi keranjang per toko, mempertahankan urutan KEMUNCULAN
/// PERTAMA tiap toko.
///
/// Urutan itu disengaja dan bukan sekadar kebetulan implementasi: server sudah
/// mengurutkan barang per merchant lalu per waktu ditambahkan, dan
/// mempertahankan urutan kemunculan membuat susunan di layar sama persis dengan
/// urutan itu. Mengurutkan ulang berdasarkan nama toko akan membuat keranjang
/// tampak "melompat" setiap kali satu barang ditambah atau dihapus, karena toko
/// bisa berpindah posisi tanpa ada yang menyentuhnya.
///
/// Barang tanpa `merchant_id` — isi keranjang TAMU, yang di localStorage tak
/// menyimpan pemilik product — dikumpulkan jadi satu kelompok tanpa nama, bukan
/// dibuang. Kalau dibuang, pembeli yang belum masuk akan melihat keranjangnya
/// kosong padahal barangnya ada.
/// Baris judul satu kelompok toko.
///
/// Ditautkan ke `/m/{merchant_id}` HANYA bila id-nya ada. Barang keranjang tamu
/// tak membawa id, dan menautkannya tetap akan menghasilkan `/m/` — tautan yang
/// terlihat sah, bisa diklik, dan mendarat di halaman kosong. Lebih baik teks
/// biasa daripada tautan yang berbohong.
fn kepala_toko(
    cart: CartContext,
    merchant_id: &str,
    merchant_name: &str,
    items: &[CartItemView],
) -> impl IntoView {
    let jumlah = items.len();
    // Tier id seluruh barang toko ini — dikirim sekali sebagai satu kelompok.
    let tier_ids: Vec<String> = items.iter().map(|i| i.tier_id.clone()).collect();
    // Tercentang HANYA bila SEMUA barang toko ini tercentang. Sebagian
    // tercentang sengaja tampil sebagai TIDAK tercentang, supaya ketukan
    // berikutnya mencentang sisanya — bukan melepas yang sudah dipilih.
    let semua_tercentang = !items.is_empty() && items.iter().all(|i| i.selected);
    // Nama toko kosong = merchant belum melengkapi profilnya (kolom
    // `merchant_details.store_name` boleh kosong), bukan galat.
    let nama = if merchant_name.trim().is_empty() {
        "Toko".to_string()
    } else {
        merchant_name.to_string()
    };
    let hitungan = format!("{jumlah} barang");

    let ikon = || {
        view! {
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                 stroke="currentColor" stroke-width="2" stroke-linecap="round"
                 stroke-linejoin="round" class="shrink-0">
                <path d="M3 9l1-5h16l1 5" />
                <path d="M4 9v11h16V9" />
                <path d="M9 20v-6h6v6" />
            </svg>
        }
    };

    if merchant_id.is_empty() {
        return view! {
            <div class="flex items-center gap-2.5 px-4 py-3 border-b border-solid \
                        border-line-soft text-content-soft">
                {centang_toko(cart, tier_ids.clone(), semua_tercentang)}
                {ikon()}
                <span class="font-sans text-[12px] font-bold truncate">{nama}</span>
                <span class="ml-auto text-[10px] text-content-muted whitespace-nowrap">
                    {hitungan}
                </span>
            </div>
        }
        .into_any();
    }

    // Kotak centang berada DI LUAR `<A>`, bukan di dalamnya: elemen interaktif
    // bersarang di dalam tautan adalah HTML tak sah, dan akibat nyatanya
    // mengetuk centang akan ikut menavigasi ke halaman toko.
    view! {
        <div class="flex items-center gap-2.5 px-4 py-3 border-b border-solid \
                    border-line-soft">
            {centang_toko(cart, tier_ids.clone(), semua_tercentang)}
            <A
                href=format!("/m/{merchant_id}")
                attr:class="flex items-center gap-2 min-w-0 flex-1 text-content \
                            no-underline transition-colors hover:text-brand"
            >
                {ikon()}
                <span class="font-sans text-[12px] font-bold truncate">{nama}</span>
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none"
                     stroke="currentColor" stroke-width="2.5" stroke-linecap="round"
                     class="shrink-0 opacity-60">
                    <polyline points="9 18 15 12 9 6" />
                </svg>
            </A>
            <span class="text-[10px] text-content-muted whitespace-nowrap">
                {hitungan}
            </span>
        </div>
    }
    .into_any()
}

/// Kotak centang "pilih seluruh barang toko ini".
fn centang_toko(cart: CartContext, tier_ids: Vec<String>, tercentang: bool) -> impl IntoView {
    view! {
        <label
            class="inline-flex items-center cursor-pointer select-none shrink-0"
            attr:aria-label="Pilih semua barang toko ini"
        >
            <input
                type="checkbox"
                class="sr-only"
                prop:checked=tercentang
                on:change=move |_| cart.select_group(tier_ids.clone(), !tercentang)
            />
            {check_box(tercentang)}
        </label>
    }
}

fn kelompokkan_per_toko(items: &[CartItemView]) -> Vec<KelompokToko> {
    let mut out: Vec<KelompokToko> = Vec::new();
    for it in items {
        match out.iter_mut().find(|g| g.merchant_id == it.merchant_id) {
            Some(g) => g.items.push(it.clone()),
            None => out.push(KelompokToko {
                merchant_id: it.merchant_id.clone(),
                merchant_name: it.merchant_name.clone(),
                items: vec![it.clone()],
            }),
        }
    }
    out
}

use crate::web::utils::rupiah_atau_gratis as price_label;

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
                            <A href="/explore" attr:class=BTN_PRIMARY>"JELAJAHI PRODUCT"</A>
                        </div>
                    }.into_any();
                }

                view! {
                    <div>
                        // ── Pilih semua ─────────────────────────────────────
                        // `mb-4` menyamai `gap-4` antar-kartu toko di bawahnya: bilah ini
                        // adalah kartu setara, jadi jaraknya ke kartu pertama harus
                        // sama dengan jarak antar-kartu — kalau tidak, kartu pertama
                        // terlihat menempel lebih rapat tanpa alasan.
                        <div class=format!("{CARD} mx-5 mt-6 mb-4 flex items-center justify-between gap-3 px-4 py-3")>
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

                        // ── Daftar barang, dikelompokkan per toko ───────────
                        //
                        // Jarak HANYA di antara toko yang berbeda. Barang dari
                        // toko yang sama menyatu dalam satu kartu, dipisah garis
                        // rambut — itulah yang membuat "berapa toko yang saya
                        // belanjai" terbaca dalam satu tatapan. Versi sebelumnya
                        // memberi jarak yang sama di antara SEMUA baris, jadi
                        // pengelompokan per toko yang sudah dihitung dengan benar
                        // tak terlihat sama sekali di layar.
                        //
                        // `overflow-hidden` pada kartunya: baris paling atas dan
                        // paling bawah tak punya sudut membulat sendiri, jadi
                        // tanpa ini latar mereka menonjol keluar dari lengkungan
                        // kartu di keempat sudutnya.
                        <div class="flex flex-col gap-4 px-5">
                            {kelompokkan_per_toko(&items)
                                .into_iter()
                                .map(|g| {
                                    view! {
                                        <section class=format!("{CARD} overflow-hidden")>
                                            {kepala_toko(
                                                cart,
                                                &g.merchant_id,
                                                &g.merchant_name,
                                                &g.items,
                                            )}
                                            {g
                                                .items
                                                .into_iter()
                                                .enumerate()
                                                .map(|(i, item)| cart_row(cart, item, i == 0))
                                                .collect_view()}
                                        </section>
                                    }
                                })
                                .collect_view()}
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
fn cart_row(cart: CartContext, item: CartItemView, pertama: bool) -> impl IntoView {
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
    // Latar hanya diberi warna saat MEMPERINGATKAN; selain itu baris mewarisi
    // latar kartu kelompoknya. Garis tepi kuning yang dulu ada ikut dibuang:
    // baris kini menyatu di dalam satu kartu per toko, dan kotak bergaris di
    // tengah tumpukan justru memecah tumpukan itu. Peringatannya tetap terbaca
    // dari rona latar plus teks "Sisa N stok — kurangi jumlahnya" di bawah.
    let tone = if warn {
        "bg-[color-mix(in_srgb,var(--warning-amber)_7%,var(--bg-card))]"
    } else {
        ""
    };
    let dim = if selected { "" } else { "opacity-60" };
    // Pemisah antar-barang SATU TOKO: garis rambut, bukan jarak kosong. Barang
    // dari toko yang sama adalah satu blok belanja; jarak di antaranya membuat
    // masing-masing tampak berdiri sendiri dan justru mengaburkan
    // pengelompokannya. Yang pertama tak diberi garis supaya tak menggandakan
    // garis bawah kepala toko tepat di atasnya.
    let pemisah = if pertama {
        ""
    } else {
        "border-t border-solid border-line-soft"
    };
    let row_class =
        format!("flex items-start gap-3 p-4 transition-colors {tone} {dim} {pemisah}");

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

            <img src=img alt=item.event_title.clone() on:error=gambar_cadangan
                 class="w-[72px] h-[72px] shrink-0 rounded-xl object-cover"/>

            <div class="flex-1 min-w-0">
                // Judul menuju halaman product-nya — tetapi hanya bila slug-nya
                // ada. Baris keranjang TAMU tak membawa slug (localStorage cuma
                // menyimpan secukupnya untuk menggambar baris), dan menautkannya
                // akan menghasilkan `/products/` yang mendarat di halaman kosong.
                {
                    let judul = item.event_title.clone();
                    let slug = item.event_slug.clone();
                    if slug.is_empty() {
                        view! {
                            <div class="font-title text-[15px] leading-snug text-content \
                                        tracking-[0.02em] truncate">
                                {judul}
                            </div>
                        }
                            .into_any()
                    } else {
                        view! {
                            <A
                                href=format!("/products/{slug}")
                                attr:class="block font-title text-[15px] leading-snug text-content \
                                            tracking-[0.02em] truncate no-underline \
                                            transition-colors hover:text-brand"
                            >
                                {judul}
                            </A>
                        }
                            .into_any()
                    }
                }
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

// ─── Uji pengelompokan per toko ───────────────────────────────────────────────
#[cfg(test)]
mod tests_kelompok {
    use super::*;

    fn barang(merchant_id: &str, merchant_name: &str, judul: &str) -> CartItemView {
        CartItemView {
            id: judul.into(),
            tier_id: judul.into(),
            event_id: judul.into(),
            event_slug: judul.into(),
            event_title: judul.into(),
            tier_name: "Reguler".into(),
            venue_name: String::new(),
            event_cover: String::new(),
            event_date: None,
            merchant_id: merchant_id.into(),
            merchant_name: merchant_name.into(),
            quantity: 1,
            unit_price: 1000,
            unit_price_snapshot: 1000,
            subtotal: 1000,
            available: 10,
            max_per_order: None,
            exceeds_stock: false,
            price_changed: false,
            selected: true,
        }
    }

    /// Sepuluh barang dari tiga toko → tiga kelompok, tak ada yang hilang.
    #[test]
    fn sepuluh_barang_tiga_toko() {
        let items: Vec<CartItemView> = (0..10)
            .map(|i| {
                let m = i % 3;
                barang(&format!("m{m}"), &format!("Toko {m}"), &format!("p{i}"))
            })
            .collect();

        let g = kelompokkan_per_toko(&items);
        assert_eq!(g.len(), 3, "tiga toko berbeda");
        assert_eq!(
            g.iter().map(|x| x.items.len()).sum::<usize>(),
            10,
            "tak boleh ada barang yang hilang saat dikelompokkan"
        );
    }

    /// Urutan kelompok mengikuti KEMUNCULAN PERTAMA, bukan abjad. Mengurutkan
    /// ulang membuat keranjang melompat tiap kali satu barang ditambah/dihapus.
    #[test]
    fn urutan_mengikuti_kemunculan_pertama() {
        let items = vec![
            barang("m-z", "Zebra", "a"),
            barang("m-a", "Apel", "b"),
            barang("m-z", "Zebra", "c"),
        ];
        let g = kelompokkan_per_toko(&items);
        assert_eq!(g[0].merchant_id, "m-z", "Zebra muncul lebih dulu");
        assert_eq!(g[1].merchant_id, "m-a");
        assert_eq!(g[0].items.len(), 2, "barang toko yang sama menyatu");
    }

    /// Urutan barang DI DALAM satu toko dipertahankan apa adanya.
    #[test]
    fn urutan_di_dalam_toko_dipertahankan() {
        let items = vec![
            barang("m1", "Satu", "pertama"),
            barang("m2", "Dua", "lain"),
            barang("m1", "Satu", "kedua"),
        ];
        let g = kelompokkan_per_toko(&items);
        let judul: Vec<&str> = g[0].items.iter().map(|i| i.event_title.as_str()).collect();
        assert_eq!(judul, vec!["pertama", "kedua"]);
    }

    /// Keranjang TAMU (tanpa merchant_id) tetap tampil — dikumpulkan jadi satu
    /// kelompok, bukan dibuang. Kalau dibuang, pembeli yang belum masuk melihat
    /// keranjangnya kosong padahal barangnya ada.
    #[test]
    fn barang_tanpa_toko_tetap_tampil() {
        let items = vec![barang("", "", "a"), barang("", "", "b")];
        let g = kelompokkan_per_toko(&items);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].items.len(), 2);
        assert!(g[0].merchant_id.is_empty());
    }

    /// Campuran: sebagian bertoko, sebagian tidak (keranjang tamu yang baru
    /// sebagian tersinkron ke server).
    #[test]
    fn campuran_bertoko_dan_tidak() {
        let items = vec![
            barang("m1", "Satu", "a"),
            barang("", "", "b"),
            barang("m1", "Satu", "c"),
        ];
        let g = kelompokkan_per_toko(&items);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].items.len(), 2);
        assert_eq!(g[1].items.len(), 1);
    }

    /// Keranjang kosong tak menghasilkan kelompok hantu.
    #[test]
    fn keranjang_kosong() {
        assert!(kelompokkan_per_toko(&[]).is_empty());
    }
}
