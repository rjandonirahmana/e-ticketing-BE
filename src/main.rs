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

use std::sync::Arc;

use anyhow::Result;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use e_ticketing::config::{config::AppConfig, database::create_pool};
use e_ticketing::service::telegram::TelegramService;
use e_ticketing::state::AppState;
use e_ticketing::utils::error::init_telegram_notifier;
use e_ticketing::api::rest_router;
use e_ticketing::web::api::upload::story_upload;
use e_ticketing::web::app::{shell, App};
use e_ticketing::ws::handler::WsAppState;
use e_ticketing::ws::routes::chat_router;
use e_ticketing::live::api::live_router;
use e_ticketing::meet::api::meet_router;

use leptos::config::get_configuration;
use leptos_axum::{generate_route_list, LeptosRoutes};

#[tokio::main]
async fn main() -> Result<()> {
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
        "Kapasitas terdeteksi (batas WS auto-skala dari RAM)"
    );

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
            capacity,
        )
        .await,
    );

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

    let leptos_router: axum::Router = axum::Router::new()
        .leptos_routes(&leptos_options, ssr_routes, {
            let opts = leptos_options.clone();
            move || shell(opts.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
        .layer(axum::middleware::from_fn(pkg_no_cache))
        // Provide AppState as Axum Extension so server functions can extract it
        .layer(axum::Extension(state.clone()))
        .with_state(leptos_options);

    // ── Upload routes ─────────────────────────────────────────────────────────
    let upload_router: axum::Router = axum::Router::new()
        .route("/upload/story", axum::routing::post(story_upload))
        .layer(axum::Extension(state.clone()));

    // ── REST API router (Next.js frontend) ───────────────────────────────────
    let rest_api = rest_router().with_state(state.clone());

    // ── Live streaming router (WebRTC SFU) ──────────────────────────────────
    let live_api = live_router(state.clone());

    // ── Meet router (WebRTC P2P mesh + waiting room) ─────────────────────────
    let meet_api = meet_router(state.clone());

    // ── WebSocket + REST API + CSS assets + SSR ───────────────────────────────
    let app = chat_router(ws_state, state.clone())
        .layer(cors)
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

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
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
