pub mod checkout;
pub mod lock;
mod metrics;
#[cfg(test)]
mod oversell_test;

use lock::VariantLockGuard;

use std::collections::HashMap;
use std::sync::Arc;

use deadpool_postgres::Pool;
use rust_decimal::Decimal;
use tokio::time::{sleep, timeout};
use validator::Validate;

use crate::models::notification::CreateNotificationInput;
use crate::models::orders::{
    CreateOrderRequest, Order, OrderDetailResponse, OrderItemResponse, OrderListItem,
    PayOrderRequest,
};
use crate::repository::order::{
    ItemRow, LockedVariant, OrderPaymentSpec, OrderRepository, OrderTx, OversellError,
};
use crate::repository::ticket::TicketRepository;
use crate::service::background::BackgroundJobs;
use crate::service::group_chat::GroupChatService;
use crate::service::norifications::NotificationService;
use crate::service::notification_store::NotificationStoreService;
use crate::utils::error::{AppError, AppResult};
use crate::utils::ulid::{bin_to_ulid_ref, id_to_vec, new_ulid, ulid_to_vec};

use self::metrics::{
    backoff_with_jitter, is_retryable_pg_error, OrderMetrics, MAX_TX_RETRY, TX_TIMEOUT,
};

// ── OrderService ──────────────────────────────────────────────────────────────

/// Job latar order (notifikasi in-app, auto-join grup) berjalan lewat eksekutor
/// bounded, bukan `tokio::spawn` telanjang. `NOTIF_CONCURRENCY` sengaja jauh di
/// bawah ukuran pool DB (default 24) agar notifikasi tak pernah menghabiskan
/// koneksi dari jalur checkout kritis saat flash-sale.
const NOTIF_CONCURRENCY: usize = 8;
const NOTIF_QUEUE_CAP: usize = 8192;

pub struct OrderService {
    pub(super) repo: Arc<dyn OrderRepository>,
    /// Keranjang: sumber kebenaran isi & harga saat checkout.
    pub(super) cart: Arc<crate::service::cart::CartService>,
    /// Kanal pembayaran & promo.
    pub(super) payment: Arc<crate::service::payment::PaymentService>,
    pub(super) redis: redis::aio::ConnectionManager,
    pub(super) pool: Pool,
    pub(super) notifier: Arc<NotificationService>,
    pub(super) notif_store: Arc<NotificationStoreService>,
    pub(super) ticket_repo: Arc<dyn TicketRepository>,
    pub(super) group_svc: Arc<GroupChatService>,
    pub(super) background: Arc<BackgroundJobs>,
}

