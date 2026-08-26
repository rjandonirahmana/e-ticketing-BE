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
use leptos_router::hooks::use_navigate;

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

/// Durasi tampil default (ms).
const TOAST_MS: u64 = 4500;
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
        set_timeout(
            move || items.update(|v| v.retain(|t| t.id != id)),
            Duration::from_millis(TOAST_MS),
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
    let navigate = use_navigate();
    view! {
        <div class="toast-host" aria-live="polite">
            <For
                each=move || ctx.items.get()
                key=|t| t.id
                children=move |t| {
                    let id = t.id;
                    let nav = navigate.clone();
                    let href = t.href.clone();
                    let clickable = href.is_some();
                    let go = move |_| {
                        if let Some(h) = href.clone() {
                            ctx.dismiss(id);
                            nav(&h, Default::default());
                        }
                    };
                    view! {
                        <div
                            class=format!(
                                "toast {}{}",
                                t.kind.cls(),
                                if clickable { " toast--link" } else { "" },
                            )
                            role="status"
                            on:click=go
                        >
                            <span class="toast-icon">{t.kind.icon()}</span>
                            <div class="toast-text">
                                <span class="toast-title">{t.title.clone()}</span>
                                {t
                                    .body
                                    .clone()
                                    .map(|b| view! { <span class="toast-body">{b}</span> })}
                            </div>
                            <button
                                class="toast-x"
                                aria-label="Tutup"
                                on:click=move |ev| {
                                    ev.stop_propagation();
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
