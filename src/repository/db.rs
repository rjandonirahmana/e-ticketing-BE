use std::time::Duration;

use anyhow::{Context, Result};
use deadpool_postgres::Pool;
use tokio_postgres::Row;

// ── Pool helper ───────────────────────────────────────────────────────────────

pub async fn get_conn(pool: &Pool) -> Result<deadpool_postgres::Object> {
    match tokio::time::timeout(Duration::from_secs(5), pool.get()).await {
        Ok(Ok(conn)) => Ok(conn),
        Ok(Err(e)) => Err(e).context("Failed to get connection from pool"),
        Err(_) => anyhow::bail!("Timeout getting connection from pool (>5s)"),
    }
}

// ── Row accessor helpers ──────────────────────────────────────────────────────

pub fn col_opt_str(row: &Row, name: &str) -> Result<Option<String>> {
    row.try_get::<_, Option<String>>(name)
        .with_context(|| format!("Column '{}' not found or wrong type", name))
}

pub fn col_opt_f64(row: &Row, name: &str) -> Option<f64> {
    row.try_get::<_, Option<f64>>(name).ok().flatten()
}

pub fn col_opt_i32(row: &Row, name: &str) -> Option<i32> {
    row.try_get::<_, Option<i32>>(name).ok().flatten()
}

// ── Error context helpers untuk row_to_* ──────────────────────────────────────

pub fn get<T: for<'a> tokio_postgres::types::FromSql<'a>>(row: &Row, col: &str) -> Result<T> {
    row.try_get::<_, T>(col)
        .with_context(|| format!("Failed to read column '{}'", col))
}

pub fn get_opt<T: for<'a> tokio_postgres::types::FromSql<'a>>(
    row: &Row,
    col: &str,
) -> Result<Option<T>> {
    row.try_get::<_, Option<T>>(col)
        .with_context(|| format!("Failed to read column '{}'", col))
}

// ── Query helpers ─────────────────────────────────────────────────────────────

/// Format error dari tokio_postgres menjadi pesan yang mudah dibaca.
/// Menampilkan semua field yang tersedia: message, detail, hint, where, table, column, constraint.
fn format_pg_error(e: &tokio_postgres::Error, query: &str) -> anyhow::Error {
    // Cek apakah ini error serialisasi parameter (terjadi sebelum query dikirim ke DB)
    let err_str = e.to_string();
    if err_str.contains("error serializing parameter") {
        // Contoh: "error serializing parameter 11: ..."
        // Coba ekstrak nomor parameter
        let param_hint = err_str
            .split_whitespace()
            .collect::<Vec<_>>()
            .windows(3)
            .find(|w| w[0] == "parameter")
            .and_then(|w| w[1].parse::<usize>().ok())
            .map(|n| format!("\n  ❌ Parameter ${n} gagal di-serialize — cek tipe data Rust vs tipe kolom PostgreSQL"))
            .unwrap_or_default();

        return anyhow::anyhow!(
            "PARAMETER SERIALIZATION ERROR{param_hint}\n  query: {query}\n  cause: {err_str}"
        );
    }

    // Error dari PostgreSQL server — parse semua field yang tersedia
    if let Some(db) = e.as_db_error() {
        use tokio_postgres::error::SqlState;

        let kind = match db.code() {
            &SqlState::UNIQUE_VIOLATION => {
                "UNIQUE VIOLATION — nilai duplikat, data sudah ada".to_string()
            }
            &SqlState::NOT_NULL_VIOLATION => {
                "NOT NULL VIOLATION — kolom wajib diisi tapi nilainya NULL".to_string()
            }
            &SqlState::FOREIGN_KEY_VIOLATION => {
                "FOREIGN KEY VIOLATION — referensi ke data yang tidak ada".to_string()
            }
            &SqlState::CHECK_VIOLATION => {
                "CHECK CONSTRAINT VIOLATION — nilai tidak memenuhi constraint".to_string()
            }
            &SqlState::INVALID_TEXT_REPRESENTATION => {
                "INVALID TYPE — tipe data tidak sesuai (misal string ke integer)".to_string()
            }
            &SqlState::NUMERIC_VALUE_OUT_OF_RANGE => {
                "NUMERIC OUT OF RANGE — angka melebihi batas kolom".to_string()
            }
            &SqlState::STRING_DATA_RIGHT_TRUNCATION => {
                "STRING TOO LONG — teks melebihi panjang maksimum kolom".to_string()
            }
            other => format!("POSTGRES ERROR [{}]", other.code()),
        };

        let mut parts = vec![
            format!("  ❌ {kind}"),
            format!("  message   : {}", db.message()),
        ];
        if let Some(d) = db.detail() {
            parts.push(format!("  detail    : {d}"));
        }
        if let Some(h) = db.hint() {
            parts.push(format!("  hint      : {h}"));
        }
        if let Some(t) = db.table() {
            parts.push(format!("  table     : {t}"));
        }
        if let Some(c) = db.column() {
            parts.push(format!("  column    : {c}"));
        }
        if let Some(k) = db.constraint() {
            parts.push(format!("  constraint: {k}"));
        }
        if let Some(w) = db.where_() {
            parts.push(format!("  where     : {w}"));
        }
        parts.push(format!("  query     : {}", query.trim()));

        anyhow::anyhow!("{}", parts.join("\n"))
    } else {
        // Error lain (koneksi putus, timeout, dll)
        anyhow::anyhow!("DB ERROR: {err_str}\n  query: {}", query.trim())
    }
}

/// Run a query that returns no rows (INSERT/UPDATE/DELETE).
/// Returns the number of rows affected.
///
/// Statement di-prepare lewat cache per-koneksi deadpool (`prepare_cached`):
/// query panas yang sama tidak lagi membayar round-trip parse+describe setiap
/// panggilan — Postgres tinggal Bind+Execute statement yang sudah tersimpan.
pub async fn exec_drop(
    pool: &Pool,
    query: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<u64> {
    let conn = get_conn(pool).await?;
    let stmt = conn
        .prepare_cached(query)
        .await
        .map_err(|e| format_pg_error(&e, query))?;
    conn.execute(&stmt, params)
        .await
        .map_err(|e| format_pg_error(&e, query))
}

/// Run a query and return all rows. Pakai statement cache (lihat `exec_drop`).
pub async fn exec_rows(
    pool: &Pool,
    query: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<Vec<Row>> {
    let conn = get_conn(pool).await?;
    let stmt = conn
        .prepare_cached(query)
        .await
        .map_err(|e| format_pg_error(&e, query))?;
    conn.query(&stmt, params)
        .await
        .map_err(|e| format_pg_error(&e, query))
}

/// Run a query and return the first row, or `None` if empty.
pub async fn exec_first(
    pool: &Pool,
    query: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<Option<Row>> {
    let rows = exec_rows(pool, query, params).await?;
    Ok(rows.into_iter().next())
}

/// Run a query and return exactly one row — error if none.
pub async fn exec_one(
    pool: &Pool,
    query: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<Row> {
    exec_first(pool, query, params).await?.ok_or_else(|| {
        anyhow::anyhow!(
            "exec_one: expected one row but got zero\n  query: {}",
            query.trim()
        )
    })
}
