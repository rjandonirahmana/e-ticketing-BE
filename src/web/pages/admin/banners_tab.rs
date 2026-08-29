//! banners_tab.rs — Tab "Spanduk" Pusat Admin: kelola banner slider Jelajah.
//!
//! Admin dapat: unggah banner baru (gambar via POST /upload/merchant-image —
//! endpoint menerima role admin — lalu `create_banner`), mengganti gambar /
//! link banner lama (`update_banner`), dan menghapus (`delete_banner`,
//! soft-delete). Setiap aksi menginvalidasi cache banner publik di server
//! sehingga slider Jelajah langsung segar.
//!
//! ── Tentang tampilannya ───────────────────────────────────────────────────
//! Versi sebelumnya menampilkan tiap spanduk sebagai gambar mungil 90×58 di
//! sisi kiri baris. Rasio itu bukan rasio tayangnya: di Jelajah spanduk tampil
//! 21:9 selebar layar. Jadi satu-satunya hal yang benar-benar perlu dilihat
//! admin — apakah gambarnya terpotong, apakah teks di dalamnya masih terbaca —
//! justru yang tak bisa dilihat dari halaman pengelolanya. Sekarang tiap kartu
//! memakai bingkai 21:9 yang sama, dan berkas yang baru dipilih pun langsung
//! dipratinjau di bingkai itu SEBELUM diunggah.

use leptos::prelude::*;

// create_banner hanya dipanggil dalam blok cfg wasm (upload) → di SSR unused.
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_imports))]
use crate::web::api::{create_banner, delete_banner, update_banner};
use crate::web::models::Banner;

/// `web_sys::File` pertama dari `<input type=file>` (via NodeRef).
#[cfg(target_arch = "wasm32")]
fn first_file(node: NodeRef<leptos::html::Input>) -> Option<web_sys::File> {
    node.get_untracked()
        .and_then(|el| el.files())
        .and_then(|fs| fs.get(0))
}

/// Bingkai pratinjau 21:9 — rasio yang SAMA dengan `.exp-bnr-slide` di Jelajah.
fn frame(img: Option<String>, tanda: Option<String>) -> impl IntoView {
    view! {
        <div class="abn-frame">
            {match img {
                Some(src) if !src.is_empty() => {
                    view! { <img class="abn-frame-img" src=src alt="" /> }.into_any()
                }
                _ => {
                    view! {
                        <div class="abn-frame-kosong">
                            <svg width="26" height="26" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                <rect x="3" y="4" width="18" height="16" rx="2"/>
                                <circle cx="8.5" cy="9.5" r="1.5"/>
                                <path d="M21 15l-5-5L5 20"/>
                            </svg>
                            <span>"Rasio tayang 21:9"</span>
                        </div>
                    }
                        .into_any()
                }
            }}
            {tanda.map(|t| view! { <span class="abn-frame-tanda">{t}</span> })}
        </div>
    }
}

