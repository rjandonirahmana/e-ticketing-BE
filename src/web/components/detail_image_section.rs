//! Komponen reusable untuk input foto detail product.
//! Dipakai di halaman create product dan edit product.
//!
//! Perubahan dari versi lama:
//! - Foto TIDAK lagi di-upload ke /upload terpisah — file langsung dikumpulkan
//!   sebagai `DetailImageUploadItem` dan diserahkan ke `create_product`/`update_product`
//!   yang mengirimnya sebagai multipart field `detail_image` ke BE.
//! - Foto lama (dari BE, sudah punya URL) dikumpulkan sebagai `DetailImagePayload`
//!   dan dikirim via JSON field `detail_images` (untuk retain tanpa re-upload).
//! - Grid layout adaptif dengan horizontal scroll: foto ditampilkan dalam grid
//!   yang bisa di-scroll ke kanan.

use crate::web::services::product::DetailImagePayload;
#[cfg(target_arch = "wasm32")]
use crate::web::services::product::{DetailImageMeta, DetailImageUploadItem};
use leptos::prelude::*;

// ─── State satu foto draft ────────────────────────────────────────────────────

#[derive(Clone)]
pub struct DetailImageDraft {
    /// Blob URL sementara untuk preview (createObjectURL).
    /// Untuk foto dari BE (edit mode): berisi URL permanen.
    pub preview_url: String,
    /// URL permanen — Some berarti foto LAMA dari BE, tidak perlu re-upload.
    /// None berarti foto BARU dari file picker, perlu dikirim sebagai file.
    pub uploaded_url: Option<String>,
    /// Unggahannya gagal. Dibedakan dari "masih berjalan" karena keduanya
    /// sama-sama ber-`uploaded_url: None`: tanpa penanda ini, satu unggahan yang
    /// gagal membuat form menolak SIMPAN dengan "Tunggu semua foto selesai
    /// diunggah" SELAMANYA — menyuruh menunggu sesuatu yang sudah berhenti.
    pub gagal: RwSignal<bool>,
    /// Persen unggahan 0–100. Foto lama dari BE langsung 100. Tetap 0 selama
    /// peramban tak melaporkan panjang total — thumbnail menanganinya sebagai
    /// "tanpa angka", bukan sebagai "macet di 0%".
    pub progres: RwSignal<u8>,
    /// File asli — Some hanya untuk foto baru yang belum pernah di-upload.
    #[cfg(target_arch = "wasm32")]
    pub file: Option<web_sys::File>,
    pub image_type: RwSignal<String>,
    pub caption: RwSignal<String>,
    /// Titik fokus `object-position` yang diatur pemakai lewat penanda seret.
    pub focus: RwSignal<String>,
}

impl DetailImageDraft {
    /// Foto baru dari file picker.
    #[cfg(target_arch = "wasm32")]
    pub fn from_file(file: web_sys::File, preview_url: String) -> Self {
        Self {
            preview_url,
            uploaded_url: None,
            gagal: RwSignal::new(false),
            progres: RwSignal::new(0),
            file: Some(file),
            image_type: RwSignal::new("other".to_string()),
            caption: RwSignal::new(String::new()),
            // Foto baru mulai dari tengah — sama dengan perilaku sebelum ada
            // fitur ini, jadi tak ada kejutan bagi yang tak menyentuhnya.
            focus: RwSignal::new("50% 50%".to_string()),
        }
    }

    /// Foto yang sudah ada dari data BE (edit mode).
    pub fn from_existing(payload: &DetailImagePayload) -> Self {
        Self {
            preview_url: payload.url.clone(),
            uploaded_url: Some(payload.url.clone()),
            // Foto lama sudah ada di server — tak ada unggahan yang bisa gagal.
            gagal: RwSignal::new(false),
            // Sudah utuh di server: 100, bukan 0. Kalau 0, thumbnail foto lama
            // akan tampak seperti unggahan yang belum mulai.
            progres: RwSignal::new(100),
            #[cfg(target_arch = "wasm32")]
            file: None,
            image_type: RwSignal::new(payload.image_type.clone()),
            caption: RwSignal::new(payload.caption.clone()),
            // Foto lama tanpa nilai fokus (data sebelum fitur ini) jatuh ke
            // tengah, bukan ke string kosong yang akan jadi CSS tak sah.
            focus: RwSignal::new(if payload.focus.trim().is_empty() {
                "50% 50%".to_string()
            } else {
                payload.focus.clone()
            }),
        }
    }

