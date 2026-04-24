use std::sync::Arc;

use anyhow::Result;
use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod middleware;
mod models;
mod repository;
mod routes;
mod service;
mod state;
mod utils;

use config::{config::AppConfig, database::create_pool};
use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
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

    // ── App state + router ────────────────────────────────────────────────
    let state = Arc::new(AppState::new(
        pool,
        &cfg.jwt_secret,
        cfg.bcrypt_cost,
        cfg.jwt_expiry_hours,
    ));
    let app = routes::build_router(state);

    // ── Bind + serve ──────────────────────────────────────────────────────
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = TcpListener::bind(&addr).await?;
    tracing::info!("KINETIC API listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
