//! config/migrate.rs — penjalan migrasi database.
//!
//! ── KENAPA ADA ─────────────────────────────────────────────────────────────
//! Sebelumnya migrasi dijalankan manual lewat klien SQL. Cara itu punya satu
//! kegagalan yang mahal dan tak kentara: **banyak klien memecah berkas SQL
//! dengan memotong pada setiap titik-koma, tanpa memahami komentar maupun
//! string.** Satu titik-koma di dalam komentar sudah cukup untuk membelah
//! `CREATE TABLE` menjadi dua kepingan rusak — pernyataannya HILANG, dan error
//! yang muncul justru di pernyataan LAIN yang merujuk tabel yang tak pernah
//! lahir. Berjam-jam bisa habis mengejar gejala di tempat yang salah.
//!
//! Modul ini mengirim berkas **apa adanya, utuh**, lewat `batch_execute`.
//! Yang memisah pernyataan adalah PostgreSQL sendiri, yang tentu saja paham
//! komentar, dollar-quote, dan string. Kelas kegagalan itu lenyap.
//!
//! ── JAMINAN LAIN ───────────────────────────────────────────────────────────
//!   • **Sekali jalan.** `schema_migrations` mencatat apa yang sudah masuk.
//!   • **Satu proses saja.** `pg_advisory_lock` menahan replika lain saat
//!     rolling deploy, jadi dua instance tak menjalankan migrasi yang sama
//!     bersamaan.
//!   • **Utuh atau tidak sama sekali.** Tiap berkas berjalan dalam transaksi
//!     sendiri, bersama pencatatannya. Tak ada lagi keadaan separuh jadi.
//!   • **Ketahuan bila diubah.** Checksum dibandingkan — berkas yang sudah
//!     dijalankan lalu disunting akan memunculkan peringatan.

use anyhow::{Context, Result};
use deadpool_postgres::Pool;

include!(concat!(env!("OUT_DIR"), "/migrations.rs"));

/// Kunci advisory yang dipakai seluruh instance aplikasi ini. Angkanya
/// sembarang, yang penting SAMA di semua replika.
const LOCK_KEY: i64 = 0x5055_4C53_4531_0001;

/// Batas garis dasar untuk database yang sudah berisi data.
///
/// Migrasi 001–021 sudah lama dijalankan dengan tangan di database berjalan,
/// dan sebagian di antaranya TIDAK aman diulang — `007_seed_bulk.sql`, misalnya,
/// akan menyuntikkan data contoh untuk kedua kalinya. Karena itu, pada database
/// yang jelas sudah terpakai (tabel `users` ada) tetapi belum punya
/// `schema_migrations`, berkas sampai batas ini hanya DICATAT, tidak dijalankan.
///
/// Database kosong tidak terkena aturan ini: di sana semuanya dijalankan urut
/// dari nol, dan itu justru yang membuat "bangun ulang dari awal" bisa
/// dipercaya lagi.
const BASELINE: &str = "021_paid_at_semantics.sql";

/// Hash isi berkas. FNV-1a 64-bit — cukup untuk mendeteksi berkas yang berubah
/// setelah dijalankan, dan tak menambah satu pun dependensi. Ini BUKAN hash
/// kriptografis dan tak dipakai untuk keamanan.
fn checksum(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{h:016x}")
}

pub async fn run(pool: &Pool) -> Result<()> {
    let mut conn = pool.get().await.context("migrate: ambil koneksi")?;

    // Kunci lebih dulu, SEBELUM apa pun dibaca: dua instance yang start
    // bersamaan tak boleh sama-sama menyimpulkan "belum ada yang dijalankan".
    conn.execute("SELECT pg_advisory_lock($1)", &[&LOCK_KEY])
        .await
        .context("migrate: pg_advisory_lock")?;

    let hasil = jalankan(&mut conn).await;

    // Lepas kunci apa pun yang terjadi — kalau tidak, instance berikutnya
    // menggantung tanpa pesan sampai koneksi ini benar-benar mati.
    if let Err(e) = conn
        .execute("SELECT pg_advisory_unlock($1)", &[&LOCK_KEY])
        .await
    {
        tracing::warn!(error = %e, "migrate: gagal melepas advisory lock");
    }

    hasil
}

