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

/// Berapa kali kode OTP boleh salah sebelum sesi registrasinya dihanguskan.
///
/// OTP di sini enam digit — satu juta kemungkinan — dan berlaku sepuluh menit.
/// Tanpa penghitung, ruang sebesar itu bukan penghalang: penyerang yang tahu
/// nomor yang sedang mendaftar cukup mengirim tebakan secepat jaringan
/// mengizinkan, dan sepuluh menit adalah waktu yang panjang untuk itu.
///
/// Perbandingan OTP-nya sudah `constant_time_eq`, tetapi itu menutup kebocoran
/// lewat WAKTU — bukan lewat pengulangan. Keduanya masalah yang berbeda dan
/// butuh jawaban yang berbeda.
///
/// Lima dipilih karena salah ketik enam digit itu wajar; lima kali berturut-turut
/// tidak. Setelah habis, sesinya DIBUANG, bukan sekadar ditolak — menyisakannya
/// berarti penyerang cukup menunggu penghitungnya kedaluwarsa lalu melanjutkan
/// dari tebakan terakhir.
const OTP_MAX_ATTEMPTS: i64 = 5;

/// Percobaan login per nomor, per jendela.
///
/// Sepuluh dalam lima menit longgar bagi manusia yang lupa kata sandinya dan
/// mencoba beberapa kali, tetapi menutup penebakan otomatis: kata sandi
/// terlemah sekalipun butuh ribuan tebakan, dan pada laju ini itu berbulan-bulan.
///
/// Jendelanya sengaja pendek dan tidak ada penguncian akun. Mengunci akun akan
/// mengubah pembatas ini menjadi senjata: siapa pun bisa mengunci siapa pun
/// hanya dengan menebak salah berkali-kali atas nama korbannya. Melambatkan
/// selama lima menit menahan penyerang tanpa memberinya kemampuan itu.
const LOGIN_MAX_PER_WINDOW: i64 = 10;

/// Umur sandi hasil "lupa password" selama ia belum dipakai.
///
/// Tiga jam: cukup lama untuk orang yang baru membuka WhatsApp beberapa saat
/// kemudian, cukup pendek supaya sandi yang tak pernah diminta pemiliknya tak
/// menganggur berhari-hari. Selama jendela ini DUA sandi berlaku — lihat
/// `pakai_sandi_menunggu`.
const SANDI_MENUNGGU_TTL: u64 = 3 * 60 * 60;

/// Umur pengajuan ganti nomor — cukup untuk membuka WhatsApp dan menyalin kode,
/// terlalu pendek untuk ditebak.
const GANTI_NOMOR_TTL: u64 = 600;
/// Sesudah ini, ajukan ulang dari awal. OTP hanya 6 digit: tanpa batas, seluruh
/// ruang tebakan habis dalam hitungan menit.
const GANTI_NOMOR_MAKS_COBA: u64 = 5;
/// Jeda minimum antar pengiriman kode.
const GANTI_NOMOR_JEDA: i64 = 60;

