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
mod ws;

use config::{config::AppConfig, database::create_pool};
use service::telegram::TelegramService;
use state::AppState;
use utils::error::init_telegram_notifier;
use ws::handler::WsAppState;
use ws::routes::chat_router;

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
    tracing::info!(host=%cfg.host, port=cfg.port, "Config loaded");

    if cfg.telegram.bot_token.is_empty() || cfg.telegram.admin_chat_id == 0 {
        tracing::warn!(
            "TELEGRAM_BOT_TOKEN / TELEGRAM_ADMIN_CHAT_ID belum di-set — alert dinonaktifkan"
        );
    } else {
        let tg = Arc::new(TelegramService::new(
            cfg.telegram.bot_token.clone(),
            cfg.telegram.admin_chat_id,
        ));
        init_telegram_notifier(tg);
        tracing::info!(
            admin_chat_id = cfg.telegram.admin_chat_id,
            "Telegram error alert aktif ✅"
        );
    }

    let pool = create_pool(&cfg.database_url, cfg.db_pool_max_size).await?;
    tracing::info!("Postgres pool ready (max={})", cfg.db_pool_max_size);

    let redis_url = format!("{}/1", cfg.redis_url.trim_end_matches('/'));
    let redis_client = redis::Client::open(redis_url.as_str())?;
    let redis_conn = redis::aio::ConnectionManager::new_with_config(
        redis_client.clone(),
        redis::aio::ConnectionManagerConfig::new()
            .set_response_timeout(Some(std::time::Duration::from_secs(10)))
            .set_connection_timeout(Some(std::time::Duration::from_secs(10)))
            .set_number_of_retries(3),
    )
    .await?;
    tracing::info!("Redis connected to DB 1");

    let ws_redis_url = format!("{}/2", cfg.redis_url.trim_end_matches('/'));
    let ws_redis_client = redis::Client::open(ws_redis_url.as_str())?;

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
    // FE dan BE di-serve dari origin yang SAMA (satu port, satu domain).
    // CORS hanya dibutuhkan kalau ada integrasi third-party.
    // Untuk development/lokal dengan proxy berbeda, bisa set CORS_ALLOW_ORIGIN.
    let cors = build_cors(&cfg);

    let app = routes::build_router(state.clone())
        .merge(chat_router(ws_state.clone(), state.clone()))
        .layer(cors);

    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("🚀 Pulse (API + Frontend) listening on http://{}", addr);
    tracing::info!(
        "   Frontend dist dir: {}",
        std::env::var("FRONTEND_DIST_DIR").unwrap_or_else(|_| "dist".into())
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn build_cors(cfg: &AppConfig) -> tower_http::cors::CorsLayer {
    use tower_http::cors::{Any, CorsLayer};

    // Jika CORS_ALLOW_ORIGIN di-set, gunakan itu. Kalau tidak, allow any
    // (aman karena BE dan FE satu origin di production).
    if let Ok(origin) = std::env::var("CORS_ALLOW_ORIGIN") {
        tracing::info!(origin=%origin, "CORS: restricted to specific origin");
        CorsLayer::new()
            .allow_methods(Any)
            .allow_headers(Any)
            .allow_origin(
                origin
                    .parse::<axum::http::HeaderValue>()
                    .expect("CORS_ALLOW_ORIGIN bukan valid header value"),
            )
    } else {
        tracing::info!("CORS: allow any origin");
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
        _ = ctrl_c => { tracing::info!("Ctrl+C received, shutting down..."); },
        _ = terminate => { tracing::info!("SIGTERM received, shutting down..."); },
    }
}
