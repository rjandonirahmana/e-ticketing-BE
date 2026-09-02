//! web/components/toast.rs — Sistem notifikasi TOAST global.
//!
//! Satu context (`ToastCtx`) disediakan di App root, satu `<ToastHost/>` dirender
//! sekali. Komponen mana pun cukup `use_toast().success("...")` untuk memunculkan
//! gelembung pesan sementara yang otomatis hilang.
//!
//! KENAPA BUKAN pustaka eksternal (leptoaster/leptos_notification): app ini sudah
//! punya pola signal + `set_timeout` (callback `Closure::once` → dibebaskan setelah
//! fire, TIDAK bocor) dan WebSocket sendiri; menambah crate hanya menduplikasi.
//! Implementasi ini ~1 file, zero-dep, dan aman SSR: daftar toast kosong di server
//! (toast hanya ditambah di client via product handler / WS) → tak ada hydration
//! mismatch. Tanpa memory leak: timer auto-dismiss satu-tembak, tak ada listener
//! global yang perlu di-cleanup.

use std::time::Duration;

use leptos::prelude::*;
use leptos_router::components::A;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
    Info,
}

impl ToastKind {
    fn cls(self) -> &'static str {
        match self {
            ToastKind::Success => "toast--success",
            ToastKind::Error => "toast--error",
            ToastKind::Info => "toast--info",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            ToastKind::Success => "✓",
            ToastKind::Error => "!",
            ToastKind::Info => "🔔",
        }
    }
}

#[derive(Clone)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub title: String,
    pub body: Option<String>,
    /// Href opsional — klik toast → navigasi (mis. `/orders/:id`, `/messages`).
    pub href: Option<String>,
}

/// Durasi tampil (ms) untuk toast yang hanya MEMBERI TAHU.
const TOAST_MS: u64 = 4500;

/// Durasi untuk toast yang bisa DITINDAKLANJUTI (punya href).
///
/// Lebih lama karena yang diminta darinya lebih banyak: memberitahu cukup
/// dibaca sekilas, tetapi menindaklanjuti berarti membaca, memutuskan,
/// menggerakkan tangan, lalu menekan. Empat setengah detik habis di tengah
/// rangkaian itu — dan toast yang lenyap tepat saat jari bergerak ke arahnya
/// terasa seperti tombol yang tak berfungsi.
const TOAST_AKSI_MS: u64 = 9000;
/// Maksimum toast tampak sekaligus (cegah tumpukan tak terbatas bila di-spam).
const MAX_VISIBLE: usize = 4;

#[derive(Clone, Copy)]
pub struct ToastCtx {
    items: RwSignal<Vec<Toast>>,
    seq: RwSignal<u64>,
}

impl ToastCtx {
    /// Toast hijau (sukses) — judul saja.
    pub fn success(&self, title: impl Into<String>) {
        self.notify(ToastKind::Success, title.into(), None, None);
    }
    /// Toast merah (error) — judul saja.
    pub fn error(&self, title: impl Into<String>) {
        self.notify(ToastKind::Error, title.into(), None, None);
    }
    /// Toast info (netral) — judul saja.
    #[allow(dead_code)]
    pub fn info(&self, title: impl Into<String>) {
        self.notify(ToastKind::Info, title.into(), None, None);
    }

    /// Toast lengkap: kind + judul + body opsional + href opsional (klik → navigasi).
    pub fn notify(
        &self,
        kind: ToastKind,
        title: String,
        body: Option<String>,
        href: Option<String>,
    ) {
        // Toast murni UI klien — no-op di server (tak ada window/timer).
        if is_server() {
            return;
        }
        let punya_href = href.is_some();
        let mut id = 0;
        self.seq.update(|s| {
            *s = s.wrapping_add(1);
            id = *s;
        });
        self.items.update(|v| {
            v.push(Toast {
                id,
                kind,
                title,
                body,
                href,
            });
            // Buang yang tertua bila melebihi kapasitas tampak.
            while v.len() > MAX_VISIBLE {
                v.remove(0);
            }
        });
        // Auto-dismiss. `set_timeout` (leptos) memakai Closure::once → callback
        // dibebaskan setelah fire (tidak bocor). No-op di server.
        let items = self.items;
        let umur = if punya_href { TOAST_AKSI_MS } else { TOAST_MS };
        set_timeout(
            move || items.update(|v| v.retain(|t| t.id != id)),
            Duration::from_millis(umur),
        );
    }

