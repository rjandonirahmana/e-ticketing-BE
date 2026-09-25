//! web/pages/marketplace/create.rs — `/marketplace/new`: form posting jual/cari
//! barang. Foto sudah di-upload SAAT dipilih (lihat `DetailImagesSection` +
//! `upload_post_image_with_progress`) — submit tinggal kirim URL yang sudah jadi.

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::web::components::detail_image_section::{DetailImageDraft, DetailImagesSection};
use crate::web::components::IconBack;
use crate::web::models::PRODUCT_CATEGORIES;

#[component]
pub fn CreatePostPage() -> impl IntoView {
    let kind = RwSignal::new("jual".to_string());
    let title = RwSignal::new(String::new());
    let description = RwSignal::new(String::new());
    let price = RwSignal::new(String::new());
    let condition = RwSignal::new("baru".to_string());
    let category = RwSignal::new(String::new());
    let city = RwSignal::new(String::new());
    let drafts: RwSignal<Vec<DetailImageDraft>> = RwSignal::new(vec![]);

    let submitting = RwSignal::new(false);
    let error = RwSignal::new(String::new());

    let nav = use_navigate();
    let submit = move |_| {
        if submitting.get_untracked() {
            return;
        }
        let t = title.get_untracked().trim().to_string();
        let d = description.get_untracked().trim().to_string();
        if t.is_empty() || d.is_empty() {
            error.set("Judul dan deskripsi wajib diisi.".into());
            return;
        }
        // Foto yang belum kelar upload (belum ada uploaded_url) TIDAK ikut
        // dikirim — konsisten dengan cara halaman produk menahan submit saat
        // ada unggahan yang masih berjalan, hanya di sini dilonggarkan
        // (marketplace tak wajib punya foto) daripada mengunci tombol.
        let images: Vec<String> = drafts.get_untracked().iter().filter_map(|dr| dr.uploaded_url.clone()).collect();

        let k = kind.get_untracked();
        let price_val: Option<i64> = {
            let p = price.get_untracked();
            let p = p.trim();
            if p.is_empty() { None } else { p.parse::<i64>().ok() }
        };
        let cond = if k == "jual" { Some(condition.get_untracked()) } else { None };
        let cat = {
            let c = category.get_untracked();
            (!c.is_empty()).then_some(c)
        };
        let ct = {
            let c = city.get_untracked();
            (!c.is_empty()).then_some(c)
        };

        submitting.set(true);
        error.set(String::new());
        let nav = nav.clone();
        leptos::task::spawn_local(async move {
            match crate::web::api::create_post(k, t, d, price_val, cond, cat, ct, images).await {
                Ok(post) => {
                    nav(&format!("/marketplace/{}", post.id), Default::default());
                }
                Err(e) => {
                    error.set(format!("Gagal memposting: {e}"));
                    submitting.set(false);
                }
            }
        });
    };

    view! {
        <Title text="Posting Barang — Hub PULSE" />
        <div class="page pk-create-page">
            <header class="page-header pk-header">
                <A href="/marketplace" attr:class="pk-back-btn" attr:aria-label="Kembali">
                    <IconBack />
                </A>
                <span class="page-logo">"POSTING BARANG"</span>
            </header>

            <div class="pk-form">
                <div class="pk-form-row pk-kind-toggle">
                    <button
                        class=move || if kind.get() == "jual" { "pk-toggle-btn pk-toggle-btn--on" } else { "pk-toggle-btn" }
                        on:click=move |_| kind.set("jual".into())
                    >
                        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                             stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M20.59 13.41L11 3.83a2 2 0 00-1.41-.58H4a1 1 0 00-1 1v5.59a2 2 0 00.58 1.41l9.58 9.59a2 2 0 002.83 0l4.59-4.59a2 2 0 000-2.83z" />
                            <circle cx="7.5" cy="7.5" r="1.5" fill="currentColor" stroke="none" />
                        </svg>
                        "Jual Barang"
                    </button>
                    <button
                        class=move || if kind.get() == "cari" { "pk-toggle-btn pk-toggle-btn--on" } else { "pk-toggle-btn" }
                        on:click=move |_| kind.set("cari".into())
                    >
                        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                             stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <circle cx="11" cy="11" r="8" />
                            <line x1="21" y1="21" x2="16.65" y2="16.65" />
                        </svg>
                        "Cari Barang"
                    </button>
                </div>

                <div class="pk-form-row">
                    <label class="pk-label">"Judul"</label>
                    <input
                        class="pk-input"
                        type="text"
                        placeholder="cth. Sepeda gunung ukuran M"
                        prop:value=move || title.get()
                        on:input=move |e| title.set(event_target_value(&e))
                    />
                </div>

                <div class="pk-form-row">
                    <label class="pk-label">"Deskripsi"</label>
                    <textarea
                        class="pk-input pk-textarea"
                        placeholder="Ceritakan kondisi barang, alasan jual/cari, dsb."
                        prop:value=move || description.get()
                        on:input=move |e| description.set(event_target_value(&e))
                    />
                </div>

                <div class="pk-form-row">
                    <label class="pk-label">
                        {move || if kind.get() == "cari" { "Perkiraan Budget (opsional)" } else { "Harga (kosongkan bila nego)" }}
                    </label>
                    <input
                        class="pk-input"
                        type="number"
                        inputmode="numeric"
                        placeholder="cth. 150000"
                        prop:value=move || price.get()
                        on:input=move |e| price.set(event_target_value(&e))
                    />
                </div>

                {move || {
                    (kind.get() == "jual").then(|| view! {
                        <div class="pk-form-row">
                            <label class="pk-label">"Kondisi"</label>
                            <div class="pk-kind-toggle">
                                <button
                                    class=move || if condition.get() == "baru" { "pk-toggle-btn pk-toggle-btn--on" } else { "pk-toggle-btn" }
                                    on:click=move |_| condition.set("baru".into())
                                >"Baru"</button>
                                <button
                                    class=move || if condition.get() == "bekas" { "pk-toggle-btn pk-toggle-btn--on" } else { "pk-toggle-btn" }
                                    on:click=move |_| condition.set("bekas".into())
                                >"Bekas"</button>
                            </div>
                        </div>
                    })
                }}

                <div class="pk-form-row">
                    <label class="pk-label">"Kategori"</label>
                    <select class="pk-input" on:change=move |e| category.set(event_target_value(&e))>
                        <option value="">"Pilih kategori"</option>
                        {PRODUCT_CATEGORIES.iter().map(|c| view! { <option value=*c>{*c}</option> }).collect_view()}
                    </select>
                </div>

                <div class="pk-form-row">
                    <label class="pk-label">"Kota"</label>
                    <input
                        class="pk-input"
                        type="text"
                        placeholder="cth. Jakarta Selatan"
                        prop:value=move || city.get()
                        on:input=move |e| city.set(event_target_value(&e))
                    />
                </div>

                <div class="pk-form-row">
                    <label class="pk-label">"Foto"</label>
                    <DetailImagesSection drafts=drafts max=6 for_marketplace=true />
                </div>

                {move || {
                    let msg = error.get();
                    (!msg.is_empty()).then(|| view! { <p class="pk-form-error">{msg}</p> })
                }}

                <button class="pk-submit-btn" disabled=move || submitting.get() on:click=submit>
                    {move || if submitting.get() { "Memposting..." } else { "Posting Sekarang" }}
                </button>
            </div>
        </div>
    }
}
