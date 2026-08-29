//! main.rs — Entry point backend e-ticketing (Leptos SSR + WebSocket).
//!
//! Cara jalankan:
//!   cargo run                     # SSR (tanpa WASM hydration)
//!   cargo leptos watch            # SSR + WASM hydration (full dev)
//!
//! Satu binary, satu port:
//!   /api-fn/*   → Leptos server functions (direct service calls)
//!   /pkg/*      → Static assets (WASM/JS/CSS) — butuh cargo leptos build
//!   /ws/*       → WebSocket
//!   /*          → Leptos SSR rendering

#![recursion_limit = "512"]

// Global allocator server: mimalloc. Mengurangi fragmentasi & overhead alokasi
// pada workload SSR + WebSocket berumur panjang — penting di box 2 vCPU / 4 GB.
// `main.rs` hanya dikompilasi untuk native (server), jadi tak menyentuh wasm.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use e_ticketing::config::{config::AppConfig, database::create_pool};
use e_ticketing::service::telegram::TelegramService;
use e_ticketing::state::AppState;
use e_ticketing::utils::error::init_telegram_notifier;
use e_ticketing::api::rest_router;
use e_ticketing::web::api::upload::{merchant_image_upload, story_upload};
use e_ticketing::web::app::{shell, App};
use e_ticketing::ws::handler::WsAppState;
use e_ticketing::ws::routes::chat_router;
use e_ticketing::live::api::live_router;
use e_ticketing::meet::api::meet_router;

use leptos::config::get_configuration;
use leptos_axum::{generate_route_list, LeptosRoutes};

