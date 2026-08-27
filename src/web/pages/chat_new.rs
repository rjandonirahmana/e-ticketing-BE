//! chat_new.rs — Percakapan dengan sebuah toko (`/pulse/toko/:merchant_id`).
//!
//! ── KENAPA HALAMAN INI ADA ───────────────────────────────────────────────────
//! Ikon chat di halaman produk menautkan ke sini secara langsung, tanpa memanggil
//! server lebih dulu. Room percakapan TIDAK dibuat saat ikonnya diklik.
//!
//! Membuat lebih awal terasa lebih sederhana, tetapi biayanya ditanggung
//! merchant: setiap orang yang sekadar menekan ikonnya lalu pergi meninggalkan
//! percakapan kosong di inbox. Makin ramai produknya, makin banyak baris kosong
//! yang harus disaring untuk menemukan pertanyaan sungguhan — dan tak satu pun
//! bisa dijelaskan asalnya.
//!
//! Di sini room lahir dari PESAN, bukan dari klik. Konsekuensinya: setiap
//! percakapan yang muncul di inbox dijamin berisi setidaknya satu pesan.
//!
//! Begitu pesan pertama terkirim, halaman berpindah ke `/pulse/{room_id}` yang
//! memegang jalur WebSocket biasa. Halaman ini sengaja TIDAK menduplikasi
//! WebSocket, riwayat, atau paginasi — ia hanya menjembatani keadaan "belum ada
//! room".

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::web::api::{find_chat_with_merchant, send_first_chat_message};
use crate::web::components::ThemeToggle;

#[component]
pub fn ChatNewPage() -> impl IntoView {
    let params = use_params_map();
    let merchant_id = move || params.read().get("merchant_id").unwrap_or_default();

    // Resource read-only: mencari, tidak membuat.
    let room = Resource::new(merchant_id, |id| async move {
        if id.is_empty() {
            return Ok(None);
        }
        find_chat_with_merchant(id).await
    });

    let draft = RwSignal::new(String::new());
    let mengirim = RwSignal::new(false);
    let galat = RwSignal::new(String::new());

    // Percakapan yang ternyata SUDAH ada langsung dialihkan ke jalur biasa.
    //
    // Pengalihannya di dalam `Effect`, bukan saat render: menavigasi di tengah
    // pembangunan view membuat router membatalkan pemasangan rute yang sedang
    // berjalan, dan halaman berhenti di layar lama tanpa pesan apa pun. Catatan
    // panjangnya ada di `web/app/guards.rs`.
    {
        let nav = use_navigate();
        Effect::new(move |sudah: Option<()>| {
            if sudah.is_some() {
                return;
            }
            if let Some(Ok(Some(id))) = room.get() {
                nav(
                    &format!("/pulse/{id}"),
                    leptos_router::NavigateOptions {
                        // `replace`: halaman jembatan ini tak boleh masuk
                        // riwayat, kalau tidak tombol Back dari ruang chat
                        // mendarat di sini lalu terlempar maju lagi.
                        replace: true,
                        ..Default::default()
                    },
                );
            }
        });
    }

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
            mengirim.set(true);
            galat.set(String::new());
            let mid = merchant_id();
            let nav = nav.clone();
            leptos::task::spawn_local(async move {
                match send_first_chat_message(mid, isi).await {
                    Ok(room_id) => {
                        draft.set(String::new());
                        nav(
                            &format!("/pulse/{room_id}"),
                            leptos_router::NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    }
                    // Pesannya SENGAJA tidak dikosongkan saat gagal — mengetik
                    // ulang kalimat yang hilang karena jaringan putus adalah
                    // kehilangan yang paling mudah dihindari.
                    Err(e) => galat.set(format!("Gagal mengirim: {e}")),
                }
                mengirim.set(false);
            });
        }
    };

    view! {
        <div class="min-h-screen bg-page flex flex-col">
            <header class="sticky top-0 z-40 flex items-center justify-between gap-3 \
                           px-4 py-3.5 bg-page border-b border-solid border-line-soft">
                <A
                    href="/pulse"
                    attr:class="flex items-center justify-center w-9 h-9 shrink-0 rounded-full \
                                bg-card border border-solid border-line text-content \
                                transition-colors hover:bg-card-hover active:scale-95"
                    attr:aria-label="Kembali ke pesan"
                >
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </A>
                <span class="font-title text-lg tracking-[0.12em] text-content">"CHAT TOKO"</span>
                <ThemeToggle />
            </header>

            // Penanti pindah ke SINI, bersama penantiannya: ikon di halaman
            // produk kini tautan biasa yang tak menunggu apa pun, sedangkan
            // yang benar-benar butuh waktu adalah pencarian percakapan di sini.
            <Suspense fallback=|| view! {
                <div class="flex flex-1 items-center justify-center">
                    <svg class="animate-spin w-8 h-8 text-brand" viewBox="0 0 32 32"
                         fill="none" aria-label="Memuat percakapan">
                        <circle cx="16" cy="16" r="14" stroke="currentColor"
                                stroke-width="2" stroke-linecap="round"
                                stroke-dasharray="66 88" />
                    </svg>
                </div>
            }>
                {move || {
                    room.get()
                        .map(|hasil| match hasil {
                            Err(_) => view! {
                                <div class="flex flex-1 flex-col items-center justify-center gap-3 px-5 text-center">
                                    <p class="text-sm text-content-muted">
                                        "Gagal membuka percakapan. Coba muat ulang halaman."
                                    </p>
                                </div>
                            }.into_any(),
                            // Sudah ada: Effect di atas sedang mengalihkan.
                            // Yang tampil sekejap hanyalah pemutar yang sama,
                            // bukan kedipan halaman kosong.
                            Ok(Some(_)) => view! {
                                <div class="flex flex-1 items-center justify-center">
                                    <svg class="animate-spin w-8 h-8 text-brand" viewBox="0 0 32 32"
                                         fill="none" aria-hidden="true">
                                        <circle cx="16" cy="16" r="14" stroke="currentColor"
                                                stroke-width="2" stroke-linecap="round"
                                                stroke-dasharray="66 88" />
                                    </svg>
                                </div>
                            }.into_any(),
                            Ok(None) => view! {
                                <div class="flex flex-1 flex-col items-center justify-center gap-3 px-8 text-center">
                                    <div class="flex items-center justify-center w-16 h-16 rounded-full \
                                                bg-card border border-solid border-line text-content-muted">
                                        <svg width="28" height="28" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="1.5" stroke-linecap="round"
                                             stroke-linejoin="round">
                                            <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
                                        </svg>
                                    </div>
                                    <p class="text-sm text-content-muted">
                                        "Mulai percakapan dengan toko ini."
                                    </p>
                                    <p class="text-[11px] text-content-muted">
                                        "Tanya stok, ukuran, atau kapan barang bisa diambil."
                                    </p>
                                </div>
                            }.into_any(),
                        })
                }}
            </Suspense>

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
                    placeholder="Tulis pesan…"
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
