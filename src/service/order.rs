use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{Duration, sleep, timeout};
use validator::Validate;

use deadpool_postgres::Pool;

use crate::models::orders::{
    CreateOrderRequest, Order, OrderDetailResponse, OrderItemResponse, PayOrderRequest,
};
use crate::repository::order::{
    ItemRow, LUA_RELEASE, LockedVariant, OrderRepository, OrderTx, OversellError,
};
use crate::utils::error::{AppError, AppResult};
use crate::utils::ulid::{id_to_vec, new_ulid, ulid_to_vec};

// ── Konstanta ─────────────────────────────────────────────────────────────────

/// TTL lock Redis. Sengaja lebih panjang dari TX_TIMEOUT * 2 karena:
/// - Heartbeat memperpanjang lock setiap TTL/3
/// - Kalau heartbeat miss 1 tick, lock masih hidup sampai DB TX selesai
/// - Jangan terlalu besar: lock zombie (crash tanpa release) makin lama nahan slot
const LOCK_TTL_MS: u64 = 25_000;

const LOCK_RETRIES: u8 = 3;
const LOCK_DELAY_MS: u64 = 80;
const MAX_TX_RETRY: u8 = 3;

/// Timeout maksimal untuk seluruh transaksi DB (lock + insert + commit).
/// Harus < LOCK_TTL_MS agar lock tidak expire sebelum TX selesai.
/// Dengan TTL=25s dan TX_TIMEOUT=12s ada margin ~13s untuk heartbeat drift.
const TX_TIMEOUT: Duration = Duration::from_secs(12);

/// Heartbeat interval = TTL / 3.
/// - Tick pertama di interval ke-1 (bukan langsung)
/// - Worst case: lock diperpanjang di ~8.3s, TX selesai di ~12s → aman
const HEARTBEAT_INTERVAL: Duration = Duration::from_millis(LOCK_TTL_MS / 3);

/// Heartbeat retry sebelum give up.
/// Kalau Redis error 1-2x karena spike, masih coba ulang.
/// Kalau 3x berturut-turut gagal → stop (Redis kemungkinan down).
const HEARTBEAT_MAX_RETRY: u8 = 3;

/// Lua script extend lock — identik dengan di repository.
const LUA_EXTEND: &str = r#"
if redis.call("get", KEYS[1]) == ARGV[1] then
    return redis.call("pexpire", KEYS[1], ARGV[2])
else
    return 0
end
"#;

// ── QueueMode ─────────────────────────────────────────────────────────────────

/// Mode queue per event. Toggle ini di-set saat event dibuat/diupdate.
///
/// - `Off`    → flow normal (default, cocok untuk event kecil–menengah)
/// - `Soft`   → Redis rate limit per user (light protection, event medium)
/// - `Strict` → full FIFO queue via Redis LIST (event besar / high-traffic)
///
/// Implementasi `Soft` dan `Strict` di-scope ke iterasi berikutnya.
/// Struct ini sudah ada agar caller tidak perlu refactor saat nanti diaktifkan.
#[derive(Debug, Clone, Default)]
pub enum QueueMode {
    #[default]
    Off,
    /// Per-user rate limit via Redis sliding window.
    /// Max N request per user per window T.
    #[allow(dead_code)]
    Soft { max_rps: u32, window_ms: u64 },
    /// Full FIFO: user masuk Redis LIST, worker pop secara urut.
    #[allow(dead_code)]
    Strict,
}

// ── VariantLockGuard ──────────────────────────────────────────────────────────

pub(crate) struct VariantLockGuard {
    redis: redis::aio::ConnectionManager,
    pub(crate) acquired_keys: Vec<String>,
    pub(crate) lock_val: String,
}