/// Entry: bangun runtime tokio dengan BATAS aman untuk box kecil, lalu jalankan
/// `run()`. Sengaja TIDAK memakai `#[tokio::main]` karena makro itu tak bisa
/// menyetel `max_blocking_threads`.
fn main() -> Result<()> {
    // Blocking-pool: bcrypt hash/verify (service/auth.rs) dijalankan lewat
    // `spawn_blocking`. Default tokio = 512 thread → storm login/registrasi bisa
    // memunculkan ratusan thread (tiap thread mencadangkan stack) → boros RAM &
    // thrash CPU di 2 vCPU. Cap skala-CPU: kelebihan request ANTRE, bukan
    // meledakkan thread. Override via env TOKIO_MAX_BLOCKING_THREADS.
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
    let max_blocking = std::env::var("TOKIO_MAX_BLOCKING_THREADS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or((cpus * 8).max(16));

    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all().max_blocking_threads(max_blocking);
    // Worker threads: default = jumlah CPU (perilaku tokio). Di VPS shared yang
    // melaporkan core host > kuota, batasi via env WORKER_THREADS agar tak
    // over-spawn worker (tiap worker punya stack) → hemat RAM.
    if let Some(n) = std::env::var("WORKER_THREADS").ok().and_then(|v| v.parse::<usize>().ok()) {
        builder.worker_threads(n.max(1));
    }
    builder.build()?.block_on(run())
}

/// Badan aplikasi (dulu `main`), berjalan di dalam runtime yang sudah dibatasi.
async fn run() -> Result<()> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "e_ticketing=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = AppConfig::from_env()?;

    tracing::info!(host = %cfg.host, port = cfg.port, "Config loaded");

    // ── Telegram notifier ────────────────────────────────────────────────────
    if !cfg.telegram.bot_token.is_empty() && cfg.telegram.admin_chat_id != 0 {
        let tg = Arc::new(TelegramService::new(
            cfg.telegram.bot_token.clone(),
            cfg.telegram.admin_chat_id,
        ));
        init_telegram_notifier(tg);
        tracing::info!(
            admin_chat_id = cfg.telegram.admin_chat_id,
            "Telegram alert aktif"
        );
    } else {
        tracing::warn!("TELEGRAM_BOT_TOKEN/TELEGRAM_ADMIN_CHAT_ID tidak di-set");
    }

    // ── Database & Redis ─────────────────────────────────────────────────────
    let pool = create_pool(&cfg.database_url, cfg.db_pool_max_size).await?;
    tracing::info!("Postgres pool ready (max={})", cfg.db_pool_max_size);

    // ── Migrasi ──────────────────────────────────────────────────────────────
    // Dijalankan SEBELUM apa pun menyentuh tabel, dijaga advisory lock sehingga
    // aman saat beberapa replika start bersamaan.
    //
    // Gagal migrasi = gagal start, disengaja. Melanjutkan dengan skema yang tak
    // cocok hanya memindahkan kegagalan ke request pertama pengguna, dalam
    // bentuk "column ... does not exist" yang jauh lebih sulit dilacak.
    //
    // `AUTO_MIGRATE=false` mematikannya bagi deployment yang menjalankan
    // migrasi lewat langkah terpisah.
    let auto_migrate = std::env::var("AUTO_MIGRATE")
        .map(|v| !matches!(v.trim().to_ascii_lowercase().as_str(), "0" | "false" | "no"))
        .unwrap_or(false);

    if auto_migrate {
        e_ticketing::config::migrate::run(&pool).await?;
    } else {
        tracing::warn!("AUTO_MIGRATE dimatikan — pastikan skema sudah dimigrasi terpisah");
    }

    let redis_url = format!("{}/1", cfg.redis_url.trim_end_matches('/'));
    let redis_conn = redis::aio::ConnectionManager::new_with_config(
        redis::Client::open(redis_url.as_str())?,
        redis::aio::ConnectionManagerConfig::new()
            .set_response_timeout(Some(std::time::Duration::from_secs(10)))
            .set_connection_timeout(Some(std::time::Duration::from_secs(10)))
            .set_number_of_retries(3),
    )
    .await?;
    tracing::info!("Redis connected to DB 1");

    let ws_redis_client =
        redis::Client::open(format!("{}/2", cfg.redis_url.trim_end_matches('/')).as_str())?;

    // ── Deteksi kapasitas VPS (cgroup-aware) → plafon auto-skala ──────────────
    let capacity = e_ticketing::utils::capacity::detect();
    tracing::info!(
        cpu_cores = capacity.cpu_cores,
        ram_mb = capacity.ram_bytes / (1024 * 1024),
        source = capacity.source,
        max_ws = capacity.recommended_max_ws,
        rec_db_pool = capacity.recommended_db_pool,
        max_upload = capacity.recommended_upload_concurrency,
        "Kapasitas terdeteksi (batas WS & upload auto-skala dari RAM)"
    );

    // ── Upload temp dir (streaming) ──────────────────────────────────────────
    // Media story di-stream ke file temp lalu diteruskan ke storage. Direktori
    // ini WAJIB disk-backed: bila tmpfs (RAM), streaming justru tetap memakan
    // RAM. Default /var/tmp (disk & persisten, beda dari /tmp yang kerap tmpfs).
    let upload_tmp_dir = PathBuf::from(
        std::env::var("UPLOAD_TMP_DIR")
            .unwrap_or_else(|_| "/var/tmp/e-ticketing-uploads".into()),
    );
    prepare_upload_tmp_dir(&upload_tmp_dir)?;

    // ── App state ────────────────────────────────────────────────────────────
    let state = Arc::new(
        AppState::new(
            pool,
            &cfg.jwt_secret,
            cfg.internal_jwt_secret.clone(),
            cfg.bcrypt_cost,
            cfg.jwt_expiry_hours,
            Arc::new(cfg.waha.clone()),
            redis_conn,
            ws_redis_client,
            cfg.rustfs.clone(),
            cfg.sfu_bind_addr.clone(),
            upload_tmp_dir,
            capacity,
        )
        .await,
    );

    // ── Pembersihan refresh token kedaluwarsa ────────────────────────────────
    // Baris yang sudah DICABUT sengaja tetap disimpan sampai kedaluwarsa —
    // itulah yang membuat deteksi pemakaian ulang bekerja, karena token curian
    // yang dicoba lagi masih ketemu barisnya. Setelah lewat masa berlaku ia tak
    // berguna lagi, dan tanpa pembersihan tabelnya tumbuh selamanya.
    {
        let svc = state.refresh_svc.clone();
        tokio::spawn(async move {
            // Dijalankan sekali saat start lalu sehari sekali. Bukan pekerjaan
            // mendesak, jadi kegagalannya cukup dicatat — tak boleh menjatuhkan
            // proses.
            loop {
                match svc.cleanup_expired().await {
                    Ok(n) if n > 0 => {
                        tracing::info!(dihapus = n, "refresh token kedaluwarsa dibersihkan")
                    }
                    Ok(_) => {}
                    // `%e` MENELAN penyebabnya: `Display` untuk
                    // `AppError::Internal` berbunyi tetap "Internal server
                    // error", apa pun galat aslinya. Yang tercatat di produksi
                    // karena itu adalah peringatan yang tak memberi tahu satu
                    // hal pun — tabel tak ada? izin kurang? pool habis? Semua
                    // terbaca sama.
                    //
                    // `{:#}` pada rantai anyhow di dalamnya mengeluarkan sebab
                    // sebenarnya, lengkap dengan query yang gagal (lihat
                    // `repository::db::format_pg_error`).
                    Err(e) => {
                        let sebab = match &e {
                            e_ticketing::utils::error::AppError::Internal(inner) => format!("{inner:#}"),
                            lain => lain.to_string(),
                        };
                        tracing::warn!(error = %sebab, "pembersihan refresh token gagal");
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(24 * 3600)).await;
            }
        });
    }

    let ws_state = Arc::new(WsAppState {
        jwt: state.jwt.clone(),
        ws_mgr: state.ws_mgr.clone(),
        group_svc: state.group_chat_svc.clone(),
    });

    // ── CORS ─────────────────────────────────────────────────────────────────
    let cors = build_cors(&cfg);

    // ── Leptos SSR router ─────────────────────────────────────────────────────
    let leptos_conf =
        get_configuration(Some("Cargo.toml"))
            .map_err(|e| anyhow::anyhow!("failed to load leptos config: {e}"))?;
    let leptos_options = leptos_conf.leptos_options;
    let bind_addr = format!("{}:{}", cfg.host, cfg.port);
    let socket_addr: std::net::SocketAddr = bind_addr
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid bind address {bind_addr}: {e}"))?;
    let site_root = leptos_options.site_root.to_string();

    tracing::info!(site_root = %site_root, bind_addr = %socket_addr, "Leptos static assets dir");

    let ssr_routes = generate_route_list(App);

    let mut leptos_router = axum::Router::new()
        .leptos_routes(&leptos_options, ssr_routes, {
            let opts = leptos_options.clone();
            move || shell(opts.clone())
        })
        // ── /pkg/* disajikan ServeDir yang MEMAKAI berkas pra-kompresi ────────
        //
        // `cargo leptos build --release --precompress` menulis `.br` dan `.gz`
        // di samping tiap aset. Sampai sekarang tak ada yang pernah membacanya:
        // seluruh `/pkg/*` jatuh ke `file_and_error_handler` di bawah, yang
        // menyajikan berkas mentah, lalu `CompressionLayer` di ujung router
        // memampatkannya ULANG — setiap permintaan, dari nol.
        //
        // Untuk aset kecil itu tak terasa. Untuk bundle WASM yang berukuran
        // megabyte, brotli adalah operasi CPU yang berat, dan ia dijalankan
        // per-permintaan di kotak 2 vCPU sambil menahan permintaan lain di
        // antrean yang sama. Kunjungan pertama seorang pengunjung karena itu
        // membayar kompresi penuh sebuah bundle multi-megabyte — dan itulah
        // yang terasa sebagai "buka pertama sangat lambat".
        //
        // `ServeDir` mengirim berkas `.br` yang sudah jadi, apa adanya, dengan
        // header `content-encoding: br`. `CompressionLayer` melihat header itu
        // dan melewatkannya, jadi tak ada kompresi kedua. Peramban lama yang
        // tak mengirim `Accept-Encoding: br` tetap dilayani berkas aslinya.
        //
        // Pakai `route_service` ber-wildcard, BUKAN `nest_service`:
        // `nest_service("/pkg", …)` memasang penangkap-semua di bawah `/pkg`
        // dan bertabrakan dengan rute alias `_bg.wasm` di bawah — axum menolak
        // keduanya saat start. Dengan wildcard, segmen statis alias tetap menang
        // atas wildcard di router matchit.
        //
        // Direktorinya `site_root` (bukan `site_root/pkg`) karena
        // `route_service` TIDAK memotong awalan path seperti `nest_service`:
        // permintaan `/pkg/x.wasm` sampai apa adanya, dan ServeDir menyusunnya
        // menjadi `site_root/pkg/x.wasm`.
        .route_service(
            "/pkg/{*path}",
            tower_http::services::ServeDir::new(&site_root)
                .precompressed_br()
                .precompressed_gzip(),
        )
        .fallback(leptos_axum::file_and_error_handler(shell));

    // Nama bundle WASM yang ditulis build vs yang dimuat glue JS bisa berbeda —
    // lihat `wasm_bg_alias`. Tanpa alias ini hydration diam-diam tak pernah
    // jalan dan seluruh aplikasi jadi HTML mati yang tampak normal.
    if let Some((alias, berkas)) = wasm_bg_alias(&site_root, &leptos_options.output_name) {
        tracing::warn!(
            alias = %alias,
            berkas = %berkas.display(),
            "bundle WASM bernama beda dari yang diminta glue JS — alias dipasang \
             agar hydration tetap jalan"
        );
        leptos_router =
            leptos_router.route_service(&alias, tower_http::services::ServeFile::new(berkas));
    }

    let leptos_router: axum::Router = leptos_router
        // Silent refresh HARUS di atas jalur Leptos: ia menyuntikkan token baru
        // ke header Cookie permintaan ini, sehingga SSR dan server function di
        // belakangnya langsung melihat pengguna yang sudah masuk.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            e_ticketing::middleware::silent_refresh::silent_refresh,
        ))
        .layer(axum::middleware::from_fn(pkg_no_cache))
        // Provide AppState as Axum Extension so server functions can extract it
        .layer(axum::Extension(state.clone()))
        .with_state(leptos_options);

    // ── Upload routes ─────────────────────────────────────────────────────────
    // DefaultBodyLimit: tanpa ini axum memakai batas default 2MB — upload video
    // story >2MB ditolak 413 sebelum sampai handler, padahal service mengizinkan
    // 50MB. Batas 52MB (media 50MB + overhead multipart) sekaligus jadi pagar
    // RAM per-request karena handler membaca file ke memori.
    let upload_router: axum::Router = axum::Router::new()
        .route("/upload/story", axum::routing::post(story_upload))
        .route(
            "/upload/merchant-image",
            axum::routing::post(merchant_image_upload),
        )
        .layer(axum::extract::DefaultBodyLimit::max(52 * 1024 * 1024))
        .layer(axum::Extension(state.clone()));

    // ── REST API router (Next.js frontend) ───────────────────────────────────
    let rest_api = rest_router().with_state(state.clone());

    // ── Live streaming router (WebRTC SFU) ──────────────────────────────────
    let live_api = live_router(state.clone());

    // ── Meet router (WebRTC P2P mesh + waiting room) ─────────────────────────
    let meet_api = meet_router(state.clone());

    // ── Health check (untuk Docker HEALTHCHECK / pingora / uptime monitor) ───
    // Sengaja tanpa query DB: liveness murah yang tak ikut tumbang saat DB
    // sibuk. Kesehatan DB sudah diverifikasi fail-fast saat startup.
    let health_router: axum::Router = axum::Router::new()
        .route("/healthz", axum::routing::get(|| async { "ok" }));

    // ── SEO: robots.txt + sitemap.xml (dinamis dari DB) ──────────────────────
    let seo_router: axum::Router = axum::Router::new()
        .route(
            "/robots.txt",
            axum::routing::get(e_ticketing::web::api::seo_routes::robots_txt),
        )
        .route(
            "/sitemap.xml",
            axum::routing::get(e_ticketing::web::api::seo_routes::sitemap_xml),
        )
        .layer(axum::Extension(state.clone()));

    // ── WebSocket + REST API + CSS assets + SSR ───────────────────────────────
    let app = chat_router(ws_state, state.clone())
        .layer(cors)
        .merge(health_router)
        .merge(seo_router)
        .merge(e_ticketing::web::assets::router())
        .merge(upload_router)
        .merge(rest_api)
        .merge(live_api)
        .merge(meet_api)
        .merge(leptos_router)
        .layer(tower_http::compression::CompressionLayer::new());

    let listener = TcpListener::bind(socket_addr).await?;
    tracing::info!("Pulse (SSR + WebSocket) listening on http://{}", bind_addr);
    tracing::info!("   Server fns   : http://{}/api-fn/*", bind_addr);
    tracing::info!("   SSR pages    : http://{}/*", bind_addr);
    tracing::info!("   WebSocket    : http://{}/ws/*", bind_addr);
    tracing::info!("   SFU (WebRTC) : udp://{}", cfg.sfu_bind_addr);

    // Saat sinyal shutdown tiba: batalkan CancellationToken WsManager agar task
    // latar (subscriber Redis, heartbeat per-koneksi, shrink) berhenti rapi —
    // bukan di-kill paksa — SEBELUM axum berhenti menerima & men-drain HTTP.
    let shutdown_state = state.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("Shutdown: membatalkan task WS (subscriber Redis, heartbeat, shrink)");
            shutdown_state.ws_mgr.shutdown();
        })
        .await?;

    Ok(())
}