    /// Apakah foto ini sudah ada di BE (tidak perlu re-upload).
    pub fn is_existing(&self) -> bool {
        self.uploaded_url.is_some()
    }

    /// Konversi ke payload untuk retain (foto lama dikirim via JSON).
    pub fn to_retain_payload(&self) -> Option<DetailImagePayload> {
        let url = self.uploaded_url.clone()?;
        Some(DetailImagePayload {
            focus: self.focus.get_untracked(),
            url,
            image_type: self.image_type.get_untracked(),
            caption: self.caption.get_untracked(),
        })
    }
}

// ─── Catatan: JENIS FOTO SUDAH DIPENSIUNKAN ───────────────────────────────────
//
// Dulu ada `type_label`, `type_badge_style`, dan `type_badge_short` yang memilah
// foto menjadi `map` (Denah Lokasi), `seat` (Peta Kursi), dan `price` (Info
// Harga). Ketiganya konsep dari masa aplikasi ini menjual tiket acara.
//
// Sebagai marketplace barang, setiap foto detail adalah foto produk — semuanya
// masuk ke galeri geser yang sama di halaman detail (lihat
// `pages/product_detail.rs`), tanpa kategori dan tanpa lencana.
//
// Medan `image_type` di payload SENGAJA dipertahankan dan selalu berisi
// `"other"`. Membuangnya dari struktur akan memutus kompatibilitas dengan baris
// `products.detail_images` yang sudah tersimpan; migrasi 026 menyeragamkan
// nilainya, dan kolomnya bisa dibuang kapan saja nanti tanpa terburu-buru.

// ─── Komponen utama ───────────────────────────────────────────────────────────

