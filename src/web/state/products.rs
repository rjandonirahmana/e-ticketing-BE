use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::web::api::{get_categories, get_products};
use crate::web::models::{format_date, Product};
use crate::web::utils::format_number;

// ── Frontend product model ──────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct ExploreProduct {
    pub id: String,
    /// Untuk link profil penyelenggara (/m/{merchant_id}) dari kartu explore.
    pub merchant_id: String,
    /// Nama toko penyelenggara (chip kartu; kosong → fallback "PENYELENGGARA").
    pub merchant_name: String,
    pub slug: String,
    pub title: String,
    pub category: Vec<String>,
    pub date: String,
    pub venue: String,
    pub city: String,
    pub price: i64,
    pub price_str: String,
    pub cover: String,
    pub is_live: bool,
    pub status: String,
    pub total_sold: i32,
    pub total_quota: i32,
}

pub fn product_to_explore_pub(e: &Product) -> ExploreProduct {
    product_to_explore(e)
}

pub(super) fn product_to_explore(e: &Product) -> ExploreProduct {
    let price_raw = e.display_price as i64;
    let price_str = if price_raw <= 0 {
        "FREE".into()
    } else {
        format!("Rp{}", format_number(price_raw))
    };
    let dt = e.start_time.unwrap_or(e.event_date);

    ExploreProduct {
        id: e.id.clone(),
        merchant_id: e.merchant_id.clone(),
        merchant_name: e.merchant_name.clone().unwrap_or_default(),
        slug: e.slug.clone(),
        title: e.name.clone(),
        category: e.category.clone(),
        date: format_date(&dt),
        venue: e.venue.clone().unwrap_or_default(),
        city: e.city.clone().unwrap_or_default(),
        price: price_raw,
        price_str,
        cover: e.cover_url.clone().unwrap_or_default(),
        is_live: e.status.eq_ignore_ascii_case("live"),
        status: e.status.clone(),
        total_sold: e.total_sold,
        total_quota: e.total_quota,
    }
}

// ── Context ───────────────────────────────────────────────────────────────────

/// Jumlah product per "halaman" (chunk) fetch. Explore memuat sebagian dulu, lalu
/// "Muat lebih banyak" mengambil chunk berikutnya via LIMIT/OFFSET (page+1).
pub const PAGE_SIZE: i64 = 20;

#[derive(Clone, Copy)]
pub struct ProductsCtx {
    pub items: RwSignal<Vec<ExploreProduct>>,
    pub categories: RwSignal<Vec<String>>,
    pub loading: RwSignal<bool>,
    /// True saat fetch chunk berikutnya (load_more) sedang berjalan.
    pub loading_more: RwSignal<bool>,
    /// True bila masih ada halaman berikutnya (page < total_pages).
    pub has_more: RwSignal<bool>,
    /// TOTAL acara di server untuk filter aktif (dari COUNT query, bukan
    /// jumlah item yang sudah termuat) — untuk label "N acara tersedia".
    pub total: RwSignal<i64>,
    pub error: RwSignal<String>,
    /// Halaman terakhir yang sudah dimuat (mulai 1).
    page: RwSignal<i64>,
    /// Kategori aktif saat ini (untuk load_more mengikuti filter).
    cur_cat: RwSignal<String>,
    // Cancels stale fetches when category changes rapidly.
    fetch_gen: RwSignal<u32>,
}

fn cat_to_opt(category: &str) -> Option<String> {
    let c = if category == "All" { "" } else { category };
    if c.is_empty() { None } else { Some(c.to_string()) }
}

/// Batas waktu memuat feed. Cukup panjang untuk menampung permintaan pertama
/// sesudah server dingin (pool Postgres belum terbentuk) dan jaringan seluler
/// yang buruk, tapi tetap ada supaya halaman tak menggantung selamanya.
const BATAS_MUAT_MS: u32 = 20_000;