/// Siapkan direktori temp upload: buat bila belum ada, uji-tulis (canary), dan
/// peringatkan bila berada di tmpfs/ramfs (RAM). Gagal buat/tulis → fail-fast:
/// lebih baik tahu saat deploy daripada setiap upload error 500.
fn prepare_upload_tmp_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("gagal membuat UPLOAD_TMP_DIR {}", dir.display()))?;

    // Canary: pastikan benar-benar writable (permission/read-only mount).
    let canary = dir.join(".write-test");
    std::fs::write(&canary, b"ok")
        .with_context(|| format!("UPLOAD_TMP_DIR {} tidak bisa ditulis", dir.display()))?;
    let _ = std::fs::remove_file(&canary);

    match mount_fstype_for(dir) {
        Some(fs) if fs == "tmpfs" || fs == "ramfs" => tracing::warn!(
            dir = %dir.display(), fstype = %fs,
            "UPLOAD_TMP_DIR berada di RAM (tmpfs/ramfs) — streaming upload tetap \
             memakai RAM. Set UPLOAD_TMP_DIR ke direktori disk (mis. /var/tmp)."
        ),
        Some(fs) => tracing::info!(dir = %dir.display(), fstype = %fs, "Upload temp dir siap (disk-backed)"),
        None => tracing::info!(dir = %dir.display(), "Upload temp dir siap"),
    }
    Ok(())
}