impl VariantLockGuard {
    pub async fn acquire(
        redis: redis::aio::ConnectionManager,
        variant_ids: &[&str],
    ) -> AppResult<Self> {
        let mut sorted: Vec<&str> = variant_ids.to_vec();
        // Sort untuk konsistensi urutan akuisisi → cegah deadlock antar request
        // yang memesan variant yang sama tapi urutan berbeda.
        // ULID string-lexicographic aman karena semua ULID same-length + same-charset.
        sorted.sort_unstable();
        sorted.dedup();

        let lock_val = new_ulid();
        let keys: Vec<String> = sorted
            .iter()
            .map(|id| format!("order:lock:variant:{}", id))
            .collect();

        let mut guard = Self {
            redis,
            acquired_keys: Vec::with_capacity(keys.len()),
            lock_val,
        };

        for key in &keys {
            let mut ok = false;
            for attempt in 0..=LOCK_RETRIES {
                let res: redis::RedisResult<Option<String>> = redis::cmd("SET")
                    .arg(key)
                    .arg(&guard.lock_val)
                    .arg("NX")
                    .arg("PX")
                    .arg(LOCK_TTL_MS)
                    .query_async(&mut guard.redis)
                    .await;

                match res {
                    Ok(Some(_)) => {
                        ok = true;
                        break;
                    }
                    Ok(None) if attempt < LOCK_RETRIES => {
                        sleep(Duration::from_millis(LOCK_DELAY_MS)).await;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        guard.release().await;
                        return Err(AppError::Internal(anyhow::anyhow!("Redis lock error: {e}")));
                    }
                }
            }

            if !ok {
                guard.release().await;
                return Err(AppError::Conflict(
                    "Tiket sedang dipesan pengguna lain, coba lagi sebentar".into(),
                ));
            }
            guard.acquired_keys.push(key.clone());
        }

        Ok(guard)
    }

    pub async fn release(&mut self) {
        let script = redis::Script::new(LUA_RELEASE);
        for key in self.acquired_keys.drain(..) {
            let _ = script
                .key(&key)
                .arg(&self.lock_val)
                .invoke_async::<i64>(&mut self.redis)
                .await;
        }
    }
}

impl Drop for VariantLockGuard {
    fn drop(&mut self) {
        if self.acquired_keys.is_empty() {
            return;
        }
        let keys = std::mem::take(&mut self.acquired_keys);
        let lock_val = self.lock_val.clone();
        let mut redis = self.redis.clone();

        tokio::spawn(async move {
            let script = redis::Script::new(LUA_RELEASE);
            for key in &keys {
                let _ = script
                    .key(key)
                    .arg(&lock_val)
                    .invoke_async::<i64>(&mut redis)
                    .await;
            }
        });
    }
}

// ── LockHeartbeat ─────────────────────────────────────────────────────────────

struct LockHeartbeat {
    handle: tokio::task::JoinHandle<()>,
}

impl LockHeartbeat {
    fn start(
        mut redis: redis::aio::ConnectionManager,
        keys: Vec<String>,
        lock_val: String,
    ) -> Self {
        let ttl_ms_str = LOCK_TTL_MS.to_string();

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
            ticker.tick().await; // skip tick pertama (langsung)

            loop {
                ticker.tick().await;

                for key in &keys {
                    // Retry loop per-key: kalau Redis error sementara,
                    // coba ulang sebelum give up.
                    // Mindset: DB = source of truth, Redis lock = optimistic gate.
                    // Kalau heartbeat gagal permanen → lock expire → request lain
                    // bisa masuk → DB guard (bump_sold_batch + quota check) yang
                    // mencegah oversell sebagai last line of defense.
                    let mut retry = 0u8;
                    loop {
                        let res: redis::RedisResult<i64> = redis::Script::new(LUA_EXTEND)
                            .key(key)
                            .arg(&lock_val)
                            .arg(&ttl_ms_str)
                            .invoke_async(&mut redis)
                            .await;

                        match res {
                            Ok(0) => {
                                // Lock sudah tidak milik kita (expired atau diambil lain)
                                // → stop heartbeat, jangan extend lock orang lain
                                tracing::warn!(
                                    key = %key,
                                    "heartbeat: lock expired or stolen, stopping"
                                );
                                return;
                            }
                            Ok(_) => break, // extended, lanjut ke key berikutnya

                            Err(e) => {
                                retry += 1;
                                if retry >= HEARTBEAT_MAX_RETRY {
                                    // Redis error persisten → give up
                                    // DB guard masih aktif sebagai safety net
                                    tracing::error!(
                                        key = %key,
                                        error = %e,
                                        "heartbeat: redis error after {} retries, giving up",
                                        HEARTBEAT_MAX_RETRY
                                    );
                                    return;
                                }
                                tracing::warn!(
                                    key = %key,
                                    error = %e,
                                    attempt = retry,
                                    "heartbeat: redis error, retrying"
                                );
                                sleep(Duration::from_millis(50)).await;
                            }
                        }
                    }
                }
            }
        });

        Self { handle }
    }

    fn stop(self) {
        self.handle.abort();
    }
}

// ── Metrics ───────────────────────────────────────────────────────────────────

