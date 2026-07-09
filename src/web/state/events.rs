use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::web::api::{get_categories, get_events};
use crate::web::models::{format_date, Event};
use crate::web::utils::format_number;

// ── Frontend event model ──────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct ExploreEvent {
    pub id: String,
    /// Untuk link profil penyelenggara (/m/{merchant_id}) dari kartu explore.
    pub merchant_id: String,
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

pub fn event_to_explore_pub(e: &Event) -> ExploreEvent {
    event_to_explore(e)
}

pub(super) fn event_to_explore(e: &Event) -> ExploreEvent {
    let price_raw = e.display_price as i64;
    let price_str = if price_raw <= 0 {
        "FREE".into()
    } else {
        format!("Rp{}", format_number(price_raw))
    };
    let dt = e.start_time.unwrap_or(e.event_date);

    ExploreEvent {
        id: e.id.clone(),
        merchant_id: e.merchant_id.clone(),
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

/// Jumlah event per "halaman" (chunk) fetch. Explore memuat sebagian dulu, lalu
/// "Muat lebih banyak" mengambil chunk berikutnya via LIMIT/OFFSET (page+1).
pub const PAGE_SIZE: i64 = 20;

#[derive(Clone, Copy)]
pub struct EventsCtx {
    pub items: RwSignal<Vec<ExploreEvent>>,
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

impl EventsCtx {
    pub fn load(&self) {
        self.load_cat(String::new());
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

            // Race the server function against an 8-second safety timeout.
            let fetch = get_events(Some(1), None, cat_opt, None, Some(PAGE_SIZE));
            let timeout = gloo_timers::future::TimeoutFuture::new(8_000);
            let result = futures::future::select(Box::pin(fetch), Box::pin(timeout)).await;

            match result {
                futures::future::Either::Left((srv_result, _)) => {
                    if fetch_gen.get_untracked() == gen {
                        match srv_result {
                            Ok(res) => {
                                has_more.set(res.page < res.total_pages);
                                total.set(res.total);
                                items.set(res.data.iter().map(event_to_explore).collect());
                            }
                            Err(e) => error.set(e.to_string()),
                        }
                    }
                }
                futures::future::Either::Right(_) => {
                    if fetch_gen.get_untracked() == gen {
                        error.set("Koneksi ke server habis waktu. Coba refresh.".to_string());
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
            let res = get_events(Some(next), None, cat_opt, None, Some(PAGE_SIZE)).await;
            if fetch_gen.get_untracked() == gen {
                if let Ok(res) = res {
                    has_more.set(res.page < res.total_pages);
                    total.set(res.total);
                    let mut more: Vec<_> = res.data.iter().map(event_to_explore).collect();
                    items.update(|v| v.append(&mut more));
                }
            }
            loading_more.set(false);
        });
    }
}

pub fn provide_events_store() {
    let ctx = EventsCtx {
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
        // jalan lebih dulu. Bonus: prefetch event untuk landing page.
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

pub fn use_events_store() -> EventsCtx {
    use_context::<EventsCtx>().expect("EventsCtx not provided")
}
