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

// ── Redis distributed lock ────────────────────────────────────────────────────

const LOCK_TTL_MS: u64 = 15_000;
const LOCK_RETRIES: u8 = 3;
const LOCK_DELAY_MS: u64 = 80;

const LUA_RELEASE: &str = r#"
if redis.call("get", KEYS[1]) == ARGV[1] then
    return redis.call("del", KEYS[1])
else
    return 0
end
"#;

struct VariantLockGuard {
    redis: redis::aio::ConnectionManager,
    acquired_keys: Vec<String>,
    lock_val: String,
}

impl VariantLockGuard {
    async fn acquire(
        redis: redis::aio::ConnectionManager,
        variant_ids: &[&str],
    ) -> AppResult<Self> {
        // Sort untuk menghindari deadlock antar request yang tumpang tindih
        let mut sorted: Vec<&str> = variant_ids.to_vec();
        sorted.sort_unstable();
        sorted.dedup(); // buang duplikat setelah sort

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
                        return Err(AppError::Internal(anyhow::anyhow!("Redis: {e}")));
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

    async fn release(&mut self) {
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

// ── OrderService ──────────────────────────────────────────────────────────────

pub struct OrderService {
    repo: Arc<dyn OrderRepository>,
    redis: redis::aio::ConnectionManager,
    /// Pool dipakai SERVICE untuk membuka transaksi DB.
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

        // Kumpulkan variant_ids unik untuk locking
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

        // — Jalankan DB transaction di service layer —
        let result = self.create_in_tx(customer_id, &req).await;

        // — Lepas lock (sukses maupun gagal) —
        lock.release().await;

        result
    }

    async fn create_in_tx(
        &self,
        customer_id: &str,
        req: &CreateOrderRequest,
    ) -> AppResult<OrderDetailResponse> {
        // Service membuka koneksi + transaksi
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("pool: {e}")))?;
        let tx = conn
            .transaction()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("begin tx: {e}")))?;

        // ─ 1. Kumpulkan variant_id bytes (unik) ─
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

        // ─ 2. SELECT FOR UPDATE + JOIN events (data lengkap, 1 query) ─
        let variants = OrderTx::lock_variants(&tx, &id_bytes_list)
            .await
            .map_err(|e| AppError::Internal(e))?;

        let variant_map: HashMap<&str, &LockedVariant> =
            variants.iter().map(|v| (v.ulid.as_str(), v)).collect();

        // ─ 3. Hitung total qty per variant (cegah oversell multi-item same variant) ─
        let mut total_qty_per_variant: HashMap<&str, i32> = HashMap::new();
        for item in &req.items {
            *total_qty_per_variant
                .entry(item.ticket_variant_id.as_str())
                .or_insert(0) += item.quantity;
        }

        // ─ 4. Validasi stok, is_active, max_per_order ─
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

        // ─ 5. Bangun item_rows + hitung grand total ─
        let mut grand_total: f64 = 0.0;
        let order_id = new_ulid();
        let order_id_bytes = ulid_to_vec(&order_id).map_err(|e| AppError::Internal(e))?;

        let mut item_rows: Vec<ItemRow> = Vec::with_capacity(req.items.len());
        for item in &req.items {
            let v = variant_map[item.ticket_variant_id.as_str()];
            // Pakai effective_price: sale_price jika sale aktif, else price
            let unit_price = v.effective_price;
            let subtotal = unit_price * item.quantity as f64;
            grand_total += subtotal;

            let oi_id = new_ulid();
            item_rows.push(ItemRow {
                oi_bytes: ulid_to_vec(&oi_id).map_err(|e| AppError::Internal(e))?,
                oi_id,
                var_bytes: v.id_bytes.clone(),
                qty: item.quantity,
                unit_price,
                subtotal,
            });
        }

        // ─ 6. INSERT order ─
        let customer_bytes = id_to_vec(customer_id).map_err(|e| AppError::Internal(e))?;
        let order_code = format!("KN{}", &order_id[..order_id.len().min(10)]);
        let expired_at = chrono::Utc::now() + chrono::Duration::hours(2);

        let order = OrderTx::insert_order(
            &tx,
            &order_id_bytes,
            &customer_bytes,
            &order_code,
            grand_total,
            expired_at,
        )
        .await
        .map_err(|e| AppError::Internal(e))?;

        // ─ 7. Batch INSERT order_items ─
        OrderTx::insert_order_items_batch(&tx, &order_id_bytes, &item_rows)
            .await
            .map_err(|e| AppError::Internal(e))?;

        // ─ 8. Batch UPDATE sold — satu query UNNEST, bukan O(n) ─
        let bump: Vec<(Vec<u8>, i32)> = {
            // Aggregate per variant (benar meski ada multi-item same variant)
            let mut agg: HashMap<Vec<u8>, i32> = HashMap::new();
            for row in &item_rows {
                *agg.entry(row.var_bytes.clone()).or_insert(0) += row.qty;
            }
            agg.into_iter().collect()
        };
        OrderTx::bump_sold_batch(&tx, &bump)
            .await
            .map_err(|e| AppError::Internal(e))?;

        // ─ Commit — service yang commit ─
        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit: {e}")))?;

        // ─ Susun response dari data in-memory (tanpa query tambahan) ─
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

        // Buka transaksi di service
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("{e}")))?;
        let tx = conn
            .transaction()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("{e}")))?;

        let order_bytes = id_to_vec(order_id).map_err(|e| AppError::Internal(e))?;

        // mark_paid juga cek expired_at > NOW() sebagai guard kedua
        let updated = OrderTx::mark_paid(&tx, &order_bytes, &req.payment_method)
            .await
            .map_err(|e| AppError::Internal(e))?;
        if updated == 0 {
            return Err(AppError::BadRequest(
                "Order tidak bisa dibayar (sudah dibayar, dibatalkan, atau expired)".into(),
            ));
        }

        let items = OrderTx::fetch_items_for_order(&tx, &order_bytes)
            .await
            .map_err(|e| AppError::Internal(e))?;

        OrderTx::mint_tickets_batch(&tx, &items)
            .await
            .map_err(|e| AppError::Internal(e))?;

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

        let order_bytes = id_to_vec(order_id).map_err(|e| AppError::Internal(e))?;

        // Ambil items untuk refund sebelum cancel
        let items = OrderTx::fetch_items_for_refund(&tx, &order_bytes)
            .await
            .map_err(|e| AppError::Internal(e))?;

        let n = OrderTx::cancel_order(&tx, &order_bytes)
            .await
            .map_err(|e| AppError::Internal(e))?;
        if n == 0 {
            return Err(AppError::BadRequest("Order tidak bisa dibatalkan".into()));
        }

        // Batch refund stok — satu UNNEST query
        OrderTx::refund_sold_batch(&tx, &items)
            .await
            .map_err(|e| AppError::Internal(e))?;

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit cancel: {e}")))?;

        Ok(())
    }
}
