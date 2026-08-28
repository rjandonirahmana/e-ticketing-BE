//! merchant_edit_product.rs — Halaman Edit Product (SSR + medit-* design).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::web::api::{get_merchant_product_detail, update_merchant_product};
use crate::web::components::detail_image_section::{DetailImageDraft, DetailImagesSection};
use crate::web::components::product_story_preview::ProductStoryPreviewInline;
use crate::web::components::variant_editor::{rows_from_product, rows_to_json, VariantEditor, VariantRow};
use crate::web::services::product::DetailImagePayload;
use crate::web::hooks::ThemeToggle;
use crate::web::utils::{map_picker, map_set, DEFAULT_LAT, DEFAULT_LNG};

// Daftar kategori tinggal di `web::models` — satu sumber untuk formulir buat
// DAN sunting. Alias ini menjaga sisa berkas tak perlu ikut berubah.
use crate::web::models::PRODUCT_CATEGORIES as CATEGORIES;

// ── Main page ─────────────────────────────────────────────────────────────────
//
// Catatan: `EditSkeleton` dibuang bersama `<Suspense>` yang membungkus form.
// Skeleton itu bukan sekadar hiasan yang hilang — ia adalah cabang DOM yang
// berbeda dari form, dan pergantian cabang itulah yang membuat halaman ini
// kadang terpampang penuh tapi mati total. Form kini selalu dirender; keadaan
// "sedang memuat" disampaikan lewat satu baris teks dan tombol yang nonaktif.

