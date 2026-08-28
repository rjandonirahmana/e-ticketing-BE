//! icons.rs — Satu tempat untuk ikon yang dipakai lebih dari satu halaman.
//!
//! ── KENAPA INI MENGECILKAN WASM ──────────────────────────────────────────────
//! Setiap `<svg>` yang ditulis di dalam `view!` bukan sekadar teks: makro itu
//! menghasilkan KODE pembangun elemen — buat node, pasang tiap atribut, sisipkan
//! ke induknya. Menyalin ikon lonceng ke tujuh berkas berarti tujuh salinan kode
//! itu di dalam bundel, bukan satu string yang dipakai bersama.
//!
//! Dengan satu komponen, tujuh tempat memanggil satu fungsi yang sama.
//!
//! ── KENAPA `ukuran` JADI PARAMETER, BUKAN KELAS ─────────────────────────────
//! Pemanggilnya memakai ukuran yang berbeda-beda (13–28px). Kalau ukurannya
//! dipatok di dalam dan pemanggil menimpanya lewat kelas, dua sumber ukuran
//! saling bertengkar dan hasilnya bergantung pada urutan CSS — persis kelas
//! masalah yang baru saja kita bereskan di layer `legacy`. Satu parameter
//! menghapus pertengkarannya.
//!
//! `stroke-width` sengaja ikut jadi parameter dengan bawaan 2: ikon kecil butuh
//! garis lebih tebal agar tetap terbaca, dan itu keputusan pemanggil.

use leptos::prelude::*;

/// Bel notifikasi. Sebelumnya disalin di 7 berkas.
#[component]
pub fn IconBell(
    #[prop(default = 18)] ukuran: i32,
    #[prop(default = 2.0)] tebal: f64,
) -> impl IntoView {
    view! {
        <svg width=ukuran height=ukuran viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width=tebal stroke-linecap="round"
             stroke-linejoin="round" aria-hidden="true">
            <path d="M18 8A6 6 0 006 8c0 7-3 9-3 9h18s-3-2-3-9" />
            <path d="M13.73 21a2 2 0 01-3.46 0" />
        </svg>
    }
}

/// Perisai — dipakai untuk admin dan lencana terverifikasi. Sebelumnya 6 salinan.
#[component]
pub fn IconShield(
    #[prop(default = 18)] ukuran: i32,
    #[prop(default = 2.0)] tebal: f64,
) -> impl IntoView {
    view! {
        <svg width=ukuran height=ukuran viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width=tebal stroke-linecap="round"
             stroke-linejoin="round" aria-hidden="true">
            <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
        </svg>
    }
}

/// Gelembung obrolan. Sebelumnya 4 salinan.
#[component]
pub fn IconChat(
    #[prop(default = 18)] ukuran: i32,
    #[prop(default = 2.0)] tebal: f64,
) -> impl IntoView {
    view! {
        <svg width=ukuran height=ukuran viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width=tebal stroke-linecap="round"
             stroke-linejoin="round" aria-hidden="true">
            <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z" />
        </svg>
    }
}

/// Etalase/toko. Dipakai kartu toko, daftar diikuti, dan penanda produk sendiri.
#[component]
pub fn IconStore(
    #[prop(default = 18)] ukuran: i32,
    #[prop(default = 2.0)] tebal: f64,
) -> impl IntoView {
    view! {
        <svg width=ukuran height=ukuran viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width=tebal stroke-linecap="round"
             stroke-linejoin="round" aria-hidden="true">
            <path d="M3 7l1.5-3h15L21 7" />
            <path d="M3 7v13a1 1 0 001 1h16a1 1 0 001-1V7" />
            <path d="M9 11h6" />
        </svg>
    }
}

/// Mata — "lihat sebagai pembeli". Sebelumnya 3 salinan.
#[component]
pub fn IconEye(
    #[prop(default = 18)] ukuran: i32,
    #[prop(default = 2.0)] tebal: f64,
) -> impl IntoView {
    view! {
        <svg width=ukuran height=ukuran viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width=tebal stroke-linecap="round"
             stroke-linejoin="round" aria-hidden="true">
            <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
            <circle cx="12" cy="12" r="3" />
        </svg>
    }
}

/// Keranjang belanja. Sebelumnya 3 salinan.
#[component]
pub fn IconCart(
    #[prop(default = 20)] ukuran: i32,
    #[prop(default = 2.0)] tebal: f64,
) -> impl IntoView {
    view! {
        <svg width=ukuran height=ukuran viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width=tebal stroke-linecap="round"
             stroke-linejoin="round" aria-hidden="true">
            <circle cx="9" cy="21" r="1" />
            <circle cx="20" cy="21" r="1" />
            <path d="M1 1h4l2.68 13.39a2 2 0 002 1.61h9.72a2 2 0 002-1.61L23 6H6" />
        </svg>
    }
}

/// Panah kembali (chevron kiri).
#[component]
pub fn IconBack(
    #[prop(default = 20)] ukuran: i32,
    #[prop(default = 2.5)] tebal: f64,
) -> impl IntoView {
    view! {
        <svg width=ukuran height=ukuran viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width=tebal stroke-linecap="round"
             aria-hidden="true">
            <polyline points="15 18 9 12 15 6" />
        </svg>
    }
}

/// Chevron kanan — penanda "ada lanjutannya" di baris yang bisa diklik.
#[component]
pub fn IconChevron(
    #[prop(default = 16)] ukuran: i32,
    #[prop(default = 2.0)] tebal: f64,
) -> impl IntoView {
    view! {
        <svg width=ukuran height=ukuran viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width=tebal stroke-linecap="round"
             aria-hidden="true">
            <polyline points="9 18 15 12 9 6" />
        </svg>
    }
}

/// Cincin berputar bergaya penanda story — dipakai setiap keadaan "sedang
/// memuat" yang berbentuk lingkaran.
///
/// Busurnya 75% keliling (`66 88` pada r=14), BUKAN lingkaran penuh: cincin
/// penuh yang berputar tak terlihat bergerak sama sekali.
#[component]
pub fn IconSpinner(#[prop(default = "w-8 h-8")] kelas: &'static str) -> impl IntoView {
    view! {
        <svg
            class=format!("animate-spin {kelas}")
            viewBox="0 0 32 32"
            fill="none"
            aria-hidden="true"
        >
            <circle cx="16" cy="16" r="14" stroke="currentColor" stroke-width="2"
                    stroke-linecap="round" stroke-dasharray="66 88" />
        </svg>
    }
}
