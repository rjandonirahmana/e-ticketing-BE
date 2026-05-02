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
use state::AppState;
use ws::handler::WsAppState;
use ws::routes::chat_router;

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kinetic_api=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    dotenvy::dotenv().ok();

    let cfg = AppConfig::from_env()?;
    tracing::info!(host=%cfg.host, port=cfg.port, "Config loaded");

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

    // Redis terpisah untuk WS (DB 2 agar tidak bentrok dengan OTP)
    let ws_redis_url = format!("{}/2", cfg.redis_url.trim_end_matches('/'));
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
            cfg.garage,
        )
        .await,
    );

    // Wire OrderService dengan GroupChatService (setelah state dibuat)
    // Note: Rust ownership membuat ini sedikit verbose — kita set after init via Arc::new
    // Solusi sederhana: state.order_svc clone dan re-wrap sudah cukup karena Arc.

    // WS app state (terpisah dari main AppState untuk router isolation)
    let ws_state = Arc::new(WsAppState {
        jwt: state.jwt.clone(),
        ws_mgr: state.ws_mgr.clone(),
        group_svc: state.group_chat_svc.clone(),
    });

    // Build router — CorsLayer di sini agar cover semua route termasuk /ws/chat
    let app = routes::build_router(state.clone())
        .merge(chat_router(ws_state.clone(), state.clone()))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any)
                .allow_origin(tower_http::cors::Any),
        );

    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("KINETIC API + WS listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