/// Fstype dari mount yang paling spesifik (prefix terpanjang) menaungi `dir`,
/// dibaca dari /proc/mounts. None di non-Linux / bila tak terbaca (best-effort).
fn mount_fstype_for(dir: &Path) -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    let target = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let mut best: Option<(usize, String)> = None;
    for line in mounts.lines() {
        let mut it = line.split_whitespace();
        let (Some(_dev), Some(mount_point), Some(fstype)) = (it.next(), it.next(), it.next())
        else {
            continue;
        };
        // Cocokkan per-komponen agar "/var" tak salah cocok dengan "/variant".
        if target.starts_with(Path::new(mount_point)) {
            let len = mount_point.len();
            if best.as_ref().is_none_or(|(l, _)| len > *l) {
                best = Some((len, fstype.to_string()));
            }
        }
    }
    best.map(|(_, fs)| fs)
}

fn build_cors(_cfg: &AppConfig) -> tower_http::cors::CorsLayer {
    use tower_http::cors::{Any, CorsLayer};
    if let Ok(origin) = std::env::var("CORS_ALLOW_ORIGIN") {
        CorsLayer::new()
            .allow_methods(Any)
            .allow_headers(Any)
            .allow_origin(
                origin
                    .parse::<axum::http::HeaderValue>()
                    .expect("invalid CORS_ALLOW_ORIGIN"),
            )
    } else {
        CorsLayer::new()
            .allow_methods(Any)
            .allow_headers(Any)
            .allow_origin(Any)
    }
}

