//! status.rs — Potret kesehatan mesin untuk tab Analitik admin.
//!
//! Dikompilasi untuk SSR **dan** WASM, jadi isinya angka jadi — pembacaan
//! `/proc` ada di `service/server_status.rs`. Pemformatannya di sini supaya
//! server dan layar mustahil berbeda pembulatan.

use serde::{Deserialize, Serialize};

/// Satu potret keadaan server saat tombolnya ditekan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusServer {
    /// Angka mesin berhasil dibaca? `false` di macOS — pembacaannya lewat
    /// `/proc`, yang hanya ada di Linux. Kartunya tetap tampil dengan
    /// keterangannya, bukan deretan nol yang terbaca seperti server menganggur.
    pub tersedia: bool,
    pub catatan: String,

    pub cpu_pct: f32,
    pub cpu_cores: usize,
    pub load1: f32,
    pub load5: f32,
    pub load15: f32,

    pub mem_total: u64,
    pub mem_terpakai: u64,
    pub mem_pct: f32,
    /// "Kontainer (cgroup)" atau "Mesin (/proc/meminfo)". Keduanya bisa berbeda
    /// jauh, dan angka tanpa keterangan asalnya membuat admin menyimpulkan yang
    /// salah tentang sisa memorinya.
    pub mem_sumber: String,
    pub swap_total: u64,
    pub swap_terpakai: u64,
    /// Memori proses aplikasi ini sendiri (RSS).
    pub app_rss: u64,

    #[serde(default)]
    pub disk: Vec<InfoDisk>,

    pub uptime_mesin: String,
    pub uptime_app: String,

    /// Latensi jalur panas. `None` = belum ada satu pun pengamatan sejak proses
    /// hidup — dibedakan dari nol, yang akan terbaca "sangat cepat".
    #[serde(default)]
    pub latensi: Vec<Latensi>,
    /// Pesan yang dibuang karena kanal penerimanya penuh.
    #[serde(default)]
    pub pesan_dibuang: u64,
    /// Sesi yang digantikan koneksi baru. Melonjak = badai sambung-ulang.
    #[serde(default)]
    pub sesi_diganti: u64,

    pub pool_max: usize,
    pub pool_size: usize,
    /// Koneksi menganggur & siap dipakai. Nol terus-menerus berarti permintaan
    /// sedang mengantre menunggu koneksi — gejala paling awal halaman lambat,
    /// dan persis keadaan yang membuat situs ini pernah jatuh.
    pub pool_idle: usize,
}

/// Ruang satu filesystem.
///
/// Angka besar di kartunya adalah SISA, bukan yang terpakai. Memori penuh
/// membuat proses dibunuh lalu dihidupkan lagi; DISK penuh membuat Postgres
/// berhenti menerima tulisan dan unggahan gagal separuh jalan — dan tak satu
/// pun dari itu pulih sendiri setelah restart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfoDisk {
    pub label: String,
    pub path: String,
    pub total: u64,
    pub terpakai: u64,
    /// Ruang yang benar-benar bisa dipakai proses biasa. BUKAN `total −
    /// terpakai`: ext4 menyimpan jatah cadangan untuk root (bawaan 5%).
    pub tersedia: u64,
    pub pct: f32,
}

/// Satu baris latensi di kartu status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Latensi {
    pub nama: String,
    pub jumlah: u64,
    /// Milidetik. `None` bila belum ada pengamatan.
    pub p50: Option<u64>,
    pub p95: Option<u64>,
    pub p99: Option<u64>,
}

/// (terpakai, tersedia) → persen ala `df`. Dipisah supaya bisa diuji tanpa
/// menyentuh filesystem.
pub fn pct_disk(terpakai: u64, tersedia: u64) -> f32 {
    let dasar = terpakai.saturating_add(tersedia);
    if dasar == 0 {
        return 0.0;
    }
    (terpakai as f64 / dasar as f64 * 100.0) as f32
}

/// Peringatan sisa disk, atau `None` bila masih lapang.
///
/// DUA syarat, bukan persen saja: 10% dari 500 GB masih 50 GB (lapang),
/// sementara 10% dari 20 GB tinggal 2 GB. Ambang mutlaknya yang menangkap VPS
/// kecil — dan VPS kecil justru yang dipakai.
pub fn peringatan_disk(tersedia: u64, pct: f32) -> Option<&'static str> {
    const GB: u64 = 1024 * 1024 * 1024;
    if tersedia < 2 * GB || pct >= 95.0 {
        Some(
            "Sisa disk KRITIS. Postgres berhenti menerima tulisan bila disk penuh, dan \
             unggahan akan gagal separuh jalan. Kosongkan sekarang: berkas sisa di \
             UPLOAD_TMP_DIR, lalu `docker system prune`.",
        )
    } else if tersedia < 5 * GB || pct >= 85.0 {
        Some("Sisa disk menipis. Periksa berkas sementara unggahan dan image Docker lama.")
    } else {
        None
    }
}

