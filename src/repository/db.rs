// Re-usable DB helpers — kept available even when the current set of repos
// only exercises a subset (matches the helper bag in the example project).
#![allow(dead_code)]

use std::time::Duration;

use anyhow::{Context, Result};
use deadpool_postgres::Pool;
use tokio_postgres::Row;

// ── Pool helper ───────────────────────────────────────────────────────────────

pub async fn get_conn(pool: &Pool) -> Result<deadpool_postgres::Object> {
    match tokio::time::timeout(Duration::from_secs(5), pool.get()).await {
        Ok(Ok(conn)) => Ok(conn),
        Ok(Err(e)) => Err(e).context("Failed to get connection from pool"),
        Err(_) => anyhow::bail!("Timeout getting connection from pool"),
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

// ── Query helpers ─────────────────────────────────────────────────────────────

/// Run a query that returns no rows (INSERT without RETURNING / UPDATE / DELETE).
/// Returns the number of rows affected.
pub async fn exec_drop(
    pool: &Pool,
    query: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<u64> {
    let conn = get_conn(pool).await?;
    let n = conn
        .execute(query, params)
        .await
        .with_context(|| "exec_drop failed")?;
    Ok(n)
}

/// Run a query and return all rows.
pub async fn exec_rows(
    pool: &Pool,
    query: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<Vec<Row>> {
    let conn = get_conn(pool).await?;
    conn.query(query, params).await.map_err(|e| {
        tracing::error!(error = %e, query, params_count = params.len(), "exec_rows failed");
        anyhow::anyhow!("exec_rows failed: {e}")
    })
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
/// Use this for `INSERT ... RETURNING` or lookups guaranteed to exist.
pub async fn exec_one(
    pool: &Pool,
    query: &str,
    params: &[&(dyn tokio_postgres::types::ToSql + Sync)],
) -> Result<Row> {
    exec_first(pool, query, params)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Expected one row but got zero"))
}
