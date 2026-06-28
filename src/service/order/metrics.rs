use rust_decimal::Decimal;
use tokio::time::Duration;

pub(super) const MAX_TX_RETRY: u8 = 3;

/// TX_TIMEOUT harus < LOCK_TTL_MS (25s).
pub(super) const TX_TIMEOUT: Duration = Duration::from_secs(12);

pub(super) struct OrderMetrics;

impl OrderMetrics {
    pub fn lock_acquired(variant_count: usize) {
        tracing::debug!(variant_count, event = "lock_acquired");
    }
    pub fn lock_conflict() {
        tracing::warn!(event = "lock_conflict");
    }
    pub fn tx_retry(attempt: u8, reason: &str) {
        tracing::warn!(attempt, reason, event = "tx_retry");
    }
    pub fn tx_timeout() {
        tracing::error!(event = "tx_timeout", timeout_secs = TX_TIMEOUT.as_secs());
    }
    pub fn oversell_rejected(variant_ids: &[String]) {
        tracing::warn!(variants = ?variant_ids, event = "oversell_rejected");
    }
    pub fn idempotency_conflict(customer_id: &str) {
        tracing::info!(customer_id, event = "idempotency_conflict");
    }
    pub fn order_created(order_id: &str, total: Decimal, item_count: usize) {
        tracing::info!(order_id, total = %total, item_count, event = "order_created");
    }
    pub fn order_paid(order_id: &str, payment_method: &str) {
        tracing::info!(order_id, payment_method, event = "order_paid");
    }
    pub fn order_cancelled(order_id: &str) {
        tracing::info!(order_id, event = "order_cancelled");
    }
}

pub(super) fn is_retryable_pg_error(e: &anyhow::Error) -> bool {
    if let Some(pg_err) = e.downcast_ref::<tokio_postgres::Error>() {
        if let Some(db_err) = pg_err.as_db_error() {
            let code = db_err.code().code();
            return code == "40001" || code == "40P01";
        }
    }
    false
}

/// Jitter tanpa external rand dependency.
pub(super) fn backoff_with_jitter(attempt: u8) -> Duration {
    let base_ms = 20 * (attempt as u64 + 1);
    let jitter_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64
        % (base_ms + 1);
    Duration::from_millis(base_ms + jitter_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Backoff selalu dalam rentang [base, 2*base] di mana base = 20*(attempt+1).
    /// Jitter = nilai 0..=base, jadi tidak pernah 0 mutlak (minimal `base`) dan
    /// tidak pernah melebihi 2*base — penting agar retry tidak meledak.
    #[test]
    fn backoff_within_bounds() {
        for attempt in 0..=MAX_TX_RETRY {
            let base = 20 * (attempt as u64 + 1);
            // Ambil beberapa sample karena jitter berasal dari jam.
            for _ in 0..50 {
                let d = backoff_with_jitter(attempt).as_millis() as u64;
                assert!(d >= base, "attempt {attempt}: {d} < base {base}");
                assert!(d <= 2 * base, "attempt {attempt}: {d} > 2*base {}", 2 * base);
            }
        }
    }

    /// Error non-Postgres (mis. anyhow biasa) tidak boleh dianggap retryable —
    /// kalau iya, order service bisa retry selamanya pada error fatal.
    #[test]
    fn non_pg_error_is_not_retryable() {
        let e = anyhow::anyhow!("kesalahan acak bukan dari Postgres");
        assert!(!is_retryable_pg_error(&e));
    }
}