/// Observability counter sederhana — drop-in, tidak butuh library metrics besar.
///
/// Di production, ganti dengan prometheus counter atau opentelemetry.
/// Untuk sekarang, semua metric di-emit via tracing event sehingga bisa
/// di-aggregate oleh log aggregator (Loki, CloudWatch, Datadog, dll).
struct OrderMetrics;

impl OrderMetrics {
    fn lock_acquired(variant_count: usize) {
        tracing::debug!(variant_count, event = "lock_acquired");
    }

    fn lock_conflict() {
        tracing::warn!(event = "lock_conflict");
    }

    fn tx_retry(attempt: u8, reason: &str) {
        tracing::warn!(attempt, reason, event = "tx_retry");
    }

    fn tx_timeout() {
        tracing::error!(event = "tx_timeout", timeout_secs = TX_TIMEOUT.as_secs());
    }

    fn oversell_rejected(variant_id: &str) {
        tracing::warn!(variant_id, event = "oversell_rejected");
    }

    fn idempotency_conflict(customer_id: &str) {
        tracing::info!(customer_id, event = "idempotency_conflict");
    }

    fn order_created(order_id: &str, total: f64, item_count: usize) {
        tracing::info!(order_id, total, item_count, event = "order_created");
    }

    fn order_paid(order_id: &str, payment_method: &str) {
        tracing::info!(order_id, payment_method, event = "order_paid");
    }

