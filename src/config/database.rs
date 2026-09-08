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

    // ── JANGAN BUANG `options` MILIK OPERATOR ────────────────────────────────
    //
    // Kalimat di atas — deadpool menerapkan `options` SESUDAH URL di-parse —
    // punya sisi lain yang mahal: menyetel `cfg.options` juga MENGHAPUS apa pun
    // yang operator tulis sendiri di `DATABASE_URL`, tanpa satu pun pesan.
    //
    // Yang paling sering ada di sana adalah `?options=-csearch_path%3D...`. Dan
    // search_path yang hilang tampak PERSIS seperti tabel yang tak pernah
    // dibuat: `relation "refresh_tokens" does not exist`, padahal tabelnya ada
    // — hanya di skema yang tak lagi dicari. Berjam-jam bisa habis mencari
    // migrasi yang sebenarnya sudah jalan.
    //
    // Karena itu digabung, bukan ditimpa. Milik operator ditulis lebih dulu
    // supaya batas waktu kita tetap jadi kata terakhir bila keduanya menyetel
    // kunci yang sama.
    let dari_url = options_dari_url(database_url);
    if let Some(o) = &dari_url {
        tracing::info!(options = %o, "options dari DATABASE_URL dipertahankan");
    }

    let mut bagian: Vec<String> = dari_url.into_iter().collect();

    if statement_timeout_ms > 0 {
        bagian.push(format!(
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

    if !bagian.is_empty() {
        cfg.options = Some(bagian.join(" "));
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

/// Ambil parameter `options=` dari query string `DATABASE_URL`, sudah
/// di-decode. `None` bila tak ada atau kosong.
fn options_dari_url(url: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|pasangan| {
        let nilai = pasangan.strip_prefix("options=")?;
        let decoded = urlencoding::decode(nilai).ok()?.into_owned();
        (!decoded.trim().is_empty()).then_some(decoded)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_path_di_url_terbaca() {
        let u = "postgres://a:b@h:5432/db?options=-csearch_path%3Dticketing";
        assert_eq!(options_dari_url(u).as_deref(), Some("-csearch_path=ticketing"));
    }

    #[test]
    fn options_di_antara_parameter_lain() {
        let u = "postgres://a:b@h:5432/db?sslmode=disable&options=-csearch_path%3Dx&connect_timeout=5";
        assert_eq!(options_dari_url(u).as_deref(), Some("-csearch_path=x"));
    }

    #[test]
    fn tanpa_options_hasilnya_none() {
        assert_eq!(options_dari_url("postgres://a:b@h:5432/db"), None);
        assert_eq!(options_dari_url("postgres://a:b@h:5432/db?sslmode=disable"), None);
        // Kosong sama dengan tidak ada — jangan kirim string kosong ke server.
        assert_eq!(options_dari_url("postgres://a:b@h:5432/db?options="), None);
    }

    /// Parameter yang KEBETULAN berakhiran "options" bukan milik kita.
    #[test]
    fn nama_parameter_dicocokkan_utuh() {
        assert_eq!(options_dari_url("postgres://a@h/db?my_options=-cx"), None);
    }
}
