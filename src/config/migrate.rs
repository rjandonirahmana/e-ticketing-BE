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
/// Migrasi 001–038 sudah dijalankan dengan tangan di database berjalan,
/// dan sebagian di antaranya TIDAK aman diulang — `007_seed_bulk.sql`, misalnya,
/// akan menyuntikkan data contoh untuk kedua kalinya. Karena itu, pada database
/// yang jelas sudah terpakai (tabel `users` ada) tetapi belum punya
/// `schema_migrations`, berkas sampai batas ini hanya DICATAT, tidak dijalankan.
///
/// Database kosong tidak terkena aturan ini: di sana semuanya dijalankan urut
/// dari nol, dan itu justru yang membuat "bangun ulang dari awal" bisa
/// dipercaya lagi.
///
/// ── WAJIB DIGESER SETIAP KALI MIGRASI DITERAPKAN DENGAN TANGAN ──────────────
/// Angka ini pernah tertinggal di 021 sementara produksi sudah dimigrasi tangan
/// sampai 038. Akibatnya penjalan memutar ulang 022, dan 022 GAGAL karena
/// menyebut `event_variants` yang sudah di-rename jadi `product_variants` oleh
/// 023 — berkas idempoten tak menolong ketika yang berubah adalah SEJARAHNYA.
/// Kalau kamu menjalankan sebuah migrasi lewat psql, geser konstanta ini ke
/// berkas itu dalam commit yang sama.
const BASELINE: &str = "038_reviews_order_fk.sql";

/// Garis dasar yang benar-benar dipakai, boleh ditimpa lewat `MIGRATE_BASELINE`.
///
/// ── KENAPA TIDAK CUKUP KONSTANTA ────────────────────────────────────────────
/// `BASELINE` benar pada hari ia ditulis. Tetapi database produksi terus maju
/// dengan tangan: pada 8 Sep 2026 ia sudah memuat 022-038 (`product_variants`
/// hasil rename 023, `deleted_at` dari 030, `reply_to_id` dari 036) sementara
/// konstanta ini masih menunjuk 021. Akibatnya penjalan migrasi menyimpulkan
/// 022 ke atas "belum pernah jalan" dan MEMUTARNYA ULANG.
///
/// Memutar ulang bukan sekadar mubazir, ia GAGAL: `022_cart_payment.sql`
/// menyebut `event_variants`, dan tabel itu sudah berganti nama menjadi
/// `product_variants` oleh 023. Berkas yang idempoten pun tak menolong, karena
/// yang berubah adalah SEJARAH, bukan isi satu berkas.
///
/// Karena itu garis dasar adalah fakta PER-DEPLOYMENT, bukan konstanta program:
/// tiap database punya titik "sampai sini sudah dikerjakan tangan" sendiri.
///
///   MIGRATE_BASELINE=038_reviews_order_fk.sql
///
/// Namanya harus sama persis dengan salah satu berkas migrasi — salah ketik
/// dibandingkan secara leksikal dan bisa mencatat sebagian acak tanpa suara.
fn baseline_terpakai() -> Result<String> {
    let Ok(minta) = std::env::var("MIGRATE_BASELINE") else {
        return Ok(BASELINE.to_string());
    };
    let minta = minta.trim().to_string();
    let daftar: Vec<&str> = MIGRATIONS.iter().map(|(n, _)| *n).collect();
    validasi_baseline(&minta, &daftar)?;
    Ok(minta)
}

/// Dipisah supaya bisa diuji tanpa menyentuh variabel lingkungan proses.
fn validasi_baseline(minta: &str, daftar: &[&str]) -> Result<()> {
    if daftar.contains(&minta) {
        return Ok(());
    }
    anyhow::bail!(
        "MIGRATE_BASELINE=`{minta}` bukan nama berkas migrasi mana pun.\n  \
         Tulis nama LENGKAP berikut ekstensinya, mis. `{}`.\n  \
         Nama yang salah dibandingkan secara leksikal dan bisa mencatat \
         sebagian migrasi tanpa satu pun peringatan.",
        daftar.last().copied().unwrap_or("038_reviews_order_fk.sql")
    )
}

