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
}

/// ~15 KB per koneksi WS (channel buffer + entri DashMap + overhead task).
const PER_WS_BYTES: u64 = 15 * 1024;

pub fn detect() -> Capacity {
    let (cpu_cores, ram_bytes, source) = detect_raw();

    // Anggarkan ~50% RAM untuk koneksi WS (sisanya: runtime, pool, cache, SFU).
    let ws_budget = (ram_bytes / 2) / PER_WS_BYTES;
    let recommended_max_ws = (ws_budget as usize).clamp(500, 50_000);

    // ~8 koneksi DB per core (OLTP), plafon wajar agar tak melebihi Postgres.
    let recommended_db_pool = ((cpu_cores * 8.0).round() as usize).clamp(8, 64);

    Capacity {
        cpu_cores,
        ram_bytes,
        source,
        recommended_max_ws,
        recommended_db_pool,
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
