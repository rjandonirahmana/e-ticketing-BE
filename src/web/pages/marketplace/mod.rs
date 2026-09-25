//! web/pages/marketplace — Marketplace C2C ("Hub"): feed jual/cari barang
//! baru/bekas oleh SEMUA user terdaftar, transaksi COD lewat chat (bukan
//! payment gateway). Terpisah dari `explore/` (yang tetap murni event/tiket
//! merchant) — lihat `service/post.rs` untuk aturan validasi lengkap.

pub mod create;
pub mod detail;

pub use create::CreatePostPage;
pub use detail::PostDetailPage;

use leptos::prelude::*;
use leptos_meta::*;

use leptos_router::components::A;

use crate::web::components::{BottomNav, EmptyState, PostCard, PostCardShimmer};
use crate::web::models::PRODUCT_CATEGORIES;

#[cfg(feature = "hydrate")]
use send_wrapper::SendWrapper;
#[cfg(feature = "hydrate")]
use wasm_bindgen::JsCast;

#[component]
pub fn MarketplaceFeed() -> impl IntoView {
    // "" = Semua. Disimpan sebagai String (bukan Option) supaya tombol filter
    // punya satu sumber kebenaran sederhana untuk kelas `--on`.
    let kind = RwSignal::new(String::new());
    let category = RwSignal::new(String::new());
    let city = RwSignal::new(String::new());
    let q = RwSignal::new(String::new());

    let items = RwSignal::new(Vec::<crate::web::models::Post>::new());
    let page = RwSignal::new(1i64);
    let total = RwSignal::new(0i64);
    let loading = RwSignal::new(true);
    let loading_more = RwSignal::new(false);
    let has_more = RwSignal::new(false);
    let error = RwSignal::new(String::new());

    // Muat halaman 1 dari filter yang berlaku (reset total). Dipisah dari
    // `load_more` karena keduanya punya penjaga beda: reset boleh menimpa
    // walau sedang loading (filter baru membatalkan permintaan lama secara
    // logis), load_more TIDAK boleh dobel-jalan.
    let reset_and_load = move || {
        loading.set(true);
        error.set(String::new());
        let k = kind.get_untracked();
        let c = category.get_untracked();
        let ct = city.get_untracked();
        let qq = q.get_untracked();
        leptos::task::spawn_local(async move {
            let res = crate::web::api::list_posts(
                (!k.is_empty()).then_some(k),
                (!c.is_empty()).then_some(c),
                (!ct.is_empty()).then_some(ct),
                (!qq.is_empty()).then_some(qq),
                Some(1),
            )
            .await;
            match res {
                Ok(pg) => {
                    has_more.set(pg.page < pg.total_pages);
                    total.set(pg.total);
                    items.set(pg.data);
                    page.set(1);
                }
                Err(e) => error.set(e.to_string()),
            }
            loading.set(false);
        });
    };

    // Dipakai hanya di listener scroll `#[cfg(feature = "hydrate")]` di bawah —
    // tanpa `allow`, build SSR-saja (yang tak pernah memasang listener DOM)
    // melihatnya seolah tak terpakai.
    #[allow(unused_variables)]
    let load_more = move || {
        if loading_more.get_untracked() || loading.get_untracked() || !has_more.get_untracked() {
            return;
        }
        loading_more.set(true);
        let k = kind.get_untracked();
        let c = category.get_untracked();
        let ct = city.get_untracked();
        let qq = q.get_untracked();
        let next = page.get_untracked() + 1;
        leptos::task::spawn_local(async move {
            let res = crate::web::api::list_posts(
                (!k.is_empty()).then_some(k),
                (!c.is_empty()).then_some(c),
                (!ct.is_empty()).then_some(ct),
                (!qq.is_empty()).then_some(qq),
                Some(next),
            )
            .await;
            if let Ok(pg) = res {
                page.set(next);
                has_more.set(pg.page < pg.total_pages);
                items.update(|v| v.extend(pg.data));
            }
            loading_more.set(false);
        });
    };

    // Muat awal + muat ulang tiap filter berganti.
    Effect::new(move |_| {
        kind.track();
        category.track();
        city.track();
        reset_and_load();
    });

    // Infinite scroll — pola sama dengan `explore/mod.rs`: prefetch ~2.5 layar
    // sebelum ujung dokumen.
    #[cfg(feature = "hydrate")]
    {
        let scroll_cb: StoredValue<Option<SendWrapper<wasm_bindgen::closure::Closure<dyn Fn()>>>> =
            StoredValue::new(None);
        Effect::new(move |_| {
            let cb = wasm_bindgen::closure::Closure::<dyn Fn()>::new(move || {
                let Some(win) = web_sys::window() else { return };
                let inner_h = win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
                let scroll_y = win.scroll_y().unwrap_or(0.0);
                let doc_h = win
                    .document()
                    .and_then(|d| d.document_element())
                    .map(|e| e.scroll_height() as f64)
                    .unwrap_or(0.0);
                let threshold = (inner_h * 2.5).max(1200.0);
                if doc_h - (scroll_y + inner_h) < threshold {
                    load_more();
                }
            });
            if let Some(win) = web_sys::window() {
                let _ = win.add_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref());
            }
            scroll_cb.set_value(Some(SendWrapper::new(cb)));
        });
        on_cleanup(move || {
            if let Some(Some(cb)) = scroll_cb.try_update_value(|o| o.take()) {
                if let Some(win) = web_sys::window() {
                    let _ = win.remove_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref());
                }
                drop(cb);
            }
        });
    }

    view! {
        <Title text="Hub — PULSE" />
        <Meta
            name="description"
            content="Jual dan cari barang baru/bekas dari sesama pengguna PULSE. COD, bayar tunai saat ketemu langsung."
        />
        <div class="page pk-page">
            <header class="page-header pk-header">
                <span class="page-logo">"HUB"</span>
                <A href="/marketplace/new" attr:class="pk-post-btn">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                         stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
                        <line x1="12" y1="5" x2="12" y2="19" />
                        <line x1="5" y1="12" x2="19" y2="12" />
                    </svg>
                    "Posting"
                </A>
            </header>

            <div class="pk-searchbar-row">
                <div class="pk-search-wrap">
                    <span class="pk-search-icon">
                        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                             stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                            <circle cx="11" cy="11" r="8" />
                            <line x1="21" y1="21" x2="16.65" y2="16.65" />
                        </svg>
                    </span>
                    <input
                        class="pk-search"
                        type="text"
                        placeholder="Cari barang..."
                        prop:value=move || q.get()
                        on:input=move |e| q.set(event_target_value(&e))
                        on:keydown=move |e| {
                            if e.key() == "Enter" {
                                reset_and_load();
                            }
                        }
                    />
                </div>
            </div>

            <div class="pk-chips">
                <button
                    class=move || if kind.get().is_empty() { "pk-chip pk-chip--on" } else { "pk-chip" }
                    on:click=move |_| kind.set(String::new())
                >"SEMUA"</button>
                <button
                    class=move || if kind.get() == "jual" { "pk-chip pk-chip--on" } else { "pk-chip" }
                    on:click=move |_| kind.set("jual".into())
                >"JUAL"</button>
                <button
                    class=move || if kind.get() == "cari" { "pk-chip pk-chip--on" } else { "pk-chip" }
                    on:click=move |_| kind.set("cari".into())
                >"CARI"</button>
            </div>

            <div class="pk-filters-row">
                <select
                    class="pk-select"
                    on:change=move |e| category.set(event_target_value(&e))
                >
                    <option value="">"Semua Kategori"</option>
                    {PRODUCT_CATEGORIES
                        .iter()
                        .map(|c| view! { <option value=*c>{*c}</option> })
                        .collect_view()}
                </select>
                <input
                    class="pk-select"
                    type="text"
                    placeholder="Kota"
                    prop:value=move || city.get()
                    on:change=move |e| city.set(event_target_value(&e))
                />
            </div>

            <div class="pk-results-bar">
                <span class="pk-results-count"><b>{move || total.get().max(0)}</b>" barang ditemukan"</span>
            </div>

            <div class="pk-feed">
                {move || {
                    if loading.get() {
                        let shims = (0..6).map(|_| view! { <PostCardShimmer /> }).collect_view();
                        view! { <div class="pk-grid">{shims}</div> }.into_any()
                    } else if !error.with(|e| e.is_empty()) {
                        view! {
                            <div class="pk-empty">
                                <EmptyState icon="⚠️" title="Gagal Memuat" body=error.get() />
                            </div>
                        }.into_any()
                    } else {
                        let list = items.get();
                        if list.is_empty() {
                            view! {
                                <div class="pk-empty">
                                    <EmptyState icon="🛍️" title="Belum Ada Barang" body="Jadi yang pertama posting di sini." />
                                </div>
                            }.into_any()
                        } else {
                            let cards = list
                                .into_iter()
                                .enumerate()
                                .map(|(i, p)| view! { <PostCard post=p index=i /> })
                                .collect_view();
                            let shims = loading_more
                                .get()
                                .then(|| (0..4).map(|_| view! { <PostCardShimmer /> }).collect_view());
                            view! { <div class="pk-grid">{cards}{shims}</div> }.into_any()
                        }
                    }
                }}
            </div>

            <BottomNav active="marketplace" />
        </div>
    }
}

