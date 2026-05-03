use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{Duration, sleep};
use validator::Validate;

use deadpool_postgres::Pool;

use crate::models::orders::{
    CreateOrderRequest, Order, OrderDetailResponse, OrderItemResponse, PayOrderRequest,
};
use crate::repository::order::{ItemRow, LockedVariant, OrderRepository, OrderTx};
use crate::utils::error::{AppError, AppResult};
use crate::utils::ulid::{id_to_vec, new_ulid, ulid_to_vec};

// ── Konstanta distributed lock ────────────────────────────────────────────────

const LOCK_TTL_MS: u64 = 15_000;
const LOCK_RETRIES: u8 = 3;
const LOCK_DELAY_MS: u64 = 80;
const MAX_TX_RETRY: u8 = 3;

// Lua script: hanya delete key kalau value masih milik kita
const LUA_RELEASE: &str = r#"
if redis.call("get", KEYS[1]) == ARGV[1] then
    return redis.call("del", KEYS[1])
else
    return 0
end
"#;

// ── VariantLockGuard ──────────────────────────────────────────────────────────

pub(crate) struct VariantLockGuard {
    redis: redis::aio::ConnectionManager,
    /// Keys yang sudah berhasil di-acquire (urutan lock).
    pub(crate) acquired_keys: Vec<String>,
    /// Nilai unik per lock session — dipakai Lua script saat release.
    pub(crate) lock_val: String,
}

impl VariantLockGuard {
    /// Acquire lock untuk setiap variant secara berurutan (sorted untuk cegah deadlock).
    /// Jika salah satu gagal setelah retry, semua lock yang sudah acquired dilepas.
    pub async fn acquire(
        redis: redis::aio::ConnectionManager,
        variant_ids: &[&str],
    ) -> AppResult<Self> {
        let mut sorted: Vec<&str> = variant_ids.to_vec();
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

    /// Explicit release — lebih cepat dari menunggu Drop.
    /// Aman dipanggil berkali-kali (acquired_keys dikosongkan setelah drain).
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

/// Safety net: kalau terjadi panic sebelum release() dipanggil,
/// Drop spawn task untuk melepas semua lock yang masih held.
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

/// Background task yang memperpanjang TTL lock setiap 1/3 LOCK_TTL_MS.
/// Mencegah lock expired di tengah transaksi DB yang lambat.
struct LockHeartbeat {
    handle: tokio::task::JoinHandle<()>,
}

impl LockHeartbeat {
    fn start(
        mut redis: redis::aio::ConnectionManager,
        keys: Vec<String>,
        lock_val: String,
    ) -> Self {
        // Refresh setiap 1/3 TTL — cukup margin untuk 2x refresh sebelum expired
        let interval = Duration::from_millis(LOCK_TTL_MS / 3);

        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.tick().await; // skip immediate first tick

            loop {
                ticker.tick().await;
                for key in &keys {
                    // GET dulu — hanya extend kalau value masih milik kita
                    let res: redis::RedisResult<Option<String>> =
                        redis::cmd("GET").arg(key).query_async(&mut redis).await;

                    match res {
                        Ok(Some(val)) if val == lock_val => {
                            let _ = redis::cmd("PEXPIRE")
                                .arg(key)
                                .arg(LOCK_TTL_MS)
                                .query_async::<i64>(&mut redis)
                                .await;
                        }
                        // Lock sudah tidak milik kita (expired atau direbut) → stop heartbeat
                        _ => return,
                    }
                }
            }
        });

        Self { handle }
    }