    /// Tutup satu toast (tombol × atau setelah klik CTA).
    pub fn dismiss(&self, id: u64) {
        self.items.update(|v| v.retain(|t| t.id != id));
    }
}

/// Sediakan `ToastCtx` — dipanggil di `provide_all_app_contexts()`.
pub fn provide_toast() {
    provide_context(ToastCtx {
        items: RwSignal::new(Vec::new()),
        seq: RwSignal::new(0),
    });
}

/// Ambil `ToastCtx` dari context. Panic bila belum di-provide (bug programmer).
pub fn use_toast() -> ToastCtx {
    use_context::<ToastCtx>().expect("ToastCtx not provided")
}

/// Host toast — dirender SEKALI di App root. Menampilkan tumpukan toast aktif.
#[component]
pub fn ToastHost() -> impl IntoView {
    let ctx = use_toast();
    view! {
        <div class="toast-host" aria-live="polite">
            <For
                each=move || ctx.items.get()
                key=|t| t.id
                children=move |t| {
                    let id = t.id;
                    let href = t.href.clone();
                    let ikon = t.kind.icon();
                    let judul = t.title.clone();
                    let isi = t.body.clone();

                    // Isi kartu, dipakai baik oleh versi bertaut maupun tidak.
                    // Dihitung SEBELUM `href` dipindahkan ke dalam `view!`.
                    let bertaut = href.is_some();
                    let muatan = move || {
                        view! {
                            <span class="toast-icon">{ikon}</span>
                            <div class="toast-text">
                                <span class="toast-title">{judul.clone()}</span>
                                {isi.clone().map(|b| view! { <span class="toast-body">{b}</span> })}
                            </div>
                        }
                    };

                    view! {
                        <div
                            class=format!(
                                "toast {}{}",
                                t.kind.cls(),
                                if bertaut { " toast--link" } else { "" },
                            )
                            role="status"
                        >
                            // ── SELURUH kartu yang bisa ditekan, bukan teksnya
                            // saja. Sebelumnya penangan klik dipasang pada
                            // pembungkusnya sementara tombol tutup berada DI
                            // DALAMNYA — dan sasaran tekan yang memuat sasaran
                            // tekan lain membuat wilayah yang benar-benar aktif
                            // sulit ditebak dari melihatnya.
                            //
                            // `<A>`, bukan `div` ber-`on:click`: ia jangkar
                            // sungguhan. Bisa dibuka di tab baru, bisa dijangkau
                            // papan ketik, dan navigasinya tak bergantung pada
                            // closure yang harus masih hidup saat ditekan.
                            {match href {
                                Some(h) => view! {
                                    <A href=h attr:class="toast-tautan">{muatan()}</A>
                                }.into_any(),
                                None => view! {
                                    <div class="toast-tautan">{muatan()}</div>
                                }.into_any(),
                            }}

                            // Tombol tutup DI LUAR jangkar, ditumpuk di atasnya.
                            // Menyarangkan tombol di dalam jangkar adalah HTML
                            // yang tak sah, dan peramban menanganinya
                            // berbeda-beda — persis jenis perbedaan yang
                            // menghasilkan "kadang bisa diklik, kadang tidak".
                            <button
                                class="toast-x"
                                aria-label="Tutup"
                                on:click=move |ev| {
                                    ev.stop_propagation();
                                    ev.prevent_default();
                                    ctx.dismiss(id);
                                }
                            >
                                "×"
                            </button>
                        </div>
                    }
                }
            />
        </div>
    }
}