    fn order_cancelled(order_id: &str) {
        tracing::info!(order_id, event = "order_cancelled");
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

fn is_retryable_pg_error(e: &anyhow::Error) -> bool {
    if let Some(pg_err) = e.downcast_ref::<tokio_postgres::Error>() {
        if let Some(db_err) = pg_err.as_db_error() {
            let code = db_err.code().code();
            // 40001 = serialization_failure
            // 40P01 = deadlock_detected
            return code == "40001" || code == "40P01";
        }
    }
    false
}

#[inline]
fn unique_variant_count(items: &[crate::models::orders::CreateOrderItemRequest]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for item in items {
        seen.insert(&item.ticket_variant_id);
    }
    seen.len()
}

// ── OrderService ──────────────────────────────────────────────────────────────

pub struct OrderService {
    repo: Arc<dyn OrderRepository>,
    redis: redis::aio::ConnectionManager,
    pool: Pool,
}

impl OrderService {
    pub fn new(
        repo: Arc<dyn OrderRepository>,
        redis: redis::aio::ConnectionManager,
        pool: Pool,
    ) -> Self {
        Self { repo, redis, pool }
    }

    // ── Create ────────────────────────────────────────────────────────────────

    pub async fn create(
        &self,
        customer_id: &str,
        req: CreateOrderRequest,
    ) -> AppResult<OrderDetailResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        let variant_ids: Vec<&str> = {
            let mut ids: Vec<&str> = req
                .items
                .iter()
                .map(|i| i.ticket_variant_id.as_str())
                .collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        };

        // Akuisisi Redis lock — fast fail kalau variant sedang dipesan lain
        let mut lock = VariantLockGuard::acquire(self.redis.clone(), &variant_ids)
            .await
            .map_err(|e| {
                if matches!(e, AppError::Conflict(_)) {
                    OrderMetrics::lock_conflict();
                }
                e
            })?;

        OrderMetrics::lock_acquired(variant_ids.len());

        // Heartbeat memperpanjang lock setiap HEARTBEAT_INTERVAL
        // agar tidak expire selama TX berjalan
        let heartbeat = LockHeartbeat::start(
            self.redis.clone(),
            lock.acquired_keys.clone(),
            lock.lock_val.clone(),
        );

        let mut result: AppResult<OrderDetailResponse> =
            Err(AppError::Internal(anyhow::anyhow!("unreachable")));
        let mut last_retryable: Option<String> = None;

        for attempt in 0..MAX_TX_RETRY {
            let tx_result = timeout(TX_TIMEOUT, self.create_in_tx(customer_id, &req)).await;

            match tx_result {
                Ok(Ok(v)) => {
                    result = Ok(v);
                    last_retryable = None;
                    break;
                }
                Ok(Err(AppError::Internal(ref e))) if is_retryable_pg_error(e) => {
                    let reason = format!("{e}");
                    OrderMetrics::tx_retry(attempt, &reason);
                    last_retryable = Some(format!("attempt {attempt}: {e}"));
                    sleep(Duration::from_millis(20 * (attempt as u64 + 1))).await;
                    continue;
                }
                Ok(Err(e)) => {
                    result = Err(e);
                    break;
                }
                Err(_elapsed) => {
                    OrderMetrics::tx_timeout();
                    result = Err(AppError::Internal(anyhow::anyhow!(
                        "transaksi timeout setelah {}s",
                        TX_TIMEOUT.as_secs()
                    )));
                    break;
                }
            }
        }

        if let Some(msg) = last_retryable {
            result = Err(AppError::Internal(anyhow::anyhow!(
                "DB conflict setelah {} retry: {}",
                MAX_TX_RETRY,
                msg
            )));
        }

        // Selalu stop heartbeat + release lock, bahkan kalau TX gagal
        heartbeat.stop();
        lock.release().await;

        result
    }

    async fn create_in_tx(
        &self,
        customer_id: &str,
        req: &CreateOrderRequest,
    ) -> AppResult<OrderDetailResponse> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("pool: {e}")))?;
        let tx = conn
            .transaction()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("begin tx: {e}")))?;

        // ─ 1. Kumpulkan variant_id bytes ─────────────────────────────────────
        let id_bytes_list: Vec<Vec<u8>> = {
            let mut seen = std::collections::HashSet::new();
            let mut out = Vec::new();
            for item in &req.items {
                if seen.insert(&item.ticket_variant_id) {
                    out.push(
                        id_to_vec(&item.ticket_variant_id)
                            .map_err(|e| AppError::BadRequest(e.to_string()))?,
                    );
                }
            }
            out
        };

        // ─ 2. SELECT FOR UPDATE ───────────────────────────────────────────────
        let variants = OrderTx::lock_variants(&tx, &id_bytes_list)
            .await
            .map_err(AppError::Internal)?;

        let variant_map: HashMap<&str, &LockedVariant> =
            variants.iter().map(|v| (v.ulid.as_str(), v)).collect();

        // ─ 3. Aggregate qty per variant ───────────────────────────────────────
        let mut total_qty_per_variant: HashMap<&str, i32> =
            HashMap::with_capacity(unique_variant_count(&req.items));
        for item in &req.items {
            *total_qty_per_variant
                .entry(item.ticket_variant_id.as_str())
                .or_insert(0) += item.quantity;
        }

        // ─ 4. Validasi stok, is_active, max_per_order ────────────────────────
        for (vid, &total_qty) in &total_qty_per_variant {
            let v = variant_map
                .get(vid)
                .ok_or_else(|| AppError::NotFound(format!("Variant '{vid}' tidak ditemukan")))?;

            if !v.is_active {
                return Err(AppError::BadRequest(format!(
                    "Variant '{}' tidak aktif",
                    v.variant_name
                )));
            }
            if let Some(max) = v.max_per_order {
                if total_qty > max {
                    return Err(AppError::BadRequest(format!(
                        "Variant '{}' maksimal {} tiket per order",
                        v.variant_name, max
                    )));
                }
            }
            let available = v.quota - v.sold;
            if total_qty > available {
                return Err(AppError::BadRequest(format!(
                    "Stok '{}' tidak cukup: diminta {}, tersedia {}",
                    v.variant_name, total_qty, available
                )));
            }
        }

        // ─ 5. Bangun item_rows + hitung grand total ───────────────────────────
        let order_id = new_ulid();
        let order_id_bytes = ulid_to_vec(&order_id).map_err(AppError::Internal)?;
        let mut grand_total: f64 = 0.0;
        let mut item_rows: Vec<ItemRow> = Vec::with_capacity(req.items.len());

        for item in &req.items {
            let v = variant_map[item.ticket_variant_id.as_str()];
            let unit_price = v.effective_price;
            let subtotal = unit_price * item.quantity as f64;
            grand_total += subtotal;

            let oi_id = new_ulid();
            item_rows.push(ItemRow {
                oi_bytes: ulid_to_vec(&oi_id).map_err(AppError::Internal)?,
                oi_id,
                var_bytes: v.id_bytes.clone(),
                qty: item.quantity,
                unit_price,
                subtotal,
            });
        }

        // ─ 6. INSERT order — atomic idempotency CTE ───────────────────────────
        let customer_bytes = id_to_vec(customer_id).map_err(AppError::Internal)?;
        let order_code = {
            let suffix = &order_id[order_id.len().saturating_sub(8)..];
            format!("KN{}", suffix.to_uppercase())
        };
        let expired_at = chrono::Utc::now() + chrono::Duration::hours(2);

        // insert_order sekarang mengembalikan (Order, is_new: bool)
        // is_new = false → idempotency conflict → order existing dikembalikan
        let (order, is_new) = OrderTx::insert_order(
            &tx,
            &order_id_bytes,
            &customer_bytes,
            &order_code,
            grand_total,
            expired_at,
            req.idempotency_key.as_deref(),
        )
        .await
        .map_err(AppError::Internal)?;

        // Idempotency conflict: kembalikan order existing tanpa insert ulang
        if !is_new {
            OrderMetrics::idempotency_conflict(customer_id);

            // Rollback agar tidak ada side effect dari TX ini
            tx.rollback().await.ok();

            // Fetch items dari DB untuk response lengkap
            let items = self.repo.list_items(&order.id).await?;
            return Ok(OrderDetailResponse {
                id: order.id,
                customer_id: order.customer_id,
                order_code: order.order_code,
                status: order.status,
                total_amount: order.total_amount,
                payment_method: order.payment_method,
                paid_at: order.paid_at,
                expired_at: order.expired_at,
                created_at: order.created_at,
                items,
            });
        }

        // ─ 7. Batch INSERT order_items ────────────────────────────────────────
        OrderTx::insert_order_items_batch(&tx, &order_id_bytes, &item_rows)
            .await
            .map_err(AppError::Internal)?;

        // ─ 8. Batch UPDATE sold — dengan DB-level oversell guard ──────────────
        //
        // bump_sold_batch sekarang:
        // 1. Pre-aggregate qty per variant (cegah double counting dalam batch)
        // 2. Guard `(quota - sold) >= total_qty` atomik di DB
        // 3. Return OversellError kalau rows_updated != expected
        //
        // Ini adalah last line of defense — Redis lock + validasi di step 4
        // sudah menangkap mayoritas kasus, tapi guard DB tetap wajib untuk
        // skenario: lock expire, heartbeat miss, network partition, bug.
        let bump: Vec<(Vec<u8>, i32)> = item_rows
            .iter()
            .map(|row| (row.var_bytes.clone(), row.qty))
            .collect();

        OrderTx::bump_sold_batch(&tx, &bump).await.map_err(|e| {
            // Bedakan oversell (business error) vs DB error (internal)
            if let Some(oe) = e.downcast_ref::<OversellError>() {
                // Log variant yang gagal untuk debugging
                let failed_variant = item_rows
                    .iter()
                    .find(|r| {
                        // Heuristik: kalau updated < expected, variant pertama
                        // di batch adalah kandidat terkuat (order dalam bump)
                        true
                    })
                    .map(|r| {
                        // Decode bytes kembali ke ULID untuk log
                        crate::utils::ulid::bin_to_ulid(r.var_bytes.clone())
                            .unwrap_or_else(|_| "unknown".into())
                    })
                    .unwrap_or_else(|| "unknown".into());

                OrderMetrics::oversell_rejected(&failed_variant);

                tracing::error!(
                    updated = oe.updated,
                    expected = oe.expected,
                    "bump_sold_batch: oversell guard triggered"
                );

                AppError::BadRequest("Stok habis saat proses, coba lagi".into())
            } else {
                AppError::Internal(e)
            }
        })?;

        // ─ Commit ─────────────────────────────────────────────────────────────
        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit: {e}")))?;

        OrderMetrics::order_created(&order.id, grand_total, item_rows.len());

        // ─ Susun response dari in-memory (tanpa query tambahan) ───────────────
        let items: Vec<OrderItemResponse> = req
            .items
            .iter()
            .zip(item_rows.iter())
            .map(|(req_item, row)| {
                let v = variant_map[req_item.ticket_variant_id.as_str()];
                OrderItemResponse {
                    id: row.oi_id.clone(),
                    ticket_variant_id: v.ulid.clone(),
                    variant_name: v.variant_name.clone(),
                    event_id: v.event_id.clone(),
                    event_name: v.event_name.clone(),
                    quantity: row.qty,
                    unit_price: row.unit_price,
                    subtotal: row.subtotal,
                }
            })
            .collect();

        Ok(OrderDetailResponse {
            id: order.id,
            customer_id: order.customer_id,
            order_code: order.order_code,
            status: order.status,
            total_amount: order.total_amount,
            payment_method: order.payment_method,
            paid_at: order.paid_at,
            expired_at: order.expired_at,
            created_at: order.created_at,
            items,
        })
    }

    // ── Detail ────────────────────────────────────────────────────────────────

    pub async fn detail(&self, order_id: &str, viewer_id: &str) -> AppResult<OrderDetailResponse> {
        let order = self
            .repo
            .find_by_id(order_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Order not found".into()))?;

        if order.customer_id != viewer_id {
            return Err(AppError::Forbidden("Not your order".into()));
        }

        let items = self.repo.list_items(order_id).await?;
        Ok(OrderDetailResponse {
            id: order.id,
            customer_id: order.customer_id,
            order_code: order.order_code,
            status: order.status,
            total_amount: order.total_amount,
            payment_method: order.payment_method,
            paid_at: order.paid_at,
            expired_at: order.expired_at,
            created_at: order.created_at,
            items,
        })
    }

    // ── List ──────────────────────────────────────────────────────────────────

    pub async fn list_mine(
        &self,
        customer_id: &str,
        page: i64,
        per_page: i64,
    ) -> AppResult<Vec<Order>> {
        let page = page.max(1);
        let per_page = per_page.clamp(1, 100);
        let offset = (page - 1) * per_page;
        Ok(self
            .repo
            .list_for_customer(customer_id, per_page, offset)
            .await?)
    }

    // ── Pay ───────────────────────────────────────────────────────────────────

    pub async fn pay(
        &self,
        order_id: &str,
        viewer_id: &str,
        req: PayOrderRequest,
    ) -> AppResult<OrderDetailResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        let order = self
            .repo
            .find_by_id(order_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Order not found".into()))?;

        if order.customer_id != viewer_id {
            return Err(AppError::Forbidden("Not your order".into()));
        }
        if order.status != "pending" {
            return Err(AppError::BadRequest(
                "Order sudah dibayar atau dibatalkan".into(),
            ));
        }
        if order.expired_at.map_or(false, |e| chrono::Utc::now() > e) {
            return Err(AppError::BadRequest("Order sudah expired".into()));
        }

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("{e}")))?;
        let tx = conn
            .transaction()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("{e}")))?;

        let order_bytes = id_to_vec(order_id).map_err(AppError::Internal)?;

        // mark_paid sudah ada guard: WHERE status='pending' AND expired_at > NOW()
        // Kalau race antara dua request pay yang sama → hanya satu yang menang
        let updated = OrderTx::mark_paid(&tx, &order_bytes, &req.payment_method)
            .await
            .map_err(AppError::Internal)?;

        if updated == 0 {
            return Err(AppError::BadRequest(
                "Order tidak bisa dibayar (sudah dibayar, dibatalkan, atau expired)".into(),
            ));
        }

        let items = OrderTx::fetch_items_for_order(&tx, &order_bytes)
            .await
            .map_err(AppError::Internal)?;

        OrderTx::mint_tickets_batch(&tx, &items)
            .await
            .map_err(AppError::Internal)?;

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit pay: {e}")))?;

        OrderMetrics::order_paid(order_id, &req.payment_method);

        self.detail(order_id, viewer_id).await
    }

    // ── Cancel ────────────────────────────────────────────────────────────────

    pub async fn cancel(&self, order_id: &str, viewer_id: &str) -> AppResult<()> {
        let order = self
            .repo
            .find_by_id(order_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Order not found".into()))?;

        if order.customer_id != viewer_id {
            return Err(AppError::Forbidden("Not your order".into()));
        }
        if order.status != "pending" {
            return Err(AppError::BadRequest(
                "Hanya order pending yang bisa dibatalkan".into(),
            ));
        }

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("{e}")))?;
        let tx = conn
            .transaction()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("{e}")))?;

        let order_bytes = id_to_vec(order_id).map_err(AppError::Internal)?;

        // Fetch dulu sebelum cancel agar refund tahu qty per variant.
        // Note: cancel_order sudah guard WHERE status='pending' →
        // race antara cancel vs pay aman: hanya satu yang menang.
        // SELECT FOR UPDATE tidak diperlukan untuk flow ini karena
        // kedua operasi melakukan UPDATE bukan read-then-branch.
        // Kalau di masa depan ada logic tambahan (refund external, audit),
        // tambahkan SELECT ... FOR UPDATE di sini.
        let items = OrderTx::fetch_items_for_refund(&tx, &order_bytes)
            .await
            .map_err(AppError::Internal)?;

        let n = OrderTx::cancel_order(&tx, &order_bytes)
            .await
            .map_err(AppError::Internal)?;

        if n == 0 {
            return Err(AppError::BadRequest("Order tidak bisa dibatalkan".into()));
        }

        OrderTx::refund_sold_batch(&tx, &items)
            .await
            .map_err(AppError::Internal)?;

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit cancel: {e}")))?;

        OrderMetrics::order_cancelled(order_id);

        Ok(())
    }
}