/// Cari berkas WASM yang namanya tak cocok dengan yang diminta glue JS.
///
/// cargo-leptos 0.3.7 menulis bundle sebagai `<output-name>.wasm`, sedangkan
/// glue JS yang ia hasilkan SENDIRI memuat `<output-name>_bg.wasm`. Namanya
/// beda satu suku kata, dan akibatnya tak sebanding: permintaan WASM dijawab
/// 404.
///
/// Yang membuat kegagalan ini mahal adalah bentuknya. Tak ada halaman error,
/// tak ada layar putih — HTML dari SSR tetap terpampang utuh, bisa digulir,
/// bisa diketik. Yang tidak terjadi hanyalah hydration, sehingga TIDAK SATU PUN
/// tombol di seluruh aplikasi bekerja. Ditekan, tak ada loading, tak ada galat,
/// tak ada permintaan ke server. Semua tampak baik-baik saja kecuali bahwa tak
/// ada yang berfungsi.
///
/// `Dockerfile` sudah menormalkan nama ini saat membangun image. Alias di sini
/// membuat jalur dev (`make dev` / `cargo leptos watch`) ikut benar tanpa perlu
/// menyalin berkas secara manual sesudah setiap rebuild — dan tetap diam bila
/// versi cargo-leptos berikutnya sudah menulis nama yang benar.
///
/// Mengembalikan `(path URL alias, berkas nyata di disk)`.
fn wasm_bg_alias(site_root: &str, output_name: &str) -> Option<(String, std::path::PathBuf)> {
    let pkg = std::path::Path::new(site_root).join("pkg");
    let diminta = pkg.join(format!("{output_name}_bg.wasm"));
    let tersedia = pkg.join(format!("{output_name}.wasm"));

    // Tak ada yang bisa dialiaskan.
    if !tersedia.exists() {
        return None;
    }

    // Berkas ber-nama benar sudah ada — tapi belum tentu masih sahih. Salinan
    // manual yang pernah dibuat orang untuk menambal masalah ini tidak ikut
    // diperbarui saat `cargo leptos watch` membangun ulang, dan salinan basi
    // JAUH lebih buruk daripada 404: glue JS yang baru bertemu WASM lama, lalu
    // hydration gagal dengan "is not a function" yang membingungkan.
    //
    // Karena itu yang dibandingkan bukan sekadar ada/tidak, melainkan umurnya.
    if diminta.exists() {
        let umur = |p: &std::path::Path| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
        };
        match (umur(&diminta), umur(&tersedia)) {
            // Salinannya lebih tua dari hasil build terbaru → jangan dipakai.
            (Some(a), Some(b)) if a < b => {}
            // Sama baru (atau umurnya tak terbaca): percayai berkas aslinya.
            _ => return None,
        }
    }

    Some((format!("/pkg/{output_name}_bg.wasm"), tersedia))
}