// ── Upload foto posting ──────────────────────────────────────────────────────

/// Sama persis dengan `merchant::upload_merchant_image_with_progress` (XHR +
/// kompresi + pelaporan progres) tapi menembak `/upload/post-image` —
/// auth-only, tanpa gerbang peran merchant, karena posting Pasar boleh dibuat
/// SEMUA user. Kompresi dipakai bersama (`merchant::kompres_gambar`) — dua
/// jalur unggah gambar tak perlu dua salinan logika resize.
#[cfg(target_arch = "wasm32")]
pub(crate) async fn upload_post_image_with_progress(
    file: &web_sys::File,
    on_progress: impl Fn(u8) + 'static,
) -> Result<String, String> {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    let form = web_sys::FormData::new().map_err(|e| format!("{:?}", e))?;
    let hasil = match crate::web::pages::merchant::kompres_gambar(file).await {
        Some(kecil) => form.append_with_blob_and_filename("file", &kecil, "unggahan.webp"),
        None => form.append_with_blob("file", file),
    };
    hasil.map_err(|e| format!("{:?}", e))?;

    let xhr = web_sys::XmlHttpRequest::new().map_err(|e| format!("{:?}", e))?;
    xhr.open_with_async("POST", "/upload/post-image", true)
        .map_err(|e| format!("{:?}", e))?;

    let upload = xhr.upload().map_err(|e| format!("{:?}", e))?;
    let cb_progress = Closure::<dyn FnMut(web_sys::ProgressEvent)>::new(
        move |e: web_sys::ProgressEvent| {
            if !e.length_computable() {
                return;
            }
            let total = e.total();
            if total <= 0.0 {
                return;
            }
            let persen = ((e.loaded() / total) * 100.0).round().clamp(0.0, 100.0);
            on_progress(persen as u8);
        },
    );
    upload.set_onprogress(Some(cb_progress.as_ref().unchecked_ref()));

    let (tx, rx) = futures::channel::oneshot::channel::<()>();
    let tx = std::cell::RefCell::new(Some(tx));
    let cb_selesai = Closure::<dyn FnMut()>::new(move || {
        if let Some(tx) = tx.borrow_mut().take() {
            let _ = tx.send(());
        }
    });
    xhr.set_onloadend(Some(cb_selesai.as_ref().unchecked_ref()));

    xhr.send_with_opt_form_data(Some(&form))
        .map_err(|e| format!("{:?}", e))?;

    let _ = rx.await;

    upload.set_onprogress(None);
    xhr.set_onloadend(None);
    drop(cb_progress);
    drop(cb_selesai);

    let status = xhr.status().map_err(|e| format!("{:?}", e))?;
    if status == 0 {
        return Err("Koneksi terputus saat mengunggah.".to_string());
    }
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }

    let teks = xhr
        .response_text()
        .map_err(|e| format!("{:?}", e))?
        .unwrap_or_default();
    let json = js_sys::JSON::parse(&teks).map_err(|_| "Jawaban server bukan JSON".to_string())?;
    js_sys::Reflect::get(&json, &wasm_bindgen::JsValue::from_str("url"))
        .ok()
        .and_then(|v| v.as_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "URL kosong dari server".to_string())
}
