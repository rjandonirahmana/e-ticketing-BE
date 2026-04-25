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

use config::{config::AppConfig, database::create_pool};
use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");
    // ── Logging ────────────────────────────────────────────────────────────
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kinetic_api=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    dotenvy::dotenv().ok();

    // ── Config ────────────────────────────────────────────────────────────
    let cfg = AppConfig::from_env()?;
    tracing::info!(host = %cfg.host, port = cfg.port, "Config loaded");

    // ── Postgres pool ─────────────────────────────────────────────────────
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

    // NOTE: dulu di sini ada `FLUSHDB` setiap startup — itu menghapus SEMUA
    // OTP & sesi pending tiap proses restart. Dihapus karena destructive.
    // Kalau memang butuh flush manual, lakukan via redis-cli, bukan otomatis.

    // ── App state + router ────────────────────────────────────────────────
    let state = Arc::new(AppState::new(
        pool,
        &cfg.jwt_secret,
        cfg.bcrypt_cost,
        cfg.jwt_expiry_hours,
        Arc::new(cfg.waha),
        redis_conn,
    ));
    let app = routes::build_router(state);

    // ── Bind + serve ──────────────────────────────────────────────────────
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("KINETIC API listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