/// Byte → "812 MB" / "3,7 GB". Basis 1024 (yang dipakai `free` dan htop),
/// supaya angkanya cocok saat admin membandingkan dengan terminal.
pub fn fmt_bytes(b: u64) -> String {
    const KB: f64 = 1024.0;
    let b = b as f64;
    let (nilai, satuan) = if b >= KB * KB * KB {
        (b / (KB * KB * KB), "GB")
    } else if b >= KB * KB {
        (b / (KB * KB), "MB")
    } else if b >= KB {
        (b / KB, "KB")
    } else {
        (b, "B")
    };
    if satuan == "GB" {
        format!("{:.1} {}", nilai, satuan).replace('.', ",")
    } else {
        format!("{:.0} {}", nilai, satuan)
    }
}

/// Detik → "3 hari 4 jam" / "12 menit".
pub fn fmt_durasi(detik: u64) -> String {
    let hari = detik / 86_400;
    let jam = (detik % 86_400) / 3_600;
    let menit = (detik % 3_600) / 60;
    if hari > 0 {
        format!("{hari} hari {jam} jam")
    } else if jam > 0 {
        format!("{jam} jam {menit} menit")
    } else if menit > 0 {
        format!("{menit} menit")
    } else {
        format!("{detik} detik")
    }
}

/// Keparahan sebuah persentase — SATU ambang untuk CPU maupun memori, supaya
/// dua kartu bersebelahan tak memakai skala berbeda diam-diam.
pub fn tingkat_pakai(pct: f32) -> (&'static str, &'static str) {
    if pct >= 90.0 {
        ("Kritis", "stat-kritis")
    } else if pct >= 75.0 {
        ("Tinggi", "stat-tinggi")
    } else if pct >= 50.0 {
        ("Sedang", "stat-sedang")
    } else {
        ("Aman", "stat-aman")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ukuran_byte_terbaca_manusia() {
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_bytes(812 * 1024 * 1024), "812 MB");
        assert_eq!(fmt_bytes(4 * 1024 * 1024 * 1024), "4,0 GB");
    }

    /// Basis 1024, bukan 1000 — supaya angkanya sama dengan `free -h`.
    #[test]
    fn basis_1024_bukan_1000() {
        assert_eq!(fmt_bytes(1_000_000_000), "954 MB");
        assert_eq!(fmt_bytes(1_073_741_824), "1,0 GB");
    }

    #[test]
    fn durasi_memilih_satuan_terbesar() {
        assert_eq!(fmt_durasi(45), "45 detik");
        assert_eq!(fmt_durasi(7_320), "2 jam 2 menit");
        assert_eq!(fmt_durasi(273_600), "3 hari 4 jam");
    }

    /// Persen disk dihitung ala `df` (terhadap terpakai+tersedia), BUKAN
    /// terhadap total — kalau tidak, angkanya tak cocok dengan `df -h` karena
    /// jatah cadangan root ikut terhitung sebagai "sisa".
    #[test]
    fn pct_disk_mengikuti_df() {
        let gb = 1024 * 1024 * 1024;
        assert_eq!(pct_disk(9 * gb, gb).round(), 90.0);
        assert_eq!(pct_disk(0, 0), 0.0);
    }

    #[test]
    fn peringatan_disk_menangkap_vps_kecil() {
        let gb = 1024 * 1024 * 1024;
        assert!(peringatan_disk(40 * gb, 20.0).is_none());
        // Persen kecil, sisa mutlak sedikit — kasus VPS kecil.
        assert!(peringatan_disk(3 * gb, 10.0).is_some());
        assert!(peringatan_disk(gb, 10.0).unwrap().contains("KRITIS"));
        // Sisa besar, persen ekstrem — disk besar yang hampir penuh.
        assert!(peringatan_disk(50 * gb, 96.0).unwrap().contains("KRITIS"));
    }

    /// Ambangnya harus MENAIK — kalau tidak, memori 95% bisa tampil "Aman".
    #[test]
    fn tingkat_pakai_naik_bertahap() {
        assert_eq!(tingkat_pakai(10.0).0, "Aman");
        assert_eq!(tingkat_pakai(60.0).0, "Sedang");
        assert_eq!(tingkat_pakai(80.0).0, "Tinggi");
        assert_eq!(tingkat_pakai(99.0).0, "Kritis");
    }
}