/// Prevent browsers from caching /pkg/* (JS/WASM) across deploys.
/// Without this, stale JS + new WASM causes "is not a function" hydration crashes.
async fn pkg_no_cache(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let is_pkg = req.uri().path().starts_with("/pkg/");
    let mut res = next.run(req).await;
    if is_pkg {
        let headers = res.headers_mut();
        headers.insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-cache, must-revalidate"),
        );
    }
    res
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("failed to install Ctrl+C handler: {e}");
        }
    };
    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c    => tracing::info!("Ctrl+C received"),
        _ = terminate => tracing::info!("SIGTERM received"),
    }
}

#[cfg(test)]
mod tests {
    /// Rute `/pkg/{*path}` (ServeDir pra-kompresi) dan rute STATIS
    /// `/pkg/<nama>_bg.wasm` (alias hydration) harus bisa hidup berdampingan.
    ///
    /// Ini diuji karena kegagalannya tidak terlihat saat kompilasi: axum
    /// memeriksa tabrakan rute saat `Router` DIBANGUN, yaitu di dalam `run()`.
    /// Bentuk yang salah — mis. `nest_service("/pkg", …)` alih-alih wildcard —
    /// membuat proses PANIK saat start, sesudah build hijau dan sesudah image
    /// ter-push. Test ini memindahkan kegagalan itu ke `cargo test`.
    #[test]
    fn rute_pkg_wildcard_dan_alias_tidak_bertabrakan() {
        let dir = std::env::temp_dir();
        let _router: axum::Router = axum::Router::new()
            .route_service(
                "/pkg/{*path}",
                tower_http::services::ServeDir::new(&dir)
                    .precompressed_br()
                    .precompressed_gzip(),
            )
            .route_service(
                "/pkg/e-ticketing_bg.wasm",
                tower_http::services::ServeFile::new(dir.join("x.wasm")),
            );
    }
}
