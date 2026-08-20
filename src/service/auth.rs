use crate::config::config::WahaConfig;
use crate::models::auth::AuthResponse;
use crate::models::users::{
    LoginRequest, RegisterRequest, UpdateProfileRequest, User, UserResponse, UserRole,
};
use crate::proto::auth::PendingUser;
use crate::repository::user::UserRepository;
use crate::utils::error::{AppError, AppResult};
use crate::utils::jwt::JwtService;
use anyhow::{anyhow, bail};
use bcrypt::{hash, verify};
use bytes::BytesMut;
use prost::Message;
use rand::RngExt;
use redis::{AsyncCommands, aio::ConnectionManager};
use reqwest::Client as HttpClient;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;
use validator::Validate;

pub struct AuthService {
    repo: Arc<dyn UserRepository>,
    jwt: JwtService,
    bcrypt_cost: u32,
    waha: Arc<WahaConfig>,
    redis: ConnectionManager,
    http: HttpClient,
}

impl AuthService {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        jwt: JwtService,
        bcrypt_cost: u32,
        waha: Arc<WahaConfig>,
        redis: ConnectionManager,
    ) -> Self {
        // HTTP client di-reuse — connection pool & TLS handshake tidak dibikin
        // ulang setiap kirim OTP.
        let http = HttpClient::builder()
            .pool_idle_timeout(Some(Duration::from_secs(30)))
            .timeout(Duration::from_secs(15))
            .build()
            .expect("build reqwest client");

        Self {
            repo,
            jwt,
            bcrypt_cost,
            waha,
            redis,
            http,
        }
    }

    // ── REGISTER INIT ──────────────────────────────────────────────────────────

    pub async fn initiate_register(&self, req: RegisterRequest) -> AppResult<()> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        let _role = match req.role.as_deref() {
            Some("merchant") => UserRole::Merchant,
            Some("customer") | None => UserRole::Customer,
            Some("admin") => {
                return Err(AppError::Forbidden(
                    "Admin accounts cannot self-register".into(),
                ));
            }
            Some(other) => {
                return Err(AppError::BadRequest(format!("Unknown role '{}'", other)));
            }
        };

        // Duplicate email guard — hanya jika email disertakan
        if let Some(ref email) = req.email {
            if self
                .repo
                .find_by_email_with_password(email)
                .await?
                .is_some()
            {
                return Err(AppError::Conflict("Email already registered".into()));
            }
        }

        if self.repo.find_by_phone(&req.phone).await?.is_some() {
            return Err(AppError::Conflict("phone already registered".into()));
        }

        let redis_key = format!("reg:kinetic:{}", req.phone);
        let mut redis = self.redis.clone();

        if let Ok(Some(_)) = redis.get::<_, Option<Vec<u8>>>(&redis_key).await {
            let ttl: i64 = redis.ttl(&redis_key).await.unwrap_or(0);
            if ttl > 540 {
                return Err(AppError::BadRequest(format!(
                    "OTP sudah dikirim. Tunggu {} detik lagi.",
                    ttl - 540
                )));
            }
        }

        let password = generate_random_password();

        let otp = format!("{:06}", rand::rng().random_range(100_000..=999_999));

        // Proto PendingUser.email masih String — pakai empty string jika None
        let pending = PendingUser {
            name: req.name.clone(),
            phone: req.phone.clone(),
            email: req.email.clone().unwrap_or_default(), // "" berarti tidak ada email
            password: password.to_string(),
            role: req.role.clone().unwrap_or_else(|| "customer".into()),
            otp: otp.clone(),
        };

        let len = pending.encoded_len();
        let mut buf = BytesMut::with_capacity(len);
        pending
            .encode(&mut buf)
            .map_err(|e| AppError::Internal(anyhow!(e)))?;

        let _: () = redis
            .set_ex(&redis_key, buf.freeze().as_ref(), 600u64)
            .await
            .map_err(|e| AppError::Internal(anyhow!(e)))?;

        self.send_wa_otp(&req.phone, &otp, &password)
            .await
            .map_err(|e| AppError::Internal(anyhow!(e)))?;

        info!(phone = %req.phone, "OTP stored & sent");
        Ok(())
    }

    // ── VERIFY REGISTER ────────────────────────────────────────────────────────

    pub async fn verify_register(&self, phone: &str, otp_input: &str) -> AppResult<AuthResponse> {
        let redis_key = format!("reg:kinetic:{}", phone);
        let mut redis = self.redis.clone();

        let bytes: Option<Vec<u8>> = redis.get(&redis_key).await.map_err(|e| {
            tracing::error!("Redis GET gagal: {e}");
            AppError::Redis(e)
        })?;

        let bytes = bytes.ok_or_else(|| {
            AppError::BadRequest("Sesi registrasi tidak ditemukan atau sudah expired".into())
        })?;

        tracing::debug!("Bytes decoded from Redis, len={}", bytes.len());

        let pending = PendingUser::decode(bytes.as_slice()).map_err(|e| {
            tracing::error!("Proto decode gagal: {e}");
            AppError::Internal(anyhow!(e))
        })?;

        tracing::debug!("PendingUser decoded: phone={}", pending.phone);

        if !constant_time_eq(&pending.otp, otp_input) {
            return Err(AppError::BadRequest("Kode OTP salah".into()));
        }

        redis.del::<_, ()>(&redis_key).await.map_err(|e| {
            tracing::error!("Redis DEL gagal: {e}");
            AppError::Redis(e)
        })?;

        let role = match pending.role.as_str() {
            "merchant" => UserRole::Merchant,
            "admin" => UserRole::Admin,
            _ => UserRole::Customer,
        };

        tracing::debug!("Hashing password...");
        // bcrypt::hash itu CPU-bound & blocking → wajib spawn_blocking biar
        // tidak nge-stuck tokio worker thread.
        let password = pending.password.clone();
        let cost = self.bcrypt_cost;
        let hashed = tokio::task::spawn_blocking(move || hash(&password, cost))
            .await
            .map_err(|e| {
                tracing::error!("spawn_blocking join gagal: {e}");
                AppError::Internal(anyhow!(e))
            })?
            .map_err(|e| {
                tracing::error!("bcrypt gagal: {e}");
                AppError::Internal(anyhow!(e))
            })?;

        let email = if pending.email.is_empty() {
            None
        } else {
            Some(pending.email.clone())
        };

        let req = RegisterRequest {
            name: pending.name,
            email,
            phone: pending.phone,
            role: Some(pending.role),
        };

        tracing::debug!("Inserting user to DB...");
        let user = self
            .repo
            .create(&req, Some(&hashed), role)
            .await
            .map_err(|e| {
                tracing::error!("DB insert gagal: {e}");
                e
            })?;

        tracing::debug!("User created: id={}", user.id);
        self.build_auth_response(user)
    }
    // ── LOGIN ──────────────────────────────────────────────────────────────────

    pub async fn login(&self, req: LoginRequest) -> AppResult<AuthResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        let found = self.repo.find_by_phone_with_password(&req.phone).await?;
        let Some(record) = found else {
            return Err(AppError::Unauthorized("Invalid email or password".into()));
        };

        // Akun phone/OTP tidak punya password — tolak login email
        let hash = record.password_hash.ok_or_else(|| {
            AppError::Unauthorized(
                "Akun ini tidak menggunakan password. Silakan login dengan OTP.".into(),
            )
        })?;

        // Catat cost yang dipakai hash existing — kalau di atas target,
        // kita re-hash di belakang setelah login sukses.
        let stored_cost = parse_bcrypt_cost(&hash);
        let target_cost = self.bcrypt_cost;

        // bcrypt::verify CPU-bound. Pindah ke blocking pool agar request lain
        // (dan login paralel) tidak ikut nyangkut di thread tokio yang sama.
        let password = req.password.clone();
        let hash_for_verify = hash.clone();
        let verify_start = std::time::Instant::now();
        let ok = tokio::task::spawn_blocking(move || verify(&password, &hash_for_verify))
            .await
            .map_err(|e| AppError::Internal(anyhow!(e)))?
            .map_err(|e| AppError::Internal(anyhow!(e)))?;

        let verify_ms = verify_start.elapsed().as_millis();
        tracing::info!(
            stored_cost = ?stored_cost,
            target_cost,
            verify_ms,
            "bcrypt verify done"
        );

        if !ok {
            return Err(AppError::Unauthorized("Invalid email or password".into()));
        }

        self.build_auth_response(record.user)
    }

    // ── ME / PROFILE ───────────────────────────────────────────────────────────

    pub async fn me(&self, user_id: &str) -> AppResult<UserResponse> {
        let user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;
        Ok(user.into())
    }

    pub async fn update_profile(
        &self,
        user_id: &str,
        req: UpdateProfileRequest,
    ) -> AppResult<UserResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
        self.repo
            .update_profile(user_id, req.name.as_deref(), req.phone.as_deref())
            .await?;
        self.me(user_id).await
    }

    // ── INTERNAL ───────────────────────────────────────────────────────────────

    async fn send_wa_otp(&self, phone: &str, otp: &str, password: &str) -> anyhow::Result<()> {
        let normalized = normalize_phone(phone)?;

        let body = json!({
            "chatId": normalized,
            "text": format!(
                "Halo! Selamat datang di Kinetic E-Ticketing 🎉\n\n\
                 Kode OTP kamu: *{}*\n\
                 Password akun kamu: *{}*\n\n\
                 ⚠️ Simpan password ini baik-baik.\n\
                 OTP berlaku 10 menit. Jangan bagikan ke siapapun.",
                otp, password
            ),
            "session": self.waha.session,
        });

        let url = format!("{}/api/sendText", self.waha.base_url);
        let mut req = self.http.post(&url).json(&body);

        if !self.waha.api_key.is_empty() {
            req = req.header("X-Api-Key", &self.waha.api_key);
        }

        let res = req.send().await?;

        if !res.status().is_success() {
            let status = res.status();
            let text = res.text().await.unwrap_or_default();
            bail!("WAHA error {status}: {text}");
        }

        info!("OTP sent to {}", normalized);
        Ok(())
    }

    fn build_auth_response(&self, user: User) -> AppResult<AuthResponse> {
        let token = self
            .jwt
            .sign(&user.id, &user.name, &user.phone, &user.role.to_string())?;
        Ok(AuthResponse {
            access_token: token,
            token_type: "Bearer".into(),
            // Diambil dari JwtService, bukan dihitung ulang di sini: angka yang
            // dijanjikan ke klien harus berasal dari sumber yang sama dengan
            // yang dipasang ke klaim `exp`.
            expires_in: self.jwt.expires_in_secs(),
            user: user.into(),
        })
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn normalize_phone(phone: &str) -> anyhow::Result<String> {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();

    let normalized = if digits.starts_with("08") {
        format!("62{}", &digits[1..])
    } else if digits.starts_with("62") {
        digits
    } else {
        return Err(anyhow::anyhow!(
            "Nomor HP harus diawali dengan '08' atau '+62'"
        ));
    };

    Ok(format!("{}@c.us", normalized))
}

/// Ekstrak cost factor dari hash bcrypt — format `$2y$NN$...` / `$2b$NN$...`.
/// Return None kalau format tak terkenali.
fn parse_bcrypt_cost(hash: &str) -> Option<u32> {
    // Hash bcrypt: $2x$cost$...
    let mut parts = hash.splitn(4, '$');
    let _empty = parts.next()?; // ""
    let _ver = parts.next()?; // "2y" / "2b" / "2a"
    let cost_str = parts.next()?;
    cost_str.parse::<u32>().ok()
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

fn generate_random_password() -> String {
    use rand::RngExt;

    const UPPER: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ"; // tanpa I, O
    const LOWER: &[u8] = b"abcdefghjkmnpqrstuvwxyz"; // tanpa i, l, o
    const DIGITS: &[u8] = b"23456789"; // tanpa 0, 1
    const SPECIAL: &[u8] = b"@#$%&";

    let mut rng = rand::rng();

    // Pola: Upper Lower Lower Lower Digit Special Lower Lower Lower
    // Contoh: "Bqrt5@xmk" — 9 karakter, mudah dibaca via WA
    let mut pass = Vec::with_capacity(9);
    pass.push(UPPER[rng.random_range(0..UPPER.len())] as char);
    for _ in 0..3 {
        pass.push(LOWER[rng.random_range(0..LOWER.len())] as char);
    }
    pass.push(DIGITS[rng.random_range(0..DIGITS.len())] as char);
    pass.push(SPECIAL[rng.random_range(0..SPECIAL.len())] as char);
    for _ in 0..3 {
        pass.push(LOWER[rng.random_range(0..LOWER.len())] as char);
    }

    pass.into_iter().collect()
}
