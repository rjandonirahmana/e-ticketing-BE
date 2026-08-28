//! rate_limit.rs — Pembatas laju jendela-tetap di atas Redis.
//!
//! ── KENAPA DIKUNCI PADA IDENTITAS, BUKAN PADA ALAMAT IP ──────────────────────
//!
//! Pembatas untuk endpoint autentikasi biasanya dikunci pada alamat IP. Di sini
//! tidak, dan itu keputusan yang disengaja.
//!
//! Aplikasi ini berjalan di belakang reverse proxy (Pingora). Soket yang dilihat
//! Axum karena itu SELALU milik proxy — alamat pengguna yang sebenarnya hanya
//! ada sebagai header `X-Forwarded-For`, dan header bisa ditulis siapa saja.
//! Pembatas yang mempercayainya menghitung jatah per nilai yang dikarang
//! penyerang: ia berhenti menghambat tepat pada orang yang perlu dihambat,
//! sambil tetap menghambat pengguna biasa. Lebih buruk daripada tidak ada,
//! karena ia tampak ada.
//!
//! Yang dipakai di sini adalah benda yang sedang diserang itu sendiri — nomor
//! telepon pada login, nomor telepon pada verifikasi OTP. Penyerang tak bisa
//! memalsukannya, sebab memalsukannya berarti menyerang akun yang lain.
//!
//! Pembatasan per-IP tetap berguna untuk menangkap penyemprotan lintas-akun,
//! tetapi tempatnya di Pingora, satu-satunya lapisan yang tahu alamat asli dan
//! tak bisa dibohongi soal itu.
//!
//! ── JENDELA TETAP, BUKAN TOKEN BUCKET ────────────────────────────────────────
//!
//! `INCR` + `EXPIRE` bersifat atomik di Redis dan benar lintas replika tanpa
//! koordinasi apa pun. Jendela tetap memang mengizinkan sedikit lebih banyak
//! percobaan di perbatasan dua jendela; untuk menahan tebakan kata sandi,
//! selisih itu tidak berarti apa-apa, sedangkan kesederhanaannya berarti banyak.
//!
//! Bandingkan dengan `RateLimitRegistry` di `ws/manager.rs`: yang itu in-memory
//! dan per-proses, cocok untuk pesan chat yang volumenya tinggi dan taruhannya
//! rendah. Yang ini menyeberangi replika, karena penyerang yang ditolak satu
//! replika tak boleh cukup dengan mencoba replika berikutnya.

use redis::{aio::ConnectionManager, AsyncCommands};

use crate::utils::error::AppError;

/// Hasil satu pemeriksaan: berapa kali kunci ini sudah dipakai dalam jendela.
pub struct Hit {
    pub count: i64,
    pub max: i64,
}

impl Hit {
    pub fn melebihi(&self) -> bool {
        self.count > self.max
    }
}

/// Naikkan penghitung `key` dan laporkan posisinya terhadap `max`.
///
/// Redis yang bermasalah TIDAK menolak permintaan. Pembatas ini menjaga
/// endpoint login, dan login yang mati total karena cache-nya sedang sakit
/// adalah kerusakan yang lebih besar daripada yang dicegahnya. Kegagalannya
/// dicatat supaya tak lolos tanpa jejak.
pub async fn hit(
    redis: &mut ConnectionManager,
    key: &str,
    max: i64,
    window_secs: i64,
) -> Hit {
    match redis.incr::<_, _, i64>(key, 1i64).await {
        Ok(count) => {
            // EXPIRE hanya pada kenaikan PERTAMA. Memasangnya setiap kali akan
            // menggeser batas akhir jendela terus-menerus, sehingga penyerang
            // yang mengetuk tanpa henti tak pernah sampai ke akhir jendelanya —
            // penghitungnya berlaku selamanya dan pengguna sahnya ikut terkunci
            // permanen.
            if count == 1 {
                let _: Result<(), _> = redis.expire(key, window_secs).await;
            }
            Hit { count, max }
        }
        Err(e) => {
            tracing::warn!(error = %e, key, "rate limit: Redis gagal — permintaan diloloskan");
            Hit { count: 0, max }
        }
    }
}

/// Bentuk siap-pakai: naikkan, dan kembalikan `Err(TooManyRequests)` bila lewat.
pub async fn jaga(
    redis: &mut ConnectionManager,
    key: &str,
    max: i64,
    window_secs: i64,
    pesan: &str,
) -> Result<(), AppError> {
    if hit(redis, key, max, window_secs).await.melebihi() {
        tracing::warn!(key, max, "rate limit terlampaui");
        return Err(AppError::TooManyRequests(pesan.to_string()));
    }
    Ok(())
}
