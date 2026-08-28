//! chat_new.rs — Tulis pertanyaan ke sebuah toko (`/pulse/toko/:merchant_id`).
//!
//! Menerima konteks produk lewat query: `?produk=<slug>&judul=<nama>`.
//!
//! ── KENAPA HALAMAN INI TIDAK MENCARI ROOM LEBIH DULU ─────────────────────────
//! Versi pertama halaman ini mencari percakapan yang sudah ada saat dimuat,
//! lalu — bila ketemu — mengalihkan ke `/pulse/{id}`
//! yang memuat riwayat dan menyambung WebSocket.
//!
//! Itu DUA perjalanan bolak-balik yang berurutan sebelum satu huruf pun bisa
//! diketik, dan yang kedua baru dimulai setelah yang pertama selesai. Untuk
//! layar yang isinya cuma "siapa tokonya" dan "apa yang mau ditanyakan", itu
//! penantian yang tak dibayar apa pun.
//!
//! Sekarang: kotak ketik tampil SEKETIKA. Yang menyusul hanya kepala toko
//! (nama, foto, rating), dan itu pun tak menghalangi mengetik.
//!
//! ── ROOM LAHIR DARI PESAN ────────────────────────────────────────────────────
//! Tak ada apa pun yang ditulis ke basis data sampai tombol kirim ditekan. Yang
//! membuka halaman ini lalu berubah pikiran tidak meninggalkan jejak — inbox
//! merchant karena itu hanya berisi percakapan yang benar-benar punya pesan.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map, use_query_map};

use crate::web::api::{get_merchant_public_profile, send_first_chat_message};
use crate::web::components::{IconBack, ThemeToggle};

