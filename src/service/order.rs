use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{sleep, timeout, Duration};
use validator::Validate;

use deadpool_postgres::Pool;
use rust_decimal::Decimal;

use crate::models::orders::{
    CreateOrderRequest, Order, OrderDetailResponse, OrderItemResponse, PayOrderRequest,
};
use crate::repository::order::{
    ItemRow, LockedVariant, OrderRepository, OrderTx, OversellError, LUA_RELEASE,
};
use crate::service::norifications::NotificationService;
use crate::utils::error::{AppError, AppResult};
use crate::utils::ulid::{bin_to_ulid_ref, id_to_vec, new_ulid, ulid_to_vec};

// ── Konstanta ─────────────────────────────────────────────────────────────────

const LOCK_TTL_MS: u64 = 25_000;
const LOCK_RETRIES: u8 = 3;
const LOCK_DELAY_MS: u64 = 80;
const MAX_TX_RETRY: u8 = 3;

/// TX_TIMEOUT harus < LOCK_TTL_MS.
/// Margin 13 detik (25s TTL − 12s timeout) sudah aman — heartbeat tidak diperlukan.
const TX_TIMEOUT: Duration = Duration::from_secs(12);

// ── QueueMode ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub enum QueueMode {
    #[default]
    Off,
    #[allow(dead_code)]
    Soft { max_rps: u32, window_ms: u64 },
    #[allow(dead_code)]
    Strict,
}

// ── VariantLockGuard ──────────────────────────────────────────────────────────

pub(crate) struct VariantLockGuard {
    redis: redis::aio::ConnectionManager,
    /// Arc sehingga ownership bisa di-share tanpa clone Vec<String>.
    pub(crate) acquired_keys: Arc<Vec<String>>,
    pub(crate) lock_val: Arc<str>,
}

impl VariantLockGuard {
    pub async fn acquire(
        redis: redis::aio::ConnectionManager,
        variant_ids: &[&str],
    ) -> AppResult<Self> {
        let mut sorted: Vec<&str> = variant_ids.to_vec();
        // Sort untuk konsistensi urutan akuisisi → cegah deadlock.
        sorted.sort_unstable();
        sorted.dedup();

        let lock_val: Arc<str> = Arc::from(new_ulid().as_str());
        let keys: Vec<String> = sorted
            .iter()
            .map(|id| format!("order:lock:variant:{}", id))
            .collect();

        let mut acquired: Vec<String> = Vec::with_capacity(keys.len());
        let mut redis_conn = redis;

        for key in &keys {
            let mut ok = false;
            for attempt in 0..=LOCK_RETRIES {
                let res: redis::RedisResult<Option<String>> = redis::cmd("SET")
                    .arg(key)
                    .arg(lock_val.as_ref())
                    .arg("NX")
                    .arg("PX")
                    .arg(LOCK_TTL_MS)
                    .query_async(&mut redis_conn)
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
                        release_keys(&mut redis_conn, &acquired, &lock_val).await;
                        return Err(AppError::Internal(anyhow::anyhow!("Redis lock error: {e}")));
                    }
                }
            }

            if !ok {
                release_keys(&mut redis_conn, &acquired, &lock_val).await;
                return Err(AppError::Conflict(
                    "Tiket sedang dipesan pengguna lain, coba lagi sebentar".into(),
                ));
            }
            acquired.push(key.clone());
        }

        Ok(Self {
            redis: redis_conn,
            acquired_keys: Arc::new(acquired),
            lock_val,
        })
    }

    pub async fn release(&mut self) {
        if self.acquired_keys.is_empty() {
            return;
        }
        release_keys(&mut self.redis, &self.acquired_keys, &self.lock_val).await;
        self.acquired_keys = Arc::new(Vec::new());
    }
}

/// Drop hanya log — tidak spawn task.
/// Redis TTL (PX 25_000) adalah safety net jika release() terlewat.
impl Drop for VariantLockGuard {
    fn drop(&mut self) {
        if !self.acquired_keys.is_empty() {
            tracing::error!(
                keys = ?self.acquired_keys.as_ref(),
                "VariantLockGuard dropped tanpa release! Lock expire via Redis TTL."
            );
        }
    }
}

