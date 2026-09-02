//! server_status.rs — Pembacaan kesehatan mesin untuk tab Analitik admin.
//!
//! Dibaca dari berkas semu Linux (`/proc`, `/sys/fs/cgroup`) yang memang
//! disediakan kernel untuk keperluan ini, plus `statvfs(2)` untuk ruang disk —
//! `/proc` hanya memuat daftar mount, bukan kapasitasnya. Sebuah crate seperti
//! `sysinfo` akan menambah puluhan detik waktu kompilasi dan selusin dependensi
//! transitif untuk empat berkas teks yang formatnya stabil sejak dua dekade.
//!
//! DI MACOS `/proc` tak ada, jadi pembacaannya gagal dengan rapi dan kartunya
//! menampilkan keterangan, bukan deretan nol yang terbaca seperti server
//! menganggur total.
//!
//! DI DALAM KONTAINER `/proc/meminfo` melaporkan memori MESIN INDUK, bukan
//! jatah kontainer. Batas cgroup v2 karena itu diperiksa lebih dulu: tanpa itu,
//! aplikasi yang dibatasi 1 GB tampak memakai 12% dari 8 GB — dan admin baru
//! tahu batasnya terlampaui ketika prosesnya sudah dibunuh OOM killer.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use deadpool_postgres::Pool;

use crate::web::status::{fmt_durasi, pct_disk, InfoDisk, Latensi, StatusServer};

/// Saat proses ini mulai. Disentuh sekali di `main` supaya jamnya benar-benar
/// dimulai saat boot, bukan saat halaman status pertama kali dibuka.
static MULAI: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Panggil sekali di awal `main()`.
pub fn catat_waktu_mulai() {
    LazyLock::force(&MULAI);
}

/// Jeda antara dua cuplikan `/proc/stat`.
///
/// Pemakaian CPU adalah SELISIH dua pembacaan; satu pembacaan hanya memberi
/// total sejak boot, yang tak berarti apa-apa. 300 ms cukup untuk angka stabil
/// dan masih terasa seketika bagi yang menekan tombolnya.
const JEDA_CUPLIK: Duration = Duration::from_millis(300);

fn baca(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn baca_angka(path: &str) -> Option<u64> {
    baca(path)?.trim().parse::<u64>().ok()
}

/// (total, menganggur) jiffies dari baris pertama `/proc/stat`.
/// "Menganggur" = idle + iowait; iowait ikut karena CPU-nya memang tak
/// mengerjakan apa pun saat itu, hanya menunggu disk.
fn cuplik_cpu() -> Option<(u64, u64)> {
    let isi = baca("/proc/stat")?;
    let baris = isi.lines().next()?;
    let angka: Vec<u64> = baris
        .split_whitespace()
        .skip(1)
        .filter_map(|v| v.parse::<u64>().ok())
        .collect();
    if angka.len() < 5 {
        return None;
    }
    Some((angka.iter().sum(), angka[3] + angka[4]))
}

async fn cpu_pct() -> Option<f32> {
    let (t1, i1) = cuplik_cpu()?;
    tokio::time::sleep(JEDA_CUPLIK).await;
    let (t2, i2) = cuplik_cpu()?;
    let dt = t2.checked_sub(t1)?;
    if dt == 0 {
        return Some(0.0);
    }
    let di = i2.saturating_sub(i1);
    Some((((dt - di.min(dt)) as f64 / dt as f64) * 100.0) as f32)
}

/// Satu nilai berlabel dari `/proc/meminfo`, dalam BYTE.
/// Format barisnya: `MemTotal:       16316576 kB`.
fn meminfo_kb(isi: &str, label: &str) -> Option<u64> {
    isi.lines()
        .find(|l| l.starts_with(label) && l[label.len()..].starts_with(':'))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<u64>().ok())
        .map(|kb| kb * 1024)
}

