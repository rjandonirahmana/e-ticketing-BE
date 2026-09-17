//! capacity.rs — Deteksi CPU/RAM efektif (cgroup-aware) + turunkan plafon.
//!
//! Kenapa cgroup, bukan `/proc/meminfo`/`nproc` saja: di Docker/VPS dengan limit,
//! `/proc/meminfo` menampilkan RAM **host**, dan `nproc` bisa lebih besar dari
//! kuota CPU container. Untuk batas yang benar, baca cgroup (v2 lalu v1), baru
//! fallback ke host.
//!
//! "Maksimal user" yang dihasilkan adalah **plafon aman berbasis memori** —
//! bukan jaminan kapasitas. Kapasitas nyata tetap perlu load test (beban idle
//! vs chat vs live sangat berbeda).

#[derive(Debug, Clone, Copy)]
pub struct Capacity {
    /// CPU efektif (kuota cgroup bila ada, mis. 2.0; selain itu jumlah core host).
    pub cpu_cores: f64,
    /// RAM efektif (limit cgroup bila ada; selain itu MemTotal host).
    pub ram_bytes: u64,
    /// Asal angka: "cgroup-v2" | "cgroup-v1" | "host".
    pub source: &'static str,
    /// Rekomendasi batas koneksi WebSocket (berbasis budget RAM).
    pub recommended_max_ws: usize,
    /// Rekomendasi ukuran pool DB (berbasis jumlah core).
    pub recommended_db_pool: usize,
    /// Rekomendasi batas upload media serentak (berbasis budget RAM). Handler
    /// upload memuat file penuh ke RAM, jadi N upload paralel = N × ukuran-file.
    /// Plafon ini mencegah OOM saat burst upload.
    pub recommended_upload_concurrency: usize,
}

// ── BATAS BUFFER PER SOCKET — angka yang MENENTUKAN biaya satu koneksi ─────
//
// Bawaan `tungstenite` (lewat `axum::extract::ws`) adalah `read_buffer_size:
// 128 KiB`, dan buffer itu DIALOKASIKAN PENUH saat socket lahir —
// `BytesMut::with_capacity(read_buffer_size)` di `FrameCodec::new`, bukan
// tumbuh sesuai pemakaian. Artinya tiap koneksi WS menagih 128 KiB sebelum satu
// byte pun dikirim: 5.000 koneksi = 640 MB, 10.000 koneksi (plafon
// `WsManager::MAX_CONNECTIONS`) = 1,25 GB. Itu 9× lipat dari `PER_WS_BYTES`
// yang dipakai perencana di bawah — perencana meramal 150 MB untuk beban yang
// sebenarnya memakan 1,25 GB, dan selisih sebesar itu adalah selisih antara
// "muat" dan OOM.
//
// Pesan aplikasi ini tak pernah mendekati 128 KiB. Chat = JSON beberapa ratus
// byte; sinyal WebRTC = SDP belasan KB. Buffer sebesar itu murni cadangan yang
// tak pernah terpakai.
//
// Angkanya ditaruh DI SINI, bersama perencana yang memakainya, karena keduanya
// harus bergerak bersama. Selama batas buffer hidup di `ws/handler.rs` dan
// ramalan biayanya di sini, tak ada satu pun yang memaksa keduanya cocok —
// dan ketidakcocokan itulah keadaan yang baru saja diperbaiki.

/// Kapasitas awal buffer baca per socket WS. Bingkai yang lebih besar tetap
/// diterima: `in_buffer.reserve(len)` menumbuhkannya sesuai panjang bingkai
/// yang benar-benar datang (dibatasi `WS_MAX_MESSAGE_*` di bawah). Yang
/// dihapus hanyalah cadangan 128 KiB yang dibayar di muka oleh SETIAP koneksi.
pub const WS_READ_BUFFER: usize = 8 * 1024;

/// Ambang penyiraman buffer tulis. Tak dialokasikan di muka (`out_buffer`
/// tumbuh dari kosong), tapi dinyatakan supaya tak ada jalur yang diam-diam
/// menimbun 128 KiB per socket saat peramban lambat membaca.
pub const WS_WRITE_BUFFER: usize = 8 * 1024;

/// Plafon satu pesan pada socket CHAT.
///
/// Ini bukan sekadar penghematan: ia menutup jalur OOM yang bisa dipicu satu
/// klien. `FrameCodec::read_frame` memanggil `in_buffer.reserve(len)` dengan
/// `len` = panjang yang DIUMUMKAN header bingkai, segera setelah header
/// terbaca dan hanya dibatasi `max_frame_size`. Dengan bawaan 16 MiB, satu
/// klien jahat cukup mengumumkan bingkai 16 MiB untuk membuat server
/// mengalokasikan 16 MiB — seratus klien begitu = 1,6 GB, tanpa pernah
/// mengirim isinya.
pub const WS_MAX_MESSAGE_CHAT: usize = 64 * 1024;

/// Plafon satu pesan pada socket SINYAL (live publish/subscribe, meet).
/// Lebih longgar daripada chat karena muatannya SDP — beberapa belas KB untuk
/// tawaran dengan banyak kandidat ICE — tetapi tetap jauh di bawah 16 MiB.
pub const WS_MAX_MESSAGE_SIGNAL: usize = 256 * 1024;

