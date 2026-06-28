//! oversell_test.rs — Integration test invariant anti-oversell di bawah
//! konkurensi Postgres nyata.
//!
//! Guard asli ada di `repository::order::STMT_BUMP_SOLD`:
//!   `UPDATE event_variants SET sold = sold + qty
//!      WHERE id = $id AND (quota - sold) >= qty`
//! Update bersyarat atomik ini + row-lock Postgres adalah yang mencegah
//! penjualan melebihi kuota saat banyak pembeli bertarung pada varian sama.
//!
//! Test ini MEREPRODUKSI pola guard tsb. pada tabel test mandiri (bukan skema
//! aplikasi penuh) supaya:
//!   - bisa dijalankan hanya dengan satu Postgres, tanpa seed users/events,
//!   - tidak rapuh terhadap drift migrasi (kode pakai `event_variants`,
//!     `migration/001.sql` masih `ticket_variants`).
//! Jika SQL guard di `repository/order.rs` berubah, sinkronkan pola di sini.
//!
//! Butuh Postgres hidup; di-`#[ignore]`. Jalankan:
//!   TEST_DATABASE_URL=postgres://user:pass@127.0.0.1/db \
//!     cargo test --features ssr -- --ignored oversell

use tokio_postgres::{Client, NoTls};

async fn connect(url: &str) -> Client {
    let (client, conn) = tokio_postgres::connect(url, NoTls)
        .await
        .expect("connect Postgres (apakah TEST_DATABASE_URL benar & Postgres hidup?)");
    tokio::spawn(async move {
        let _ = conn.await;
    });
    client
}

/// 20 pembeli paralel, qty 1 masing-masing, kuota 5 → tepat 5 yang berhasil,
/// `sold` akhir = 5 dan TIDAK PERNAH melebihi `quota` (dijaga juga oleh CHECK).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "butuh Postgres; set TEST_DATABASE_URL lalu jalankan dengan --ignored"]
async fn oversell_guard_caps_at_quota_under_concurrency() {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("set TEST_DATABASE_URL untuk menjalankan test oversell");

    const QUOTA: i32 = 5;
    const BUYERS: i32 = 20;
    let tbl = format!("_oversell_test_{}", std::process::id());

    let setup = connect(&url).await;
    setup
        .batch_execute(&format!(
            "DROP TABLE IF EXISTS {tbl};
             CREATE TABLE {tbl} (
                 id    bytea PRIMARY KEY,
                 quota integer NOT NULL CHECK (quota >= 0),
                 sold  integer NOT NULL DEFAULT 0 CHECK (sold >= 0 AND sold <= quota)
             );
             INSERT INTO {tbl} (id, quota, sold) VALUES ('\\x01'::bytea, {QUOTA}, 0);"
        ))
        .await
        .expect("setup tabel test");

    // Tiap pembeli koneksi sendiri → konkurensi sungguhan (bukan serial).
    let mut handles = Vec::new();
    for _ in 0..BUYERS {
        let url = url.clone();
        let tbl = tbl.clone();
        handles.push(tokio::spawn(async move {
            let c = connect(&url).await;
            // Pola identik dengan STMT_BUMP_SOLD: update bersyarat atomik.
            let updated = c
                .execute(
                    &format!(
                        "UPDATE {tbl} SET sold = sold + 1 \
                         WHERE id = '\\x01'::bytea AND (quota - sold) >= 1"
                    ),
                    &[],
                )
                .await
                .expect("update guard");
            updated // 1 jika berhasil, 0 jika ditolak guard
        }));
    }

    let mut succeeded: u64 = 0;
    for h in handles {
        succeeded += h.await.expect("join task");
    }

    // Cek hasil akhir.
    let row = setup
        .query_one(
            &format!("SELECT sold FROM {tbl} WHERE id = '\\x01'::bytea"),
            &[],
        )
        .await
        .expect("baca sold");
    let sold: i32 = row.get(0);

    let _ = setup.batch_execute(&format!("DROP TABLE IF EXISTS {tbl}")).await;

    assert_eq!(succeeded, QUOTA as u64, "harus tepat {QUOTA} pembeli berhasil");
    assert_eq!(sold, QUOTA, "sold akhir harus = quota, tidak oversell");
    assert!(sold <= QUOTA, "sold tidak boleh melebihi quota");
}