/// (total, terpakai, sumber). Batas kontainer menang atas memori mesin induk.
fn memori() -> Option<(u64, u64, String)> {
    if let (Some(batas), Some(pakai)) = (
        baca_angka("/sys/fs/cgroup/memory.max"),
        baca_angka("/sys/fs/cgroup/memory.current"),
    ) {
        // Sebagian cgroup dilaporkan dengan batas raksasa (praktis "tanpa
        // batas") — itu bukan angka yang berguna.
        if batas > 0 && batas < (1 << 60) {
            return Some((batas, pakai.min(batas), "Kontainer (cgroup)".into()));
        }
    }
    let isi = baca("/proc/meminfo")?;
    let total = meminfo_kb(&isi, "MemTotal")?;
    // MemAvailable, bukan MemFree: cache halaman bisa direbut kembali kapan
    // saja, jadi menghitungnya sebagai "terpakai" membuat setiap server Linux
    // yang sehat tampak nyaris kehabisan memori.
    let tersedia = meminfo_kb(&isi, "MemAvailable").unwrap_or_else(|| {
        meminfo_kb(&isi, "MemFree").unwrap_or(0)
            + meminfo_kb(&isi, "Cached").unwrap_or(0)
            + meminfo_kb(&isi, "Buffers").unwrap_or(0)
    });
    Some((
        total,
        total.saturating_sub(tersedia.min(total)),
        "Mesin (/proc/meminfo)".into(),
    ))
}

fn swap() -> (u64, u64) {
    let Some(isi) = baca("/proc/meminfo") else {
        return (0, 0);
    };
    let total = meminfo_kb(&isi, "SwapTotal").unwrap_or(0);
    let bebas = meminfo_kb(&isi, "SwapFree").unwrap_or(0);
    (total, total.saturating_sub(bebas.min(total)))
}

fn app_rss() -> u64 {
    baca("/proc/self/status")
        .and_then(|isi| meminfo_kb(&isi, "VmRSS"))
        .unwrap_or(0)
}

fn loadavg() -> (f32, f32, f32) {
    let Some(isi) = baca("/proc/loadavg") else {
        return (0.0, 0.0, 0.0);
    };
    let mut it = isi.split_whitespace();
    let mut ambil = || it.next().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
    (ambil(), ambil(), ambil())
}

fn uptime_mesin() -> u64 {
    baca("/proc/uptime")
        .and_then(|s| s.split_whitespace().next().map(str::to_string))
        .and_then(|s| s.parse::<f64>().ok())
        .map(|d| d as u64)
        .unwrap_or(0)
}

/// (total, terpakai, tersedia) satu filesystem, dalam byte.
///
/// `statvfs(2)`, bukan memanggil `df`: yang terakhir berarti menjalankan proses
/// tiap tombol ditekan dan bergantung pada base image yang menyertakan
/// coreutils. Berjalan di macOS juga (statvfs ada di POSIX), jadi kartu
/// penyimpanan tetap berisi di mesin pengembang meski CPU/memorinya tidak.
fn ruang_disk(path: &std::path::Path) -> Option<(u64, u64, u64)> {
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    // SAFETY: `c` C-string valid selama pemanggilan; `s` milik kita, dinolkan
    // lebih dulu. statvfs hanya MENULIS ke `s` dan tak menyimpan pointer apa pun.
    let s = unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut s) != 0 {
            return None;
        }
        s
    };
    // f_frsize = ukuran blok sesungguhnya; f_bsize hanya ukuran blok "yang
    // disukai" I/O dan pada sebagian filesystem berbeda dari yang dipakai
    // menghitung f_blocks. Salah memilih = angka melenceng berkali lipat.
    let unit = if s.f_frsize > 0 {
        s.f_frsize as u64
    } else {
        s.f_bsize as u64
    };
    let total = (s.f_blocks as u64).checked_mul(unit)?;
    let bebas_root = (s.f_bfree as u64).saturating_mul(unit);
    let tersedia = (s.f_bavail as u64).saturating_mul(unit);
    Some((total, total.saturating_sub(bebas_root), tersedia))
}

/// Jalur yang benar-benar ADA terdekat, menaiki induknya.
///
/// `UPLOAD_TMP_DIR` dibuat saat pertama dipakai, jadi di server yang belum
/// pernah menerima unggahan direktorinya belum ada — dan statvfs atas jalur
/// yang tak ada gagal. Yang ingin diketahui admin toh disk TEMPAT direktori itu
/// akan lahir, dan induknya ada di disk yang sama.
fn jalur_terdekat(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut p = path.to_path_buf();
    loop {
        if p.exists() {
            return Some(p);
        }
        if !p.pop() || p.as_os_str().is_empty() {
            return None;
        }
    }
}

