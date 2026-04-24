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

    // Test connection
    let client = pool.get().await?;
    client.check_connection().await?;

    Ok(pool)
}