/// Biaya RAM satu koneksi WS, dipakai perencana di bawah.
///
/// = buffer baca (dialokasikan di muka) + ~8 KB sisanya: future tiga task
/// (baca/tulis/heartbeat), tiga channel mpsc bounded, entri sesi di DashMap,
/// indeks keanggotaan room, dan ember rate-limit.
///
/// Angka ini BENAR hanya selama `WS_READ_BUFFER` benar-benar dipasang di tiap
/// `on_upgrade`. Kalau kelak ada endpoint WS baru yang lupa memasangnya, ia
/// menagih 128 KiB dan ramalan ini kembali berbohong — pasang
/// `crate::ws::konfigurasi_socket()` di setiap upgrade, jangan menyalin angka.
const PER_WS_BYTES: u64 = WS_READ_BUFFER as u64 + 8 * 1024;

pub fn detect() -> Capacity {
    let (cpu_cores, ram_bytes, source) = detect_raw();

    // Anggarkan ~50% RAM untuk koneksi WS (sisanya: runtime, pool, cache, SFU).
    let ws_budget = (ram_bytes / 2) / PER_WS_BYTES;
    let recommended_max_ws = (ws_budget as usize).clamp(500, 50_000);

    // ~8 koneksi DB per core (OLTP), plafon wajar agar tak melebihi Postgres.
    let recommended_db_pool = ((cpu_cores * 8.0).round() as usize).clamp(8, 64);

    // Upload di-stream ke file temp (RAM per upload hanya ~64 KB chunk, bukan
    // ukuran file penuh), jadi plafonnya dibatasi paralelisme CPU + I/O disk,
    // bukan RAM. ~32 slot per core, clamp [16, 128]. Jauh lebih longgar daripada
    // pendekatan buffer-ke-RAM lama karena tak lagi berisiko OOM.
    let recommended_upload_concurrency =
        ((cpu_cores * 32.0).round() as usize).clamp(16, 128);

    Capacity {
        cpu_cores,
        ram_bytes,
        source,
        recommended_max_ws,
        recommended_db_pool,
        recommended_upload_concurrency,
    }
}

fn detect_raw() -> (f64, u64, &'static str) {
    // ── cgroup v2 ──────────────────────────────────────────────────────────
    let v2_mem = read_first("/sys/fs/cgroup/memory.max").and_then(parse_mem_limit);
    let v2_cpu = read_first("/sys/fs/cgroup/cpu.max").and_then(parse_cpu_max);
    if v2_mem.is_some() || v2_cpu.is_some() {
        let ram = v2_mem.or_else(host_ram).unwrap_or(DEFAULT_RAM);
        let cpu = v2_cpu.unwrap_or_else(host_cpu);
        return (cpu, ram, "cgroup-v2");
    }

    // ── cgroup v1 ──────────────────────────────────────────────────────────
    let v1_mem =
        read_first("/sys/fs/cgroup/memory/memory.limit_in_bytes").and_then(parse_mem_limit);
    let v1_cpu = cgroup_v1_cpu();
    if v1_mem.is_some() || v1_cpu.is_some() {
        let ram = v1_mem.or_else(host_ram).unwrap_or(DEFAULT_RAM);
        let cpu = v1_cpu.unwrap_or_else(host_cpu);
        return (cpu, ram, "cgroup-v1");
    }

    // ── host (VM penuh / dev) ──────────────────────────────────────────────
    (host_cpu(), host_ram().unwrap_or(DEFAULT_RAM), "host")
}

const DEFAULT_RAM: u64 = 2 * 1024 * 1024 * 1024; // fallback 2 GB (mis. dev macOS)

fn read_first(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// "max" → tak terbatas (None). Angka → bytes. Nilai sangat besar (v1 unlimited
/// sentinel ~ i64::MAX) dianggap tak terbatas.
fn parse_mem_limit(s: String) -> Option<u64> {
    if s == "max" {
        return None;
    }
    let n: u64 = s.parse().ok()?;
    // v1 unlimited biasanya 0x7FFF_FFFF_FFFF_F000-an; abaikan bila tak masuk akal.
    if n == 0 || n > (1u64 << 60) {
        return None;
    }
    Some(n)
}

/// cgroup v2 `cpu.max`: "<quota> <period>" atau "max <period>".
fn parse_cpu_max(s: String) -> Option<f64> {
    let mut it = s.split_whitespace();
    let quota = it.next()?;
    let period: f64 = it.next()?.parse().ok()?;
    if quota == "max" || period <= 0.0 {
        return None;
    }
    let q: f64 = quota.parse().ok()?;
    Some((q / period).max(0.1))
}

fn cgroup_v1_cpu() -> Option<f64> {
    let quota: f64 = read_first("/sys/fs/cgroup/cpu/cpu.cfs_quota_us")?.parse().ok()?;
    let period: f64 = read_first("/sys/fs/cgroup/cpu/cpu.cfs_period_us")?.parse().ok()?;
    if quota <= 0.0 || period <= 0.0 {
        return None; // -1 = unlimited
    }
    Some((quota / period).max(0.1))
}

fn host_cpu() -> f64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as f64)
        .unwrap_or(1.0)
}

/// MemTotal dari /proc/meminfo (kB) → bytes.
fn host_ram() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}