async fn jalankan(conn: &mut deadpool_postgres::Object) -> Result<()> {
    conn.batch_execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version     TEXT        PRIMARY KEY,
             checksum    TEXT        NOT NULL,
             applied_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
         )",
    )
    .await
    .context("migrate: buat schema_migrations")?;

    let sudah: std::collections::HashMap<String, String> = conn
        .query("SELECT version, checksum FROM schema_migrations", &[])
        .await
        .context("migrate: baca schema_migrations")?
        .into_iter()
        .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
        .collect();

    // Database yang sudah terpakai tapi belum pernah dicatat: pasang garis dasar.
    let perlu_baseline = if sudah.is_empty() {
        conn.query_one("SELECT to_regclass('users') IS NOT NULL AS ada", &[])
            .await
            .context("migrate: deteksi database terpakai")?
            .get::<_, bool>("ada")
    } else {
        false
    };

    if perlu_baseline {
        tracing::warn!(
            baseline = BASELINE,
            "migrate: database sudah berisi data tetapi belum punya schema_migrations — \
             berkas sampai baseline DICATAT tanpa dijalankan"
        );
    }

    let mut dijalankan = 0_usize;

    for (nama, sql) in MIGRATIONS {
        let cs = checksum(sql);

        if let Some(lama) = sudah.get(*nama) {
            if lama != &cs {
                // Bukan error: memaksa gagal di sini akan mengunci deployment
                // hanya karena komentar diperbaiki. Tapi harus terlihat, karena
                // artinya isi berkas tak lagi sama dengan yang pernah masuk.
                tracing::warn!(
                    migration = nama,
                    "migrate: berkas BERUBAH setelah dijalankan — perubahan itu tidak ikut masuk"
                );
            }
            continue;
        }

        if perlu_baseline && *nama <= BASELINE {
            conn.execute(
                "INSERT INTO schema_migrations (version, checksum) VALUES ($1, $2)
                 ON CONFLICT (version) DO NOTHING",
                &[nama, &cs],
            )
            .await
            .with_context(|| format!("migrate: catat baseline {nama}"))?;
            continue;
        }

        tracing::info!(migration = nama, "migrate: menjalankan");

        let tx = conn
            .transaction()
            .await
            .with_context(|| format!("migrate: buka transaksi {nama}"))?;

        // Berkas dikirim UTUH. PostgreSQL yang memisah pernyataannya.
        tx.batch_execute(sql)
            .await
            .with_context(|| format!("migrate: GAGAL di {nama}"))?;

        tx.execute(
            "INSERT INTO schema_migrations (version, checksum) VALUES ($1, $2)",
            &[nama, &cs],
        )
        .await
        .with_context(|| format!("migrate: catat {nama}"))?;

        tx.commit()
            .await
            .with_context(|| format!("migrate: commit {nama}"))?;

        dijalankan += 1;
    }

    if dijalankan == 0 {
        tracing::info!("migrate: skema sudah mutakhir");
    } else {
        tracing::info!(count = dijalankan, "migrate: selesai");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Urutan berkas adalah SATU-SATUNYA yang menentukan urutan penerapan
    /// migrasi. Kalau daftar hasil build tak lagi terurut, migrasi bisa
    /// berjalan sebelum tabel yang dirujuknya lahir — persis kegagalan yang
    /// modul ini dibuat untuk mencegah.
    #[test]
    fn daftar_migrasi_terurut() {
        let nama: Vec<&str> = MIGRATIONS.iter().map(|(n, _)| *n).collect();
        let mut urut = nama.clone();
        urut.sort_unstable();
        assert_eq!(nama, urut, "MIGRATIONS harus urut menurut nama berkas");
        assert!(!MIGRATIONS.is_empty(), "tak ada migrasi yang ter-embed");
    }

    /// Garis dasar memisahkan "sudah pernah dijalankan dengan tangan" dari
    /// "harus dijalankan sekarang". Perbandingan string itu load-bearing:
    /// salah arah, dan `007_seed_bulk.sql` akan menyuntik data contoh untuk
    /// kedua kalinya ke database produksi.
    #[test]
    fn baseline_memisahkan_lama_dan_baru() {
        assert!("007_seed_bulk.sql" <= BASELINE, "007 harus di bawah baseline");
        assert!("021_paid_at_semantics.sql" <= BASELINE, "021 adalah baseline");
        assert!("022_cart_payment.sql" > BASELINE, "022 harus dijalankan");
        assert!("022a_orders_payment_repair.sql" > BASELINE);
        assert!("023_products_rename.sql" > BASELINE);
    }

    /// Checksum harus stabil untuk isi yang sama dan berbeda untuk isi berbeda.
    /// Tanpa itu, peringatan "berkas berubah setelah dijalankan" jadi omong
    /// kosong: entah tak pernah menyala, atau menyala di setiap start.
    #[test]
    fn checksum_stabil_dan_peka() {
        let a = "CREATE TABLE x (id int);";
        assert_eq!(checksum(a), checksum(a));
        assert_ne!(checksum(a), checksum("CREATE TABLE y (id int);"));
        assert_eq!(checksum(a).len(), 16);
    }
}
