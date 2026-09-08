//! components/nav_progress.rs — garis progres saat berpindah halaman.
//!
//! ── MASALAH YANG DISELESAIKAN ──────────────────────────────────────────────
//! Sebagian rute menunggu server function sebelum ada yang bisa dirender. Di
//! antara ketukan dan halaman baru, layar tidak berubah sama sekali — dan yang
//! dilakukan orang saat layar tak berubah adalah mengetuk lagi. Ketukan kedua
//! itu bukan hanya sia-sia: ia menembakkan navigasi lagi ke rute yang sama.
//!
//! Garis tipis ini menjawabnya dengan satu kalimat: "ketukanmu terdaftar,
//! sedang jalan". Ia tak mengklaim tahu berapa lama — lihat catatan animasinya
//! di `58-page-transition.css`.
//!
//! ── KENAPA ADA JEDA 150 ms SEBELUM TAMPIL ──────────────────────────────────
//! Sebagian besar navigasi di aplikasi ini selesai jauh di bawah 150 ms (cache
//! server-side sudah menanganinya). Menampilkan garis untuk perpindahan
//! secepat itu menghasilkan kedipan yang justru terbaca sebagai gangguan, bukan
//! sebagai kabar. Jedanya dikerjakan `animation-delay` di CSS, bukan timer di
//! sini: perpindahan yang selesai sebelum jeda habis tak pernah menampilkan
//! apa pun, dan tak ada timer yang perlu dibatalkan.

use leptos::prelude::*;

/// `sedang_pindah` berasal dari `<Router set_is_routing>` — router yang
/// menyalakannya sebelum memuat rute dan mematikannya setelah selesai.
#[component]
pub fn NavProgress(#[prop(into)] sedang_pindah: Signal<bool>) -> impl IntoView {
    view! {
        // `aria-hidden`: ini hiasan status, bukan informasi. Pembaca layar
        // sudah mengumumkan perpindahan halaman lewat perubahan judul dan
        // fokus; membacakan garis ini lagi hanya menambah kebisingan.
        <div
            class="nav-progress"
            class:nav-progress--aktif=move || sedang_pindah.get()
            aria-hidden="true"
        >
            <span class="nav-progress__bar"></span>
        </div>
    }
}
