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