pub(super) fn view_banners(
    banners: Vec<Banner>,
    refetch: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let busy = RwSignal::new(false);
    let msg: RwSignal<Option<(String, bool)>> = RwSignal::new(None);
    let new_link = RwSignal::new(String::new());
    let new_file_ref: NodeRef<leptos::html::Input> = NodeRef::new();
    // Pratinjau berkas yang BARU dipilih, sebagai object-URL.
    let pratinjau: RwSignal<Option<String>> = RwSignal::new(None);
    let nama_berkas: RwSignal<Option<String>> = RwSignal::new(None);
    // Target "Ganti Gambar": id banner yang sedang diganti gambarnya. Satu
    // input file tersembunyi dipakai bersama semua kartu.
    let swap_target: StoredValue<i64> = StoredValue::new(0);
    let swap_file_ref: NodeRef<leptos::html::Input> = NodeRef::new();
    // Hapus bersifat permanen bagi yang melihatnya dan tak punya urungkan, jadi
    // ia butuh dua ketukan. 0 = tak ada yang sedang dikonfirmasi.
    let konfirmasi: RwSignal<i64> = RwSignal::new(0);

    let jml = banners.len();

    // ── Berkas baru dipilih → pratinjau ──────────────────────────────────────
    let on_pilih = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            // Object-URL lama DICABUT sebelum diganti. Tanpa ini tiap gambar
            // yang pernah dipilih tetap dipegang peramban sampai halaman
            // ditutup — bocor yang sunyi, dan berkas spanduk tidak kecil.
            if let Some(lama) = pratinjau.get_untracked() {
                let _ = web_sys::Url::revoke_object_url(&lama);
            }
            match first_file(new_file_ref) {
                Some(f) => {
                    nama_berkas.set(Some(f.name()));
                    pratinjau.set(web_sys::Url::create_object_url_with_blob(&f).ok());
                }
                None => {
                    nama_berkas.set(None);
                    pratinjau.set(None);
                }
            }
        }
    };

    // ── Unggah + buat banner baru ─────────────────────────────────────────────
    let on_create = move |_| {
        if busy.get_untracked() {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let Some(file) = first_file(new_file_ref) else {
                msg.set(Some(("Pilih gambar spanduk dulu.".into(), true)));
                return;
            };
            busy.set(true);
            msg.set(Some(("Mengunggah…".into(), false)));
            let link = new_link.get_untracked();
            leptos::task::spawn_local(async move {
                let res = async {
                    let url =
                        crate::web::pages::merchant::upload_merchant_image(&file).await?;
                    create_banner(url, Some(link)).await.map_err(|e| e.to_string())
                }
                .await;
                match res {
                    Ok(()) => {
                        msg.set(Some(("Spanduk tayang di Jelajah.".into(), false)));
                        new_link.set(String::new());
                        nama_berkas.set(None);
                        if let Some(lama) = pratinjau.get_untracked() {
                            let _ = web_sys::Url::revoke_object_url(&lama);
                        }
                        pratinjau.set(None);
                        if let Some(el) = new_file_ref.get_untracked() {
                            el.set_value("");
                        }
                        refetch();
                    }
                    Err(e) => msg.set(Some((format!("Gagal: {e}"), true))),
                }
                busy.set(false);
            });
        }
    };

    // ── Ganti gambar banner lama (input file bersama) ─────────────────────────
    let on_swap_change = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            let id = swap_target.get_value();
            if id == 0 || busy.get_untracked() {
                return;
            }
            let Some(file) = first_file(swap_file_ref) else { return };
            busy.set(true);
            msg.set(Some(("Mengunggah gambar baru…".into(), false)));
            leptos::task::spawn_local(async move {
                let res = async {
                    let url =
                        crate::web::pages::merchant::upload_merchant_image(&file).await?;
                    update_banner(id, Some(url), None).await.map_err(|e| e.to_string())
                }
                .await;
                match res {
                    Ok(()) => {
                        msg.set(Some((format!("Gambar spanduk #{id} diperbarui."), false)));
                        refetch();
                    }
                    Err(e) => msg.set(Some((format!("Gagal: {e}"), true))),
                }
                if let Some(el) = swap_file_ref.get_untracked() {
                    el.set_value("");
                }
                busy.set(false);
            });
        }
    };

    view! {
        <section class="mhub-products-section abn">

            <div class="abn-head">
                <h3 class="mhub-products-title">"Spanduk"</h3>
                <span class="abn-count">
                    {jml}
                    {if jml == 1 { " tayang" } else { " tayang" }}
                </span>
            </div>
            <p class="abn-hint">
                "Tampil sebagai carousel di bagian atas halaman Jelajah. \
                 Gambar dipangkas ke rasio 21:9 — pastikan bagian pentingnya di tengah."
            </p>

            // ── Unggah spanduk baru ───────────────────────────────────────────
            <div class="abn-new">
                // Seluruh area pratinjau adalah `<label>` untuk input file, jadi
                // target ketuknya sebesar kartunya sendiri — bukan tombol
                // "Choose File" bawaan peramban yang lebarnya 90px dan tampil
                // berbeda di tiap peramban.
                <label class="abn-drop">
                    <input
                        class="abn-file-hidden"
                        type="file"
                        accept="image/*"
                        node_ref=new_file_ref
                        on:change=on_pilih
                    />
                    {move || frame(pratinjau.get(), None)}
                    <span class="abn-drop-kaki">
                        {move || {
                            nama_berkas
                                .get()
                                .unwrap_or_else(|| "Ketuk untuk pilih gambar".to_string())
                        }}
                    </span>
                </label>

                <input
                    class="abn-field"
                    type="text"
                    placeholder="Link tujuan klik (opsional, mis. /produk/slug)"
                    prop:value=move || new_link.get()
                    on:input=move |ev| new_link.set(event_target_value(&ev))
                />

                <button
                    class="abn-btn abn-btn--primary abn-btn--blok"
                    disabled=move || busy.get()
                    on:click=on_create
                >
                    {move || if busy.get() { "Memproses…" } else { "Unggah & Tayangkan" }}
                </button>
            </div>

            {move || {
                msg.get()
                    .map(|(t, err)| {
                        let cls = if err { "abn-msg abn-msg--err" } else { "abn-msg" };
                        view! {
                            <div class=cls>
                                <span>{t}</span>
                                <button
                                    class="abn-msg-x"
                                    on:click=move |_| msg.set(None)
                                >
                                    "✕"
                                </button>
                            </div>
                        }
                    })
            }}

            // Input file tersembunyi untuk aksi "Ganti Gambar" per kartu.
            <input
                class="abn-file-hidden"
                type="file"
                accept="image/*"
                node_ref=swap_file_ref
                on:change=on_swap_change
            />

            {if banners.is_empty() {
                view! {
                    <div class="mhub-empty">
                        <div class="mhub-empty-icon-wrap">
                            <svg width="38" height="38" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                <rect x="3" y="4" width="18" height="16" rx="2"/>
                                <circle cx="8.5" cy="9.5" r="1.5"/>
                                <path d="M21 15l-5-5L5 20"/>
                            </svg>
                        </div>
                        <p class="mhub-empty-title">"Belum Ada Spanduk"</p>
                        <p class="mhub-empty-body">
                            "Unggah spanduk pertama lewat kartu di atas — langsung tampil di Jelajah."
                        </p>
                    </div>
                }
                    .into_any()
            } else {
                banners
                    .into_iter()
                    .enumerate()
                    .map(|(i, b)| {
                        let id = b.id;
                        let img = b.image_url.clone();
                        let judul = b.title.clone().filter(|t| !t.trim().is_empty());
                        let link_sig = RwSignal::new(b.link_url.clone().unwrap_or_default());
                        let on_save_link = move |_| {
                            if busy.get_untracked() {
                                return;
                            }
                            busy.set(true);
                            let link = link_sig.get_untracked();
                            leptos::task::spawn_local(async move {
                                match update_banner(id, None, Some(link)).await {
                                    Ok(()) => {
                                        msg.set(Some((format!("Link spanduk #{id} disimpan."), false)));
                                        refetch();
                                    }
                                    Err(e) => msg.set(Some((format!("Gagal: {e}"), true))),
                                }
                                busy.set(false);
                            });
                        };
                        let on_delete = move |_| {
                            if busy.get_untracked() {
                                return;
                            }
                            konfirmasi.set(0);
                            busy.set(true);
                            leptos::task::spawn_local(async move {
                                match delete_banner(id).await {
                                    Ok(()) => {
                                        msg.set(Some((format!("Spanduk #{id} dihapus."), false)));
                                        refetch();
                                    }
                                    Err(e) => msg.set(Some((format!("Gagal: {e}"), true))),
                                }
                                busy.set(false);
                            });
                        };
                        let on_swap = move |_| {
                            swap_target.set_value(id);
                            #[cfg(target_arch = "wasm32")]
                            if let Some(el) = swap_file_ref.get_untracked() {
                                el.click();
                            }
                        };
                        view! {
                            <article class="abn-card">
                                {frame(Some(img), Some(format!("{}", i + 1)))}
                                <div class="abn-body">
                                    <div class="abn-meta">
                                        <span class="abn-meta-judul">
                                            {judul.unwrap_or_else(|| format!("Spanduk #{id}"))}
                                        </span>
                                        <span class="abn-meta-urut">
                                            "urutan " {b.sort_order}
                                        </span>
                                    </div>

                                    <label class="abn-label">"Link tujuan klik"</label>
                                    <div class="abn-row">
                                        <input
                                            class="abn-field"
                                            type="text"
                                            placeholder="kosong = tidak bisa diklik"
                                            prop:value=move || link_sig.get()
                                            on:input=move |ev| link_sig.set(event_target_value(&ev))
                                        />
                                        <button
                                            class="abn-btn abn-btn--primary"
                                            disabled=move || busy.get()
                                            on:click=on_save_link
                                        >
                                            "Simpan"
                                        </button>
                                    </div>

                                    <div class="abn-acts">
                                        <button
                                            class="abn-btn abn-btn--ghost"
                                            disabled=move || busy.get()
                                            on:click=on_swap
                                        >
                                            "Ganti Gambar"
                                        </button>
                                        {move || {
                                            if konfirmasi.get() == id {
                                                view! {
                                                    <div class="abn-konfirm">
                                                        <button
                                                            class="abn-btn abn-btn--danger"
                                                            disabled=move || busy.get()
                                                            on:click=on_delete
                                                        >
                                                            "Ya, hapus"
                                                        </button>
                                                        <button
                                                            class="abn-btn abn-btn--ghost"
                                                            on:click=move |_| konfirmasi.set(0)
                                                        >
                                                            "Batal"
                                                        </button>
                                                    </div>
                                                }
                                                    .into_any()
                                            } else {
                                                view! {
                                                    <button
                                                        class="abn-btn abn-btn--danger-ghost"
                                                        disabled=move || busy.get()
                                                        on:click=move |_| konfirmasi.set(id)
                                                    >
                                                        "Hapus"
                                                    </button>
                                                }
                                                    .into_any()
                                            }
                                        }}
                                    </div>
                                </div>
                            </article>
                        }
                    })
                    .collect_view()
                    .into_any()
            }}
        </section>
    }
}