#[component]
pub fn ChatNewPage() -> impl IntoView {
    let params = use_params_map();
    let query = use_query_map();
    let merchant_id = move || params.read().get("merchant_id").unwrap_or_default();

    // Konteks produk datang lewat query, BUKAN lewat permintaan tambahan.
    // Halaman produk sudah memegang judul dan slug-nya, jadi meneruskannya di
    // URL berbiaya nol — sedangkan mengambilnya ulang di sini berarti satu
    // perjalanan bolak-balik lagi untuk data yang baru saja ada di layar.
    let produk_slug = move || query.read().get("produk").unwrap_or_default();
    let produk_judul = move || query.read().get("judul").unwrap_or_default();
    let produk_sampul = move || query.read().get("sampul").unwrap_or_default();

    // Kepala toko: SATU permintaan, dan ia tak menghalangi kotak ketik.
    // `new_blocking` supaya kunjungan langsung (atau muat ulang) sudah membawa
    // datanya di HTML pertama — bukan kerangka yang berkedip.
    let toko = Resource::new_blocking(merchant_id, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("merchant kosong".into()));
        }
        get_merchant_public_profile(id).await
    });

    let draft = RwSignal::new(String::new());
    let mengirim = RwSignal::new(false);
    let galat = RwSignal::new(String::new());

    let kirim = {
        let nav = use_navigate();
        move |_| {
            if mengirim.get_untracked() {
                return;
            }
            let isi = draft.get_untracked().trim().to_string();
            if isi.is_empty() {
                return;
            }

            // Konteks produk ikut dikirim sebagai bagian pesan. Merchant yang
            // menjual puluhan barang tak bisa menebak pertanyaan ini tentang
            // yang mana, dan pembeli hampir tak pernah menyebutkannya sendiri.
            let judul = produk_judul();
            let slug = produk_slug();
            let pesan = if judul.is_empty() {
                isi
            } else if slug.is_empty() {
                format!("[{judul}]\n{isi}")
            } else {
                format!("[{judul}] /products/{slug}\n{isi}")
            };

            mengirim.set(true);
            galat.set(String::new());
            let mid = merchant_id();
            let nav = nav.clone();
            leptos::task::spawn_local(async move {
                match send_first_chat_message(mid, pesan).await {
                    Ok(room_id) => {
                        draft.set(String::new());
                        nav(
                            &format!("/pulse/{room_id}"),
                            leptos_router::NavigateOptions {
                                // Halaman tulis ini tak boleh masuk riwayat:
                                // tombol Back dari ruang chat harus kembali ke
                                // produk, bukan ke kotak ketik kosong.
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                    // Isi kolom SENGAJA tidak dikosongkan saat gagal —
                    // mengetik ulang kalimat yang hilang karena jaringan putus
                    // adalah kehilangan yang paling mudah dihindari.
                    Err(e) => galat.set(format!("Gagal mengirim: {e}")),
                }
                mengirim.set(false);
            });
        }
    };

    view! {
        <div class="min-h-screen bg-page flex flex-col">
            // ── Kepala: identitas toko ──────────────────────────────────────
            // Dulu judulnya "TANYA TOKO" (generik) DAN ada kartu toko terpisah di
            // bawahnya — nama tokonya karena itu muncul dua kali di satu layar
            // setinggi 200px. Digabung jadi satu kepala bergaya ruang obrolan:
            // avatar, nama, rating. Itu juga yang membebaskan ruang untuk foto
            // produk yang ditanyakan.
            //
            // `no-underline` diperlukan: CSS lama menyetel garis bawah pada `a`
            // polos, dan sejak layer `legacy` berada di bawah utilities, utility
            // inilah yang menang. Tanpanya seluruh baris — termasuk teks rating —
            // tergaris bawah seperti tautan mentah.
            <header class="sticky top-0 z-40 flex items-center gap-3 \
                           px-4 py-3 bg-page border-b border-solid border-line-soft">
                <A
                    href="/pulse"
                    attr:class="flex items-center justify-center w-9 h-9 shrink-0 rounded-full \
                                bg-card border border-solid border-line text-content no-underline \
                                transition-colors hover:bg-card-hover active:scale-95"
                    attr:aria-label="Kembali"
                >
                    <IconBack />
                </A>

                <Suspense fallback=|| view! {
                    <div class="flex flex-1 items-center gap-2.5 min-w-0">
                        <div class="w-9 h-9 shrink-0 rounded-full bg-elevated animate-pulse"/>
                        <div class="h-3.5 w-28 rounded-md bg-elevated animate-pulse"/>
                    </div>
                }>
                    {move || {
                        toko.get().map(|hasil| match hasil {
                            Err(_) => view! {
                                <span class="flex-1 min-w-0 truncate font-title text-base \
                                             tracking-[0.06em] text-content">
                                    "Tanya Toko"
                                </span>
                            }.into_any(),
                            Ok(m) => {
                                let logo = m.logo_url.clone().unwrap_or_default();
                                let nama = m.store_name.clone();
                                let jml = m.rating_count;
                                let rating = m.rating_avg;
                                let verified = m.verified;
                                view! {
                                    <A
                                        href=format!("/m/{}", m.merchant_id)
                                        attr:class="flex flex-1 items-center gap-2.5 min-w-0 no-underline"
                                    >
                                        {if logo.is_empty() {
                                            view! { <div class="w-9 h-9 shrink-0 rounded-full bg-elevated"/> }.into_any()
                                        } else {
                                            view! {
                                                <img src=logo alt=nama.clone() loading="lazy"
                                                     class="w-9 h-9 shrink-0 rounded-full object-cover"/>
                                            }.into_any()
                                        }}
                                        <span class="flex flex-col min-w-0">
                                            <span class="flex items-center gap-1.5 min-w-0">
                                                <span class="truncate font-sans text-sm font-bold text-content">
                                                    {nama}
                                                </span>
                                                {verified.then(|| view! {
                                                    <svg class="shrink-0 text-brand" width="13" height="13"
                                                         viewBox="0 0 24 24" fill="currentColor"
                                                         aria-label="Terverifikasi">
                                                        <path d="M12 2l2.4 2.1 3.2-.4 1 3 2.8 1.6-1.3 2.9 1.3 2.9-2.8 1.6-1 3-3.2-.4L12 22l-2.4-2.1-3.2.4-1-3L2.6 15.7 3.9 12.8 2.6 9.9l2.8-1.6 1-3 3.2.4z"/>
                                                    </svg>
                                                })}
                                            </span>
                                            <span class="text-[11px] text-content-muted">
                                                {if jml == 0 {
                                                    "Belum ada ulasan".to_string()
                                                } else {
                                                    format!("★ {rating:.1} · {jml} ulasan")
                                                }}
                                            </span>
                                        </span>
                                    </A>
                                }.into_any()
                            }
                        })
                    }}
                </Suspense>

                <ThemeToggle />
            </header>

            // ── Produk yang ditanyakan ─────────────────────────────────────
            // Foto ikut ditampilkan, dan itu bukan hiasan: merchant yang menjual
            // puluhan barang mengenali produknya dari gambar jauh lebih cepat
            // daripada dari nama, dan pembeli jadi yakin ia menanyakan yang benar
            // sebelum menekan kirim.
            //
            // Sumbernya dari query — halaman produk sudah memegangnya, jadi tak
            // ada permintaan tambahan sama sekali.
            {move || {
                let judul = produk_judul();
                (!judul.is_empty()).then(|| {
                    let slug = produk_slug();
                    let sampul = produk_sampul();
                    let href = if slug.is_empty() {
                        String::new()
                    } else {
                        format!("/products/{slug}")
                    };
                    let isi = view! {
                        {if sampul.is_empty() {
                            view! {
                                <div class="w-14 h-14 shrink-0 rounded-xl bg-elevated"/>
                            }.into_any()
                        } else {
                            view! {
                                <img
                                    src=sampul
                                    alt=judul.clone()
                                    loading="lazy"
                                    decoding="async"
                                    class="w-14 h-14 shrink-0 rounded-xl object-cover"
                                />
                            }.into_any()
                        }}
                        <span class="flex flex-col min-w-0">
                            <span class="text-[10px] tracking-[0.08em] text-content-muted">
                                "MENANYAKAN"
                            </span>
                            <span class="truncate text-[13px] font-semibold text-content">
                                {judul}
                            </span>
                        </span>
                    };
                    // Kartu jadi TAUTAN hanya bila slug-nya ada. Elemen yang
                    // terlihat bisa diklik tapi tak menuju ke mana pun selalu
                    // terbaca sebagai rusak.
                    if href.is_empty() {
                        view! {
                            <div class="mx-5 mt-4 flex items-center gap-3 p-3 rounded-2xl \
                                        bg-card border border-solid border-line-soft">
                                {isi}
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <A
                                href=href
                                attr:class="mx-5 mt-4 flex items-center gap-3 p-3 rounded-2xl \
                                            bg-card border border-solid border-line-soft no-underline \
                                            transition-colors hover:bg-card-hover"
                            >
                                {isi}
                            </A>
                        }.into_any()
                    }
                })
            }}

            <div class="flex-1 px-5 pt-4">
                <p class="text-[12px] text-content-muted">
                    "Tanya stok, ukuran, ongkir, atau kapan barang bisa diambil."
                </p>
            </div>

            {move || {
                let pesan = galat.get();
                (!pesan.is_empty()).then(|| view! {
                    <p class="px-5 pb-2 text-[12px] text-danger">{pesan}</p>
                })
            }}

            // ── Kolom ketik ─────────────────────────────────────────────────
            <div class="sticky bottom-0 flex items-end gap-2 px-4 \
                        pt-3 pb-[calc(12px+env(safe-area-inset-bottom,8px))] \
                        bg-page border-t border-solid border-line-soft">
                <textarea
                    class="flex-1 min-h-11 max-h-32 px-3.5 py-2.5 rounded-2xl resize-none \
                           bg-card border border-solid border-line text-content \
                           text-sm placeholder:text-content-muted"
                    rows="1"
                    placeholder="Tulis pertanyaan…"
                    prop:value=move || draft.get()
                    on:input=move |e| draft.set(event_target_value(&e))
                />
                <button
                    class="inline-flex items-center justify-center w-11 h-11 shrink-0 rounded-full \
                           bg-brand text-on-brand border-0 cursor-pointer \
                           transition-opacity hover:opacity-90 disabled:opacity-50"
                    disabled=move || mengirim.get() || draft.get().trim().is_empty()
                    aria-label="Kirim"
                    on:click=kirim
                >
                    {move || if mengirim.get() {
                        view! {
                            <svg class="animate-spin w-5 h-5" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                                <circle cx="12" cy="12" r="10" stroke="currentColor"
                                        stroke-width="3" opacity="0.3" />
                                <path d="M22 12a10 10 0 0 0-10-10" stroke="currentColor"
                                      stroke-width="3" stroke-linecap="round" />
                            </svg>
                        }.into_any()
                    } else {
                        view! {
                            <svg width="19" height="19" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2" stroke-linecap="round"
                                 stroke-linejoin="round">
                                <line x1="22" y1="2" x2="11" y2="13" />
                                <polygon points="22 2 15 22 11 13 2 9 22 2" />
                            </svg>
                        }.into_any()
                    }}
                </button>
            </div>
        </div>
    }
}
