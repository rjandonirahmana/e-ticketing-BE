//! utils/watchdog.rs — detektor macetnya proses.
//!
//! ── KENAPA ADA ─────────────────────────────────────────────────────────────
//! Insiden 3 Sep 2026: pingora melaporkan `Upstream ReadTimedout` beruntun ke
//! 127.0.0.1:3100 selama ±60 detik. Dari sisi proxy, keduanya terlihat sama
//! persis — "app tak menjawab" — padahal penyebabnya bisa dua hal yang
//! penanganannya berlawanan:
//!
//!   a. Worker tokio TERBLOKIR (kode sinkron/CPU-bound menahan worker, atau
//!      query yang menggantung). Obatnya ada di kode aplikasi.
//!   b. PROSESNYA yang tak dijadwalkan sistem operasi (rebutan CPU di box 2
//!      vCPU, swap thrash, memori habis). Obatnya ada di kapasitas mesin.
//!
//! Tanpa pengukuran, keduanya cuma tebakan. Modul ini memisahkannya dengan
//! menjalankan DUA pengawas yang mengukur hal berbeda:
//!
//!   - [`spawn_task`]  — satu task tokio biasa. Ia telat kalau runtime-nya
//!                       tersendat, apa pun sebabnya.
//!   - [`spawn_thread`] — satu OS thread di luar runtime. Ia telat HANYA kalau
//!                       kernel benar-benar tak menjalankan proses ini.
//!
//! Cara membaca hasilnya:
//!
//!   | task telat | thread telat | artinya                                   |
//!   |------------|--------------|-------------------------------------------|
//!   | ya         | tidak        | worker tokio terblokir → bug di kode       |
//!   | ya         | ya           | proses tak dapat CPU → mesin/memori        |
//!   | tidak      | ya           | (mustahil) — pengawas thread yang bermasalah |
//!
//! ── KENAPA `sleep`, BUKAN `interval` ───────────────────────────────────────
//! `tokio::time::interval` MENGEJAR tick yang terlewat: setelah macet 60 detik
//! ia menembakkan puluhan tick sekaligus dan `elapsed` tiap tick tampak normal
//! — persis informasi yang sedang kita cari, hilang. `sleep` selalu mengukur
//! selisih sebenarnya antara "minta tidur 1 detik" dan "benar-benar bangun".

use std::time::{Duration, Instant};

/// Jarak antar-denyut. 1 detik: cukup rapat untuk menangkap sendatan yang
/// terasa pengguna, cukup jarang untuk tak berarti apa-apa bagi CPU.
const DETAK: Duration = Duration::from_secs(1);

/// Batas bawah keterlambatan yang dianggap sendatan, bila `WATCHDOG_LAG_MS`
/// tak di-set.
///
/// Bukan angka kecil dengan sengaja. Runtime yang sehat pun bisa telat puluhan
/// hingga ratusan milidetik saat render SSR berat atau kompresi brotli sedang
/// jalan; menyalakan peringatan di situ hanya melatih orang mengabaikannya.
/// Telat lebih dari dua detik untuk tidur satu detik bukan lagi kesibukan
/// biasa.
const AMBANG_BAWAAN_MS: u64 = 2_000;