async fn release_keys(redis: &mut redis::aio::ConnectionManager, keys: &[String], lock_val: &str) {
    let script = redis::Script::new(LUA_RELEASE);
    for key in keys {
        let _ = script
            .key(key.as_str())
            .arg(lock_val)
            .invoke_async::<i64>(redis)
            .await;
    }
}

// ── HEARTBEAT DIHAPUS ─────────────────────────────────────────────────────────
//
// RACE CONDITION di versi sebelumnya:
//
//   Scenario:
//     [A] main:      heartbeat.stop()   → kirim cancel via oneshot
//     [B] main:      lock.release()     → DEL key di Redis
//     [C] heartbeat: pipeline PEXPIRE sudah dikirim ke Redis, menunggu response
//     [D] heartbeat: response tiba → key di-extend (PEXPIRE pada key yang baru di-DEL
//                    akan me-recreate key jika Redis versi < 7, atau tidak re-extend
//                    pada Redis 7+ tapi MASIH berpotensi window kecil)
//
//   Hasil: lock "bangkit dari mati" dengan TTL 25s → lock zombie.
//   Variant tidak bisa dipesan siapapun selama 25s. Throughput anjlok.
//
// SOLUSI: Hapus heartbeat sepenuhnya.
//
// JUSTIFIKASI:
//   - TX_TIMEOUT = 12s, LOCK_TTL = 25s → margin 13 detik.
//   - Selama tidak ada blocking I/O di luar timeout(), lock tidak akan expire.
//   - Heartbeat menambah kompleksitas, 1 Redis RTT ekstra, dan race condition.
//   - TTL Redis adalah safety net yang cukup untuk crash/hang scenario.

// ── Metrics ───────────────────────────────────────────────────────────────────

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
    fn oversell_rejected(variant_ids: &[String]) {
        tracing::warn!(variants = ?variant_ids, event = "oversell_rejected");
    }
    fn idempotency_conflict(customer_id: &str) {
        tracing::info!(customer_id, event = "idempotency_conflict");
    }
    fn order_created(order_id: &str, total: Decimal, item_count: usize) {
        tracing::info!(order_id, total = %total, item_count, event = "order_created");
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
            // 40001 = serialization_failure, 40P01 = deadlock_detected
            return code == "40001" || code == "40P01";
        }
    }
    false
}

/// FIX [P2-8]: Jitter tanpa external rand dependency.
/// subsec_nanos() menghasilkan nilai 0..1_000_000_000 yang cukup acak untuk
/// mencegah thundering herd. Tidak memerlukan tambahan entry di Cargo.toml.
fn backoff_with_jitter(attempt: u8) -> Duration {
    let base_ms = 20 * (attempt as u64 + 1);
    let jitter_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64
        % (base_ms + 1);
    Duration::from_millis(base_ms + jitter_ms)
}

// ── OrderService ──────────────────────────────────────────────────────────────

pub struct OrderService {
    repo: Arc<dyn OrderRepository>,
    redis: redis::aio::ConnectionManager,
    pool: Pool,
    notifier: Arc<NotificationService>, // 🔥 tambah
}