    /// Matikan heartbeat — dipanggil setelah lock direlease.
    fn stop(self) {
        self.handle.abort();
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Cek apakah PostgreSQL error layak di-retry.
/// 40001 = serialization_failure, 40P01 = deadlock_detected.
fn is_retryable_pg_error(e: &anyhow::Error) -> bool {
    if let Some(pg_err) = e.downcast_ref::<tokio_postgres::Error>() {
        if let Some(db_err) = pg_err.as_db_error() {
            let code = db_err.code().code();
            return code == "40001" || code == "40P01";
        }
    }
    false
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

        // Kumpulkan variant_ids unik untuk locking (sorted untuk cegah deadlock)
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

        // — Acquire Redis distributed lock —
        let mut lock = VariantLockGuard::acquire(self.redis.clone(), &variant_ids).await?;

        // — Spawn heartbeat untuk mencegah lock expired di tengah transaksi —
        let heartbeat = LockHeartbeat::start(
            self.redis.clone(),
            lock.acquired_keys.clone(),
            lock.lock_val.clone(),
        );

        // — Retry loop untuk DB deadlock / serialization failure —
        let mut result: AppResult<OrderDetailResponse> =
            Err(AppError::Internal(anyhow::anyhow!("unreachable")));
        let mut last_retryable: Option<String> = None;

        for attempt in 0..MAX_TX_RETRY {
            match self.create_in_tx(customer_id, &req).await {
                Ok(v) => {
                    result = Ok(v);
                    last_retryable = None;
                    break;
                }
                Err(AppError::Internal(ref e)) if is_retryable_pg_error(e) => {
                    last_retryable = Some(format!("attempt {attempt}: {e}"));
                    // Exponential backoff kecil: 20ms, 40ms, 60ms
                    sleep(Duration::from_millis(20 * (attempt as u64 + 1))).await;
                    continue;
                }
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
        }

        // Kalau semua retry habis karena DB conflict
        if let Some(msg) = last_retryable {
            result = Err(AppError::Internal(anyhow::anyhow!(
                "DB conflict setelah {} retry: {}",
                MAX_TX_RETRY,
                msg
            )));
        }

        // — Matikan heartbeat & lepas lock (sukses maupun gagal) —
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

        // ─ 0. Idempotency check ─────────────────────────────────────────────
        // Mencegah double order dari retry / double-click client
        if let Some(ikey) = &req.idempotency_key {
            let existing: Option<String> = tx
                .query_opt(
                    "SELECT id FROM orders WHERE idempotency_key = $1 AND customer_id = $2",
                    &[ikey, &customer_id],
                )
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("idempotency check: {e}")))?
                .map(|r| r.get(0));

            if let Some(existing_id) = existing {
                tx.rollback().await.ok();
                // Return order yang sudah ada tanpa insert baru
                return self.detail(&existing_id, customer_id).await;
            }
        }

        // ─ 1. Kumpulkan variant_id bytes (unik) ─────────────────────────────
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

        // ─ 2. SELECT FOR UPDATE + JOIN events (data lengkap, 1 query) ────────
        let variants = OrderTx::lock_variants(&tx, &id_bytes_list)
            .await
            .map_err(AppError::Internal)?;

        let variant_map: HashMap<&str, &LockedVariant> =
            variants.iter().map(|v| (v.ulid.as_str(), v)).collect();

        // ─ 3. Aggregate total qty per variant (cegah oversell multi-item) ────
        let mut total_qty_per_variant: HashMap<&str, i32> =
            HashMap::with_capacity(variant_ids_capacity(&req.items));
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

        // ─ 6. INSERT order ────────────────────────────────────────────────────
        let customer_bytes = id_to_vec(customer_id).map_err(AppError::Internal)?;

        // Order code: prefix + 8 char suffix dari bagian random ULID
        // (bukan 10 char awal yang merupakan timestamp — lebih collision-resistant)
        let order_code = {
            let suffix = &order_id[order_id.len().saturating_sub(8)..];
            format!("KN{}", suffix.to_uppercase())
        };

        let expired_at = chrono::Utc::now() + chrono::Duration::hours(2);

        let order = OrderTx::insert_order(
            &tx,
            &order_id_bytes,
            &customer_bytes,
            &order_code,
            grand_total,
            expired_at,
            req.idempotency_key.as_deref(), // simpan ke kolom idempotency_key
        )
        .await
        .map_err(AppError::Internal)?;

        // ─ 7. Batch INSERT order_items ────────────────────────────────────────
        OrderTx::insert_order_items_batch(&tx, &order_id_bytes, &item_rows)
            .await
            .map_err(AppError::Internal)?;

        // ─ 8. Batch UPDATE sold (UNNEST — satu query, bukan O(n)) ─────────────
        let bump: Vec<(Vec<u8>, i32)> = {
            let mut agg: HashMap<Vec<u8>, i32> = HashMap::with_capacity(item_rows.len());
            for row in &item_rows {
                *agg.entry(row.var_bytes.clone()).or_insert(0) += row.qty;
            }
            agg.into_iter().collect()
        };
        OrderTx::bump_sold_batch(&tx, &bump)
            .await
            .map_err(AppError::Internal)?;

        // ─ Commit ─────────────────────────────────────────────────────────────
        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit: {e}")))?;

        // ─ Susun response dari data in-memory (tanpa query tambahan) ──────────
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

        // Early checks sebelum membuka transaksi
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

        // mark_paid cek expired_at > NOW() sebagai guard kedua di DB level
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

        // Ambil items untuk refund stok sebelum cancel
        let items = OrderTx::fetch_items_for_refund(&tx, &order_bytes)
            .await
            .map_err(AppError::Internal)?;

        let n = OrderTx::cancel_order(&tx, &order_bytes)
            .await
            .map_err(AppError::Internal)?;

        if n == 0 {
            return Err(AppError::BadRequest("Order tidak bisa dibatalkan".into()));
        }

        // Batch refund stok — satu UNNEST query
        OrderTx::refund_sold_batch(&tx, &items)
            .await
            .map_err(AppError::Internal)?;

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit cancel: {e}")))?;

        Ok(())
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Hitung jumlah variant unik dari items — untuk HashMap::with_capacity.
#[inline]
fn variant_ids_capacity(items: &[crate::models::orders::CreateOrderItemRequest]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for item in items {
        seen.insert(&item.ticket_variant_id);
    }
    seen.len()
}
