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

/// Rujukan produk dalam pesan lama tampil sebagai JUDULNYA saja.
///
/// Pesan pertama dari halaman produk berbentuk `[Judul] /products/slug\nisi`.
/// Kartu penuh seperti di ruang obrolan justru menutupi percakapan di pratinjau
/// sesempit ini, dan alamat mentahnya lebih buruk lagi — dua puluh empat
/// karakter acak yang harus dilompati mata untuk sampai ke kalimatnya.
fn ringkas_pesan(teks: &str) -> String {
    let Some(sisa) = teks.strip_prefix('[') else {
        return teks.to_string();
    };
    let Some(mulai) = sisa.find("/products/") else {
        return teks.to_string();
    };
    let Some(tutup) = sisa[..mulai].rfind(']') else {
        return teks.to_string();
    };
    let judul = sisa[..tutup].trim();
    // Sesudah slug: sisa kalimatnya.
    let alamat = &sisa[mulai + "/products/".len()..];
    let batas = alamat.find(char::is_whitespace).unwrap_or(alamat.len());
    let isi = alamat[batas..].trim();
    if isi.is_empty() {
        format!("[{judul}]")
    } else {
        format!("[{judul}] {isi}")
    }
}

#[component]
pub fn ChatNewPage() -> impl IntoView {
    let auth = use_context::<crate::web::app::AuthResource>();
    let current_user_id = move || {
        auth.and_then(|a| a.get())
            .and_then(|r| r.ok())
            .flatten()
            .map(|u| u.id)
    };
    let params = use_params_map();
    let query = use_query_map();
    let merchant_id = move || params.read().get("merchant_id").unwrap_or_default();

    // Konteks produk datang lewat query, BUKAN lewat permintaan tambahan.
    // Halaman produk sudah memegang judul dan slug-nya, jadi meneruskannya di
    // URL berbiaya nol — sedangkan mengambilnya ulang di sini berarti satu
    // perjalanan bolak-balik lagi untuk data yang baru saja ada di layar.
    let produk_slug = move || query.read().get("product").unwrap_or_default();
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

    // ── Percakapan yang sudah ada ─────────────────────────────────────────
    // Berjalan BERDAMPINGAN dengan kotak ketik, tidak menghalanginya.
    //
    // Komentar di kepala berkas ini menolak "mencari room lebih dulu", dan
    // alasannya benar — tapi ia berlaku untuk pencarian yang MENGHALANGI:
    // dua perjalanan berurutan sebelum satu huruf pun bisa diketik. Yang ini
    // tidak menahan apa pun; kotak ketiknya tetap tampil seketika dan riwayat
    // menyusul bila memang ada.
    //
    // Tanpa ini, pembeli yang sudah pernah bicara dengan toko itu melihat layar
    // kosong dan mengulang pertanyaan yang sudah dijawab — sementara merchant
    // menerima pertanyaan yang tampak datang dari orang asing, padahal
    // percakapannya masih hidup beberapa baris di bawah.
    let riwayat = Resource::new(merchant_id, |id| async move {
        if id.is_empty() {
            return None;
        }
        crate::web::api::cari_chat_merchant(id).await.ok().flatten()
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
                // Riwayat bila ada; bila tidak, ajakan yang lama. Keduanya tak
                // pernah tampil bersamaan — orang yang percakapannya sudah
                // berjalan tak perlu diberi tahu apa yang boleh ditanyakan.
                <Suspense fallback=|| view! {
                    <p class="text-[12px] text-content-muted">
                        "Tanya stok, ukuran, ongkir, atau kapan barang bisa diambil."
                    </p>
                }>
                    {move || match riwayat.get().flatten() {
                        None => view! {
                            <p class="text-[12px] text-content-muted">
                                "Tanya stok, ukuran, ongkir, atau kapan barang bisa diambil."
                            </p>
                        }.into_any(),
                        Some((room_id, pesan)) => {
                            let me = current_user_id().unwrap_or_default();
                            view! {
                                <div class="mb-3 flex items-center justify-between gap-3">
                                    <span class="text-[10px] tracking-[0.08em] text-content-muted">
                                        "PERCAKAPAN SEBELUMNYA"
                                    </span>
                                    // Pratinjau ini sengaja pendek. Yang ingin
                                    // dilihat orang adalah "sampai mana tadi",
                                    // dan untuk membaca seluruhnya ada ruang
                                    // obrolan yang memang dibuat untuk itu.
                                    <A
                                        href=format!("/pulse/{room_id}")
                                        attr:class="text-[11px] font-semibold text-brand no-underline"
                                    >
                                        "Buka semua"
                                    </A>
                                </div>
                                <div class="flex flex-col gap-1.5">
                                    {pesan.into_iter().map(|m| {
                                        let sendiri = m.sender_id == me;
                                        let kelas = if sendiri {
                                            "self-end max-w-[85%] rounded-2xl px-3 py-2 \
                                             bg-brand text-white text-[13px] leading-snug"
                                        } else {
                                            "self-start max-w-[85%] rounded-2xl px-3 py-2 \
                                             bg-card border border-solid border-line-soft \
                                             text-content text-[13px] leading-snug"
                                        };
                                        // Rujukan produk pada pesan lama tampil
                                        // sebagai judulnya saja — kartu penuh di
                                        // pratinjau sesempit ini justru menutupi
                                        // percakapannya.
                                        let teks = ringkas_pesan(&m.content);
                                        view! { <span class=kelas>{teks}</span> }
                                    }).collect_view()}
                                </div>
                            }.into_any()
                        }
                    }}
                </Suspense>
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