/// Ubah galat mentah jadi kalimat yang menunjuk sebab yang BENAR.
///
/// Yang dibedakan hanya yang bisa ditindaklanjuti pengguna secara berbeda:
/// sesi habis (harus masuk lagi), peramban memang sedang luring (periksa
/// jaringan), dan sisanya — masalah di sisi server, yang tak ada gunanya
/// disuruh "periksa koneksi".
fn pesan_galat_muat(raw: &str) -> String {
    let r = raw.to_lowercase();
    if r.contains("unauth") || r.contains("401") || r.contains("session") {
        return "Sesi kamu berakhir. Masuk lagi untuk melanjutkan.".to_string();
    }
    // `navigator.onLine` hanya bisa dipercaya saat ia bilang FALSE: true belum
    // tentu berarti internetnya jalan, tapi false hampir pasti benar.
    let luring = web_sys::window()
        .map(|w| !w.navigator().on_line())
        .unwrap_or(false);
    if luring {
        return "Perangkat sedang tidak terhubung ke internet.".to_string();
    }
    "Gagal memuat data dari server. Coba muat ulang halaman.".to_string()
}

impl ProductsCtx {
    pub fn load(&self) {
        self.load_cat(String::new());
    }

    /// True begitu store sudah "aktif" di klien — yaitu page-1 sudah di-seed dari
    /// resource SSR ATAU sebuah fetch sudah dimulai (fetch_gen naik dari 0).
    /// Reaktif (melacak `fetch_gen`). Dipakai ExplorePage: sebelum aktif, feed
    /// dirender dari resource SSR (HTML awal berisi kartu, bukan shimmer); setelah
    /// aktif, feed beralih membaca store (mendukung filter + append halaman).
    pub fn is_active(&self) -> bool {
        self.fetch_gen.get() > 0
    }

    /// Seed halaman PERTAMA dari resource SSR (dipanggil sekali pasca-hydration)
    /// supaya store mengambil alih TANPA refetch — datanya sudah tertanam di HTML
    /// yang di-SSR. Menghindari "lambat saat pertama diakses": tanpa ini feed baru
    /// terisi setelah bundle WASM diunduh+hydrate lalu memicu fetch tersendiri.
    pub fn seed_first(&self, res: &crate::web::models::PaginatedProducts, category: String) {
        self.cur_cat.set(category);
        self.page.set(1);
        self.has_more.set(res.page < res.total_pages);
        self.total.set(res.total);
        self.items
            .set(res.data.iter().map(product_to_explore).collect());
        self.error.set(String::new());
        self.loading.set(false);
        // Tandai aktif (fetch_gen 0→1) → feed ExplorePage beralih baca store.
        self.fetch_gen.update(|g| *g = g.wrapping_add(1));
    }

    /// Muat halaman PERTAMA untuk kategori (reset daftar).
    pub fn load_cat(&self, category: String) {
        if is_server() {
            return;
        }
        self.loading.set(true);
        self.error.set(String::new());
        self.cur_cat.set(category.clone());
        self.page.set(1);

        // Increment generation so any in-flight fetch from the previous
        // category becomes a no-op when it completes.
        let gen = self.fetch_gen.get_untracked().wrapping_add(1);
        self.fetch_gen.set(gen);

        let fetch_gen = self.fetch_gen;
        let items = self.items;
        let loading = self.loading;
        let has_more = self.has_more;
        let total = self.total;
        let error = self.error;

        spawn_local(async move {
            let cat_opt = cat_to_opt(&category);

            // Batas waktu pengaman. DINAIKKAN dari 8 detik.
            //
            // Delapan detik terdengar longgar di jaringan kantor, tapi permintaan
            // pertama sesudah proses server baru hidup harus menunggu pool
            // Postgres terbentuk, dan pengunjung di jaringan seluler pinggiran
            // rutin melewatinya. Yang terjadi kemudian bukan sekadar pesan
            // keliru: `select` MEMBUANG future yang kalah, jadi permintaan yang
            // sebenarnya baik-baik saja ikut DIBATALKAN tepat sebelum ia
            // menjawab — pengguna diberi tahu "tak bisa terhubung" oleh kode
            // yang barusan memutus hubungannya sendiri.
            let fetch = get_products(Some(1), None, cat_opt, None, Some(PAGE_SIZE));
            let timeout = gloo_timers::future::TimeoutFuture::new(BATAS_MUAT_MS);
            let result = futures::future::select(Box::pin(fetch), Box::pin(timeout)).await;

            match result {
                futures::future::Either::Left((srv_result, _)) => {
                    if fetch_gen.get_untracked() == gen {
                        match srv_result {
                            Ok(res) => {
                                has_more.set(res.page < res.total_pages);
                                total.set(res.total);
                                items.set(res.data.iter().map(product_to_explore).collect());
                                // Bersihkan galat lama secara eksplisit. Reset di
                                // awal `load_cat` saja tak cukup: `load_more` dan
                                // jalur muat lain memakai `error` yang sama, dan
                                // banner yang tertinggal dari kegagalan sebelumnya
                                // akan bertahan di layar meski data terbaru sudah
                                // tampil di bawahnya.
                                error.set(String::new());
                            }
                            // Pesannya dibedakan menurut SEBABNYA. Sebelumnya
                            // setiap galat — 500 dari server, gagal deserialisasi,
                            // sesi kedaluwarsa — muncul sebagai "tidak bisa
                            // terhubung ke server", padahal server justru sedang
                            // terhubung dan menjawab. Diagnosis yang salah membuat
                            // orang mencari masalah di jaringannya berjam-jam.
                            Err(e) => error.set(pesan_galat_muat(&e.to_string())),
                        }
                    }
                }
                futures::future::Either::Right(_) => {
                    if fetch_gen.get_untracked() == gen {
                        error.set(
                            "Server lama menjawab. Coba muat ulang halaman.".to_string(),
                        );
                    }
                }
            }
            loading.set(false);
        });
    }