#[component]
pub fn DetailImagesSection(drafts: RwSignal<Vec<DetailImageDraft>>) -> impl IntoView {
    let active_idx: RwSignal<Option<usize>> = RwSignal::new(None);
    // Index thumbnail yang sedang di-drag (HTML5 drag-and-drop asli).
    let drag_from: RwSignal<Option<usize>> = RwSignal::new(None);

    // Pindahkan item dari `from` ke posisi `to` (reorder galeri) + jaga
    // `active_idx` tetap menunjuk foto yang sama.
    let move_item = move |from: usize, to: usize| {
        drafts.update(|d| {
            if from < d.len() && to < d.len() && from != to {
                let item = d.remove(from);
                d.insert(to, item);
            }
        });
        active_idx.set(Some(to));
        drag_from.set(None);
    };

    // ── Tambah file ───────────────────────────────────────────────────────────
    #[cfg(target_arch = "wasm32")]
    let on_add_file = move |ev: leptos::ev::Event| {
        use leptos::wasm_bindgen::JsCast;

        let input = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok());
        let Some(input) = input else {
            web_sys::console::error_1(&"[DetailImage] target bukan HtmlInputElement".into());
            return;
        };

        let Some(files) = input.files() else {
            web_sys::console::warn_1(&"[DetailImage] input.files() kosong".into());
            return;
        };

        let Some(file) = files.get(0) else {
            return;
        };

        web_sys::console::log_1(
            &format!(
                "[DetailImage] file dipilih: name={} size={} type={}",
                file.name(),
                file.size(),
                file.type_()
            )
            .into(),
        );

        let count = drafts.with(|d| d.len());
        if count >= 6 {
            web_sys::console::warn_1(&"[DetailImage] batas 6 foto tercapai".into());
            return;
        }

        let url = match web_sys::Url::create_object_url_with_blob(&file) {
            Ok(u) => u,
            Err(e) => {
                web_sys::console::error_1(
                    &format!("[DetailImage] gagal create_object_url: {:?}", e).into(),
                );
                return;
            }
        };

        let new_idx = count;
        // Kunci pencocokan robust: blob URL unik per file. Dipakai untuk
        // menyetel uploaded_url ke draft yang BENAR meski user reorder/hapus
        // draft lain selagi upload berjalan.
        let match_key = url.clone();
        let file_for_upload = file.clone();
        drafts.update(|d| d.push(DetailImageDraft::from_file(file, url)));
        // Signal progres draft ini diambil SEKARANG dan dipegang langsung oleh
        // pelapor di bawah. Alternatifnya — `drafts.update(...)` pada tiap
        // peristiwa progres — akan menulis ulang seluruh Vec puluhan kali per
        // detik, dan setiap tulisan itu me-render ulang SEMUA thumbnail. Signal
        // per-draft membuat pembaruannya berhenti di satu angka saja.
        let progres_sig = drafts.with_untracked(|d| {
            d.iter().find(|x| x.preview_url == match_key).map(|x| x.progres)
        });
        active_idx.set(Some(new_idx));
        input.set_value("");

        // Upload SEKARANG (bukan saat submit) → draft langsung punya URL permanen.
        // Menghindari menyimpan `web_sys::File` sampai submit (rumit lintas-target)
        // dan menyederhanakan serialisasi: saat simpan semua draft sudah ber-URL.
        leptos::task::spawn_local(async move {
            let lapor = move |p: u8| {
                if let Some(sig) = progres_sig {
                    sig.set(p);
                }
            };
            match crate::web::pages::merchant::upload_merchant_image_with_progress(
                &file_for_upload,
                lapor,
            )
            .await
            {
                Ok(permanent) => {
                    drafts.update(|d| {
                        if let Some(dr) = d.iter_mut().find(|x| x.preview_url == match_key) {
                            dr.uploaded_url = Some(permanent);
                        }
                    });
                }
                Err(e) => {
                    web_sys::console::error_1(
                        &format!("[DetailImage] upload gagal: {e}").into(),
                    );
                    // Ditandai di draft-nya sendiri supaya thumbnail berhenti
                    // berkata "MENGUNGGAH…" dan form bisa memberi alasan yang
                    // benar saat SIMPAN ditekan.
                    drafts.update(|d| {
                        if let Some(dr) = d.iter_mut().find(|x| x.preview_url == match_key) {
                            dr.gagal.set(true);
                        }
                    });
                }
            }
        });
    };
    #[cfg(not(target_arch = "wasm32"))]
    let on_add_file = move |_: leptos::ev::Event| {};

    view! {
        <div style="display:flex;flex-direction:column;gap:12px">
            // ── File input + info ──────────────────────────────────────────────
            <div style="display:flex;align-items:center;gap:8px">
                <label style="position:relative;cursor:pointer;display:flex;align-items:center;justify-content:center;
                width:44px;height:44px;background:var(--bg-elevated);
                border:1.5px solid var(--border-soft);border-radius:10px;
                transition:all .2s;flex-shrink:0">
                    <input
                        type="file"
                        accept="image/*"
                        style="display:none"
                        on:change=on_add_file
                    />
                    <svg
                        width="18"
                        height="18"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                        style="color:var(--text-muted)"
                    >
                        <circle cx="12" cy="13" r="3" />
                        <path d="M5 8h14a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2v-8a2 2 0 012-2z" />
                        <polyline points="21 15 16 10 5 21" />
                    </svg>
                </label>
                <div style="font-size:11px;color:var(--text-muted);line-height:1.5">
                    <p style="margin:0;font-weight:600">"TAMBAH FOTO DETAIL"</p>
                    <p style="margin:0;opacity:.7">"JPG, PNG, hingga 6 foto · seret untuk urutkan"</p>
                </div>
            </div>

            // ── Thumbnail grid dengan horizontal scroll ───────────────────────
            {move || {
                (!drafts.with(|d| d.is_empty()))
                    .then(|| {
                        view! {
                            <div style="display:flex;flex-direction:column;gap:8px;overflow:hidden">
                                <div style="display:grid;gap:8px;overflow-x:auto;overflow-y:hidden;
                                padding-bottom:6px;
                                grid-auto-flow:column;grid-auto-columns:minmax(100px, 1fr);
                                scroll-snap-type:x mandatory;
                                -webkit-overflow-scrolling:touch">
                                    {move || {
                                        drafts
                                            .with(|d| {
                                                d.iter()
                                                    .enumerate()
                                                    .map(|(idx, draft)| {
                                                        let is_active = move || active_idx.get() == Some(idx);
                                                        let preview = draft.preview_url.clone();
                                                        // Belum punya URL permanen: entah masih berjalan,
                                                        // entah sudah gagal — dibedakan oleh `gagal`.
                                                        let belum_terunggah = draft.uploaded_url.is_none();
                                                        let gagal = draft.gagal;
                                                        let progres = draft.progres;

                                                        view! {
                                                            <button
                                                                draggable="true"
                                                                style=move || {
                                                                    let active_style = if is_active() {
                                                                        "border-color:#93c5fd;box-shadow:0 0 0 2px rgba(147,197,253,.3)"
                                                                    } else {
                                                                        "border-color:var(--border-soft)"
                                                                    };
                                                                    // Item yang sedang di-drag di-redupkan.
                                                                    let dim = if drag_from.get() == Some(idx) { "opacity:.4;" } else { "" };
                                                                    format!(
                                                                        "position:relative;width:100px;height:100px;
                                                        border:2px solid {};padding:0;background:var(--bg-elevated);
                                                        border-radius:12px;cursor:grab;flex-shrink:0;
                                                        overflow:hidden;display:flex;align-items:center;
                                                        justify-content:center;transition:all .2s;
                                                        scroll-snap-align:start;{}",
                                                                        active_style, dim,
                                                                    )
                                                                }
                                                                on:click=move |_| active_idx.set(Some(idx))
                                                                on:dragstart=move |_| drag_from.set(Some(idx))
                                                                on:dragover=move |e| e.prevent_default()
                                                                on:drop=move |e| {
                                                                    e.prevent_default();
                                                                    if let Some(from) = drag_from.get_untracked() {
                                                                        move_item(from, idx);
                                                                    }
                                                                }
                                                                on:dragend=move |_| drag_from.set(None)
                                                            >
                                                                <img
                                                                    src=preview
                                                                    style="width:100%;height:100%;object-fit:cover"
                                                                    alt=format!("Detail {}", idx)
                                                                />
                                                                // Overlay: "mengunggah…" selagi berjalan,
                                                                // "GAGAL" bila unggahannya berhenti — dulu
                                                                // keduanya tampil sebagai "MENGUNGGAH…" yang
                                                                // tak pernah selesai.
                                                                {move || belum_terunggah.then(|| {
                                                                    let rusak = gagal.get();
                                                                    let gaya = if rusak {
                                                                        "position:absolute;inset:0;display:flex;align-items:center;\
                                                                         justify-content:center;background:rgba(120,20,20,.72);\
                                                                         color:#fff;font-size:9px;font-weight:700;letter-spacing:.05em"
                                                                    } else {
                                                                        "position:absolute;inset:0;display:flex;align-items:center;\
                                                                         justify-content:center;background:rgba(0,0,0,.5);\
                                                                         color:#fff;font-size:9px;font-weight:700;letter-spacing:.05em"
                                                                    };
                                                                    view! {
                                                                        <div style=gaya>
                                                                            // Angka menggantikan "MENGUNGGAH…"
                                                                            // begitu terukur. Untuk foto besar,
                                                                            // label statis tak terbedakan dari
                                                                            // unggahan yang sudah mati.
                                                                            // `!= 0`, BUKAN `> 0`: `>` di dalam
                                                                            // makro view! diurai sebagai penutup tag.
                                                                            {if rusak {
                                                                                "GAGAL".to_string()
                                                                            } else {
                                                                                // 100% dari `upload.onprogress` cuma
                                                                                // berarti byte sudah diserahkan ke jaringan;
                                                                                // server masih meneruskannya ke storage.
                                                                                // Menampilkan "100%" di situ membuat thumbnail
                                                                                // tampak selesai padahal belum — dan justru
                                                                                // di fase itulah menunggunya paling lama.
                                                                                let p = progres.get();
                                                                                if p >= 100 {
                                                                                    "MEMPROSES…".to_string()
                                                                                } else if p != 0 {
                                                                                    format!("{p}%")
                                                                                } else {
                                                                                    "MENGUNGGAH…".to_string()
                                                                                }
                                                                            }}
                                                                        </div>
                                                                    }
                                                                })}
                                                            </button>
                                                        }
                                                    })
                                                    .collect_view()
                                            })
                                    }}
                                </div>
                            </div>
                        }
                    })
            }}

            // ── Detail edit panel saat ada yang dipilih ────────────────────────
            {move || {
                active_idx
                    .get()
                    .and_then(|idx| {
                        drafts
                            .with(|d| {
                                d.get(idx)
                                    .map(|draft| {
                                        let caption = draft.caption;
                                        let list_len = d.len();
                                        let preview = draft.preview_url.clone();

                                        view! {
                                            <div style="display:flex;flex-direction:column;gap:14px;
                                            padding:14px;background:var(--bg-elevated);
                                            border-radius:12px;border:1px solid var(--border-soft)">

                                                // ── Preview detail ────────────────────────────
                                                <div style="position:relative;
                                                width:100%;aspect-ratio:4/3;
                                                background:var(--bg-surface);
                                                border-radius:8px;overflow:hidden">
                                                    <img
                                                        src=preview
                                                        style="width:100%;height:100%;object-fit:cover"
                                                        alt="Detail preview"
                                                    />
                                                    <div style="position:absolute;top:8px;left:8px;right:8px;
                                                    display:flex;gap:6px;align-items:center">
                                                        // Tombol tutup
                                                        <button
                                                            style="width:28px;height:28px;border-radius:8px;
                                                             background:rgba(0,0,0,.55);border:none;
                                                             color:#fff;font-size:14px;cursor:pointer;
                                                             display:flex;align-items:center;
                                                             justify-content:center;flex-shrink:0"
                                                            on:click=move |_| active_idx.set(None)
                                                            aria-label="Tutup"
                                                        >
                                                            "✕"
                                                        </button>
                                                        <span style="flex:1"></span>

                                                        // Tombol geser kiri
                                                        {(idx > 0)
                                                            .then(|| {
                                                                view! {
                                                                    <button
                                                                        style="width:28px;height:28px;border-radius:8px;\
                                                                         background:rgba(0,0,0,.55);border:none;\
                                                                         color:#fff;font-size:14px;cursor:pointer;\
                                                                         display:flex;align-items:center;\
                                                                         justify-content:center"
                                                                        on:click=move |_| {
                                                                            drafts
                                                                                .update(|d| {
                                                                                    let new_idx = idx - 1;
                                                                                    if new_idx < d.len() {
                                                                                        d.swap(idx, new_idx);
                                                                                        active_idx.set(Some(new_idx));
                                                                                    }
                                                                                });
                                                                        }
                                                                        aria-label="Geser ke kiri"
                                                                    >
                                                                        "‹"
                                                                    </button>
                                                                }
                                                            })}

                                                        // Tombol geser kanan
                                                        {(idx < list_len - 1)
                                                            .then(|| {
                                                                view! {
                                                                    <button
                                                                        style="width:28px;height:28px;border-radius:8px;\
                                                                         background:rgba(0,0,0,.55);border:none;\
                                                                         color:#fff;font-size:14px;cursor:pointer;\
                                                                         display:flex;align-items:center;\
                                                                         justify-content:center"
                                                                        on:click=move |_| {
                                                                            drafts
                                                                                .update(|d| {
                                                                                    let new_idx = idx + 1;
                                                                                    if new_idx < d.len() {
                                                                                        d.swap(idx, new_idx);
                                                                                        active_idx.set(Some(new_idx));
                                                                                    }
                                                                                });
                                                                        }
                                                                        aria-label="Geser ke kanan"
                                                                    >
                                                                        "›"
                                                                    </button>
                                                                }
                                                            })}
                                                    </div>
                                                </div>

                                                // ── Input keterangan ────────────────────────
                                                <div style="display:flex;flex-direction:column;gap:5px">
                                                    <label style="font-size:9px;font-weight:800;\
                                                     letter-spacing:.14em;color:var(--text-muted);\
                                                     text-transform:uppercase">"KETERANGAN"</label>
                                                    <textarea
                                                        class="medit-input"
                                                        style="min-height:60px;resize:none;padding-top:9px;font-size:13px"
                                                        placeholder="cth. Tampak belakang — bahan katun, jahitan rangkap"
                                                        prop:value=move || caption.get()
                                                        on:input=move |e| caption.set(event_target_value(&e))
                                                    />
                                                    <p style="font-size:10px;color:var(--text-muted);line-height:1.5">
                                                        "Keterangan singkat ditampilkan di bawah foto pada halaman detail produk."
                                                    </p>
                                                </div>

                                                // ── Delete button ────────────────────────────
                                                <button
                                                    style="width:100%;padding:8px;background:#dc2626;color:#fff;
                                                     border:none;border-radius:8px;cursor:pointer;
                                                     font-size:12px;font-weight:700;letter-spacing:.05em;
                                                     transition:all .2s"
                                                    on:click=move |_| {
                                                        drafts
                                                            .update(|d| {
                                                                if idx < d.len() {
                                                                    d.remove(idx);
                                                                }
                                                            });
                                                        active_idx.set(None);
                                                    }
                                                >
                                                    "HAPUS FOTO"
                                                </button>
                                            </div>
                                        }
                                    })
                            })
                    })
            }}

            // ── Empty state ────────────────────────────────────────────────────
            {move || {
                drafts
                    .with(|d| d.is_empty())
                    .then(|| {
                        view! {
                            <div style="display:flex;flex-direction:column;align-items:center;\
                             gap:8px;padding:24px 16px;\
                             border:1.5px dashed rgba(200,255,94,.2);\
                             border-radius:14px;text-align:center">
                                <svg
                                    width="32"
                                    height="32"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="1.2"
                                    stroke-linecap="round"
                                    style="color:var(--text-muted);opacity:.5"
                                >
                                    <rect x="3" y="3" width="18" height="18" rx="2" />
                                    <circle cx="8.5" cy="8.5" r="1.5" />
                                    <polyline points="21 15 16 10 5 21" />
                                </svg>
                                <p style="font-size:12px;color:var(--text-muted);margin:0;line-height:1.6">
                                    "Tambahkan foto denah, peta kursi, atau info harga"<br />
                                    "agar pembeli tahu detail produknya."
                                </p>
                            </div>
                        }
                    })
            }}

            // ── Counter ────────────────────────────────────────────────────────
            {move || {
                let count = drafts.with(|d| d.len());
                let n_new = drafts.with(|d| d.iter().filter(|x| !x.is_existing()).count());
                (count > 0)
                    .then(|| {
                        view! {
                            <p style="font-size:10px;color:var(--text-muted);margin:0;line-height:1.6">
                                {format!("{}/6 foto", count)}
                                {(n_new > 0).then(|| format!(" · {} baru akan di-upload", n_new))}
                                " — Klik foto untuk edit, ‹ › untuk reorder, scroll ke kanan untuk melihat lebih banyak."
                            </p>
                        }
                    })
            }}
        </div>
    }
}

