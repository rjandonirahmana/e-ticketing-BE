use anyhow::Context;
use std::env;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub db_pool_max_size: usize,
    pub jwt_secret: String,
    pub internal_jwt_secret: String,
    pub jwt_expiry_hours: i64,
    pub bcrypt_cost: u32,
    pub redis_url: String,
    pub waha: WahaConfig,
    pub rustfs: RustFsConfig,
    pub telegram: TelegramConfig,
    pub sfu_bind_addr: String,
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

        Ok(Self {
            host: "0.0.0.0".into(),
            port: 3000,
            database_url: env::var("DATABASE_URL").context("DATABASE_URL is required")?,
            // Throughput DB-bound ≈ pool_size / latensi_query. Default dinaikkan
            // 16 → 24 untuk box ~2 vCPU. JANGAN set membabi buta: total koneksi =
            // db_pool_max_size × jumlah instance app, harus < Postgres
            // `max_connections` (default 100) dengan sisa untuk admin/migrasi.
            // Lebih banyak koneksi hanya membantu bila Postgres punya headroom
            // CPU/IO; kalau tidak, malah memperburuk (context-switch).
            db_pool_max_size: env::var("DB_POOL_MAX_SIZE")
                .unwrap_or_else(|_| "24".into())
                .parse()
                .context("DB_POOL_MAX_SIZE must be a number")?,
            jwt_secret: env::var("JWT_SECRET").context("JWT_SECRET is required")?,
            // Secret terpisah untuk token internal (service-to-service). Jika tidak
            // di-set, jatuh ke JWT_SECRET agar tetap berjalan di lingkungan dev.
            internal_jwt_secret: env::var("INTERNAL_JWT_SECRET")
                .or_else(|_| env::var("JWT_SECRET"))
                .context("INTERNAL_JWT_SECRET or JWT_SECRET is required")?,
            jwt_expiry_hours: env::var("JWT_EXPIRY_HOURS")
                .unwrap_or_else(|_| "24".into())
                .parse()
                .context("JWT_EXPIRY_HOURS must be a number")?,
            bcrypt_cost: env::var("BCRYPT_COST")
                .unwrap_or_else(|_| "10".into())
                .parse()
                .context("BCRYPT_COST must be a number")?,
            redis_url: env::var("REDIS_URL").context("REDIS_URL is required")?,
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
            sfu_bind_addr: env::var("SFU_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:4000".into()),
        })
    }
}