fn ambang() -> Duration {
    let ms = std::env::var("WATCHDOG_LAG_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(AMBANG_BAWAAN_MS);
    Duration::from_millis(ms)
}

/// Potret murah keadaan mesin, dibaca HANYA di tepi sendatan (saat mulai dan
/// saat pulih) — bukan tiap denyut. Tiga pembacaan `/proc` yang masing-masing
/// beberapa ratus byte; di jalur yang sudah bermasalah pun ia tak menambah
/// beban berarti, dan tanpanya lognya cuma bilang "telat" tanpa menyebut
/// mesinnya sedang kenapa.
fn potret() -> (u64, f32, f32, u64, u64) {
    let rss_mb = crate::service::server_status::app_rss() / 1024;
    let (l1, l5, _) = crate::service::server_status::loadavg();
    let (swap_total_kb, swap_pakai_kb) = crate::service::server_status::swap();
    (rss_mb, l1, l5, swap_pakai_kb / 1024, swap_total_kb / 1024)
}

/// Pengawas RUNTIME: sebuah task tokio biasa yang mengukur keterlambatannya
/// sendiri. Kalau ia telat, berarti tak ada worker yang sempat menjalankannya.
pub fn spawn_task() {
    let ambang = ambang();
    tokio::spawn(async move {
        let mut macet_sejak: Option<Instant> = None;
        let mut lag_puncak = Duration::ZERO;

        loop {
            let mulai = Instant::now();
            tokio::time::sleep(DETAK).await;
            let lag = mulai.elapsed().saturating_sub(DETAK);

            if lag > ambang {
                lag_puncak = lag_puncak.max(lag);
                if macet_sejak.is_none() {
                    macet_sejak = Some(mulai);
                    let (rss_mb, load1, load5, swap_mb, swap_total_mb) = potret();
                    tracing::warn!(
                        lag_ms = lag.as_millis() as u64,
                        rss_mb,
                        load1,
                        load5,
                        swap_mb,
                        swap_total_mb,
                        "RUNTIME TERSENDAT — task tokio tak dijadwalkan tepat waktu"
                    );
                }
            } else if let Some(sejak) = macet_sejak.take() {
                let (rss_mb, load1, load5, swap_mb, swap_total_mb) = potret();
                tracing::warn!(
                    durasi_s = sejak.elapsed().as_secs(),
                    lag_puncak_ms = lag_puncak.as_millis() as u64,
                    rss_mb,
                    load1,
                    load5,
                    swap_mb,
                    swap_total_mb,
                    "RUNTIME PULIH"
                );
                lag_puncak = Duration::ZERO;
            }
        }
    });
}

/// Pengawas PROSES: OS thread yang tidur di luar runtime tokio sepenuhnya.
///
/// Ia sengaja tak menyentuh apa pun milik tokio. Kalau thread ini telat, tak
/// ada penjelasan lain selain kernel yang tak menjalankan proses ini — CPU
/// direbut tetangga, atau memori sedang di-swap masuk-keluar. Itulah yang
/// membedakan "bug di kode kita" dari "mesinnya kekecilan", dan tanpa thread
/// ini keduanya menghasilkan log yang identik.
pub fn spawn_thread() {
    let ambang = ambang();
    let hasil = std::thread::Builder::new()
        .name("watchdog-os".into())
        .spawn(move || {
            let mut macet_sejak: Option<Instant> = None;
            let mut lag_puncak = Duration::ZERO;

            loop {
                let mulai = Instant::now();
                std::thread::sleep(DETAK);
                let lag = mulai.elapsed().saturating_sub(DETAK);

                if lag > ambang {
                    lag_puncak = lag_puncak.max(lag);
                    if macet_sejak.is_none() {
                        macet_sejak = Some(mulai);
                        let (rss_mb, load1, load5, swap_mb, swap_total_mb) = potret();
                        tracing::warn!(
                            lag_ms = lag.as_millis() as u64,
                            rss_mb,
                            load1,
                            load5,
                            swap_mb,
                            swap_total_mb,
                            "PROSES TAK DAPAT CPU — bukan tokio yang tersendat, \
                             melainkan kernel tak menjalankan proses ini (rebutan \
                             CPU / swap / memori)"
                        );
                    }
                } else if let Some(sejak) = macet_sejak.take() {
                    let (rss_mb, load1, load5, swap_mb, swap_total_mb) = potret();
                    tracing::warn!(
                        durasi_s = sejak.elapsed().as_secs(),
                        lag_puncak_ms = lag_puncak.as_millis() as u64,
                        rss_mb,
                        load1,
                        load5,
                        swap_mb,
                        swap_total_mb,
                        "PROSES DAPAT CPU LAGI"
                    );
                    lag_puncak = Duration::ZERO;
                }
            }
        });

    // Gagal membuat thread pengawas TIDAK boleh menjatuhkan aplikasi: ia alat
    // diagnosis, bukan bagian dari layanan. Tapi ia juga tak boleh hilang
    // diam-diam — kalau tidak, suatu hari lognya sepi dan orang menyimpulkan
    // mesinnya baik-baik saja padahal pengawasnya memang tak pernah ada.
    if let Err(e) = hasil {
        tracing::warn!(error = %e, "watchdog OS thread gagal dibuat — diagnosis sendatan proses tidak aktif");
    }
}