impl OrderService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repo: Arc<dyn OrderRepository>,
        redis: redis::aio::ConnectionManager,
        pool: Pool,
        notifier: Arc<NotificationService>,
        notif_store: Arc<NotificationStoreService>,
        ticket_repo: Arc<dyn TicketRepository>,
        group_svc: Arc<GroupChatService>,
        cart: Arc<crate::service::cart::CartService>,
        payment: Arc<crate::service::payment::PaymentService>,
    ) -> Self {
        Self {
            repo,
            cart,
            payment,
            redis,
            pool,
            notifier,
            notif_store,
            ticket_repo,
            group_svc,
            background: BackgroundJobs::new(NOTIF_CONCURRENCY, NOTIF_QUEUE_CAP),
        }
    }

    // ── Create ────────────────────────────────────────────────────────────────

    /// Buat order dari daftar tiket yang disebut pemanggil (jalur REST lama).
    pub async fn create(
        &self,
        customer_id: &str,
        req: CreateOrderRequest,
        is_premium: bool,
    ) -> AppResult<OrderDetailResponse> {
        self.create_inner(customer_id, req, is_premium, None).await
    }

    /// Inti pembuatan order: kunci varian → transaksi → notifikasi.
    ///
    /// `pricing` berisi kanal pembayaran & promo bila order lahir dari halaman
    /// checkout; `None` untuk jalur yang hanya menyebut tiket.
    pub(super) async fn create_inner(
        &self,
        customer_id: &str,
        req: CreateOrderRequest,
        is_premium: bool,
        pricing: Option<&self::checkout::CheckoutPricing<'_>>,
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

        for attempt in 0..MAX_TX_RETRY {
            let mut lock = VariantLockGuard::acquire(self.redis.clone(), &variant_ids, is_premium)
                .await
                .map_err(|e| {
                    if matches!(e, AppError::Conflict(_)) {
                        OrderMetrics::lock_conflict();
                    }
                    e
                })?;

            OrderMetrics::lock_acquired(variant_ids.len());

            let tx_result =
                timeout(TX_TIMEOUT, self.create_in_tx(customer_id, &req, pricing)).await;

            lock.release().await;

            match tx_result {
                Ok(Ok(v)) => {
                    self.notifier
                        .notify_order_created(customer_id.to_string(), v.clone());

                    {
                        let notif_store = self.notif_store.clone();
                        let uid = customer_id.to_string();
                        let order_id = v.id.clone();
                        let order_code = v.order_code.clone();
                        self.background.spawn(async move {
                            let body = format!("Order {order_code} menunggu pembayaran.");
                            if let Err(e) = notif_store
                                .create(CreateNotificationInput::order(
                                    uid,
                                    order_id,
                                    "Pesanan Dibuat",
                                    body,
                                ))
                                .await
                            {
                                tracing::warn!(error = %e, "in-app order notification failed");
                            }
                        });
                    }

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

    pub(super) async fn create_in_tx(
        &self,
        customer_id: &str,
        req: &CreateOrderRequest,
        pricing: Option<&self::checkout::CheckoutPricing<'_>>,
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

        let id_bytes_list: Vec<Vec<u8>> = bytes_per_variant.into_values().collect();

        let variants = OrderTx::lock_variants(&tx, &id_bytes_list)
            .await
            .map_err(AppError::Internal)?;

        let variant_map: HashMap<&str, &LockedVariant> =
            variants.iter().map(|v| (v.ulid.as_str(), v)).collect();

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

        let customer_bytes = id_to_vec(customer_id).map_err(AppError::Internal)?;
        let order_code = {
            let suffix = &order_id[order_id.len().saturating_sub(8)..];
            format!("KN{suffix}")
        };
        let expired_at = chrono::Utc::now() + chrono::Duration::hours(2);

        // `grand_total` sampai titik ini adalah SUBTOTAL: harga tiket menurut
        // harga yang baru saja dikunci di dalam transaksi. Potongan promo dan
        // biaya kanal dihitung di atasnya — dan sengaja dihitung DI SINI, bukan
        // dari angka yang tadi ditampilkan di keranjang, supaya perubahan harga
        // yang terjadi di sela-sela checkout tak pernah menagih angka lama.
        let subtotal = grand_total;
        let pay_calc = pricing.map(|p| p.compute(subtotal));
        let payable = pay_calc.as_ref().map_or(subtotal, |c| c.total);

        // Rakit spesifikasi pembayaran di sini agar seluruh pinjaman (&str)
        // hidup sampai `insert_order` selesai.
        let reference = pay_calc.as_ref().and_then(|c| c.reference.clone());
        let spec = match (pricing, pay_calc.as_ref()) {
            (Some(p), Some(c)) => OrderPaymentSpec {
                cart_bytes: p.cart_bytes.as_deref(),
                subtotal,
                discount: c.discount,
                promo_code: c.promo_code.as_deref(),
                vendor: Some(p.method.vendor.as_str()),
                code: Some(p.method.code.as_str()),
                charge: c.charge,
                payment_expired_at: c.payment_expired_at,
                reference: reference.as_deref(),
                link_pay: None,
            },
            // Jalur lama (REST `POST /api/orders`) tak menyebut kanal
            // pembayaran: order lahir polos dan kanalnya tercatat saat `pay()`.
            _ => OrderPaymentSpec::plain(subtotal),
        };

        // ── Keranjang tujuan ────────────────────────────────────────────
        // Jalur checkout membawa keranjang yang sedang dipakai pembeli. Jalur
        // beli-langsung tidak punya keranjang sama sekali, jadi dibuatkan
        // keranjang sekali-pakai yang lahir sudah tertutup. Dua-duanya berakhir
        // sebagai keranjang tertutup dengan baris pesanan di dalamnya, sehingga
        // sisa sistem hanya perlu mengenal SATU bentuk.
        let cart_bytes: Vec<u8> = match pricing.and_then(|p| p.cart_bytes.clone()) {
            Some(b) => b,
            None => {
                let id = ulid_to_vec(&new_ulid()).map_err(AppError::Internal)?;
                OrderTx::insert_closed_cart(&tx, &id, &customer_bytes)
                    .await
                    .map_err(AppError::Internal)?;
                id
            }
        };

        let (order, is_new) = OrderTx::insert_order(
            &tx,
            &order_id_bytes,
            &customer_bytes,
            &order_code,
            payable,
            expired_at,
            req.idempotency_key.as_deref(),
            &spec,
        )
        .await
        .map_err(AppError::Internal)?;

        if !is_new {
            OrderMetrics::idempotency_conflict(customer_id);

            if let Err(e) = tx.commit().await {
                tracing::warn!(
                    error = %e,
                    "commit idempotency tx failed; connection mungkin dirty, akan di-drop"
                );
                drop(conn);
            }

            let items = self.repo.list_items(&order.id).await?;
            return Ok(build_detail_response(order, items));
        }

        // Bekukan baris keranjang dengan harga yang baru dikunci, lalu tutup
        // keranjangnya — keduanya di dalam transaksi yang sama dengan lahirnya
        // order. Kalau ordernya batal, keranjang kembali terbuka utuh.
        OrderTx::freeze_cart_items(&tx, &cart_bytes, &item_rows)
            .await
            .map_err(AppError::Internal)?;

        OrderTx::close_cart(&tx, &cart_bytes)
            .await
            .map_err(AppError::Internal)?;

        // Barang yang tidak dicentang pembeli diselamatkan ke keranjang baru.
        // Hanya berlaku pada jalur checkout: jalur beli-langsung memakai
        // keranjang sekali-pakai yang seluruh isinya memang dibeli.
        if pricing.is_some() {
            let next_cart = ulid_to_vec(&new_ulid()).map_err(AppError::Internal)?;
            OrderTx::rescue_unselected(&tx, &cart_bytes, &next_cart, &customer_bytes)
                .await
                .map_err(AppError::Internal)?;
        }

        let bump: Vec<(&[u8], i32)> = item_rows
            .iter()
            .map(|row| (row.var_bytes.as_slice(), row.qty))
            .collect();

        OrderTx::bump_sold_batch(&tx, &bump).await.map_err(|e| {
            if let Some(oe) = e.downcast_ref::<OversellError>() {
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

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit: {e}")))?;

        OrderMetrics::order_created(&order.id, payable, item_rows.len());

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
        // Order & items independen secara data → ambil paralel (1 latensi,
        // bukan 2 berurutan). Cek kepemilikan tetap dilakukan SEBELUM data
        // dikembalikan — items tak pernah bocor ke viewer yang salah.
        let (order, items) =
            tokio::try_join!(self.repo.find_by_id(order_id), self.repo.list_items(order_id))?;
        let order = order.ok_or_else(|| AppError::NotFound("Order not found".into()))?;

        if order.customer_id != viewer_id {
            return Err(AppError::Forbidden("Not your order".into()));
        }

        Ok(build_detail_response(order, items))
    }

    // ── List ──────────────────────────────────────────────────────────────────

    pub async fn list_mine(
        &self,
        customer_id: &str,
        page: i64,
        per_page: i64,
    ) -> AppResult<Vec<OrderListItem>> {
        let page = page.max(1);
        let per_page = per_page.clamp(1, 100);
        let offset = (page - 1) * per_page;
        Ok(self
            .repo
            .list_for_customer_enriched(customer_id, per_page, offset)
            .await?)
    }

    // ── Pay ───────────────────────────────────────────────────────────────────

    pub async fn pay(
        &self,
        order_id: &str,
        viewer_id: &str,
        user_name: &str,
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

        let paid_order = OrderTx::mark_paid(&tx, &order_bytes, &req.payment_method)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::BadRequest(
                    "Order tidak bisa dibayar (sudah dibayar, dibatalkan, atau expired)".into(),
                )
            })?;

        let mint_items = OrderTx::fetch_items_for_mint(&tx, &order_bytes)
            .await
            .map_err(AppError::Internal)?;

        OrderTx::mint_tickets_batch(&tx, &mint_items, &order_bytes)
            .await
            .map_err(AppError::Internal)?;

        let items = OrderTx::fetch_items_detail(&tx, &order_bytes)
            .await
            .map_err(AppError::Internal)?;

        tx.commit()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("commit pay: {e}")))?;

        OrderMetrics::order_paid(order_id, &req.payment_method);

        let event_ids: std::collections::HashSet<String> =
            items.iter().map(|i| i.event_id.clone()).collect();

        let paid_response = build_detail_response(paid_order, items);

        // ── AUTO-JOIN GRUP DIHAPUS ──────────────────────────────────────────
        // Membeli barang tidak lagi memasukkan pembeli ke grup produk. Alasan
        // grup itu ada hanya masuk akal untuk tiket acara: orang yang pergi ke
        // konser yang sama memang punya sesuatu untuk dibicarakan.
        //
        // Di marketplace barang, yang dibutuhkan pembeli adalah bertanya kepada
        // PENJUALNYA — stok, ukuran, ongkir, kapan bisa diambil — dan
        // pertanyaan itu tidak boleh terbaca oleh semua orang yang kebetulan
        // membeli barang yang sama. Di dalamnya ada alamat, nomor pesanan, dan
        // keluhan.
        //
        // Percakapan kini dibuka atas kemauan pembeli lewat tombol di halaman
        // produk (`GroupChatService::ensure_dm`), bukan diciptakan diam-diam
        // oleh pembayaran. Lihat migrasi 027.
        let _ = (&self.group_svc, user_name, &event_ids);

        self.notifier
            .notify_order_paid(viewer_id.to_string(), paid_response.clone());

        {
            let notif_store = self.notif_store.clone();
            let ticket_repo = self.ticket_repo.clone();
            let uid = viewer_id.to_string();
            let oid = order_id.to_string();
            self.background.spawn(async move {
                match ticket_repo.list_by_order(&oid, &uid, 100, 0).await {
                    Ok(tickets) => {
                        for t in tickets {
                            let body = format!(
                                "Tiket {} sudah aktif. Tunjukkan QR saat masuk.",
                                t.ticket_code
                            );
                            if let Err(e) = notif_store
                                .create(CreateNotificationInput::ticket(
                                    uid.clone(),
                                    t.id,
                                    "Pembayaran Berhasil",
                                    body,
                                ))
                                .await
                            {
                                tracing::warn!(error = %e, "in-app ticket notification failed");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "list tickets for in-app notif failed")
                    }
                }
            });
        }

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

pub(super) fn build_detail_response(
    order: Order,
    items: Vec<OrderItemResponse>,
) -> OrderDetailResponse {
    OrderDetailResponse {
        id: order.id,
        customer_id: order.customer_id,
        order_code: order.order_code,
        status: order.status,
        total_amount: order.total_amount,
        subtotal_amount: order.subtotal_amount,
        discount_amount: order.discount_amount,
        promo_code: order.promo_code,
        payment_method: order.payment_method,
        payment_vendor: order.payment_vendor,
        payment_code: order.payment_code,
        // Nama & instruksi kanal tidak ada di tabel `orders` — keduanya milik
        // `payment_methods`. Jalur checkout mengisinya lewat `enrich_payment`;
        // jalur lain membiarkannya kosong ketimbang menebak.
        payment_name: None,
        payment_charge: order.payment_charge,
        payment_expired_at: order.payment_expired_at,
        payment_reference: order.payment_reference,
        payment_instruction: None,
        link_pay: order.link_pay,
        paid_at: order.paid_at,
        expired_at: order.expired_at,
        created_at: order.created_at,
        items,
    }
}
