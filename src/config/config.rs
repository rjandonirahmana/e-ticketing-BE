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
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            host: env::var("APP_HOST").unwrap_or_else(|_| "0.0.0.0".into()),
            port: env::var("APP_PORT")
                .unwrap_or_else(|_| "8080".into())
                .parse()
                .context("APP_PORT must be a valid port number")?,
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
            bcrypt_cost: env::var("BCRYPT_COST")
                .unwrap_or_else(|_| "12".into())
                .parse()
                .context("BCRYPT_COST must be a number")?,
        })
    }
}
