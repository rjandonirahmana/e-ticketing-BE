//! banners_tab.rs — Tab "Spanduk" Pusat Admin: kelola banner slider explore.
//!
//! Admin dapat: unggah banner baru (gambar via POST /upload/merchant-image —
//! endpoint menerima role admin — lalu `create_banner`), mengganti gambar /
//! link banner lama (`update_banner`), dan menghapus (`delete_banner`,
//! soft-delete). Setiap aksi menginvalidasi cache banner publik di server
//! sehingga slider explore langsung segar.

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

pub(super) fn view_banners(
    banners: Vec<Banner>,
    refetch: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let busy = RwSignal::new(false);
    let msg = RwSignal::new(String::new());
    let new_link = RwSignal::new(String::new());
    let new_file_ref: NodeRef<leptos::html::Input> = NodeRef::new();
    // Target "Ganti Gambar": id banner yang sedang diganti gambarnya. Satu
    // input file tersembunyi dipakai bersama semua baris.
    let swap_target: StoredValue<i64> = StoredValue::new(0);
    let swap_file_ref: NodeRef<leptos::html::Input> = NodeRef::new();

    // ── Unggah + buat banner baru ─────────────────────────────────────────────
    let on_create = move |_| {
        if busy.get_untracked() {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let Some(file) = first_file(new_file_ref) else {
                msg.set("Pilih file gambar dulu.".into());
                return;
            };
            busy.set(true);
            msg.set("Mengunggah…".into());
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
                        msg.set("Banner ditambahkan.".into());
                        new_link.set(String::new());
                        if let Some(el) = new_file_ref.get_untracked() {
                            el.set_value("");
                        }
                        refetch();
                    }
                    Err(e) => msg.set(format!("Gagal: {e}")),
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
            msg.set("Mengunggah gambar baru…".into());
            leptos::task::spawn_local(async move {
                let res = async {
                    let url =
                        crate::web::pages::merchant::upload_merchant_image(&file).await?;
                    update_banner(id, Some(url), None).await.map_err(|e| e.to_string())
                }
                .await;
                match res {
                    Ok(()) => {
                        msg.set(format!("Gambar banner #{id} diperbarui."));
                        refetch();
                    }
                    Err(e) => msg.set(format!("Gagal: {e}")),
                }
                if let Some(el) = swap_file_ref.get_untracked() {
                    el.set_value("");
                }
                busy.set(false);
            });
        }
    };

    view! {
        <section class="mhub-products-section">
            <div class="mhub-products-header">
                <h3 class="mhub-products-title">"Pengelolaan Spanduk"</h3>
            </div>

            // ── Form tambah banner ────────────────────────────────────────────
            <div class="admin-banner-form">
                <input type="file" accept="image/*" node_ref=new_file_ref />
                <input
                    type="text"
                    placeholder="Link tujuan klik (opsional, mis. /products/slug)"
                    prop:value=move || new_link.get()
                    on:input=move |ev| new_link.set(event_target_value(&ev))
                />
                <div class="admin-banner-actions">
                    <button
                        class="admin-banner-btn"
                        disabled=move || busy.get()
                        on:click=on_create
                    >
                        "Unggah & Tayangkan"
                    </button>
                    <span class="admin-banner-msg">{move || msg.get()}</span>
                </div>
            </div>

            // Input file tersembunyi untuk aksi "Ganti Gambar" per baris.
            <input
                type="file"
                accept="image/*"
                style="display:none"
                node_ref=swap_file_ref
                on:change=on_swap_change
            />

            {if banners.is_empty() {
                view! {
                    <div class="mhub-empty">
                        <div class="mhub-empty-icon-wrap">
                            <svg width="38" height="38" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                <rect x="3" y="3" width="18" height="18" rx="2"/>
                                <path d="M3 9h18M9 21V9"/>
                            </svg>
                        </div>
                        <p class="mhub-empty-title">"Belum Ada Spanduk"</p>
                        <p class="mhub-empty-body">
                            "Unggah banner pertama lewat form di atas — langsung tampil di slider explore."
                        </p>
                    </div>
                }.into_any()
            } else {
                banners.into_iter().map(|b| {
                    let id = b.id;
                    let img = b.image_url.clone();
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
                                    msg.set(format!("Link banner #{id} disimpan."));
                                    refetch();
                                }
                                Err(e) => msg.set(format!("Gagal: {e}")),
                            }
                            busy.set(false);
                        });
                    };
                    let on_delete = move |_| {
                        if busy.get_untracked() {
                            return;
                        }
                        busy.set(true);
                        leptos::task::spawn_local(async move {
                            match delete_banner(id).await {
                                Ok(()) => {
                                    msg.set(format!("Banner #{id} dihapus."));
                                    refetch();
                                }
                                Err(e) => msg.set(format!("Gagal: {e}")),
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
                        <div class="admin-banner-row">
                            <div class="admin-banner-thumb">
                                {if img.is_empty() {
                                    view! { <div class="admin-banner-no-img">"🖼"</div> }.into_any()
                                } else {
                                    view! {
                                        <img src=img alt=format!("Banner #{id}")
                                             class="admin-banner-img"/>
                                    }.into_any()
                                }}
                            </div>
                            <div class="admin-banner-info">
                                <p class="admin-banner-title">"Spanduk #"{id}</p>
                                <input
                                    type="text"
                                    placeholder="Link tujuan klik"
                                    prop:value=move || link_sig.get()
                                    on:input=move |ev| link_sig.set(event_target_value(&ev))
                                />
                                <div class="admin-banner-actions">
                                    <button
                                        class="admin-banner-btn admin-banner-btn--ghost"
                                        disabled=move || busy.get()
                                        on:click=on_save_link
                                    >
                                        "Simpan Link"
                                    </button>
                                    <button
                                        class="admin-banner-btn admin-banner-btn--ghost"
                                        disabled=move || busy.get()
                                        on:click=on_swap
                                    >
                                        "Ganti Gambar"
                                    </button>
                                    <button
                                        class="admin-banner-btn admin-banner-btn--danger"
                                        disabled=move || busy.get()
                                        on:click=on_delete
                                    >
                                        "Hapus"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </section>
    }
}