#[component]
pub fn MerchantEditProductPage() -> impl IntoView {
    let params = use_params_map();
    let slug = move || params.read().get("slug").unwrap_or_default();

    // `AuthResource` tak lagi dibaca di sini — wewenang ditegakkan server
    // (`require_roles` di `get_merchant_product_detail`), dan `MerchantGuard` di
    // tabel route sudah menahan yang bukan merchant sebelum halaman ini dirender.

    // ── Skeleton TIDAK BOLEH ABADI ───────────────────────────────────────────
    // Keadaan `not_ready` (sesi belum terbaca / slug belum ada) dirender sebagai
    // skeleton yang sama persis dengan keadaan "sedang memuat". Keduanya tak
    // bisa dibedakan mata, dan tak satu pun punya jalan keluar: bila sesuatu
    // membuat data tak pernah datang — permintaan menggantung, sesi tak kunjung
    // terbaca — halaman berkedip selamanya tanpa memberi tahu apa pun.
    //
    // Penanda ini menyala sesudah tenggang wajar dan mengubah kedipan itu jadi
    // pesan yang bisa ditindaklanjuti. Ia TIDAK membatalkan apa pun: kalau
    // datanya akhirnya datang, halaman tetap terisi seperti biasa.
    let terlalu_lama = RwSignal::new(false);
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |sudah: Option<()>| {
        // Sekali saja per kunjungan halaman.
        if sudah.is_some() {
            return;
        }
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(12_000).await;
            terlalu_lama.set(true);
        });
    });

    // ── PENGGERBANG `is_logged_in()` DIBUANG ────────────────────────────────
    // Sumber resource ini dulu `(slug(), is_logged_in())`, dan fetcher-nya
    // menolak dengan `not_ready` selama sesi belum terbaca. Itu memasang dua
    // kegagalan sekaligus:
    //
    //   1. Halaman berkedip selamanya. `not_ready` dirender sebagai skeleton
    //      yang sama persis dengan "sedang memuat", tanpa pesan dan tanpa
    //      percobaan ulang — persis "shimmer terus, refresh baru muncul".
    //
    //   2. SIMPAN mati diam-diam, dan ini yang paling menipu. Bila data tak
    //      pernah datang, Effect penyemai tak pernah jalan, sehingga SELURUH
    //      signal form tetap kosong — termasuk `f_name`. Isian di layar tampak
    //      terisi karena HTML dari server masih terpampang, tapi yang dibaca
    //      `do_save` adalah signal, dan `f_name` yang kosong langsung tertahan
    //      di `name.trim().len() < 3`. Tombolnya berfungsi; datanya yang tak
    //      pernah ada.
    //
    // Penggerbang itu juga TAK MENJAGA APA PUN: `get_merchant_product_detail`
    // memanggil `require_roles(&["merchant","admin"])` di server. Menebak status
    // login di klien lebih dulu hanya menambah satu cara untuk gagal, tanpa
    // menambah satu pun lapis keamanan.
    //
    // Sekarang: satu-satunya syarat adalah slug ada. Yang belum masuk akan
    // dijawab 401 oleh server dan tampil sebagai galat yang jujur.
    let product_data = Resource::new(
        slug,
        |s| async move {
            if s.is_empty() {
                return Err(ServerFnError::ServerError("not_ready".into()));
            }
            get_merchant_product_detail(s).await
        },
    );

    let f_name     = RwSignal::new(String::new());
    let f_desc     = RwSignal::new(String::new());
    let f_venue    = RwSignal::new(String::new());
    let f_city     = RwSignal::new(String::new());
    let f_date     = RwSignal::new(String::new());
    let f_time     = RwSignal::new(String::new());
    let f_cat: RwSignal<Vec<String>> = RwSignal::new(vec![]);
    let f_lat      = RwSignal::new(DEFAULT_LAT);
    let f_lng      = RwSignal::new(DEFAULT_LNG);
    let loc_touched = RwSignal::new(false);
    let initialized = RwSignal::new(false);

    // Varian tiket: di-prefill dari data product saat termuat (Effect di bawah).
    let v_rows: RwSignal<Vec<VariantRow>> = RwSignal::new(vec![]);
    let v_removed: RwSignal<Vec<String>> = RwSignal::new(vec![]);

    // Peta di-init oleh skrip auto-init di shell (via data-attribute pada div) —
    // tak bergantung hydration. Saat data product termuat, pin disetel ke lokasi
    // tersimpan lewat `map_set` di Effect di bawah.
    Effect::new(move |_| {
        if initialized.get() {
            let la = f_lat.get_untracked();
            let lo = f_lng.get_untracked();
            // Buat peta bila belum dibuat auto-init (jalur hydration), lalu pusatkan
            // ke lokasi tersimpan (kalau peta sudah dibuat auto-init di koordinat default).
            map_picker("edit-loc-map", "edit-lat", "edit-lng", la, lo);
            map_set("edit-loc-map", la, lo);
        }
    });

    let cover_preview: RwSignal<Option<String>> = RwSignal::new(None);
    // Cover yang tersimpan di server. Dipegang terpisah supaya pratinjau bisa
    // dikembalikan apa adanya bila unggahan cover baru gagal.
    let cover_lama: RwSignal<Option<String>> = RwSignal::new(None);
    // Cover baru (URL permanen) — kosong = pertahankan cover lama saat simpan.
    let cover_url = RwSignal::new(String::new());
    let cover_uploading = RwSignal::new(false);
    // Persen unggahan cover (0–100). Tetap 0 bila peramban/proxy tak melaporkan
    // panjang total — UI di bawah menanganinya sebagai "tanpa angka", bukan
    // sebagai "macet di 0%".
    let cover_progress = RwSignal::new(0u8);
    // Galeri foto detail — di-seed dari data product agar foto lama tak hilang.
    let drafts: RwSignal<Vec<DetailImageDraft>> = RwSignal::new(vec![]);
    let navigate = use_navigate();
    let saving   = RwSignal::new(false);
    let error_msg = RwSignal::new(String::new());
    let saved    = RwSignal::new(false);

    // Populate form when data loads
    //
    // ── `initialized` DIBACA UNTRACKED, DAN ITU PENTING ──────────────────────
    // Efek ini MENULIS `initialized` di baris terakhirnya. Kalau ia juga
    // MEMBACA-nya secara tracked, signal itu jadi dependensinya sendiri: setiap
    // penulisan menjadwalkan efeknya berjalan lagi.
    //
    // Yang membuatnya berbahaya bukan putaran itu sendiri (putarannya berhenti
    // karena penjaga), melainkan jalur pasca-simpan. Di sana `do_save` sengaja
    // melakukan `initialized.set(false)` lalu `product_data.refetch()` supaya
    // form disemai ULANG dengan data segar — itulah satu-satunya cara varian
    // yang baru dibuat mendapatkan `id`-nya, dan tanpa id, SIMPAN berikutnya
    // MEMBUAT ULANG varian yang sama alih-alih memperbaruinya.
    //
    // Dengan pembacaan tracked, `set(false)` itu ikut menjadwalkan efek ini —
    // dan ia bisa berjalan sebelum refetch mendarat, yaitu saat resource masih
    // memegang data LAMA. Efek lalu menyemai ulang dari data lama dan menyetel
    // `initialized` kembali ke true, sehingga ketika data baru akhirnya datang,
    // penjaga menolaknya. Hasilnya persis kebalikan dari yang dimaksudkan:
    // form tetap basi dan varian baru tetap tanpa id.
    //
    // `get_untracked()` membuat satu-satunya dependensi efek ini adalah
    // `product_data`. `set(false)` tak lagi memicu apa pun; yang membangunkan
    // efek hanya kedatangan data — dan saat itu penjaganya memang sudah false.
    Effect::new(move |_| {
        if let Some(Ok(ev)) = product_data.get() {
            if !initialized.get_untracked() {
                f_name.set(ev.name.clone());
                f_desc.set(ev.description.clone().unwrap_or_default());
                f_venue.set(ev.venue.clone().unwrap_or_default());
                f_city.set(ev.city.clone().unwrap_or_default());
                use chrono::Datelike;
                let d = ev.event_date;
                f_date.set(format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()));
                if let Some(st) = ev.start_time {
                    use chrono::Timelike;
                    f_time.set(format!("{:02}:{:02}", st.hour(), st.minute()));
                }
                f_cat.set(ev.category.clone());
                if let (Some(la), Some(lo)) = (ev.latitude, ev.longitude) {
                    f_lat.set(la);
                    f_lng.set(lo);
                    loc_touched.set(true);
                }
                if let Some(url) = ev.cover_url {
                    if !url.is_empty() {
                        cover_preview.set(Some(url.clone()));
                        cover_lama.set(Some(url));
                    }
                }
                // Seed galeri foto detail dari data product. WAJIB: tanpa ini,
                // submit akan mengirim "[]" dan MENGHAPUS foto lama.
                let seeded: Vec<DetailImageDraft> = ev
                    .detail_images
                    .iter()
                    .map(|d| DetailImageDraft::from_existing(&DetailImagePayload {
                        url: d.url.clone(),
                        image_type: d.image_type.clone(),
                        caption: d.caption.clone(),
                        focus: d.focus.clone(),
                    }))
                    .collect();
                drafts.set(seeded);
                v_rows.set(rows_from_product(&ev.product_variants));
                initialized.set(true);
            }
        }
    });

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
                        // Mulai dari 0 setiap kali, bukan dari sisa unggahan
                        // sebelumnya — kalau tidak, memilih foto kedua terlihat
                        // seolah langsung lompat ke 100%.
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
                                    // Kegagalan ini dulu hanya jatuh ke console.
                                    // Di layar, pratinjau cover baru tetap
                                    // terpampang (itu blob lokal, bukan hasil
                                    // unggah), jadi semuanya tampak beres —
                                    // padahal SIMPAN akan mempertahankan cover
                                    // LAMA tanpa sepatah kata pun.
                                    web_sys::console::error_1(
                                        &format!("[Cover] upload gagal: {e}").into(),
                                    );
                                    error_msg.set(format!(
                                        "Foto cover gagal diunggah: {e}. Cover lama dipertahankan — coba pilih ulang fotonya."
                                    ));
                                    cover_preview.set(cover_lama.get_untracked());
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
        let berhasil = saved.get();
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

    // Setiap penolakan di `do_save` melewati sini: banner DI LAYAR sekaligus
    // baris DI CONSOLE.
    //
    // Kenapa dua-duanya. Banner-nya dirender di puncak form sementara tombolnya
    // di dasar, jadi meski sudah digulir otomatis ia tetap bisa terlewat — dan
    // ketika seseorang melaporkan "simpan tidak bisa", yang paling dibutuhkan
    // justru kalimat yang tak sempat ia baca. Console menyimpannya apa adanya,
    // dengan awalan tetap supaya gampang dicari.
    let tolak = move |alasan: &str| {
        error_msg.set(alasan.to_string());
        #[cfg(target_arch = "wasm32")]
        web_sys::console::warn_1(&format!("[EditProduct] simpan DITOLAK: {alasan}").into());
    };

    let do_save = move |_: leptos::ev::MouseEvent| {
        error_msg.set(String::new());
        saved.set(false);
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(&"[EditProduct] tombol SIMPAN ditekan".into());

        // Data product belum termuat = seluruh signal form masih kosong. Menyimpan
        // sekarang berarti mengirim isian kosong untuk product yang sebenarnya
        // berisi; ditahan di sini dengan alasan yang menyebut penyebabnya,
        // bukan ditolak oleh "Nama produk minimal 3 karakter" yang menyesatkan.
        if !initialized.get_untracked() {
            tolak("Data produk belum selesai dimuat. Tunggu sebentar lalu coba lagi.");
            return;
        }

        let name = f_name.get_untracked();
        if name.trim().len() < 3 { tolak("Nama produk minimal 3 karakter."); return; }
        let desc  = f_desc.get_untracked();
        let venue = f_venue.get_untracked();
        let city  = f_city.get_untracked();
        let date  = f_date.get_untracked();
        if date.is_empty() { tolak("Tanggal produk wajib diisi."); return; }
        let time  = f_time.get_untracked();
        let cats  = f_cat.get_untracked().join(",");
        let current_slug = slug();

        // Validasi + serialisasi varian tiket (termasuk id varian yang dihapus
        // dari form — server menonaktifkannya).
        let variants_json = match rows_to_json(&v_rows.get_untracked(), &v_removed.get_untracked()) {
            Ok(j) => j,
            Err(m) => { tolak(&m); return; }
        };

        // Combine date + time into RFC3339 format that the server can parse.
        // Server expects chrono::DateTime<Utc>, so "2024-01-15" alone fails.
        let date_iso = if !time.is_empty() {
            format!("{}T{}:00Z", date, time)
        } else {
            format!("{}T00:00:00Z", date)
        };

        let (lat, lng) = if loc_touched.get_untracked() {
            (Some(f_lat.get_untracked()), Some(f_lng.get_untracked()))
        } else {
            (None, None)
        };

        // Foto masih diunggah? Tunggu agar URL tak hilang.
        if cover_uploading.get_untracked() {
            // Angkanya ikut disebut. "Tunggu foto cover selesai diunggah." saja
            // tak memberi tahu apakah tinggal sekejap atau masih separuh jalan —
            // dan justru ketidaktahuan itu yang membuat orang menekan SIMPAN
            // berkali-kali lalu menyimpulkan tombolnya rusak.
            // 100% BUKAN berarti selesai — lihat catatan dua fase di indikator
            // unggah di bawah. Pada fase itu byte sudah terkirim dan yang
            // ditunggu adalah server, jadi menyuruh "tunggu sampai 100%" pada
            // seseorang yang sudah melihat 100% adalah instruksi yang mustahil
            // dijalankan.
            let p = cover_progress.get_untracked();
            if p >= 100 {
                tolak("Foto cover sudah terkirim dan sedang diproses server. Sebentar lagi.");
            } else if p > 0 {
                tolak(&format!("Foto cover baru terunggah {p}%. Tunggu sampai selesai."));
            } else {
                tolak("Tunggu foto cover selesai diunggah.");
            }
            return;
        }
        // Gagal ≠ sedang berjalan. Sebelumnya keduanya dijawab "Tunggu semua
        // foto detail selesai diunggah" — dan untuk unggahan yang sudah berhenti
        // karena galat, penantian itu tak pernah berakhir: SIMPAN tertahan
        // permanen tanpa satu pun petunjuk tentang apa yang harus dilakukan.
        let foto = drafts.get_untracked();
        if foto.iter().any(|d| d.uploaded_url.is_none() && d.gagal.get_untracked()) {
            tolak("Ada foto detail yang gagal diunggah. Hapus foto itu, atau pilih ulang filenya.");
            return;
        }
        if foto.iter().any(|d| d.uploaded_url.is_none()) {
            // Sebut BERAPA yang masih jalan dan sejauh mana yang paling
            // tertinggal. "Tunggu semua foto detail selesai diunggah." tak
            // memberi tahu apakah tinggal satu foto di 98% atau lima foto yang
            // baru mulai — dan bedanya adalah antara menunggu sedetik atau
            // semenit.
            let belum: Vec<u8> = foto
                .iter()
                .filter(|d| d.uploaded_url.is_none())
                .map(|d| d.progres.get_untracked())
                .collect();
            let terkecil = belum.iter().copied().min().unwrap_or(0);
            if terkecil >= 100 {
                tolak(&format!(
                    "{} foto detail sudah terkirim dan sedang diproses server. Sebentar lagi.",
                    belum.len()
                ));
            } else if terkecil != 0 {
                tolak(&format!(
                    "{} foto detail masih diunggah (paling lambat {terkecil}%). Tunggu sampai selesai.",
                    belum.len()
                ));
            } else {
                tolak(&format!("{} foto detail masih diunggah. Tunggu sampai selesai.", belum.len()));
            }
            return;
        }
        // cover kosong = pertahankan cover lama (COALESCE di server).
        let cover = cover_url.get_untracked();
        // detail_images SELALU dikirim (galeri di-seed dari data lama), jadi
        // urutan/hapus/tambah tersimpan. Array kosong = user hapus semua.
        let payloads: Vec<DetailImagePayload> = drafts
            .get_untracked()
            .iter()
            .filter_map(|d| d.to_retain_payload())
            .collect();
        let detail_json = serde_json::to_string(&payloads).unwrap_or_else(|_| "[]".to_string());

        saving.set(true);
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(
            &"[EditProduct] semua validasi lolos — mengirim ke server…".into(),
        );
        leptos::task::spawn_local(async move {
            match update_merchant_product(current_slug, name, desc, venue, city, date_iso.clone(), date_iso, cats, lat, lng, variants_json, cover, detail_json).await {
                Ok(_) => {
                    #[cfg(target_arch = "wasm32")]
                    web_sys::console::log_1(&"[EditProduct] server: tersimpan".into());
                    saved.set(true);
                    saving.set(false);

                    // ── Ambil ulang data, jangan biarkan form basi ───────────
                    // Varian yang baru ditambah dikirim tanpa `id` — itulah cara
                    // server tahu harus MEMBUAT-nya. Sesudah tersimpan, baris di
                    // form masih tak ber-id, jadi menekan SIMPAN sekali lagi
                    // membuat varian yang sama DUA KALI (dan tiga kali, dst.).
                    //
                    // Memuat ulang mengembalikan baris-baris itu lengkap dengan
                    // id-nya, sehingga simpan berikutnya memperbarui — bukan
                    // menambah. `initialized` diturunkan supaya Effect penyemai
                    // mau bekerja sekali lagi dengan data yang baru.
                    v_removed.set(vec![]);
                    cover_url.set(String::new());
                    initialized.set(false);
                    product_data.refetch();
                }
                Err(e) => {
                    #[cfg(target_arch = "wasm32")]
                    web_sys::console::error_1(
                        &format!("[EditProduct] server MENOLAK: {e}").into(),
                    );
                    error_msg.set(e.to_string());
                    saving.set(false);
                }
            }
        });
    };

    view! {
        <div class="medit-page">

            // ── TIRAI "SEDANG MENYIMPAN" ────────────────────────────────────
            // Tombol SIMPAN berada di DASAR form yang panjang. Setelah ditekan,
            // halaman menggulir ke atas untuk memperlihatkan banner — sehingga
            // satu-satunya penanda proses (label tombol yang berubah jadi
            // "MENYIMPAN…") justru berada di luar layar tepat ketika ia paling
            // dibutuhkan. Yang terlihat pengguna hanyalah halaman yang diam,
            // dan dugaan yang wajar adalah tombolnya tak bekerja — lalu ia
            // menekannya lagi.
            //
            // Tirai ini menutup seluruh layar selama permintaan berjalan, jadi
            // penanda prosesnya tak mungkin terlewat di posisi gulir mana pun,
            // sekaligus memblokir klik kedua secara fisik. Ditulis dengan
            // utility Tailwind (bukan kelas medit-*) mengikuti arah `pages/cart.rs`.
            //
            // `aria-live="assertive"` + `role="status"`: pembaca layar
            // mengumumkannya, karena bagi pengguna yang tak melihat tirai, diam
            // sepenuhnya adalah satu-satunya umpan balik yang tersisa.
            {move || saving.get().then(|| view! {
                // ── z-[2000], BUKAN z-[60] ──────────────────────────────────
                // Halaman ini memuat peta Leaflet, dan Leaflet menaruh
                // kontrolnya di `z-index: 1000` (lihat styles/leaflet.css:
                // `.leaflet-top`, `.leaflet-bottom`, `.leaflet-control`), jauh
                // di atas apa pun yang dipakai aplikasi ini sendiri — tumpukan
                // tertinggi di styles/parts hanyalah 300.
                //
                // `.leaflet-container` cuma `position: relative` tanpa z-index,
                // jadi ia TIDAK membentuk stacking context sendiri: angka 1000
                // itu bersaing langsung dengan tirai ini. Pada z-[60] petanya
                // menembus tirai dan tetap terlihat — persis yang dilaporkan.
                //
                // 2000 dipilih supaya di atas Leaflet tapi tetap di bawah
                // `#hydration-loader` (9999, lihat web/app/shell.rs) yang memang
                // harus selalu paling atas.
                <div role="status" aria-live="assertive"
                     class="fixed inset-0 z-[2000] flex flex-col items-center justify-center gap-3 \
                            bg-overlay backdrop-blur-sm">
                    <svg class="animate-spin w-9 h-9 text-brand" viewBox="0 0 24 24"
                         fill="none" aria-hidden="true">
                        <circle cx="12" cy="12" r="10" stroke="currentColor"
                                stroke-width="3" opacity="0.25"/>
                        <path d="M22 12a10 10 0 0 0-10-10" stroke="currentColor"
                              stroke-width="3" stroke-linecap="round"/>
                    </svg>
                    <p class="font-sans text-xs font-bold tracking-[0.08em] text-content">
                        "MENYIMPAN PERUBAHAN…"
                    </p>
                    <p class="text-[11px] text-content-muted">"Jangan tutup halaman ini."</p>
                </div>
            })}

            <header class="page-header medit-page-header">
                <A href="/merchant" attr:class="back-btn" attr:aria-label="Kembali">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="page-logo">"EDIT PRODUK"</span>
                <div class="header-actions">
                    <ThemeToggle/>
                    <A href="/notifications" attr:class="bell-btn" attr:aria-label="Notifikasi">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9"/>
                            <path d="M13.73 21a2 2 0 0 1-3.46 0"/>
                        </svg>
                    </A>
                </div>
            </header>

            // ── Status pemuatan DIPISAH dari form ────────────────────────
            //
            // Dulu SELURUH form hidup di dalam `<Suspense>` sebagai salah satu
            // cabang `match product_data.get()`. Bentuk DOM halaman jadi berubah
            // menurut keadaan resource: server merender cabang `Ok` (form
            // lengkap) ke HTML, sedangkan klien saat hydrate bisa mengevaluasi
            // cabang lain lebih dulu (`None` → skeleton) dan memasang
            // reaktivitas pada pohon yang tak sama dengan yang ada di layar.
            //
            // Hasilnya persis keluhan "tak terjadi apa-apa": form dari server
            // tetap terpampang dan tetap bisa diketik — tapi `on:click` SIMPAN
            // tak terpasang pada tombol yang benar-benar ada di DOM. Ditekan,
            // tak ada loading, tak ada banner galat, tak ada permintaan ke
            // server. Tak ada yang rusak yang bisa dilihat; tak ada yang jalan.
            //
            // Sekarang bentuknya tetap: form SELALU dirender, resource hanya
            // mengisi nilainya (lewat Effect penyemai) dan menyalakan strip
            // status di bawah ini. Tak ada lagi cabang yang menukar pohon DOM.
            <div class="medit-container">

                // ── Strip status pemuatan data ────────────────────────────
                {move || {
                    match product_data.get() {
                        None => view! {
                            <p style="font-size:12px;color:var(--text-muted);margin:0 0 10px">
                                "Memuat data produk…"
                            </p>
                        }.into_any(),
                        Some(Ok(_)) => ().into_any(),
                        // `not_ready` = slug belum terbaca. Masih wajar ditunggu
                        // sebentar; sesudah itu katakan apa adanya + beri jalan
                        // keluar, jangan berkedip selamanya.
                        Some(Err(e)) if e.to_string().contains("not_ready") => {
                            if terlalu_lama.get() {
                                view! {
                                    <div>
                                        <div class="medit-error-banner">
                                            "Data produk tak kunjung termuat. Sesi mungkin "
                                            "belum terbaca di halaman ini."
                                        </div>
                                        <button
                                            class="medit-cancel-btn"
                                            on:click=move |_| {
                                                #[cfg(target_arch = "wasm32")]
                                                if let Some(w) = web_sys::window() {
                                                    let _ = w.location().reload();
                                                }
                                            }
                                        >
                                            "MUAT ULANG HALAMAN"
                                        </button>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <p style="font-size:12px;color:var(--text-muted);margin:0 0 10px">
                                        "Memuat data produk…"
                                    </p>
                                }.into_any()
                            }
                        }
                        Some(Err(_)) => view! {
                            <div>
                                <div class="medit-error-banner">
                                    "Produk tidak ditemukan atau akses ditolak."
                                </div>
                                <A href="/merchant" attr:class="medit-cancel-btn">"← Kembali"</A>
                            </div>
                        }.into_any(),
                    }
                }}


                // ── Feedback ──────────────────────────────────
                {move || saved.get().then(|| view! {
                    <div class="medit-success-banner">
                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                            <polyline points="20 6 9 17 4 12"/>
                        </svg>
                        "Product berhasil diperbarui!"
                    </div>
                })}
                {move || (!error_msg.get().is_empty()).then(|| view! {
                    <div class="medit-error-banner">
                        {move || error_msg.get()}
                    </div>
                })}

                // ── INFO DASAR ────────────────────────────────
                <div class="medit-section-header">
                    <span class="medit-section-label">"INFO DASAR"</span>
                </div>

                <div class="medit-field-group">
                    <label class="medit-field-label">"NAMA PRODUK"</label>
                    <input type="text" class="medit-input"
                           placeholder="Nama produk"
                           prop:value=move || f_name.get()
                           on:input=move |e| f_name.set(event_target_value(&e))/>
                </div>

                <div class="medit-field-group">
                    <label class="medit-field-label">"DESKRIPSI"</label>
                    <textarea class="medit-input medit-textarea"
                              placeholder="Deskripsi produk..."
                              prop:value=move || f_desc.get()
                              on:input=move |e| f_desc.set(event_target_value(&e))>
                    </textarea>
                </div>

                // ── KATEGORI ──────────────────────────────────
                <div class="medit-field-group">
                    <label class="medit-field-label">"KATEGORI"</label>
                    <div class="medit-category-grid">
                        {CATEGORIES.iter().map(|cat| {
                            let c = cat.to_string();
                            let c2 = c.clone();
                            let c3 = c.clone();
                            view! {
                                <label class="medit-checkbox-label">
                                    <input type="checkbox"
                                           prop:checked=move || f_cat.get().contains(&c2)
                                           on:change=move |_| {
                                               f_cat.update(|cats| {
                                                   if cats.contains(&c3) { cats.retain(|x| x != &c3); }
                                                   else { cats.push(c3.clone()); }
                                               });
                                           }/>
                                    <span>{c}</span>
                                </label>
                            }
                        }).collect_view()}
                    </div>
                </div>

                // ── FOTO COVER ────────────────────────────────
                <div class="medit-field-group">
                    <label class="medit-field-label">"FOTO COVER"</label>
                    {move || cover_preview.get().map(|url| view! {
                        <div class="medit-cover-preview">
                            <img src=url alt="Cover preview"/>
                        </div>
                    })}
                    <div class="medit-file-input-wrapper">
                        <input type="file" class="medit-file-input" accept="image/*"
                               on:change=on_cover_change/>
                        <span class="medit-file-input-label">"GANTI FOTO"</span>
                    </div>
                    // ── Progres unggah cover ────────────────────────────────
                    // Sebelumnya hanya "Mengunggah cover…" tanpa angka, dan
                    // untuk berkas besar di jaringan lambat kalimat itu bisa
                    // bertahan puluhan detik tanpa berubah sedikit pun — tak
                    // terbedakan dari unggahan yang sudah mati. Sementara itu
                    // tombol SIMPAN menolak dengan "Tunggu foto cover selesai
                    // diunggah." yang juga tak memberi tahu tinggal berapa lama.
                    //
                    // `width` dipasang lewat `style` inline, bukan kelas
                    // Tailwind: nilainya lahir saat berjalan, dan kelas yang
                    // dirakit (`format!("w-[{p}%]")`) tak terlihat oleh pemindai
                    // Tailwind sehingga gayanya hilang senyap di produksi —
                    // aturan yang sama yang ditulis di kepala `pages/cart.rs`.
                    //
                    // ⚠️ Tak satu pun ekspresi di bawah memakai `>`. Parser
                    // makro `view!` memperlakukan `>` di dalam nilai atribut
                    // sebagai PENUTUP TAG, lalu sisa markupnya diurai sebagai
                    // teks — galatnya muncul sebagai "open tag has no
                    // corresponding close tag" di baris yang tak bersalah.
                    // Jebakan yang sama sudah tercatat di
                    // `components/variant_editor.rs`. Karena itu nilainya
                    // dihitung DI LUAR `view!`, dan pembandingnya `!= 0`.
                    // ── DUA FASE, BUKAN SATU ────────────────────────────────
                    // `xhr.upload.onprogress` mengukur byte yang diserahkan ke
                    // TUMPUKAN JARINGAN, bukan pekerjaan yang sudah selesai.
                    // Begitu badan permintaan masuk buffer soket, ia melapor
                    // 100% — padahal server baru MULAI: ia masih meneruskan
                    // fotonya ke RustFS/S3 dan belum menjawab apa pun.
                    //
                    // Di localhost atau LAN, penyerahan itu selesai nyaris
                    // seketika, sehingga bar melompat ke 100% pada detik pertama
                    // lalu diam lama di sana. Itu bukan angka yang salah hitung —
                    // itu angka yang benar untuk pertanyaan yang salah, dan
                    // efeknya lebih buruk daripada tak ada angka: pengguna
                    // menyimpulkan unggahannya menggantung.
                    //
                    // Karena itu 100% TIDAK ditampilkan sebagai angka. Ia berganti
                    // menjadi fase kedua yang jujur: byte sudah terkirim, server
                    // sedang memproses, dan lamanya memang tak bisa diukur dari
                    // sisi peramban.
                    {move || cover_uploading.get().then(|| {
                        let p = cover_progress.get();
                        let diproses = p >= 100;
                        let terukur = p != 0 && !diproses;
                        // Bilah berdenyut dipakai untuk DUA keadaan yang sama-sama
                        // tak punya angka sah: belum terukur, dan sedang diproses
                        // server. Keduanya harus bergerak supaya terbaca hidup.
                        let kelas_bilah = if terukur {
                            "h-full rounded-full bg-brand transition-[width] duration-200"
                        } else {
                            "h-full rounded-full bg-brand animate-pulse"
                        };
                        let gaya_bilah = if terukur {
                            format!("width:{p}%")
                        } else {
                            "width:100%".to_string()
                        };
                        let label = if diproses {
                            "Memproses di server…"
                        } else {
                            "Mengunggah cover…"
                        };
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

                // ── FOTO DETAIL (galeri, bisa di-drag urutannya) ──
                <div class="medit-field-group">
                    <label class="medit-field-label">"FOTO DETAIL"</label>
                    <DetailImagesSection drafts=drafts />
                </div>

                // ── STORY PREVIEW (sama seperti create product) ──
                <ProductStoryPreviewInline
                    title=Signal::derive(move || f_name.get())
                    cover_url=Signal::derive(move || cover_preview.get())
                    description=Signal::derive(move || f_desc.get())
                    on_share_click=Callback::new({
                        let navigate = navigate.clone();
                        move |_| {
                            #[cfg(target_arch = "wasm32")]
                            {
                                let params = web_sys::UrlSearchParams::new()
                                    .expect("UrlSearchParams");
                                params.append("event_title", &f_name.get_untracked());
                                params.append(
                                    "event_cover",
                                    &cover_preview.get_untracked().unwrap_or_default(),
                                );
                                params.append("product_desc", &f_desc.get_untracked());
                                params.append("event_slug", &slug());
                                let qs = params.to_string();
                                navigate(
                                    &format!("/story?{}", qs),
                                    Default::default(),
                                );
                            }
                            #[cfg(not(target_arch = "wasm32"))]
                            let _ = &navigate;
                        }
                    })
                />

                // ── TANGGAL & WAKTU ───────────────────────────
                <div class="medit-section-header">
                    <span class="medit-section-label">"WAKTU & TEMPAT"</span>
                </div>

                <div class="medit-field-group">
                    <label class="medit-field-label">"TANGGAL"</label>
                    <input type="date" class="medit-input"
                           prop:value=move || f_date.get()
                           on:input=move |e| f_date.set(event_target_value(&e))/>
                </div>

                <div class="medit-grid-2">
                    <div class="medit-field-group">
                        <label class="medit-field-label">"WAKTU MULAI"</label>
                        <input type="time" class="medit-input"
                               prop:value=move || f_time.get()
                               on:input=move |e| f_time.set(event_target_value(&e))/>
                    </div>
                    <div class="medit-field-group">
                        <label class="medit-field-label">"KOTA"</label>
                        <input type="text" class="medit-input"
                               placeholder="Jakarta"
                               prop:value=move || f_city.get()
                               on:input=move |e| f_city.set(event_target_value(&e))/>
                    </div>
                </div>

                <div class="medit-field-group">
                    <label class="medit-field-label">"NAMA LOKASI"</label>
                    <input type="text" class="medit-input"
                           placeholder="Gelora Bung Karno"
                           prop:value=move || f_venue.get()
                           on:input=move |e| f_venue.set(event_target_value(&e))/>
                </div>

                // ── VARIAN TIKET ──────────────────────────────
                <VariantEditor rows=v_rows removed_ids=v_removed />

                // ── LOKASI DI PETA ────────────────────────────
                <div class="medit-section-header">
                    <span class="medit-section-label">"LOKASI DI PETA"</span>
                </div>
                <p style="font-size:12px;color:var(--text-muted);margin:0 0 10px">
                    "Klik peta atau geser pin untuk menandai lokasi toko."
                </p>
                <div id="edit-loc-map"
                     data-map-picker="1"
                     data-lat-input="edit-lat"
                     data-lng-input="edit-lng"
                     style="width:100%;height:300px;border-radius:12px;overflow:hidden;border:1px solid var(--border);margin-bottom:12px;background:var(--bg-elevated)">
                </div>
                <div class="medit-grid-2">
                    <div class="medit-field-group">
                        <label class="medit-field-label">"LATITUDE"</label>
                        <input id="edit-lat" type="number" step="any" class="medit-input"
                               placeholder="-6.2088"
                               prop:value=move || f_lat.get().to_string()
                               on:input=move |e| {
                                   loc_touched.set(true);
                                   if let Ok(v) = event_target_value(&e).parse::<f64>() {
                                       f_lat.set(v);
                                       map_set("edit-loc-map", v, f_lng.get_untracked());
                                   }
                               }/>
                    </div>
                    <div class="medit-field-group">
                        <label class="medit-field-label">"LONGITUDE"</label>
                        <input id="edit-lng" type="number" step="any" class="medit-input"
                               placeholder="106.8456"
                               prop:value=move || f_lng.get().to_string()
                               on:input=move |e| {
                                   loc_touched.set(true);
                                   if let Ok(v) = event_target_value(&e).parse::<f64>() {
                                       f_lng.set(v);
                                       map_set("edit-loc-map", f_lat.get_untracked(), v);
                                   }
                               }/>
                    </div>
                </div>

                // ── SUBMIT ────────────────────────────────────
                <div class="medit-actions">
                    // Tombolnya menyatakan kesiapannya sendiri. Selama data
                    // product belum termuat, isian form masih kosong dan menyimpan
                    // hanya akan mengirim kekosongan itu — jadi tombolnya
                    // nonaktif DAN mengatakan alasannya, bukan diam lalu
                    // menolak dengan pesan yang tak nyambung.
                    //
                    // ── EMPAT KEADAAN, BUKAN TIGA ───────────────────────────
                    // Sesudah simpan berhasil, `do_save` menurunkan
                    // `initialized` supaya form disemai ulang dari data segar.
                    // Dengan tiga keadaan, jeda itu terbaca "MEMUAT DATA
                    // EVENT…" — kalimat yang sama persis dengan keadaan
                    // SEBELUM apa pun tersimpan. Bagi yang menekan simpan, itu
                    // tampak seperti pekerjaannya dibuang dan halaman mengulang
                    // dari awal. Keadaan keempat memisahkan "menyegarkan
                    // sesudah tersimpan" dari "belum pernah termuat".
                    <button class="medit-submit-btn"
                            disabled=move || saving.get() || !initialized.get()
                            on:click=do_save>
                        {move || saving.get().then(|| view! {
                            <svg class="animate-spin w-4 h-4 shrink-0" viewBox="0 0 24 24"
                                 fill="none" aria-hidden="true">
                                <circle cx="12" cy="12" r="10" stroke="currentColor"
                                        stroke-width="3" opacity="0.25"/>
                                <path d="M22 12a10 10 0 0 0-10-10" stroke="currentColor"
                                      stroke-width="3" stroke-linecap="round"/>
                            </svg>
                        })}
                        {move || if saving.get() {
                            "MENYIMPAN…"
                        } else if saved.get() && !initialized.get() {
                            "TERSIMPAN — MENYEGARKAN…"
                        } else if !initialized.get() {
                            "MEMUAT DATA PRODUK…"
                        } else {
                            "SIMPAN PERUBAHAN"
                        }}
                    </button>
                    <A href="/merchant" attr:class="medit-cancel-btn">"BATAL"</A>
                </div>

            </div>
        </div>
    }
}