/// Hash isi berkas. FNV-1a 64-bit — cukup untuk mendeteksi berkas yang berubah
/// setelah dijalankan, dan tak menambah satu pun dependensi. Ini BUKAN hash
/// kriptografis dan tak dipakai untuk keamanan.
fn checksum(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        // CATATAN: konstanta ini 0x1000000001b3 — SATU NOL LEBIH BANYAK daripada
        // bilangan prima FNV-1a 64-bit yang sebenarnya (0x100000001b3). Awalnya
        // salah ketik, tetapi sekarang TIDAK BOLEH diperbaiki: setiap checksum
        // yang sudah tercatat di `schema_migrations` produksi dihitung dengan
        // konstanta ini, dan menggantinya membuat SELURUH migrasi berteriak
        // "berkas BERUBAH" di tiap start. Fungsinya sebagai pendeteksi
        // perubahan tetap utuh — sebaran bitnya tak dipakai untuk apa pun
        // selain perbandingan sama/tidak. Dikunci oleh test nilai harfiah.
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{h:016x}")
}

pub async fn run(pool: &Pool) -> Result<()> {
    let mut conn = pool.get().await.context("migrate: ambil koneksi")?;

    // ── Migrasi DIKECUALIKAN dari `statement_timeout` ────────────────────────
    //
    // Pool memasang batas umur statement (lihat `config/database.rs`) supaya
    // satu query yang menggantung tak menahan koneksi selamanya. Migrasi adalah
    // satu-satunya tempat batas itu salah: `CREATE INDEX` di tabel `stories`
    // yang sudah ratusan ribu baris memang butuh lebih lama dari batas request
    // biasa, dan dipotong di tengah ia gagal — aplikasinya menolak start karena
    // migrasi gagal, dengan pesan "canceling statement due to statement
    // timeout" yang sama sekali tak menyebut migrasi.
    //
    // `RESET` di bawah mengembalikannya ke nilai bawaan sesi, yaitu yang
    // dikirim lewat `options` saat koneksi dibuka. Itu penting: koneksi ini
    // KEMBALI ke pool setelah selesai, dan tanpa dikembalikan ia akan melayani
    // request biasa tanpa batas waktu selamanya.
    conn.batch_execute("SET statement_timeout = 0")
        .await
        .context("migrate: mematikan statement_timeout")?;

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

    // Kembalikan batas waktu SEBELUM koneksi ini dilepas ke pool — lihat
    // catatan di atas. Gagal di sini berarti koneksinya sudah rusak; ia akan
    // dibuang saat recycle, jadi cukup dicatat.
    if let Err(e) = conn.batch_execute("RESET statement_timeout").await {
        tracing::warn!(error = %e, "migrate: gagal mengembalikan statement_timeout");
    }

    hasil
}

/// Migrasi yang ada di binari tetapi belum tercatat di database — tanpa
/// menjalankan apa pun.
///
/// Dipakai HANYA ketika `AUTO_MIGRATE` dimatikan. Tanpa ini, deployment yang
/// mematikan migrasi berjalan dengan skema tertinggal secara diam-diam, dan
/// kegagalannya baru muncul berjam-jam kemudian sebagai `relation "..." does
/// not exist` dari sebuah tugas latar — jauh dari sebabnya. Persis yang terjadi
/// pada `refresh_tokens`: migrasi 025 tak pernah masuk, dan yang melaporkannya
/// adalah pembersih token harian, bukan startup.
pub async fn tertinggal(pool: &Pool) -> Result<Vec<&'static str>> {
    let conn = pool.get().await.context("periksa migrasi: ambil koneksi")?;

    let ada_tabel = conn
        .query_one(
            "SELECT to_regclass('schema_migrations') IS NOT NULL AS ada",
            &[],
        )
        .await
        .context("periksa migrasi: deteksi schema_migrations")?
        .get::<_, bool>("ada");

    // Belum ada tabel catatan sama sekali = tak satu pun tercatat.
    if !ada_tabel {
        return Ok(MIGRATIONS.iter().map(|(n, _)| *n).collect());
    }

    let sudah: std::collections::HashSet<String> = conn
        .query("SELECT version FROM schema_migrations", &[])
        .await
        .context("periksa migrasi: baca schema_migrations")?
        .into_iter()
        .map(|r| r.try_get::<_, String>(0))
        .collect::<Result<_, _>>()
        .context("periksa migrasi: kolom `version` bukan teks")?;

    Ok(MIGRATIONS
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| !sudah.contains(*n))
        .collect())
}

