use anyhow::Context;
use std::env;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub db_pool_max_size: usize,
    pub jwt_secret: String,
    pub jwt_expiry_hours: i64,
    pub bcrypt_cost: u32,
    pub redis_url: String,
    pub waha: WahaConfig,
}

#[derive(Clone, Debug)]
pub struct WahaConfig {
    pub base_url: String,
    pub session: String,
    pub api_key: String,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            host: "0.0.0.0".into(),
            port: 8080,
            database_url: env::var("DATABASE_URL").context("DATABASE_URL is required")?,
            db_pool_max_size: env::var("DB_POOL_MAX_SIZE")
                .unwrap_or_else(|_| "16".into())
                .parse()
                .context("DB_POOL_MAX_SIZE must be a number")?,
            jwt_secret: env::var("JWT_SECRET").context("JWT_SECRET is required")?,
            jwt_expiry_hours: env::var("JWT_EXPIRY_HOURS")
                .unwrap_or_else(|_| "24".into())
                .parse()
                .context("JWT_EXPIRY_HOURS must be a number")?,
            // bcrypt minimum cost = 4. Default 10 = ~80ms per hash, masih
            // aman dengan spawn_blocking. Kalau dulu sempat di-set "1", bcrypt
            // akan panic karena di bawah batas minimum.
            bcrypt_cost: env::var("BCRYPT_COST")
                .unwrap_or_else(|_| "10".into())
                .parse()
                .context("BCRYPT_COST must be a number")?,
            redis_url: env::var("REDIS_URL").unwrap_or_else(|_| "".into()),

            waha: WahaConfig {
                base_url: std::env::var("WAHA_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:3000".to_string()),
                session: std::env::var("WAHA_SESSION").unwrap_or_else(|_| "default".to_string()),
                api_key: std::env::var("WAHA_API_KEY").unwrap_or_default(),
            },
        })
    }
}