/// Perbandingan string dengan waktu tetap.
///
/// `==` biasa berhenti pada perbedaan pertama, jadi waktunya membocorkan berapa
/// digit awal yang sudah benar. Untuk OTP 6 digit, kebocoran itu memangkas
/// ruang tebakan dari sejuta menjadi enam puluh.
fn waktu_tetap_sama(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
const LOGIN_WINDOW_SECS: i64 = 300;

/// Verifikasi OTP per nomor, per jendela — lapis KEDUA di atas
/// `OTP_MAX_ATTEMPTS`.
///
/// Keduanya dibutuhkan karena menjaga hal yang berbeda. `OTP_MAX_ATTEMPTS`
/// terikat pada satu sesi registrasi dan hangus bersamanya; tanpa lapis ini,
/// penyerang cukup meminta kode BARU setiap lima tebakan dan melanjutkan
/// dengan penghitung yang bersih. Pembatas per-jendela ini tidak ikut
/// direset oleh sesi baru, jadi ia membatasi laju penebakan secara keseluruhan.
const OTP_VERIFY_MAX_PER_WINDOW: i64 = 15;
const OTP_VERIFY_WINDOW_SECS: i64 = 600;

/// Permintaan registrasi per nomor, per jam.
///
/// Yang dijaga di sini bukan hanya basis data. Tiap panggilan yang lolos
/// mengirim satu pesan WhatsApp lewat WAHA — biaya nyata per permintaan, dan
/// satu-satunya endpoint di aplikasi ini yang membelanjakan uang atas nama
/// orang yang belum punya akun.
const REGISTER_MAX_PER_HOUR: i64 = 5;
const REGISTER_WINDOW_SECS: i64 = 3600;

pub struct AuthService {
    repo: Arc<dyn UserRepository>,
    jwt: JwtService,
    bcrypt_cost: u32,
    waha: Arc<WahaConfig>,
    redis: ConnectionManager,
    http: HttpClient,
    /// Hash umpan untuk login dengan nomor yang tidak terdaftar.
    ///
    /// Tanpa ini, kedua kemungkinan menjawab dengan kalimat yang sama tetapi
    /// dalam waktu yang sangat berbeda: nomor tak terdaftar kembali seketika,
    /// nomor terdaftar membayar `bcrypt::verify` lebih dulu — ratusan
    /// milidetik, dan log servernya sendiri mencatat `verify_ms`, jadi
    /// selisihnya memang sebesar itu dan memang terukur dari luar.
    ///
    /// Selisih itu menjawab pertanyaan yang justru disembunyikan oleh pesan
    /// errornya: apakah nomor ini punya akun. Siapa pun bisa menanyakannya
    /// untuk sebarang nomor, tanpa kredensial apa pun.
    ///
    /// Dihitung SEKALI saat pertama dibutuhkan, bukan saat start.
    ///
    /// Versi pertama menghitungnya di `AuthService::new()`, dan itu menahan
    /// seluruh proses selama hash itu dibuat. Pada `BCRYPT_COST` yang wajar
    /// (10-12) jedanya ratusan milidetik dan tak terasa. Tetapi biaya bcrypt
    /// BERLIPAT DUA tiap kenaikan satu cost: pada 17 ia menjadi 2^7 = 128 kali
    /// lebih lambat, yakni belasan detik — dijalankan sinkron, di dalam
    /// konstruktor, sebelum server sempat mengikat port.
    ///
    /// Dari luar itu tidak terlihat seperti `boot lambat`. Docker sudah membuka
    /// port host, jadi proxy berhasil menyambung lalu menunggu header yang tak
    /// kunjung datang — `ReadTimedout while reading response headers`, circuit
    /// breaker terbuka, dan halaman putih. Persis seperti aplikasi yang hang.
    ///
    /// `OnceLock` memindahkan ongkos itu ke percobaan login pertama dengan
    /// nomor tak terdaftar, di dalam `spawn_blocking`, dan hanya sekali seumur
    /// proses. Sifat yang dibutuhkan tetap utuh: jalur `nomor tak ada` dan
    /// jalur `nomor ada` sama-sama membayar satu verifikasi bcrypt.
    dummy_hash: std::sync::OnceLock<String>,
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
            dummy_hash: std::sync::OnceLock::new(),
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

        // Penjaga di bawah ("OTP sudah dikirim, tunggu N detik") hanya menahan
        // pengiriman ULANG selagi sesi sebelumnya masih hidup — ia berhenti
        // menahan begitu sesinya kedaluwarsa atau terpakai. Pembatas per-jam ini
        // yang membatasi jumlah pesan WhatsApp yang bisa dipicu satu nomor dalam
        // sehari, dan biayanya nyata.
        crate::utils::rate_limit::jaga(
            &mut redis,
            &format!("rl:register:{}", req.phone),
            REGISTER_MAX_PER_HOUR,
            REGISTER_WINDOW_SECS,
            "Terlalu banyak permintaan kode. Coba lagi satu jam lagi.",
        )
        .await?;

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

        // Lapis kedua di atas `OTP_MAX_ATTEMPTS` — lihat catatan di konstantanya.
        // Yang ini tidak ikut hangus bersama sesi, jadi meminta kode baru tak
        // memulihkan jatah penebakan.
        crate::utils::rate_limit::jaga(
            &mut redis,
            &format!("rl:otp:{phone}"),
            OTP_VERIFY_MAX_PER_WINDOW,
            OTP_VERIFY_WINDOW_SECS,
            "Terlalu banyak percobaan kode. Coba lagi beberapa menit lagi.",
        )
        .await?;

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
            // Penghitung hidup sependek sesi registrasinya sendiri. Ia sengaja
            // TIDAK diberi TTL yang lebih panjang: begitu sesinya kedaluwarsa
            // tak ada lagi yang bisa ditebak, dan penghitung yang tertinggal
            // hanya akan menghukum orang berikutnya yang memakai nomor itu.
            let attempt_key = format!("reg:kinetic:attempt:{phone}");
            let attempts: i64 = redis.incr(&attempt_key, 1i64).await.unwrap_or(1);
            if attempts == 1 {
                // Samakan umurnya dengan sesi registrasi (600 detik).
                let _: Result<(), _> = redis.expire(&attempt_key, 600).await;
            }

            if attempts >= OTP_MAX_ATTEMPTS {
                // Hanguskan sesinya, bukan sekadar tolak tebakan ini.
                let _: Result<(), _> = redis.del(&redis_key).await;
                let _: Result<(), _> = redis.del(&attempt_key).await;
                tracing::warn!(
                    phone = %phone,
                    attempts,
                    "OTP salah berulang — sesi registrasi dihanguskan"
                );
                return Err(AppError::BadRequest(
                    "Terlalu banyak percobaan. Minta kode baru.".into(),
                ));
            }

            return Err(AppError::BadRequest("Kode OTP salah".into()));
        }

        redis.del::<_, ()>(&redis_key).await.map_err(|e| {
            tracing::error!("Redis DEL gagal: {e}");
            AppError::Redis(e)
        })?;

        // Kode yang benar mengakhiri sesinya, jadi penghitungnya tak punya lagi
        // yang perlu dijaga. Dibuang di sini supaya percobaan yang gagal
        // sebelumnya tidak ikut terbawa ke registrasi berikutnya dari nomor
        // yang sama.
        let _: Result<(), _> = redis.del(format!("reg:kinetic:attempt:{phone}")).await;

        // `admin` SENGAJA tidak ada di sini.
        //
        // `initiate_register` sudah menolaknya, jadi sesi tertunda semestinya
        // tak pernah memuat peran itu. Tetapi yang dibaca di sini datang dari
        // Redis, bukan dari permintaan yang barusan divalidasi — dan sesuatu
        // yang datang dari penyimpanan luar tidak boleh dipercaya untuk
        // menerbitkan hak akses tertinggi di sistem ini.
        //
        // Dulu baris `admin => UserRole::Admin` ada di sini. Ia hanya bisa
        // tercapai bila isi Redis dikarang, tetapi bila itu terjadi, hasilnya
        // adalah akun admin yang lahir lewat pendaftaran biasa. Sekarang
        // apa pun yang bukan `merchant` menjadi `customer`, jadi jalur ini
        // secara struktur tidak bisa lagi mencetak admin.
        //
        // Satu-satunya admin dibuat langsung di database, dan sejak migrasi
        // 032 hanya boleh ada satu.
        let role = match pending.role.as_str() {
            "merchant" => UserRole::Merchant,
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
    // ── GANTI NOMOR HP ─────────────────────────────────────────────────────────
    //
    // ── KENAPA HARUS ADA OTP ───────────────────────────────────────────────
    // Nomor HP di sini bukan sekadar data kontak: ia IDENTITAS LOGIN
    // (`find_by_phone`), penerima OTP, dan penerima reset sandi. Membiarkan
    // seseorang menulis nomor apa pun tanpa bukti kepemilikan berarti ia bisa
    // memindahkan akunnya ke nomor orang lain — atau, lebih sering, salah ketik
    // satu digit lalu terkunci selamanya dari akunnya sendiri, karena sandi
    // pemulihan dikirim ke nomor yang tak pernah ia pegang.
    //
    // Kodenya dikirim ke NOMOR BARU, bukan nomor lama. Yang perlu dibuktikan
    // adalah "saya memegang nomor ini"; bahwa ia pemilik akun sudah dibuktikan
    // oleh sesi yang sedang berjalan.

    fn ganti_nomor_key(user_id: &str) -> String {
        format!("hp:ganti:{user_id}")
    }

    /// Ajukan penggantian nomor: kirim OTP ke NOMOR BARU.
    pub async fn mulai_ganti_nomor(&self, user_id: &str, nomor_baru: &str) -> AppResult<String> {
        let nomor_baru = nomor_baru.trim();
        if nomor_baru.is_empty() {
            return Err(AppError::UnprocessableEntity("Nomor HP baru wajib diisi.".into()));
        }
        let ternormalisasi = normalize_phone(nomor_baru)
            .map_err(|_| AppError::UnprocessableEntity("Nomor HP tidak sah.".into()))?;

        let saya = self.me(user_id).await?;
        if normalize_phone(&saya.phone).ok().as_deref() == Some(ternormalisasi.as_str()) {
            return Err(AppError::UnprocessableEntity(
                "Nomor itu sama dengan nomor Anda sekarang.".into(),
            ));
        }

        // Pemeriksaan PERTAMA. Ada pemeriksaan kedua saat verifikasi, dan itu
        // yang menentukan — di antara keduanya ada jendela 10 menit, dan siapa
        // pun bisa mendaftar dengan nomor itu di sela-selanya.
        if let Some(lain) = self.repo.find_by_phone(nomor_baru).await? {
            if lain.id != user_id {
                return Err(AppError::UnprocessableEntity(
                    "Nomor itu sudah dipakai akun lain.".into(),
                ));
            }
        }

        let mut redis = self.redis.clone();
        crate::utils::rate_limit::jaga(
            &mut redis,
            &format!("rl:hp:{user_id}"),
            1,
            GANTI_NOMOR_JEDA,
            "Kode sudah dikirim kurang dari semenit lalu. Periksa WhatsApp nomor baru Anda.",
        )
        .await?;

        let otp = format!("{:06}", rand::rng().random_range(100_000..=999_999));
        let pengajuan = json!({
            "nomor": ternormalisasi,
            "otp": otp,
            "percobaan": 0,
        })
        .to_string();

        // KIRIM DULU, baru simpan — pengajuan untuk pesan yang tak pernah
        // terkirim hanya menahan orang selama 10 menit tanpa guna.
        self.send_wa_ganti_nomor(&ternormalisasi, &otp)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "ganti nomor: gagal kirim WA");
                AppError::Internal(anyhow!("Gagal mengirim WhatsApp ke nomor baru. Pastikan nomornya benar."))
            })?;

        redis
            .set_ex::<_, _, ()>(&Self::ganti_nomor_key(user_id), &pengajuan, GANTI_NOMOR_TTL)
            .await
            .map_err(|e| AppError::Internal(anyhow!(e)))?;

        Ok(format!("Kode verifikasi dikirim ke WhatsApp {ternormalisasi}."))
    }

    /// Verifikasi OTP; bila cocok, nomor akun BENAR-BENAR berpindah.
    pub async fn verifikasi_ganti_nomor(&self, user_id: &str, otp_input: &str) -> AppResult<String> {
        let mut redis = self.redis.clone();
        let key = Self::ganti_nomor_key(user_id);

        let json_str: Option<String> = redis.get(&key).await.unwrap_or(None);
        let Some(json_str) = json_str else {
            return Err(AppError::UnprocessableEntity(
                "Pengajuan ganti nomor sudah kedaluwarsa. Ulangi dari awal.".into(),
            ));
        };
        let Ok(mut v) = serde_json::from_str::<serde_json::Value>(&json_str) else {
            return Err(AppError::UnprocessableEntity(
                "Pengajuan tidak terbaca. Ulangi dari awal.".into(),
            ));
        };

        let otp_benar = v["otp"].as_str().unwrap_or_default().to_string();
        let nomor_baru = v["nomor"].as_str().unwrap_or_default().to_string();
        let mut percobaan = v["percobaan"].as_u64().unwrap_or(0);

        // Perbandingan waktu-tetap: OTP hanya 6 digit, dan `==` biasa bocor
        // berapa digit awal yang sudah benar lewat waktu yang dipakainya.
        if !waktu_tetap_sama(&otp_benar, otp_input.trim()) {
            percobaan += 1;
            if percobaan >= GANTI_NOMOR_MAKS_COBA {
                let _: Result<(), _> = redis.del::<_, ()>(&key).await;
                return Err(AppError::UnprocessableEntity(
                    "Terlalu banyak percobaan. Ajukan ganti nomor lagi dari awal.".into(),
                ));
            }
            // TTL DIPERTAHANKAN: percobaan gagal tak boleh memperpanjang umur
            // pengajuan, kalau tidak seseorang bisa menahannya hidup selamanya.
            let sisa: i64 = redis.ttl(&key).await.unwrap_or(0);
            v["percobaan"] = serde_json::json!(percobaan);
            let _: Result<(), _> = redis
                .set_ex::<_, _, ()>(&key, v.to_string(), sisa.max(1) as u64)
                .await;
            return Err(AppError::UnprocessableEntity(format!(
                "Kode salah. Sisa {} percobaan.",
                GANTI_NOMOR_MAKS_COBA - percobaan
            )));
        }

        // Pemeriksaan KEDUA, dan yang ini yang menentukan.
        if let Some(lain) = self.repo.find_by_phone(&nomor_baru).await? {
            if lain.id != user_id {
                let _: Result<(), _> = redis.del::<_, ()>(&key).await;
                return Err(AppError::UnprocessableEntity(
                    "Nomor itu keburu dipakai akun lain. Coba nomor lain.".into(),
                ));
            }
        }

        self.repo
            .update_profile(user_id, None, Some(&nomor_baru), None)
            .await?;
        let _: Result<(), _> = redis.del::<_, ()>(&key).await;
        tracing::info!(user_id, "nomor HP berhasil diganti");
        Ok(format!("Nomor HP berhasil diganti ke {nomor_baru}."))
    }

    async fn send_wa_ganti_nomor(&self, phone: &str, otp: &str) -> anyhow::Result<()> {
        let body = json!({
            "chatId": phone,
            "text": format!(
                "🔄 *Ganti Nomor Kinetic*\n\nKode verifikasi: *{otp}*\n\n\
                 Berlaku 10 menit. Masukkan kode ini di halaman Edit Profil untuk \
                 memindahkan akun Anda ke nomor ini.\n\n\
                 Bila Anda tidak meminta ini, abaikan saja — nomor lama tetap berlaku."
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
        Ok(())
    }

    // ── LUPA SANDI ─────────────────────────────────────────────────────────────

    /// Kunci Redis tempat sandi-menunggu milik satu akun disimpan.
    fn sandi_menunggu_key(user_id: &str) -> String {
        format!("pw:menunggu:{user_id}")
    }

    /// Cocokkan `password` dengan sandi-menunggu milik `user_id`. Bila cocok:
    /// TULIS ke `users.password_hash`, buang kuncinya, lalu `true`.
    ///
    /// ── KENAPA SANDI RESET TIDAK LANGSUNG DITULIS KE DB ─────────────────────
    /// Menekan "lupa password" hanya butuh mengetik nomor HP ORANG LAIN. Bila
    /// reset langsung menimpa DB, siapa pun bisa mengunci siapa pun keluar dari
    /// akunnya: korban tak melakukan apa-apa, sandinya mendadak tak berlaku, dan
    /// sandi penggantinya ada di WhatsApp yang mungkin tak pernah ia buka — atau
    /// tak pernah sampai.
    ///
    /// Karena itu sandi baru MENUNGGU: selama tiga jam DUA sandi sama-sama
    /// berlaku, yang lama dan yang baru. Yang menentukan bukan permintaan
    /// resetnya, melainkan sandi mana yang benar-benar DIPAKAI masuk. Tak
    /// dipakai sama sekali → kuncinya kedaluwarsa sendiri dan sandi di DB tak
    /// pernah tersentuh.
    ///
    /// Biayanya: satu bcrypt tambahan pada login yang gagal, dan hanya bila
    /// memang ada sandi-menunggu (GET dulu, verify belakangan).
    async fn pakai_sandi_menunggu(&self, user_id: &str, password: &str) -> bool {
        let mut redis = self.redis.clone();
        let key = Self::sandi_menunggu_key(user_id);

        // Redis mati → tak ada sandi-menunggu yang bisa dibaca; login berjalan
        // seperti biasa dengan sandi DB. Jangan menggagalkan login karenanya.
        let hash: Option<String> = match redis.get(&key).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(user_id, error = %e, "gagal membaca sandi-menunggu");
                return false;
            }
        };
        let Some(hash) = hash else { return false };

        let pw = password.to_string();
        let h = hash.clone();
        let cocok = tokio::task::spawn_blocking(move || verify(&pw, &h))
            .await
            .map(|r| r.unwrap_or(false))
            .unwrap_or(false);
        if !cocok {
            return false;
        }

        // DITULIS DULU, baru kuncinya dibuang. Urutan sebaliknya membuat sandi
        // baru lenyap bila penulisan DB gagal — dan pemiliknya tinggal dengan
        // sandi lama yang barusan ia yakini sudah diganti.
        if let Err(e) = self.repo.update_password_hash(user_id, &hash).await {
            tracing::error!(user_id, error = %e, "gagal menulis sandi baru ke DB");
            return false;
        }
        let _: Result<(), _> = redis.del::<_, ()>(&key).await;
        tracing::info!(user_id, "sandi dari lupa-password dipakai — DB diperbarui");
        true
    }

    /// Lupa password lewat WhatsApp.
    ///
    /// Cari akun dari nomor HP → buat sandi baru → kirim lewat WA → simpan
    /// HASH-nya di Redis selama tiga jam. Mengembalikan kalimat yang bisa
    /// langsung ditampilkan.
    ///
    /// **Basis data TIDAK disentuh di sini.** Sandi lama tetap berlaku sampai
    /// seseorang benar-benar masuk memakai sandi barunya — lihat
    /// `pakai_sandi_menunggu`.
    pub async fn forgot_password(&self, phone: &str) -> AppResult<String> {
        let phone = phone.trim();
        if phone.is_empty() {
            return Err(AppError::UnprocessableEntity(
                "Nomor HP wajib diisi.".into(),
            ));
        }

        // Jawaban yang SAMA untuk nomor terdaftar dan tidak, supaya halaman ini
        // tak bisa dipakai memetakan nomor mana yang punya akun.
        const JAWABAN: &str = "Jika nomor tersebut terdaftar, password baru sudah \
                               dikirim ke WhatsApp-nya. Password lama masih bisa \
                               dipakai sampai Anda masuk memakai yang baru.";

        let found = self.repo.find_by_phone(phone).await?;
        let Some(user) = found else {
            tracing::info!("forgot_password: tak ada akun dengan nomor tersebut");
            return Ok(JAWABAN.into());
        };

        // BATAS LAJU per nomor. Tanpa ini siapa saja bisa membanjiri WA korban
        // dengan sandi baru berulang kali.
        let mut redis = self.redis.clone();
        crate::utils::rate_limit::jaga(
            &mut redis,
            &format!("rl:fp:{phone}"),
            1,
            600,
            "Password baru sudah dikirim kurang dari 10 menit lalu. Periksa WhatsApp Anda dulu.",
        )
        .await?;

        let sandi_baru = generate_random_password();
        let pw = sandi_baru.clone();
        let cost = self.bcrypt_cost;
        let hash_baru = tokio::task::spawn_blocking(move || hash(&pw, cost))
            .await
            .map_err(|e| AppError::Internal(anyhow!(e)))?
            .map_err(|e| AppError::Internal(anyhow!(e)))?;

        // KIRIM DULU, baru simpan. Urutan sebaliknya menyisakan sandi-menunggu
        // untuk pesan yang tak pernah terkirim — tak merusak apa pun (sandi lama
        // tetap jalan), tapi menaruh sandi yang tak diketahui siapa pun di Redis
        // selama tiga jam tak ada gunanya.
        if let Err(e) = self.send_wa_reset(&user.phone, &sandi_baru).await {
            tracing::error!(error = %e, "forgot_password: gagal mengirim WA");
            return Err(AppError::Internal(anyhow!(
                "Gagal mengirim WhatsApp. Coba lagi sebentar lagi."
            )));
        }

        let key = Self::sandi_menunggu_key(&user.id);
        if let Err(e) = redis
            .set_ex::<_, _, ()>(&key, &hash_baru, SANDI_MENUNGGU_TTL)
            .await
        {
            // WA sudah terkirim tapi hash-nya tak tersimpan: sandi di pesan itu
            // tak akan bisa dipakai. Katakan apa adanya — mendiamkannya membuat
            // orang mencoba sandi yang mustahil berhasil.
            tracing::error!(error = %e, "forgot_password: gagal menyimpan sandi-menunggu");
            return Err(AppError::Internal(anyhow!(
                "Password baru gagal disiapkan. Coba lagi sebentar lagi."
            )));
        }

        Ok(JAWABAN.into())
    }

    // ── LOGIN ──────────────────────────────────────────────────────────────────

    pub async fn login(&self, req: LoginRequest) -> AppResult<AuthResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        // Dikunci pada nomor yang sedang dicoba, bukan pada alamat pemanggil.
        // Alasannya di `utils::rate_limit`: di belakang proxy, alamat itu tak
        // bisa dipercaya, sedangkan nomornya tak bisa dipalsukan tanpa berpindah
        // menyerang akun lain.
        //
        // Diperiksa SEBELUM query pertama, jadi banjir percobaan berhenti tanpa
        // menyentuh basis data sama sekali.
        let mut redis = self.redis.clone();
        crate::utils::rate_limit::jaga(
            &mut redis,
            &format!("rl:login:{}", req.phone),
            LOGIN_MAX_PER_WINDOW,
            LOGIN_WINDOW_SECS,
            "Terlalu banyak percobaan masuk. Coba lagi beberapa menit lagi.",
        )
        .await?;

        let found = self.repo.find_by_phone_with_password(&req.phone).await?;
        let Some(record) = found else {
            // Nomor tak terdaftar tetap membayar satu verifikasi bcrypt.
            //
            // Hasilnya dibuang — yang dibeli di sini bukan jawabannya melainkan
            // WAKTUNYA, supaya kedua jalur tak lagi bisa dibedakan dari luar.
            // Tanpa baris ini, pesan error yang sudah sengaja disamarkan
            // dibatalkan oleh stopwatch.
            let umpan = match self.dummy_hash.get() {
                Some(h) => h.clone(),
                None => {
                    // Pembuatan pertama ikut di blocking pool — bcrypt itu
                    // CPU-bound, dan pada cost tinggi ia bisa memakan belasan
                    // detik. Menjalankannya di thread async akan membekukan
                    // seluruh runtime, bukan cuma permintaan ini.
                    // Biayanya DIBATASI, dan itu justru membuat penyamaran
                    // waktunya lebih benar — bukan sebaliknya.
                    //
                    // Yang harus ditiru adalah ongkos jalur `nomor ADA`, dan
                    // jalur itu memverifikasi hash yang SUDAH TERSIMPAN, dengan
                    // cost yang tertanam di dalam hash itu sendiri — bukan
                    // `BCRYPT_COST` yang sedang dikonfigurasi. Keduanya kerap
                    // berbeda: log produksi memperlihatkan `stored_cost=10`
                    // sementara `target_cost=17`.
                    //
                    // Memakai cost konfigurasi mentah-mentah karena itu bisa
                    // membuat jalur umpan JAUH lebih lambat daripada jalur
                    // asli — kebocoran waktu yang sama, hanya terbalik arahnya,
                    // ditambah satu permintaan yang menggantung belasan detik.
                    // Batas atas 12 menjaganya tetap sekelas verifikasi nyata.
                    let cost = self.bcrypt_cost.clamp(4, 12);
                    let dibuat = tokio::task::spawn_blocking(move || {
                        hash("kata-sandi-umpan-tak-terpakai", cost)
                    })
                    .await
                    .map_err(|e| AppError::Internal(anyhow!(e)))?
                    .map_err(|e| AppError::Internal(anyhow!(e)))?;
                    // Balapan antar-permintaan pertama tak masalah: yang kalah
                    // memakai nilai pemenang, dan keduanya sama sahnya.
                    self.dummy_hash.get_or_init(|| dibuat).clone()
                }
            };
            let password = req.password.clone();
            let _ = tokio::task::spawn_blocking(move || verify(&password, &umpan)).await;
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
            // Sandi DB tak cocok — mungkin ini sandi hasil "lupa password" yang
            // belum pernah dipakai. Di titik INILAH, dan hanya di sini, sandi
            // lama benar-benar berganti.
            if self.pakai_sandi_menunggu(&record.user.id, &req.password).await {
                return self.build_auth_response(record.user);
            }
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
        // NOMOR HP SENGAJA DIABAIKAN DI SINI.
        //
        // Dulu ia ikut ditulis, dan itu berarti nomor — identitas login,
        // penerima OTP, penerima reset sandi — bisa dipindahkan hanya dengan
        // mengetiknya di formulir, tanpa satu pun bukti kepemilikan. Salah
        // ketik satu digit sudah cukup untuk mengunci orang keluar dari akunnya
        // sendiri, karena sandi pemulihannya akan dikirim ke nomor yang tak
        // pernah ia pegang.
        //
        // Penggantian nomor kini punya jalurnya sendiri dengan OTP ke NOMOR
        // BARU: `mulai_ganti_nomor` → `verifikasi_ganti_nomor`.
        if req.phone.is_some() {
            tracing::debug!(user_id, "update_profile: field phone diabaikan (pakai jalur OTP)");
        }
        // Email: string kosong berarti "kosongkan", bukan "jangan sentuh".
        // Keduanya harus bisa dibedakan, kalau tidak orang yang ingin menghapus
        // emailnya tak punya cara menyatakannya.
        let email: Option<Option<&str>> = req.email.as_deref().map(|e| {
            let e = e.trim();
            if e.is_empty() { None } else { Some(e) }
        });
        self.repo
            .update_profile(user_id, req.name.as_deref(), None, email)
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

    async fn send_wa_reset(&self, phone: &str, password: &str) -> anyhow::Result<()> {
        let normalized = normalize_phone(phone)?;

        let body = json!({
            "chatId": normalized,
            // Kalimat terakhir bukan basa-basi: tanpa itu orang mengira sandi
            // lamanya sudah mati dan panik ketika ia ternyata masih bisa masuk.
            "text": format!(
                "🔑 *Reset Password Kinetic*\n\nPassword baru Anda: *{password}*\n\n\
                 Berlaku 3 jam. Password LAMA masih bisa dipakai — yang baru \
                 menggantikannya hanya setelah Anda berhasil masuk dengan password ini.\n\n\
                 Jangan bagikan ke siapa pun."
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
        info!("reset password terkirim ke {}", normalized);
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


