//! following.rs — Daftar toko yang diikuti pengguna (`/following`).
//!
//! Dijangkau dari kartu "Toko yang Diikuti" di halaman profil. Datanya pribadi:
//! `get_my_following` mengambil identitas dari token, bukan dari parameter, jadi
//! halaman ini tak bisa dipakai membaca daftar milik orang lain.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::get_my_following;
use crate::web::components::{BottomNav, ThemeToggle};

/// Kartu satu toko. Dipakai berulang, jadi rangkaian kelasnya disatukan supaya
/// tak pernah bergeser antar-baris.
const KARTU: &str = "flex items-center gap-3 p-3 rounded-2xl bg-card \
     border border-solid border-line-soft transition-colors hover:bg-card-hover";

#[component]
pub fn FollowingPage() -> impl IntoView {
    // Blocking supaya HTML pertama sudah berisi daftarnya — halaman ini kerap
    // dibuka langsung dari profil dan tak ada gunanya menampilkan kerangka lalu
    // menukarnya sepersekian detik kemudian.
    let data = Resource::new_blocking(|| (), |_| get_my_following(None));

    view! {
        <div class="min-h-screen bg-page pb-24">
            <header class="sticky top-0 z-40 flex items-center justify-between gap-3 \
                           px-4 py-3.5 bg-page border-b border-solid border-line-soft">
                <A
                    href="/profile"
                    attr:class="flex items-center justify-center w-9 h-9 shrink-0 rounded-full \
                                bg-card border border-solid border-line text-content \
                                transition-colors hover:bg-card-hover active:scale-95"
                    attr:aria-label="Kembali ke profil"
                >
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="font-title text-lg tracking-[0.12em] text-content">"TOKO DIIKUTI"</span>
                <ThemeToggle />
            </header>

            <Suspense fallback=|| view! {
                <div class="flex flex-col gap-3 px-5 pt-6">
                    {(0..5).map(|_| view! {
                        <div class=format!("{KARTU} animate-pulse")>
                            <div class="w-12 h-12 shrink-0 rounded-full bg-elevated"/>
                            <div class="flex-1 flex flex-col gap-2">
                                <div class="h-3.5 w-1/2 rounded-md bg-elevated"/>
                                <div class="h-3 w-1/3 rounded-md bg-elevated"/>
                            </div>
                        </div>
                    }).collect_view()}
                </div>
            }>
                {move || {
                    data.get()
                        .map(|hasil| match hasil {
                            Err(_) => view! {
                                <div class="flex flex-col items-center gap-3 px-5 py-16 text-center">
                                    <p class="text-sm text-content-muted">
                                        "Gagal memuat daftar toko. Coba muat ulang halaman."
                                    </p>
                                </div>
                            }.into_any(),
                            Ok(d) if d.items.is_empty() => view! {
                                // Kosong BUKAN galat, dan jalan keluarnya diberikan
                                // di sini juga: halaman kosong tanpa tombol hanya
                                // memberi tahu ada yang tak ada, tanpa memberi tahu
                                // apa yang bisa dilakukan.
                                <div class="flex flex-col items-center justify-center gap-4 px-5 py-16 text-center">
                                    <div class="flex items-center justify-center w-16 h-16 rounded-full \
                                                bg-card border border-solid border-line text-content-muted">
                                        <svg width="28" height="28" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                            <path d="M3 7l1.5-3h15L21 7"/>
                                            <path d="M3 7v13a1 1 0 001 1h16a1 1 0 001-1V7"/>
                                            <path d="M9 11h6"/>
                                        </svg>
                                    </div>
                                    <p class="text-sm text-content-muted">
                                        "Kamu belum mengikuti toko mana pun."
                                    </p>
                                    <A
                                        href="/explore"
                                        attr:class="inline-flex items-center justify-center min-h-11 px-6 \
                                                    rounded-full bg-brand text-on-brand font-sans text-xs \
                                                    font-bold tracking-[0.08em] transition-opacity hover:opacity-90"
                                    >
                                        "JELAJAHI TOKO"
                                    </A>
                                </div>
                            }.into_any(),
                            Ok(d) => {
                                let jumlah = d.total;
                                view! {
                                    <div class="px-5 pt-6">
                                        <p class="mb-3 text-[11px] tracking-[0.08em] text-content-muted">
                                            {format!("{jumlah} TOKO")}
                                        </p>
                                        <div class="flex flex-col gap-3">
                                            {d.items.into_iter().map(|m| {
                                                let href = format!("/m/{}", m.merchant_id);
                                                let logo = m.logo_url.clone();
                                                let nama = m.store_name.clone();
                                                let terverifikasi = m.verified;
                                                view! {
                                                    <A href=href attr:class=KARTU>
                                                        {if logo.is_empty() {
                                                            view! {
                                                                <div class="w-12 h-12 shrink-0 rounded-full bg-elevated"/>
                                                            }.into_any()
                                                        } else {
                                                            view! {
                                                                <img
                                                                    src=logo
                                                                    alt=nama.clone()
                                                                    loading="lazy"
                                                                    decoding="async"
                                                                    class="w-12 h-12 shrink-0 rounded-full object-cover"
                                                                />
                                                            }.into_any()
                                                        }}
                                                        <div class="flex-1 min-w-0">
                                                            <div class="flex items-center gap-1.5">
                                                                <span class="truncate font-sans text-sm font-bold text-content">
                                                                    {nama}
                                                                </span>
                                                                {terverifikasi.then(|| view! {
                                                                    <svg class="shrink-0 text-brand" width="14" height="14"
                                                                         viewBox="0 0 24 24" fill="currentColor"
                                                                         aria-label="Terverifikasi">
                                                                        <path d="M12 2l2.4 2.1 3.2-.4 1 3 2.8 1.6-1.3 2.9 1.3 2.9-2.8 1.6-1 3-3.2-.4L12 22l-2.4-2.1-3.2.4-1-3L2.6 15.7 3.9 12.8 2.6 9.9l2.8-1.6 1-3 3.2.4z"/>
                                                                    </svg>
                                                                })}
                                                            </div>
                                                            <span class="text-[11px] text-content-muted">
                                                                "Lihat toko"
                                                            </span>
                                                        </div>
                                                        <svg class="shrink-0 text-content-muted" width="16" height="16"
                                                             viewBox="0 0 24 24" fill="none" stroke="currentColor"
                                                             stroke-width="2" stroke-linecap="round">
                                                            <polyline points="9 18 15 12 9 6"/>
                                                        </svg>
                                                    </A>
                                                }
                                            }).collect_view()}
                                        </div>
                                    </div>
                                }.into_any()
                            }
                        })
                }}
            </Suspense>

            <BottomNav active="profile" />
        </div>
    }
}