/// Ruang disk tiap FILESYSTEM yang dipakai aplikasi.
///
/// Dua jalur diperiksa — disk sistem dan direktori berkas sementara unggahan —
/// lalu yang berada di filesystem SAMA dibuang (dibandingkan lewat nomor device
/// `st_dev`). Di penataan biasa hasilnya satu kartu; entri kedua muncul justru
/// saat unggahan ditaruh di volume terpisah, dan itu persis keadaan yang perlu
/// dilihat sendiri-sendiri.
fn daftar_disk(upload_tmp: &std::path::Path) -> Vec<InfoDisk> {
    use std::os::unix::fs::MetadataExt;

    let kandidat: [(&str, std::path::PathBuf); 2] = [
        ("Disk sistem", std::path::PathBuf::from("/")),
        ("Unggahan sementara", upload_tmp.to_path_buf()),
    ];

    let mut keluar = Vec::new();
    let mut device_terlihat = Vec::new();
    for (label, path) in kandidat {
        let Some(nyata) = jalur_terdekat(&path) else {
            continue;
        };
        let Ok(meta) = std::fs::metadata(&nyata) else {
            continue;
        };
        if device_terlihat.contains(&meta.dev()) {
            continue;
        }
        let Some((total, terpakai, tersedia)) = ruang_disk(&nyata) else {
            continue;
        };
        device_terlihat.push(meta.dev());
        keluar.push(InfoDisk {
            label: label.to_string(),
            path: path.display().to_string(),
            total,
            terpakai,
            tersedia,
            pct: pct_disk(terpakai, tersedia),
        });
    }
    keluar
}

/// Potret lengkap. Memakan ~300 ms karena CPU butuh dua cuplikan.
pub async fn potret(pool: &Pool, upload_tmp: &std::path::Path) -> StatusServer {
    let cpu = cpu_pct().await;
    let mem = memori();
    let (swap_total, swap_terpakai) = swap();
    let (load1, load5, load15) = loadavg();
    let st = pool.status();

    let (mem_total, mem_terpakai, mem_sumber) = mem
        .clone()
        .unwrap_or((0, 0, "tak terbaca".into()));
    let mem_pct = if mem_total > 0 {
        (mem_terpakai as f64 / mem_total as f64 * 100.0) as f32
    } else {
        0.0
    };

    let tersedia = cpu.is_some() && mem.is_some();
    StatusServer {
        tersedia,
        catatan: if tersedia {
            "Dibaca dari /proc dan cgroup mesin ini.".into()
        } else {
            // Kalimatnya menyebut SEBABNYA, bukan sekadar "gagal": di mesin
            // pengembang ini normal dan tak perlu ditindaklanjuti.
            "CPU/memori tak terbaca — /proc hanya ada di Linux. Kartu penyimpanan \
             dan kolam koneksi tetap sahih."
                .into()
        },
        cpu_pct: cpu.unwrap_or(0.0),
        cpu_cores: std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(0),
        load1,
        load5,
        load15,
        mem_total,
        mem_terpakai,
        mem_pct,
        mem_sumber,
        swap_total,
        swap_terpakai,
        app_rss: app_rss(),
        disk: daftar_disk(upload_tmp),
        uptime_mesin: fmt_durasi(uptime_mesin()),
        uptime_app: fmt_durasi(MULAI.elapsed().as_secs()),
        latensi: {
            use crate::service::metrik::METRIK;
            [
                ("Kirim chat", &METRIK.chat_kirim),
                ("Kueri DB", &METRIK.db_kueri),
                ("Publish Redis", &METRIK.redis_publish),
            ]
            .into_iter()
            .map(|(nama, h)| Latensi {
                nama: nama.to_string(),
                jumlah: h.jumlah(),
                p50: h.persentil(0.50),
                p95: h.persentil(0.95),
                p99: h.persentil(0.99),
            })
            .collect()
        },
        pesan_dibuang: crate::service::metrik::METRIK
            .pesan_dibuang
            .load(std::sync::atomic::Ordering::Relaxed),
        sesi_diganti: crate::service::metrik::METRIK
            .sesi_diganti
            .load(std::sync::atomic::Ordering::Relaxed),
        pool_max: st.max_size,
        pool_size: st.size,
        pool_idle: st.available.max(0) as usize,
    }
}
