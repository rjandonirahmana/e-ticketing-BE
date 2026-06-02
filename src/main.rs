//! main.rs — Entry point backend e-ticketing (API + Leptos SSR).
//!
//! Cara jalankan:
//!   cargo run                     # SSR + API (tanpa WASM hydration)
//!   cargo leptos watch            # SSR + API + WASM hydration (full dev)
//!
//! Satu binary, satu port:
//!   /api/*      → REST API handlers (Axum)
//!   /api-fn/*   → Leptos server functions
//!   /pkg/*      → Static assets (WASM/JS/CSS) — butuh cargo leptos build
//!   /*          → Leptos SSR rendering

// View Leptos membangun tipe nested sangat dalam (terutama ExplorePage);
// batas default 128 tidak cukup → naikkan agar type-checking tidak overflow.
#![recursion_limit = "512"]

use std::sync::Arc;

use anyhow::Result;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod config;
mod middleware;
mod models;
mod proto;
mod repository;
mod routes;
mod service;
mod state;
mod utils;
mod web;
mod ws;
// Modul CSR dicompile sebagai rlib di server (tanpa WASM),
// referensinya hanya dari src/lib.rs saat feature "hydrate".
#[allow(dead_code)]
mod csr;

use config::{config::AppConfig, database::create_pool};
use service::telegram::TelegramService;
use state::AppState;
use utils::error::init_telegram_notifier;
use web::app::{shell, App};
use ws::handler::WsAppState;
use ws::routes::chat_router;

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
    let addr = format!("{}:{}", cfg.host, cfg.port);

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
            "Telegram alert aktif ✅"
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

    // ── App state ────────────────────────────────────────────────────────────
    let state = Arc::new(
        AppState::new(
            pool,
            &cfg.jwt_secret,
            cfg.internal_jwt_secret.clone(),
            cfg.bcrypt_cost.clone(),
            cfg.jwt_expiry_hours.clone(),
            Arc::new(cfg.waha.clone()),
            redis_conn,
            ws_redis_client,
            cfg.rustfs.clone(),
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

    // ── API router (semua /api/* routes) ────────────────────────────────────
    let api_router = routes::build_router(state.clone())
        .merge(chat_router(ws_state, state.clone()))
        .layer(cors);

    // ── Leptos SSR router ─────────────────────────────────────────────────────
    let leptos_conf =
        get_configuration(Some("Cargo.toml")).expect("[package.metadata.leptos] tidak ditemukan di Cargo.toml");
    let leptos_options = leptos_conf.leptos_options;

    // Gunakan site_addr dari [package.metadata.leptos] sebagai bind address.
    // Ini penting agar cargo leptos watch bisa detect server di port yang benar.
    // Override via env: LEPTOS_SITE_ADDR=0.0.0.0:8080
    let bind_addr = leptos_options.site_addr.to_string();

    let site_root = leptos_options.site_root.to_string();
    tracing::info!(
        site_root = %site_root,
        bind_addr = %bind_addr,
        "Leptos static assets dir (WASM/CSS)"
    );

    let ssr_routes = generate_route_list(App);

    let leptos_router: axum::Router = axum::Router::new()
        .leptos_routes(&leptos_options, ssr_routes, {
            let opts = leptos_options.clone();
            move || shell(opts.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    // ── Gabung API + SSR + CSS ───────────────────────────────────────────────
    // `/styles/*.css` disajikan dari CSS yang di-embed ke binary (lihat
    // `web::assets`). Di-merge di level paling atas agar tidak tertutup oleh
    // fallback SSR dan tidak bergantung working-directory.
    let app = api_router
        .merge(crate::web::assets::router())
        .merge(leptos_router);

    let listener = TcpListener::bind(&bind_addr).await?;
    tracing::info!("🚀 Pulse (API + SSR) listening on http://{}", bind_addr);
    tracing::info!("   API          : http://{}/api/*", bind_addr);
    tracing::info!("   Server fns   : http://{}/api-fn/*", bind_addr);
    tracing::info!("   SSR pages    : http://{}/*", bind_addr);
    tracing::info!("   Tip: pakai 'cargo leptos watch' untuk dev dengan WASM hydration");

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

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
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