impl OrderService {
    pub fn new(
        repo: Arc<dyn OrderRepository>,
        redis: redis::aio::ConnectionManager,
        pool: Pool,
        notifier: Arc<NotificationService>,
    ) -> Self {
        Self {
            repo,
            redis,
            pool,
            notifier,
        }
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

        // Lock diakuisisi ulang per attempt — tidak tertahan selama seluruh retry loop.
        // Tanpa heartbeat, implementasi ini simple dan race-free.
        for attempt in 0..MAX_TX_RETRY {
            let mut lock = VariantLockGuard::acquire(self.redis.clone(), &variant_ids)
                .await
                .map_err(|e| {
                    if matches!(e, AppError::Conflict(_)) {
                        OrderMetrics::lock_conflict();
                    }
                    e
                })?;

            OrderMetrics::lock_acquired(variant_ids.len());

            let tx_result = timeout(TX_TIMEOUT, self.create_in_tx(customer_id, &req)).await;

            // Release selalu sebelum retry atau return.
            lock.release().await;

            match tx_result {
                Ok(Ok(v)) => {
                    // 🔥🔥🔥 FIRE-AND-FORGET: spawn detached, tidak blocking return
                    self.notifier.notify_order_created(
                        customer_id.to_string(),
                        v.clone(), // pastikan OrderDetailResponse derive Clone
                    );
                    return Ok(v);
                }

                Ok(Err(AppError::Internal(ref e))) if is_retryable_pg_error(e) => {
                    let reason = format!("{e}");
                    OrderMetrics::tx_retry(attempt, &reason);

                    if attempt + 1 < MAX_TX_RETRY {
                        sleep(backoff_with_jitter(attempt)).await;
                        continue;
                    }
                    return Err(AppError::Internal(anyhow::anyhow!(
                        "DB conflict setelah {} retry: {}",
                        MAX_TX_RETRY,
                        reason
                    )));
                }

                Ok(Err(e)) => return Err(e),

                Err(_elapsed) => {
                    OrderMetrics::tx_timeout();
                    return Err(AppError::Internal(anyhow::anyhow!(
                        "transaksi timeout setelah {}s",
                        TX_TIMEOUT.as_secs()
                    )));
                }
            }
        }

        Err(AppError::Internal(anyhow::anyhow!(
            "DB conflict setelah {} retry",
            MAX_TX_RETRY
        )))
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

        // ─ 1. Single-pass aggregation ─────────────────────────────────────────
        let mut qty_per_variant: HashMap<&str, i32> = HashMap::new();
        let mut bytes_per_variant: HashMap<&str, Vec<u8>> = HashMap::new();

        for item in &req.items {
            let vid = item.ticket_variant_id.as_str();
            *qty_per_variant.entry(vid).or_insert(0) += item.quantity;
            if let std::collections::hash_map::Entry::Vacant(e) = bytes_per_variant.entry(vid) {
                let bytes = id_to_vec(vid).map_err(|e| AppError::BadRequest(e.to_string()))?;
                e.insert(bytes);
            }
        }

        // into_values() → Vec<Vec<u8>> owned, kompatibel dengan lock_variants(&[Vec<u8>]).
        // bytes_per_variant tidak dipakai lagi setelah ini (variant data diambil dari
        // query result melalui variant_map).
        let id_bytes_list: Vec<Vec<u8>> = bytes_per_variant.into_values().collect();

        // ─ 2. SELECT FOR UPDATE ───────────────────────────────────────────────
        let variants = OrderTx::lock_variants(&tx, &id_bytes_list)
            .await
            .map_err(AppError::Internal)?;

        let variant_map: HashMap<&str, &LockedVariant> =
            variants.iter().map(|v| (v.ulid.as_str(), v)).collect();

        // ─ 3. Validasi stok, is_active, max_per_order ────────────────────────
        for (vid, &total_qty) in &qty_per_variant {
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

        // ─ 4. Bangun item_rows + hitung grand total ───────────────────────────
        let order_id = new_ulid();
        let order_id_bytes = ulid_to_vec(&order_id).map_err(AppError::Internal)?;
        let mut grand_total = Decimal::ZERO;
        let mut item_rows: Vec<ItemRow> = Vec::with_capacity(req.items.len());

        for item in &req.items {
            let v = variant_map[item.ticket_variant_id.as_str()];
            let unit_price = v.effective_price;
            let subtotal = unit_price * Decimal::from(item.quantity);
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

        // ─ 5. INSERT order — atomic idempotency CTE ───────────────────────────
        let customer_bytes = id_to_vec(customer_id).map_err(AppError::Internal)?;
        let order_code = {
            // ULID sudah uppercase by spec — .to_uppercase() redundan dan allocates.
            let suffix = &order_id[order_id.len().saturating_sub(8)..];
            format!("KN{suffix}")
        };
        let expired_at = chrono::Utc::now() + chrono::Duration::hours(2);

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

        if !is_new {
            OrderMetrics::idempotency_conflict(customer_id);

            // FIX [P0-3]: COMMIT bukan ROLLBACK untuk idempotency conflict.
            //
            // TX ini adalah read-only (insert tidak terjadi karena ON CONFLICT DO NOTHING).
            // COMMIT lebih reliable karena:
            //   - ROLLBACK yang gagal (network timeout) → connection dikembalikan ke pool
            //     dalam keadaan transaksi masih aktif di sisi PostgreSQL.
            //   - drop(conn) setelah rollback gagal TIDAK memusnahkan connection —
            //     deadpool mengembalikannya ke pool karena connection object masih valid.
            //   - COMMIT pada read-only TX selalu berhasil kecuali koneksi benar-benar putus,
            //     dalam hal itu deadpool akan mendeteksi dan drop connection dengan benar.
            if let Err(e) = tx.commit().await {
                tracing::warn!(
                    error = %e,
                    "commit idempotency tx failed; connection mungkin dirty, akan di-drop"
                );
                // drop eksplisit sebagai sinyal ke deadpool bahwa connection ini bermasalah.
                // Deadpool akan close underlying connection saat Client di-drop dalam
                // keadaan error.
                drop(conn);
            }

            let items = self.repo.list_items(&order.id).await?;
            return Ok(build_detail_response(order, items));
        }

        // ─ 6. Batch INSERT order_items ────────────────────────────────────────
        OrderTx::insert_order_items_batch(&tx, &order_id_bytes, &item_rows)
            .await
            .map_err(AppError::Internal)?;

        // ─ 7. Batch UPDATE sold — DB-level oversell guard ─────────────────────
        //
        // FIX [P1-5]: bump menggunakan &[u8] borrow — tidak ada clone var_bytes.
        // Lifetime slices terikat ke item_rows yang hidup selama fungsi ini.
        let bump: Vec<(&[u8], i32)> = item_rows
            .iter()
            .map(|row| (row.var_bytes.as_slice(), row.qty))
            .collect();

        OrderTx::bump_sold_batch(&tx, &bump).await.map_err(|e| {
            if let Some(oe) = e.downcast_ref::<OversellError>() {
                // bin_to_ulid_ref menerima &[u8] — tidak ada clone di error path.
                let failed_variants: Vec<String> = oe
                    .variant_ids
                    .iter()
                    .filter_map(|b| bin_to_ulid_ref(b).ok())
                    .collect();

                OrderMetrics::oversell_rejected(&failed_variants);

                tracing::error!(
                    updated = oe.updated,
                    expected = oe.expected,
                    variants = ?failed_variants,
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

        // ─ Response dari in-memory (tanpa query tambahan) ─────────────────────
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

        Ok(build_detail_response(order, items))
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
        Ok(build_detail_response(order, items))
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

        // FIX [P2-7]: mark_paid menggunakan RETURNING → dapat Order langsung.
        // Mengeliminasi self.detail() post-commit yang butuh 2 query DB tambahan
        // (find_by_id + list_items = 2 pool checkout + 2 query).
        let paid_order = OrderTx::mark_paid(&tx, &order_bytes, &req.payment_method)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::BadRequest(
                    "Order tidak bisa dibayar (sudah dibayar, dibatalkan, atau expired)".into(),
                )
            })?;

        // fetch_items_for_mint: (bytes, qty) untuk mint_tickets_batch.
        let mint_items = OrderTx::fetch_items_for_mint(&tx, &order_bytes)
            .await
            .map_err(AppError::Internal)?;

        OrderTx::mint_tickets_batch(&tx, &mint_items)
            .await
            .map_err(AppError::Internal)?;

        // fetch_items_detail: full OrderItemResponse untuk response.
        // Query ini di dalam TX → data konsisten dengan paid_order di atas.
        let items = OrderTx::fetch_items_detail(&tx, &order_bytes)
            .await
            .map_err(AppError::Internal)?;

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit pay: {e}")))?;

        OrderMetrics::order_paid(order_id, &req.payment_method);

        // BUG FIX: kirim notifikasi WA ke customer setelah pembayaran berhasil.
        // Sebelumnya pay() tidak mengirim notifikasi apapun.
        // Fire-and-forget: tidak blocking response API.
        let paid_response = build_detail_response(paid_order, items);
        self.notifier
            .notify_order_paid(viewer_id.to_string(), paid_response.clone());

        // Build response dari data TX — tidak ada post-commit query.
        Ok(paid_response)
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

// ── Builder helper ────────────────────────────────────────────────────────────

fn build_detail_response(order: Order, items: Vec<OrderItemResponse>) -> OrderDetailResponse {
    OrderDetailResponse {
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
    }
}
