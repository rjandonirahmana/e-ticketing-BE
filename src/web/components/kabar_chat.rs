//! kabar_chat.rs — Kabar pesan masuk di HALAMAN MANA PUN.
//!
//! ── ONGKOS SERVERNYA NOL ──────────────────────────────────────────────────
//! Ini tak membuka koneksi apa pun. `provide_chat_bus()` di root sudah memegang
//! SATU WebSocket untuk seluruh aplikasi — dipakai lencana navbar, daftar
//! `/pulse`, dan ruang obrolan. Komponen ini hanya menumpang aliran peristiwa
//! yang sudah mengalir.
//!
//! Itu keputusan yang sudah diambil kemarin dan sekarang berbuah: kalau tiap
//! halaman membuka koneksinya sendiri, fitur ini akan berarti satu soket per
//! halaman per pengguna — dan pada server yang menyimpan sesi per `user_id`,
//! halaman-halaman itu justru akan saling mematikan.
//!
//! Yang ditambahkan ke server: nol soket, nol kueri, nol tugas latar.

use leptos::prelude::*;

/// Dengarkan pesan masuk dan munculkan toast. Dirender SEKALI di root.
#[component]
pub fn KabarChat() -> impl IntoView {
    #[cfg(target_arch = "wasm32")]
    {
        use leptos_router::hooks::use_location;

        let bus = crate::web::components::use_chat_bus();
        let toast = crate::web::components::use_toast();
        let lokasi = use_location();
        let auth = use_context::<crate::web::app::AuthResource>();

        Effect::new(move |_| {
            let Some(evt) = bus.and_then(|b| b.peristiwa.get()) else {
                return;
            };
            if evt.get("type").and_then(|t| t.as_str()) != Some("new_message") {
                return;
            }

            // Semua bacaan di bawah ini UNTRACKED. Effect ini hanya boleh
            // bangun oleh peristiwa baru; kalau lokasi atau auth ikut terlacak,
            // berpindah halaman akan memunculkan ulang toast untuk pesan LAMA
            // yang masih tersimpan di slot peristiwa.
            let saya = auth
                .and_then(|a| a.get_untracked())
                .and_then(|r| r.ok())
                .flatten()
                .map(|u| u.id);
            let pengirim = evt.get("sender_id").and_then(|v| v.as_str());
            if matches!((pengirim, saya.as_deref()), (Some(a), Some(b)) if a == b) {
                return;
            }

            let Some(room_id) = evt.get("room_id").and_then(|v| v.as_str()) else {
                return;
            };

            // Ruangan yang SEDANG dibuka sudah punya kabarnya sendiri — pil di
            // bawah header dan gelembung yang tersorot. Toast di atasnya berarti
            // memberi tahu dua kali tentang pesan yang sudah terlihat.
            let jalur = lokasi.pathname.get_untracked();
            if jalur == format!("/pulse/{room_id}") {
                return;
            }

            let nama = evt
                .get("sender_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Pesan baru");
            let isi: String = evt
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .chars()
                .take(60)
                .collect();
            // Rujukan produk tampil sebagai judulnya, bukan alamat mentah.
            let isi = if isi.starts_with('[') {
                isi.split(" /products/").next().unwrap_or(&isi).to_string()
            } else {
                isi
            };

            toast.notify(
                crate::web::components::ToastKind::Info,
                nama.to_string(),
                Some(isi),
                Some(format!("/pulse/{room_id}")),
            );
        });
    }

    // Tak merender apa pun sendiri — toast-nya ditampung `<ToastHost/>` yang
    // sudah ada. Satu tempat untuk semua notifikasi, bukan dua yang saling
    // menumpuk di sudut yang sama.
    view! {}
}
