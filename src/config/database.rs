use deadpool_postgres::{Config, Pool, Runtime};
use tokio_postgres::NoTls;

pub async fn create_pool(database_url: &str, max_size: usize) -> anyhow::Result<Pool> {
    let mut cfg = Config::new();
    cfg.url = Some(database_url.to_string());
    cfg.pool = Some(deadpool_postgres::PoolConfig {
        max_size,
        ..Default::default()
    });

    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;

    // Smoke-test the pool with a trivial query so a misconfigured DATABASE_URL
    // fails fast at startup instead of on the first request.
    let client = pool.get().await?;
    client.simple_query("SELECT 1").await?;

    Ok(pool)
}
