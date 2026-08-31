//! merchant_create_product.rs — Halaman Buat Product Baru (SSR + medit-* design).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::web::api::create_merchant_product;
use crate::web::app::AuthResource;
use crate::web::components::detail_image_section::{DetailImageDraft, DetailImagesSection};
use crate::web::components::product_story_preview::ProductStoryPreviewInline;
use crate::web::components::variant_editor::{new_variant_row, rows_to_json, VariantEditor, VariantRow};
use crate::web::services::product::DetailImagePayload;
use crate::web::hooks::ThemeToggle;
use crate::web::utils::{map_picker, map_set, DEFAULT_LAT, DEFAULT_LNG};

// Daftar kategori tinggal di `web::models` — satu sumber untuk formulir buat
// DAN sunting. Alias ini menjaga sisa berkas tak perlu ikut berubah.
use crate::web::models::PRODUCT_CATEGORIES as CATEGORIES;

/// Tanggal hari ini dalam format `YYYY-MM-DD`.
///
/// Dibaca dari jam PERAMBAN, bukan server: satu-satunya gunanya adalah mengisi
/// kolom `event_date` yang tak lagi ditanyakan, dan selisih zona waktu di situ
/// tak berpengaruh pada apa pun yang dilihat penjual maupun pembeli.
fn js_hari_ini() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        let d = js_sys::Date::new_0();
        return format!(
            "{:04}-{:02}-{:02}",
            d.get_full_year(),
            d.get_month() + 1,
            d.get_date()
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    "1970-01-01".to_string()
}

