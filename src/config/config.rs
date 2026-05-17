use anyhow::Context;
use std::env;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub db_pool_max_size: usize,
    pub jwt_secret: String,
    /// Secret bersama dengan FE Leptos untuk validasi X-App-Token.
    /// Set di .env: INTERNAL_JWT_SECRET=<random-string-minimal-32-char>
    pub internal_jwt_secret: String,
    pub jwt_expiry_hours: i64,
    pub bcrypt_cost: u32,
    pub redis_url: String,
    pub waha: WahaConfig,
    pub rustfs: RustFsConfig,
    pub telegram: TelegramConfig,
}

#[derive(Clone, Debug)]
pub struct WahaConfig {
    pub base_url: String,
    pub session: String,
    pub api_key: String,
}

#[derive(Clone, Debug)]
pub struct RustFsConfig {
    pub endpoint: String,
    pub access_key: String,
    pub secret_key: String,
    pub bucket: String,
    pub public_url: String,
}

#[derive(Clone, Debug)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub admin_chat_id: i64,
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let rustfs = RustFsConfig {
            endpoint: env::var("RUSTFS_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:9000".into()),
            access_key: env::var("RUSTFS_ACCESS_KEY").context("RUSTFS_ACCESS_KEY is required")?,
            secret_key: env::var("RUSTFS_SECRET_KEY").context("RUSTFS_SECRET_KEY is required")?,
            bucket: "ticketing".into(),
            public_url: env::var("RUSTFS_PUBLIC_URL")
                .unwrap_or_else(|_| "https://image.ulalaapi.store".into()),
        };

        // INTERNAL_JWT_SECRET boleh tidak di-set di development (fallback ke default).
        // DI PRODUCTION wajib di-set ke nilai acak yang panjang!
        let internal_jwt_secret = env::var("INTERNAL_JWT_SECRET")
            .unwrap_or_else(|_| {
                tracing::warn!(
                    "INTERNAL_JWT_SECRET tidak di-set — pakai default dev (TIDAK AMAN DI PROD!)"
                );
                "kinetic-internal-dev-secret-changeme-in-production".into()
            });

        Ok(Self {
            host: "0.0.0.0".into(),
            port: 8080,
            database_url: env::var("DATABASE_URL").context("DATABASE_URL is required")?,
            db_pool_max_size: env::var("DB_POOL_MAX_SIZE")
                .unwrap_or_else(|_| "16".into())
                .parse()
                .context("DB_POOL_MAX_SIZE must be a number")?,
            jwt_secret: env::var("JWT_SECRET").context("JWT_SECRET is required")?,
            internal_jwt_secret,
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
            rustfs,
            telegram: TelegramConfig {
                bot_token: env::var("TELEGRAM_BOT_TOKEN").unwrap_or_default(),
                admin_chat_id: env::var("TELEGRAM_ADMIN_CHAT_ID")
                    .unwrap_or_else(|_| "0".into())
                    .parse()
                    .context("TELEGRAM_ADMIN_CHAT_ID must be a number")?,
            },
        })
    }
}