/// Apakah daftar kolom itu bentuk `schema_migrations` milik modul ini?
///
/// Dipisah dari query supaya keputusannya bisa diuji tanpa database — dan ini
/// keputusan yang pantas diuji: terlalu longgar, kita menulis ke tabel milik
/// alat lain; terlalu ketat, deployment yang sehat menolak start.
fn bentuk_cocok(kolom: &[(String, String)]) -> bool {
    let teks = |nama: &str| {
        kolom
            .iter()
            .any(|(n, t)| n == nama && matches!(t.as_str(), "text" | "character varying"))
    };
    teks("version") && teks("checksum")
}

/// Pastikan `schema_migrations` yang ADA memang milik modul ini.
///
/// `CREATE TABLE IF NOT EXISTS` di atas tidak memeriksa apa pun: bila tabel
/// dengan nama itu sudah ada — dibuat alat migrasi lain, atau versi lama modul
/// ini — PostgreSQL hanya berkata `already exists, skipping` lalu melanjutkan
/// seolah semuanya beres. Kegagalannya baru muncul satu baris kemudian saat
/// kolomnya dibaca sebagai teks, dalam bentuk yang tak menyebut sebabnya sama
/// sekali.
///
/// Lebih baik berhenti di sini dan MENYEBUTKAN apa yang sebenarnya ada.
async fn periksa_bentuk(conn: &deadpool_postgres::Object) -> Result<()> {
    let kolom = conn
        .query(
            "SELECT column_name::text, data_type::text
               FROM information_schema.columns
              WHERE table_schema = current_schema()
                AND table_name   = 'schema_migrations'
              ORDER BY ordinal_position",
            &[],
        )
        .await
        .context("migrate: baca bentuk schema_migrations")?;

    let terbaca: Vec<(String, String)> = kolom
        .iter()
        .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1)))
        .collect();

    if bentuk_cocok(&terbaca) {
        return Ok(());
    }

    let terlihat: Vec<String> = terbaca
        .iter()
        .map(|(n, t)| format!("{n} {t}"))
        .collect();

    anyhow::bail!(
        "tabel `schema_migrations` sudah ada tetapi BUKAN milik modul ini.\n  \
         diharapkan : version TEXT, checksum TEXT\n  \
         yang ada   : {}\n  \
         Ia milik alat migrasi lain — mis. `ppm/scripts/migrate.sh` di repo sebelah \
         membuat `version integer, name text, checksum text`. Artinya DATABASE_URL \
         ini menunjuk database milik aplikasi LAIN, dan itu yang perlu dibetulkan \
         lebih dulu. Periksa dengan `\\d schema_migrations`, dan JANGAN menghapus \
         tabelnya sebelum yakin ia bukan catatan migrasi aplikasi itu.",
        if terlihat.is_empty() { "(tak ada kolom terbaca)".to_string() } else { terlihat.join(", ") }
    )
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

    // ── `metode`: DIJALANKAN di sini, atau DIANGGAP sudah jalan? ─────────────
    //
    // Tanpa perbedaan ini, dua keadaan yang perlakuannya berlawanan tampak
    // identik: (a) database lama yang migrasinya dikerjakan tangan lalu dicatat
    // sebagai garis dasar, dan (b) instalasi baru yang penjalannya baru sampai
    // separuh. Menganggap (b) sebagai (a) berarti mencatat sisanya tanpa pernah
    // menjalankannya — skema yang tak pernah lahir, dicap selesai.
    //
    // Baris lama bernilai NULL dan dibaca sebagai `baseline`: sebelum kolom ini
    // ada, satu-satunya yang mencatat tanpa menjalankan memang jalur baseline.
    conn.batch_execute("ALTER TABLE schema_migrations ADD COLUMN IF NOT EXISTS metode TEXT")
        .await
        .context("migrate: tambah kolom metode")?;

    periksa_bentuk(conn).await?;

    let baris = conn
        .query("SELECT version, checksum FROM schema_migrations", &[])
        .await
        .context("migrate: baca schema_migrations")?;

    // `try_get`, bukan `get`. `get` PANIK saat tipe kolom tak sesuai, dan panik
    // di sini muncul sebagai stack trace tokio-postgres yang tak menyebut satu
    // pun kata "migrasi" — pembacanya harus menebak sendiri bahwa masalahnya
    // ada di tabel catatan, bukan di kodenya.
    let mut sudah: std::collections::HashMap<String, String> =
        std::collections::HashMap::with_capacity(baris.len());
    for r in baris {
        // `Option<String>`, supaya NULL bisa dibedakan dari tipe yang salah.
        // Keduanya gagal di `String` polos dengan pesan yang sama persis, dan
        // penanganannya berbeda: yang satu baris rusak, yang lain tabel asing.
        let version: Option<String> = r
            .try_get(0)
            .context("migrate: kolom `version` di schema_migrations bukan teks")?;
        let checksum: Option<String> = r
            .try_get(1)
            .context("migrate: kolom `checksum` di schema_migrations bukan teks")?;
        match (version, checksum) {
            (Some(v), Some(c)) => {
                sudah.insert(v, c);
            }
            (v, _) => anyhow::bail!(
                "schema_migrations memuat baris dengan kolom NULL (version={:?}) — \
                 catatan migrasi rusak dan tak bisa dipercaya; perbaiki barisnya \
                 sebelum menjalankan ulang",
                v
            ),
        }
    }

    // Database yang sudah terpakai tapi belum pernah dicatat: pasang garis dasar.
    let baseline = baseline_terpakai()?;
    let diminta_operator = std::env::var("MIGRATE_BASELINE").is_ok();

    // ── KAPAN GARIS DASAR BOLEH DIPASANG ────────────────────────────────────
    //
    // Syarat lamanya adalah "catatan masih kosong", dan syarat itu terlalu
    // rapuh: percobaan yang gagal di tengah meninggalkan 001-021 tercatat, dan
    // sejak itu garis dasar TAK PERNAH BISA dipasang lagi — persis keadaan yang
    // butuh diperbaiki. Setiap start berikutnya mengulang kegagalan yang sama.
    //
    // Syarat yang benar adalah tentang APA yang tercatat, bukan berapa
    // banyaknya: selama penjalan ini belum pernah benar-benar MENJALANKAN satu
    // migrasi pun di sini, database ini masih database lama yang dikerjakan
    // tangan, dan garis dasarnya boleh dipasang atau diperluas. Begitu ada satu
    // saja yang berstatus `jalan`, database ini miliknya penjalan — mencatat
    // sisanya tanpa menjalankan akan melewatkan skema yang benar-benar kurang.
    let ada_yang_dijalankan = conn
        .query_one(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE metode = 'jalan') AS ada",
            &[],
        )
        .await
        .context("migrate: deteksi migrasi yang pernah dijalankan")?
        .get::<_, bool>("ada");

    let database_terpakai = conn
        .query_one("SELECT to_regclass('users') IS NOT NULL AS ada", &[])
        .await
        .context("migrate: deteksi database terpakai")?
        .get::<_, bool>("ada");

    let perlu_baseline = diminta_operator || (database_terpakai && !ada_yang_dijalankan);

    // SELALU dicatat, bukan hanya saat garis dasar dipasang. Tanpa baris ini,
    // `MIGRATE_BASELINE` yang tak sampai ke proses (lupa di-export, terhalang
    // pembungkus seperti `make`) tak bisa dibedakan dari yang sampai tapi tak
    // berpengaruh — dan yang terlihat cuma migrasi gagal yang sama persis.
    tracing::info!(
        baseline = %baseline,
        dari_env = diminta_operator,
        database_terpakai,
        ada_yang_dijalankan,
        pasang_baseline = perlu_baseline,
        "migrate: garis dasar yang dipakai"
    );

    if perlu_baseline {
        tracing::warn!(
            baseline = %baseline,
            diminta_operator,
            "migrate: berkas sampai baseline DICATAT tanpa dijalankan"
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

        if perlu_baseline && *nama <= baseline.as_str() {
            conn.execute(
                "INSERT INTO schema_migrations (version, checksum, metode)
                 VALUES ($1, $2, 'baseline')
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
            "INSERT INTO schema_migrations (version, checksum, metode)
             VALUES ($1, $2, 'jalan')",
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
        let daftar: Vec<&str> = MIGRATIONS.iter().map(|(n, _)| *n).collect();

        // Konstanta yang tak menunjuk berkas mana pun akan dibandingkan secara
        // leksikal dan mencatat sebagian migrasi tanpa suara.
        assert!(
            daftar.contains(&BASELINE),
            "BASELINE `{BASELINE}` harus salah satu berkas migrasi"
        );

        // Berkas yang tak aman diulang harus berada DI BAWAH garis dasar,
        // supaya ia dicatat, bukan dijalankan ulang.
        assert!("007_seed_bulk.sql" <= BASELINE);
        assert!("021_paid_at_semantics.sql" <= BASELINE);

        // 022 menyebut `event_variants`, yang sudah di-rename oleh 023. Selama
        // produksi sudah melewati 023 dengan tangan, 022 TIDAK boleh diputar
        // ulang — inilah kegagalan 8 Sep 2026.
        assert!(
            "022_cart_payment.sql" <= BASELINE,
            "022 harus dicatat, bukan dijalankan ulang"
        );
        assert!("023_products_rename.sql" <= BASELINE);
    }

    /// Tabel milik alat lain tak boleh dianggap milik kita — di situlah panik
    /// "error deserializing column 0" berasal: `CREATE TABLE IF NOT EXISTS`
    /// diam saja, lalu kolomnya dibaca sebagai teks.
    #[test]
    fn bentuk_asing_ditolak() {
        let kita = vec![
            ("version".into(), "text".into()),
            ("checksum".into(), "text".into()),
            ("applied_at".into(), "timestamp with time zone".into()),
        ];
        assert!(bentuk_cocok(&kita));

        // varchar juga sah — beberapa alat membuatnya begitu.
        let varchar = vec![
            ("version".into(), "character varying".into()),
            ("checksum".into(), "character varying".into()),
        ];
        assert!(bentuk_cocok(&varchar));

        // sqlx: version BIGINT, tak ada checksum bertipe teks.
        let sqlx = vec![
            ("version".into(), "bigint".into()),
            ("description".into(), "text".into()),
            ("checksum".into(), "bytea".into()),
        ];
        assert!(!bentuk_cocok(&sqlx), "version bigint bukan milik kita");

        // tabel ada tapi tanpa checksum sama sekali.
        let tanpa_checksum = vec![("version".into(), "text".into())];
        assert!(!bentuk_cocok(&tanpa_checksum));

        assert!(!bentuk_cocok(&[]), "tak ada kolom = bukan tabel kita");
    }

    /// Garis dasar yang salah ketik lebih berbahaya daripada yang ditolak:
    /// perbandingannya leksikal, jadi `038` (tanpa ekstensi) akan mencatat
    /// sebagian migrasi dan mendiamkan sisanya.
    #[test]
    fn baseline_harus_nama_berkas_utuh() {
        let daftar = ["021_paid_at_semantics.sql", "038_reviews_order_fk.sql"];
        assert!(validasi_baseline("038_reviews_order_fk.sql", &daftar).is_ok());
        assert!(validasi_baseline("038", &daftar).is_err(), "tanpa ekstensi harus ditolak");
        assert!(validasi_baseline("", &daftar).is_err());
        assert!(validasi_baseline("999_tak_ada.sql", &daftar).is_err());
    }

    /// Semua berkas yang ter-embed harus bisa dipakai sebagai baseline —
    /// kalau tidak, operator tak punya cara menyebut titik yang benar.
    #[test]
    fn tiap_migrasi_sah_jadi_baseline() {
        let daftar: Vec<&str> = MIGRATIONS.iter().map(|(n, _)| *n).collect();
        for n in &daftar {
            assert!(validasi_baseline(n, &daftar).is_ok(), "{n} harus sah");
        }
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

        // Nilai HARFIAH, bukan sekadar "stabil terhadap dirinya sendiri".
        // Prosedur pemulihan mencatat baris `schema_migrations` dari luar
        // (skrip yang menghitung FNV-1a sendiri); kalau algoritma di sini
        // bergeser, checksum lama tak lagi cocok dan setiap start akan
        // berteriak "berkas BERUBAH" untuk seluruh migrasi sekaligus.
        assert_eq!(checksum(a), "3dc6929e81eefda5");
    }
}
