use std::sync::Arc;

use anyhow::Result;
use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

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

    // FIX: dotenvy MUST come before tracing init so RUST_LOG from .env is picked up.
    // Previously this was called AFTER tracing init → RUST_LOG from .env file ignored.
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kinetic_api=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = AppConfig::from_env()?;
    tracing::info!(host=%cfg.host, port=cfg.port, "Config loaded");

    if cfg.telegram.bot_token.is_empty() || cfg.telegram.admin_chat_id == 0 {
        tracing::warn!("TELEGRAM_BOT_TOKEN / TELEGRAM_ADMIN_CHAT_ID belum di-set — alert dinonaktifkan");
    } else {
        let tg = Arc::new(TelegramService::new(
            cfg.telegram.bot_token.clone(),
            cfg.telegram.admin_chat_id,
        ));
        init_telegram_notifier(tg);
        tracing::info!(admin_chat_id = cfg.telegram.admin_chat_id, "Telegram error alert aktif ✅");
    }

    let pool = create_pool(&cfg.database_url, cfg.db_pool_max_size).await?;
    tracing::info!("Postgres pool ready (max={})", cfg.db_pool_max_size);

    let redis_url    = format!("{}/1", cfg.redis_url.trim_end_matches('/'));
    let redis_client = redis::Client::open(redis_url.as_str())?;
    let redis_conn   = redis::aio::ConnectionManager::new_with_config(
        redis_client.clone(),
        redis::aio::ConnectionManagerConfig::new()
            .set_response_timeout(Some(std::time::Duration::from_secs(10)))
            .set_connection_timeout(Some(std::time::Duration::from_secs(10)))
            .set_number_of_retries(3),
    ).await?;
    tracing::info!("Redis connected to DB 1");

    let ws_redis_url    = format!("{}/2", cfg.redis_url.trim_end_matches('/'));
    let ws_redis_client = redis::Client::open(ws_redis_url.as_str())?;

    let state = Arc::new(
        AppState::new(
            pool,
            &cfg.jwt_secret,
            cfg.bcrypt_cost,
            cfg.jwt_expiry_hours,
            Arc::new(cfg.waha),
            redis_conn,
            ws_redis_client,
            cfg.rustfs,
        ).await,
    );

    let ws_state = Arc::new(WsAppState {
        jwt:       state.jwt.clone(),
        ws_mgr:    state.ws_mgr.clone(),
        group_svc: state.group_chat_svc.clone(),
    });

    let app = routes::build_router(state.clone())
        .merge(chat_router(ws_state.clone(), state.clone()))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any)
                .allow_origin(tower_http::cors::Any),
        );

    let addr     = format!("{}:{}", cfg.host, cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("KINETIC API + WS listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