#[component]
pub fn MerchantCreateProductPage() -> impl IntoView {
    let _auth = use_context::<AuthResource>().expect("AuthResource missing");
    let _navigate = use_navigate();

    let f_name     = RwSignal::new(String::new());
    let f_desc     = RwSignal::new(String::new());
    let f_cat: RwSignal<Vec<String>> = RwSignal::new(vec![]);
    let f_date     = RwSignal::new(String::new());
    // Tiga sinyal ini DIBUANG bersama field-nya; nilainya kini ditetapkan di
    // pengirim (`"00:00"` dan string kosong). Menyimpan sinyal yang tak pernah
    // dibaca hanya menyisakan jejak bahwa field-nya "sebentar lagi kembali".

    let f_city     = RwSignal::new(String::new());
    let f_lat      = RwSignal::new(DEFAULT_LAT);
    let f_lng      = RwSignal::new(DEFAULT_LNG);
    let loc_touched = RwSignal::new(false);

    // Varian tiket: mulai dengan satu baris default (samakan dengan perilaku
    // lama "Umum"/gratis/kuota 100) — merchant tinggal mengubah/menambah.
    let v_rows: RwSignal<Vec<VariantRow>> =
        RwSignal::new(vec![new_variant_row(None, "Umum", "0", "100")]);
    let v_removed: RwSignal<Vec<String>> = RwSignal::new(vec![]);

    // Peta di-init dua jalur (idempoten, guard `_leaflet_id` di shell):
    // (1) skrip auto-init di shell via data-attribute — jalan tanpa hydration;
    // (2) Effect ini — jalur hydration/SPA sebagai cadangan.
    Effect::new(move |_| {
        map_picker(
            "create-loc-map",
            "create-lat",
            "create-lng",
            f_lat.get_untracked(),
            f_lng.get_untracked(),
        );
    });

    let cover_preview: RwSignal<Option<String>> = RwSignal::new(None);
    // URL permanen cover setelah di-upload ke /upload/merchant-image.
    let cover_url = RwSignal::new(String::new());
    let cover_uploading = RwSignal::new(false);
    // Persen unggahan cover (0–100). 0 = peramban tak melaporkan panjang total;
    // UI menampilkannya tanpa angka, bukan sebagai "macet di 0%".
    let cover_progress = RwSignal::new(0u8);
    // Foto detail product (galeri multi-foto, bisa di-drag urutannya).
    let drafts: RwSignal<Vec<DetailImageDraft>> = RwSignal::new(vec![]);

    let submitting = RwSignal::new(false);
    let error_msg  = RwSignal::new(String::new());
    let success_msg = RwSignal::new(String::new());

    // Cover: preview + upload SEKARANG ke storage (WASM-only). Menyimpan URL
    // permanen di `cover_url` agar submit tinggal mengirim string (tanpa File).
    let on_cover_change = move |ev: leptos::ev::Event| {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            if let Some(input) = ev.target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
            {
                if let Some(files) = input.files() {
                    if let Some(file) = files.get(0) {
                        if let Ok(url) = web_sys::Url::create_object_url_with_blob(&file) {
                            cover_preview.set(Some(url));
                        }
                        cover_uploading.set(true);
                        cover_progress.set(0);
                        leptos::task::spawn_local(async move {
                            let lapor = move |p: u8| cover_progress.set(p);
                            match crate::web::pages::merchant::upload_merchant_image_with_progress(
                                &file, lapor,
                            )
                            .await
                            {
                                Ok(u) => cover_url.set(u),
                                Err(e) => {
                                    // Sama seperti di halaman edit: kegagalan
                                    // yang hanya masuk console membuat product
                                    // tersimpan tanpa cover, tanpa ada yang tahu.
                                    web_sys::console::error_1(
                                        &format!("[Cover] upload gagal: {e}").into(),
                                    );
                                    error_msg.set(format!(
                                        "Foto cover gagal diunggah: {e}. Coba pilih ulang fotonya."
                                    ));
                                    cover_preview.set(None);
                                }
                            }
                            cover_uploading.set(false);
                        });
                    }
                }
            }
        }
        let _ = ev;
    };


    // ── Umpan balik harus TERLIHAT ────────────────────────────────────────────
    // Banner sukses/galat dirender di PUNCAK form, sedangkan tombol simpan ada
    // di DASAR form yang panjang. Akibatnya setiap penolakan validasi —
    // "Tanggal produk wajib diisi", "Tunggu foto selesai diunggah", atau galat
    // dari server — muncul di layar yang sedang tak dilihat siapa pun: pengguna
    // menekan SIMPAN, halaman diam, dan satu-satunya kesimpulan yang masuk akal
    // baginya adalah tombolnya rusak.
    //
    // Effect ini menggulirkan banner ke dalam pandangan begitu isinya berubah.
    // Hanya di klien (wasm) — tak ada yang perlu digulir saat render server.
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        let ada_galat = !error_msg.get().is_empty();
        let berhasil = !success_msg.get().is_empty();
        if !(ada_galat || berhasil) {
            return;
        }
        // `scroll_to_with_x_and_y`, bukan versi ber-`ScrollToOptions`: yang
        // terakhir menuntut dua fitur web-sys tambahan hanya demi animasi halus,
        // dan yang dibutuhkan di sini cuma pesannya terlihat.
        if let Some(win) = web_sys::window() {
            win.scroll_to_with_x_and_y(0.0, 0.0);
        }
    });

    let do_submit = move |_: leptos::ev::MouseEvent| {
        error_msg.set(String::new());
        success_msg.set(String::new());

        let name = f_name.get_untracked();
        if name.trim().is_empty() { error_msg.set("Nama product wajib diisi.".into()); return; }
        let desc = f_desc.get_untracked();
        if desc.trim().is_empty() { error_msg.set("Deskripsi product wajib diisi.".into()); return; }
        let cats = f_cat.get_untracked();
        if cats.is_empty() { error_msg.set("Pilih minimal satu kategori.".into()); return; }
        // Tanggal/waktu/nama-lokasi tak lagi ditanyakan (lihat catatan di
        // markup). Nilainya diisi di sini supaya bentuk permintaan ke server
        // tak berubah: `event_date` masih NOT NULL di basis data.
        //
        // Hari ini, bukan tanggal kosong: `event_date` dipakai urutan bawaan,
        // dan string kosong akan ditolak parser tanggal di server dengan pesan
        // yang menyebut field yang tak pernah dilihat penjual.
        let date = f_date.get_untracked();
        let date = if date.trim().is_empty() {
            js_hari_ini()
        } else {
            date
        };
        let time = "00:00".to_string();
        let venue = String::new();
        let city = f_city.get_untracked();
        if city.trim().is_empty() {
            error_msg.set("Kota wajib diisi.".into());
            return;
        }

        // Validasi + serialisasi varian tiket (nama/harga/kuota per baris).
        let variants_json = match rows_to_json(&v_rows.get_untracked(), &v_removed.get_untracked()) {
            Ok(j) => j,
            Err(m) => { error_msg.set(m); return; }
        };

        // Foto masih diunggah? Cegah simpan agar URL tak hilang.
        if cover_uploading.get_untracked() {
            let p = cover_progress.get_untracked();
            error_msg.set(if p >= 100 {
                "Foto cover sudah terkirim dan sedang diproses server. Sebentar lagi.".to_string()
            } else if p != 0 {
                format!("Foto cover baru terunggah {p}%. Tunggu sampai selesai.")
            } else {
                "Tunggu foto cover selesai diunggah.".to_string()
            });
            return;
        }
        // Unggahan yang GAGAL tak akan pernah selesai — dibedakan supaya
        // SIMPAN tidak tertahan permanen oleh pesan "tunggu".
        let foto = drafts.get_untracked();
        if foto.iter().any(|d| d.uploaded_url.is_none() && d.gagal.get_untracked()) {
            error_msg.set(
                "Ada foto detail yang gagal diunggah. Hapus foto itu, atau pilih ulang filenya."
                    .into(),
            );
            return;
        }
        if foto.iter().any(|d| d.uploaded_url.is_none()) {
            let belum: Vec<u8> = foto
                .iter()
                .filter(|d| d.uploaded_url.is_none())
                .map(|d| d.progres.get_untracked())
                .collect();
            let terkecil = belum.iter().copied().min().unwrap_or(0);
            error_msg.set(if terkecil >= 100 {
                format!(
                    "{} foto detail sudah terkirim dan sedang diproses server. Sebentar lagi.",
                    belum.len()
                )
            } else if terkecil != 0 {
                format!(
                    "{} foto detail masih diunggah (paling lambat {terkecil}%). Tunggu sampai selesai.",
                    belum.len()
                )
            } else {
                format!("{} foto detail masih diunggah. Tunggu sampai selesai.", belum.len())
            });
            return;
        }
        // Serialisasi foto detail terurut → JSON (foto lama & baru sama saja di
        // sini karena semuanya sudah punya URL permanen).
        let cover = cover_url.get_untracked();
        let payloads: Vec<DetailImagePayload> = drafts
            .get_untracked()
            .iter()
            .filter_map(|d| d.to_retain_payload())
            .collect();
        let detail_json = serde_json::to_string(&payloads).unwrap_or_else(|_| "[]".to_string());

        let cats_str = cats.join(",");
        let start_iso = format!("{}T{}:00Z", date, time);
        let (lat, lng) = if loc_touched.get_untracked() {
            (Some(f_lat.get_untracked()), Some(f_lng.get_untracked()))
        } else {
            (None, None)
        };
        submitting.set(true);

        leptos::task::spawn_local(async move {
            match create_merchant_product(name, desc, venue, city, start_iso.clone(), start_iso, cats_str, lat, lng, variants_json, cover, detail_json).await {
                Ok(_slug) => {
                    success_msg.set("Product berhasil dibuat!".into());
                    submitting.set(false);
                    #[cfg(target_arch = "wasm32")]
                    if let Some(win) = web_sys::window() {
                        let path = if _slug.is_empty() { "/merchant".to_string() }
                            else { format!("/merchant/products/{}/edit", _slug) };
                        let _ = win.location().replace(&path);
                    }
                }
                Err(e) => {
                    error_msg.set(format!("Gagal membuat product: {}", e));
                    submitting.set(false);
                }
            }
        });
    };

    view! {
        <div class="medit-page">
            <header class="page-header medit-page-header">
                <A href="/merchant" attr:class="back-btn" attr:aria-label="Kembali">
                    <svg
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                    >
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </A>
                <span class="page-logo">"BUAT PRODUCT"</span>
                <div class="header-actions">
                    <ThemeToggle />
                    <A href="/notifications" attr:class="bell-btn" attr:aria-label="Notifikasi">
                        <svg
                            width="18"
                            height="18"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2"
                            stroke-linecap="round"
                        >
                            <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
                            <path d="M13.73 21a2 2 0 0 1-3.46 0" />
                        </svg>
                    </A>
                </div>
            </header>

            <div class="medit-container">

                // ── Feedback ──────────────────────────────────────────────────
                {move || {
                    (!error_msg.get().is_empty())
                        .then(|| {
                            view! {
                                <div class="medit-error-banner">
                                    <svg
                                        width="14"
                                        height="14"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                    >
                                        <circle cx="12" cy="12" r="10" />
                                        <line x1="12" y1="8" x2="12" y2="12" />
                                        <line x1="12" y1="16" x2="12.01" y2="16" />
                                    </svg>
                                    {move || error_msg.get()}
                                </div>
                            }
                        })
                }}
                {move || {
                    (!success_msg.get().is_empty())
                        .then(|| {
                            view! {
                                <div class="medit-success-banner">
                                    <svg
                                        width="14"
                                        height="14"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        stroke-linecap="round"
                                    >
                                        <polyline points="20 6 9 17 4 12" />
                                    </svg>
                                    {move || success_msg.get()}
                                </div>
                            }
                        })
                }}
                // ── INFO DASAR ────────────────────────────────────────────────
                <div class="medit-section-header">
                    <span class="medit-section-label">"INFO DASAR"</span>
                </div>
                <div class="medit-field-group">
                    <label class="medit-field-label">"NAMA PRODUCT"</label>
                    <input
                        type="text"
                        class="medit-input"
                        placeholder="cth. Kaos Katun Lengan Panjang"
                        prop:value=move || f_name.get()
                        on:input=move |e| f_name.set(event_target_value(&e))
                    />
                </div>
                <div class="medit-field-group">
                    <label class="medit-field-label">"DESKRIPSI"</label>
                    <textarea
                        class="medit-input medit-textarea"
                        placeholder="Ceritakan tentang product Anda..."
                        prop:value=move || f_desc.get()
                        on:input=move |e| f_desc.set(event_target_value(&e))
                    ></textarea>
                </div>
                // ── KATEGORI ──────────────────────────────────────────────────
                <div class="medit-field-group">
                    <label class="medit-field-label">"KATEGORI"</label>
                    <div class="medit-category-grid">
                        {CATEGORIES
                            .iter()
                            .map(|cat| {
                                let c = cat.to_string();
                                let c2 = c.clone();
                                view! {
                                    <label class="medit-checkbox-label">
                                        <input
                                            type="checkbox"
                                            on:change=move |_| {
                                                f_cat
                                                    .update(|cats| {
                                                        if cats.contains(&c) {
                                                            cats.retain(|x| x != &c);
                                                        } else {
                                                            cats.push(c.clone());
                                                        }
                                                    });
                                            }
                                        />
                                        <span>{c2}</span>
                                    </label>
                                }
                            })
                            .collect_view()}
                    </div>
                </div>
                // ── FOTO COVER ────────────────────────────────────────────────
                <div class="medit-field-group">
                    <label class="medit-field-label">"FOTO COVER"</label>
                    <div class="medit-file-input-wrapper">
                        <input
                            type="file"
                            class="medit-file-input"
                            accept="image/*"
                            on:change=on_cover_change
                        />
                        <span class="medit-file-input-label">"PILIH FOTO"</span>
                    </div>
                    {move || {
                        cover_preview
                            .get()
                            .map(|url| {
                                view! {
                                    <div class="medit-cover-preview">
                                        <img src=url alt="Cover preview" />
                                    </div>
                                }
                            })
                    }}
                    // `!= 0`, BUKAN `> 0`: `>` di dalam nilai atribut makro
                    // view! diurai sebagai penutup tag. Nilainya juga dihitung
                    // di LUAR view! karena alasan yang sama.
                    // Dua fase — alasan lengkapnya di `merchant_edit_product.rs`:
                    // `xhr.upload.onprogress` melapor 100% begitu byte masuk buffer
                    // soket, sedangkan server baru mulai meneruskannya ke storage.
                    // 100% karena itu berganti jadi fase "memproses", bukan angka.
                    {move || cover_uploading.get().then(|| {
                        let p = cover_progress.get();
                        let diproses = p >= 100;
                        let terukur = p != 0 && !diproses;
                        let kelas_bilah = if terukur {
                            "h-full rounded-full bg-brand transition-[width] duration-200"
                        } else {
                            "h-full rounded-full bg-brand animate-pulse"
                        };
                        let gaya_bilah = if terukur { format!("width:{p}%") } else { "width:100%".to_string() };
                        let label = if diproses { "Memproses di server…" } else { "Mengunggah cover…" };
                        view! {
                            <div class="mt-1.5 flex flex-col gap-1.5">
                                <div class="flex items-center justify-between gap-2">
                                    <span class="text-[11px] text-content-muted">{label}</span>
                                    {terukur.then(|| view! {
                                        <span class="text-[11px] font-bold text-brand tabular-nums">
                                            {format!("{p}%")}
                                        </span>
                                    })}
                                </div>
                                <div class="h-1 w-full overflow-hidden rounded-full bg-elevated">
                                    <div class=kelas_bilah style=gaya_bilah />
                                </div>
                            </div>
                        }
                    })}
                </div>
                // ── FOTO DETAIL (galeri, bisa di-drag urutannya) ──────────────
                <div class="medit-field-group">
                    <label class="medit-field-label">"FOTO DETAIL"</label>
                    <DetailImagesSection drafts=drafts />
                </div>
                // ── STORY PREVIEW ─────────────────────────────────────────────
                <ProductStoryPreviewInline
                    title=Signal::derive(move || f_name.get())
                    cover_url=Signal::derive(move || cover_preview.get())
                    description=Signal::derive(move || f_desc.get())
                    on_share_click=Callback::new(move |_| {
                        #[cfg(target_arch = "wasm32")]
                        {
                            let params = web_sys::UrlSearchParams::new().expect("UrlSearchParams");
                            params.append("event_title", &f_name.get_untracked());
                            params
                                .append(
                                    "event_cover",
                                    &cover_preview.get_untracked().unwrap_or_default(),
                                );
                            params.append("product_desc", &f_desc.get_untracked());
                            params.append("event_slug", "draft");
                            params.append("from_create", "1");
                            let qs = params.to_string();
                            _navigate(&format!("/story?{}", qs), Default::default());
                        }
                    })
                />
                // ── TANGGAL, WAKTU, NAMA LOKASI: DIBUANG DARI FORMULIR ────────
                //
                // Ini toko, bukan penjualan tiket acara. Meminta penjual kaos
                // mengisi "waktu selesai" dan "nama lokasi (cth. Gelora Bung
                // Karno)" bukan sekadar berlebihan — ia membuat orang berhenti
                // dan menebak-nebak apa yang sebenarnya diminta, pada formulir
                // yang seharusnya bisa diselesaikan tanpa berpikir.
                //
                // Kolom `event_date` di basis data TETAP ADA dan tetap NOT NULL
                // — ia dipakai urutan bawaan (`ORDER BY e.event_date`) dan
                // beberapa tampilan. Yang berubah hanya SIAPA yang mengisinya:
                // sekarang server, dengan waktu produk dibuat. Membuat kolomnya
                // nullable berarti menyentuh setiap jalur baca yang sudah
                // mengandalkannya — perubahan yang jauh lebih besar daripada
                // yang diminta di sini, dan tak satu pun terlihat oleh penjual.
                //
                // `f_date` / `f_time` / `f_end_time` / `f_venue` masih dipakai
                // saat menyusun permintaan (diisi nilai bawaan), jadi model
                // server tak perlu ikut berubah.
                <div class="medit-field-group">
                    <label class="medit-field-label">"KOTA / LOKASI TOKO"</label>
                    <input
                        type="text"
                        class="medit-input"
                        placeholder="cth. Jakarta Pusat — asal pengiriman"
                        prop:value=move || f_city.get()
                        on:input=move |e| f_city.set(event_target_value(&e))
                    />
                </div>
                // ── VARIAN PRODUK (ukuran/warna + harga + stok) ───────────────
                <VariantEditor rows=v_rows removed_ids=v_removed />
                // ── LOKASI DI PETA ────────────────────────────────────────────
                <div class="medit-section-header">
                    <span class="medit-section-label">"LOKASI DI PETA"</span>
                </div>
                <p style="font-size:12px;color:var(--text-muted);margin:0 0 10px">
                    "Klik peta atau geser pin untuk menandai lokasi toko. Koordinat terisi otomatis."
                </p>
                <div
                    id="create-loc-map"
                    data-map-picker="1"
                    data-lat-input="create-lat"
                    data-lng-input="create-lng"
                    style="width:100%;height:300px;border-radius:12px;overflow:hidden;border:1px solid var(--border);margin-bottom:12px;background:var(--bg-elevated)"
                ></div>
                <div class="medit-grid-2">
                    <div class="medit-field-group">
                        <label class="medit-field-label">"LATITUDE"</label>
                        <input
                            id="create-lat"
                            type="number"
                            step="any"
                            class="medit-input"
                            placeholder="-6.2088"
                            prop:value=move || f_lat.get().to_string()
                            on:input=move |e| {
                                loc_touched.set(true);
                                if let Ok(v) = event_target_value(&e).parse::<f64>() {
                                    f_lat.set(v);
                                    map_set("create-loc-map", v, f_lng.get_untracked());
                                }
                            }
                        />
                    </div>
                    <div class="medit-field-group">
                        <label class="medit-field-label">"LONGITUDE"</label>
                        <input
                            id="create-lng"
                            type="number"
                            step="any"
                            class="medit-input"
                            placeholder="106.8456"
                            prop:value=move || f_lng.get().to_string()
                            on:input=move |e| {
                                loc_touched.set(true);
                                if let Ok(v) = event_target_value(&e).parse::<f64>() {
                                    f_lng.set(v);
                                    map_set("create-loc-map", f_lat.get_untracked(), v);
                                }
                            }
                        />
                    </div>
                </div>
                // ── SUBMIT ────────────────────────────────────────────────────
                <div class="medit-actions">
                    <button
                        class="medit-submit-btn"
                        disabled=move || submitting.get()
                        on:click=do_submit
                    >
                        {move || if submitting.get() { "Membuat Product..." } else { "BUAT PRODUCT" }}
                    </button>
                    <A href="/merchant" attr:class="medit-cancel-btn">
                        "BATAL"
                    </A>
                </div>

            </div>
        </div>
    }
}
