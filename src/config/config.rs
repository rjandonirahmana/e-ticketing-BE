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
    pub garage: GarageConfig,
}

#[derive(Clone, Debug)]
pub struct WahaConfig {
    pub base_url: String,
    pub session: String,
    pub api_key: String,
}

#[derive(Clone, Debug)]
pub struct GarageConfig {
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    pub public_url: String,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        // Garage optional — jika env tidak ada, storage dinonaktifkan
        let garage = GarageConfig {
            endpoint: env::var("GARAGE_ENDPOINT")
                .unwrap_or_else(|_| "http://77.237.242.1:3900".into()),
            access_key: env::var("GARAGE_ACCESS_KEY")
                .unwrap_or_else(|_| "http://77.237.242.1:3900".into()),
            secret_key: env::var("GARAGE_SECRET_KEY")
                .unwrap_or_else(|_| "http://77.237.242.1:3900".into()),
            bucket: env::var("GARAGE_BUCKET").unwrap_or_else(|_| "image".into()),
            public_url: env::var("GARAGE_PUBLIC_URL")
                .unwrap_or_else(|_| "https://ulalaapi.store/image".into()),
        };

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
            bcrypt_cost: env::var("BCRYPT_COST")
                .unwrap_or_else(|_| "10".into())
                .parse()
                .context("BCRYPT_COST must be a number")?,
            redis_url: env::var("REDIS_URL").unwrap_or_else(|_| "".into()),
            waha: WahaConfig {
                base_url: env::var("WAHA_BASE_URL")
                    .unwrap_or_else(|_| "http://localhost:3000".into()),
                session: env::var("WAHA_SESSION").unwrap_or_else(|_| "default".into()),
                api_key: env::var("WAHA_API_KEY").unwrap_or_default(),
            },
            garage,
        })
    }
}
