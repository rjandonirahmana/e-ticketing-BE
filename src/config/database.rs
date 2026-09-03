use std::time::Duration;

use deadpool_postgres::{Config, Pool, Runtime};
use tokio_postgres::NoTls;

/// Batas umur satu statement, dalam milidetik, bila `DB_STATEMENT_TIMEOUT_MS`
/// tak di-set. `0` mematikannya (perilaku lama: menunggu selamanya).
const STATEMENT_TIMEOUT_MS_BAWAAN: u64 = 15_000;

/// Batas transaksi yang dibuka lalu ditinggal menganggur. Baris yang dikunci
/// transaksi seperti itu tak bisa disentuh siapa pun sampai koneksinya mati —
/// satu task yang tersendat di tengah transaksi cukup untuk membekukan
/// checkout semua orang.
const IDLE_TX_TIMEOUT_MS: u64 = 30_000;

pub async fn create_pool(database_url: &str, max_size: usize) -> anyhow::Result<Pool> {
    let mut cfg = Config::new();
    cfg.url = Some(database_url.to_string());

    // ── Batas waktu DI SISI POSTGRES ─────────────────────────────────────────
    //
    // Timeout pool di bawah cuma membatasi ANTREAN menunggu koneksi. Begitu
    // sebuah request memegang koneksi, query-nya sendiri tak punya batas apa
    // pun: satu query yang menggantung (menunggu lock, seq scan tabel besar,
    // Postgres yang sedang tersendat) menahan koneksi itu, task-nya, dan
    // request-nya SELAMANYA. Klien menyerah lebih dulu, proxy mencatat 502,
    // tapi pekerjaannya di server tak pernah berhenti — jadi beban justru
    // menumpuk sementara tak ada satu pun yang terlayani.
    //
    // `options` diterapkan deadpool SESUDAH URL di-parse, jadi ia menang atas
    // apa pun yang tertulis di `DATABASE_URL`.
    let statement_timeout_ms = std::env::var("DB_STATEMENT_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(STATEMENT_TIMEOUT_MS_BAWAAN);

    if statement_timeout_ms > 0 {
        cfg.options = Some(format!(
            "-c statement_timeout={statement_timeout_ms} \
             -c idle_in_transaction_session_timeout={IDLE_TX_TIMEOUT_MS}"
        ));
        tracing::info!(
            statement_timeout_ms,
            idle_tx_timeout_ms = IDLE_TX_TIMEOUT_MS,
            "Batas waktu query Postgres aktif"
        );
    } else {
        tracing::warn!(
            "DB_STATEMENT_TIMEOUT_MS=0 — query yang menggantung akan menahan \
             koneksi pool tanpa batas waktu"
        );
    }

    // ── Koneksi yang MATI TANPA PAMIT ────────────────────────────────────────
    //
    // Postgres di sini diakses lewat gateway bridge docker (172.17.0.1). Bila
    // sambungan itu putus tanpa RST — host di-restart, NAT lupa entrinya,
    // firewall membuang state — koneksi TCP di sisi kita tetap tampak terbuka
    // dan query di atasnya menunggu balasan yang tak akan pernah datang.
    //
    // Keepalive membuat kernel yang menemukannya, lalu koneksi mati itu
    // dikeluarkan dari pool alih-alih membekukan setiap request yang kebagian
    // dia. Tanpa ini, `statement_timeout` di atas pun tak menolong: batas itu
    // ditegakkan Postgres, dan Postgres-lah yang sedang tak bisa dihubungi.
    cfg.keepalives = Some(true);
    cfg.keepalives_idle = Some(Duration::from_secs(30));
    cfg.connect_timeout = Some(Duration::from_secs(5));

    cfg.pool = Some(deadpool_postgres::PoolConfig {
        max_size,
        // Fail-fast saat DB jenuh: tanpa timeout, request yang menunggu koneksi
        // menumpuk tanpa batas (tiap request = task + buffer hidup) sampai RAM
        // habis. Lebih baik sebagian request gagal cepat (5xx singkat) daripada
        // seluruh server ikut tumbang.
        timeouts: deadpool_postgres::Timeouts {
            // Maks. menunggu slot koneksi dari pool saat semua sibuk.
            wait: Some(Duration::from_secs(5)),
            // Maks. membuka koneksi TCP baru ke Postgres.
            create: Some(Duration::from_secs(5)),
            // Maks. health-check koneksi idle sebelum dipakai ulang.
            recycle: Some(Duration::from_secs(2)),
        },
        ..Default::default()
    });

    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;

    // Smoke-test the pool with a trivial query so a misconfigured DATABASE_URL
    // fails fast at startup instead of on the first request.
    let client = pool.get().await?;
    client.simple_query("SELECT 1").await?;

    Ok(pool)
}