    /// Muat halaman BERIKUTNYA (LIMIT/OFFSET) dan APPEND ke daftar.
    pub fn load_more(&self) {
        if is_server() {
            return;
        }
        if self.loading.get_untracked()
            || self.loading_more.get_untracked()
            || !self.has_more.get_untracked()
        {
            return;
        }
        self.loading_more.set(true);
        let next = self.page.get_untracked() + 1;
        self.page.set(next);

        let gen = self.fetch_gen.get_untracked(); // append hanya bila kategori tak berubah
        let fetch_gen = self.fetch_gen;
        let items = self.items;
        let has_more = self.has_more;
        let total = self.total;
        let loading_more = self.loading_more;
        let cat = self.cur_cat.get_untracked();

        spawn_local(async move {
            let cat_opt = cat_to_opt(&cat);
            let res = get_products(Some(next), None, cat_opt, None, Some(PAGE_SIZE)).await;
            if fetch_gen.get_untracked() == gen {
                if let Ok(res) = res {
                    has_more.set(res.page < res.total_pages);
                    total.set(res.total);
                    let mut more: Vec<_> = res.data.iter().map(product_to_explore).collect();
                    items.update(|v| v.append(&mut more));
                }
            }
            loading_more.set(false);
        });
    }
}

pub fn provide_products_store() {
    let ctx = ProductsCtx {
        items: RwSignal::new(Vec::new()),
        categories: RwSignal::new(vec!["All".to_string()]),
        // Start as loading=true so SSR renders the shimmer. The client
        // hydrates with the same initial state → no hydration mismatch.
        // ExplorePage's Effect triggers the actual fetch post-hydration.
        loading: RwSignal::new(true),
        loading_more: RwSignal::new(false),
        has_more: RwSignal::new(false),
        total: RwSignal::new(0),
        error: RwSignal::new(String::new()),
        page: RwSignal::new(1),
        cur_cat: RwSignal::new(String::new()),
        fetch_gen: RwSignal::new(0),
    };

    // Load categories from BE — client only.
    if !is_server() {
        let cats_signal = ctx.categories;
        spawn_local(async move {
            if let Ok(mut cats) = get_categories().await {
                cats.retain(|c| !c.is_empty());
                let mut full = vec!["All".to_string()];
                full.extend(cats);
                cats_signal.set(full);
            }
        });

        // Fallback fetch awal (hydration-safe). Normalnya ExplorePage memicu
        // fetch lewat Effect-nya sendiri. Tapi bila Effect itu tidak jalan
        // (mis. hydration tersendat di komponen lain), `loading` akan menggantung
        // true selamanya → shimmer tak pernah hilang. Penjaga ini menunggu satu
        // tick (agar tidak menabrak render hydration), lalu — jika belum ada
        // fetch yang dimulai (fetch_gen masih 0) dan masih loading — memicu fetch
        // sendiri. fetch_gen mencegah double-fetch bila Effect ExplorePage sempat
        // jalan lebih dulu. Bonus: prefetch product untuk landing page.
        let ctx_fb = ctx;
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(80).await;
            if ctx_fb.fetch_gen.get_untracked() == 0 && ctx_fb.loading.get_untracked() {
                ctx_fb.load();
            }
        });
    }

    provide_context(ctx);
}

pub fn use_products_store() -> ProductsCtx {
    use_context::<ProductsCtx>().expect("ProductsCtx not provided")
}