// ─── Helpers untuk page submit ────────────────────────────────────────────────

/// Kumpulkan semua draft menjadi dua bucket:
///   1. `upload_items`  — foto BARU (harus dikirim sebagai file multipart ke BE)
///   2. `retain_items`  — foto LAMA dari BE (dikirim via JSON untuk dipertahankan)
///
/// Dipanggil di `do_submit` pada halaman create/edit sebelum memanggil API.
/// Tidak ada network call di sini — semua I/O dilakukan di service layer.
#[cfg(target_arch = "wasm32")]
pub async fn collect_detail_drafts(
    drafts: &[DetailImageDraft],
) -> (Vec<DetailImageUploadItem>, Vec<DetailImagePayload>) {
    use wasm_bindgen_futures::JsFuture;

    let mut upload_items: Vec<DetailImageUploadItem> = Vec::new();
    let mut retain_items: Vec<DetailImagePayload> = Vec::new();

    for draft in drafts {
        if draft.is_existing() {
            // Foto lama — pertahankan via JSON (metadata bisa berubah)
            if let Some(payload) = draft.to_retain_payload() {
                retain_items.push(payload);
            }
        } else if let Some(file) = &draft.file {
            // Foto baru — baca bytes dari File, siapkan sebagai upload item
            let mime = file.type_();
            match JsFuture::from(file.array_buffer()).await {
                Ok(ab) => {
                    let arr = web_sys::js_sys::Uint8Array::new(&ab);
                    let bytes = arr.to_vec();
                    upload_items.push(DetailImageUploadItem {
                        bytes,
                        mime,
                        meta: DetailImageMeta {
                            image_type: draft.image_type.get_untracked(),
                            caption: draft.caption.get_untracked(),
                        },
                    });
                }
                Err(_) => {
                    web_sys::console::warn_1(
                        &"Detail image: gagal baca bytes, foto dilewati".into(),
                    );
                }
            }
        }
    }

    (upload_items, retain_items)
}
