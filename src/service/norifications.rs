use redis::{aio::ConnectionManager as RedisConn, AsyncCommands};
use reqwest::Client as HttpClient;
use serde_json::json;
use std::sync::Arc;
use tracing;

use crate::config::config::WahaConfig;
use crate::models::orders::OrderDetailResponse;
use crate::repository::user::UserRepository;
use crate::utils::error::notify_background_error;
use crate::utils::phone::normalize_phone;

/// TTL dedup key di Redis — 24 jam cukup untuk mencegah double-notif dari
/// retry client atau race condition pada pay(), tapi tidak terlalu lama
/// jika order ID di-reuse (seharusnya tidak, tapi defence in depth).
const NOTIF_DEDUP_TTL_SECS: u64 = 86_400; // 24 jam

#[derive(Clone)]
pub struct NotificationService {
    http: HttpClient,
    waha: Arc<WahaConfig>,
    user_repo: Arc<dyn UserRepository>,
    /// FIX: Redis untuk dedup notifikasi WA.
    /// Mencegah double-kirim jika pay() dipanggil 2× (race/retry client).
    redis: RedisConn,
}

impl NotificationService {
    pub fn new(
        http: HttpClient,
        waha: Arc<WahaConfig>,
        user_repo: Arc<dyn UserRepository>,
        redis: RedisConn,
    ) -> Self {
        Self {
            http,
            waha,
            user_repo,
            redis,
        }
    }

    // ── Fire-and-forget spawners ──────────────────────────────────────────────

    /// Kirim notifikasi WA saat order berhasil dibuat.
    /// Fire-and-forget: tidak blocking response API.
    /// Dedup via Redis — maksimal 1 notif per order_id per 24 jam.
    pub fn notify_order_created(&self, customer_id: String, order: OrderDetailResponse) {
        let svc = self.clone();
        tokio::spawn(async move {
            // Dedup check: SET NX EX — atomik, tidak ada race condition
            let dedup_key = format!("notif:created:{}", order.id);
            if !svc.acquire_dedup(&dedup_key).await {
                tracing::debug!(order_id = %order.id, "WA order-created: skipped (dedup)");
                return;
            }

            if let Err(e) = svc.send_order_created_wa(&customer_id, &order).await {
                let detail = format!(
                    "customer_id={} order_id={} error={:#}",
                    customer_id, order.id, e
                );
                tracing::error!(
                    customer_id,
                    order_id = %order.id,
                    error = %e,
                    "WA order-created notification failed"
                );
                notify_background_error("WA_OrderCreated", detail);
            } else {
                tracing::info!(customer_id, order_id = %order.id, "WA order-created sent");
            }
        });
    }

    /// Kirim notifikasi WA saat pembayaran berhasil.
    /// Fire-and-forget: tidak blocking response API.
    /// Dedup via Redis — mencegah double WA jika pay() dipanggil 2×.
    pub fn notify_order_paid(&self, customer_id: String, order: OrderDetailResponse) {
        let svc = self.clone();
        tokio::spawn(async move {
            let dedup_key = format!("notif:paid:{}", order.id);
            if !svc.acquire_dedup(&dedup_key).await {
                tracing::debug!(order_id = %order.id, "WA order-paid: skipped (dedup)");
                return;
            }

            if let Err(e) = svc.send_order_paid_wa(&customer_id, &order).await {
                let detail = format!(
                    "customer_id={} order_id={} error={:#}",
                    customer_id, order.id, e
                );
                tracing::error!(
                    customer_id,
                    order_id = %order.id,
                    error = %e,
                    "WA order-paid notification failed"
                );
                notify_background_error("WA_OrderPaid", detail);
            } else {
                tracing::info!(customer_id, order_id = %order.id, "WA order-paid sent");
            }
        });
    }

    // ── Dedup helper ──────────────────────────────────────────────────────────

    /// SET key "1" NX EX ttl — return true jika berhasil set (kita yang pertama),
    /// false jika key sudah ada (sudah pernah notif).
    /// Menggunakan SET NX yang atomic — tidak ada race condition antar dua instance.
    async fn acquire_dedup(&self, key: &str) -> bool {
        let mut conn = self.redis.clone();
        let result: redis::RedisResult<Option<String>> = redis::cmd("SET")
            .arg(key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(NOTIF_DEDUP_TTL_SECS)
            .query_async(&mut conn)
            .await;

        match result {
            Ok(Some(_)) => true, // SET berhasil — kita yang pertama
            Ok(None) => false,   // Key sudah ada — skip
            Err(e) => {
                // Redis error — fail open: tetap kirim notif daripada silent skip
                tracing::warn!(key, error = %e, "Redis dedup check failed, sending anyway");
                true
            }
        }
    }

    // ── Private WA senders ────────────────────────────────────────────────────

    async fn send_order_created_wa(
        &self,
        customer_id: &str,
        order: &OrderDetailResponse,
    ) -> anyhow::Result<()> {
        let user = self
            .user_repo
            .find_by_id(customer_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user {customer_id} not found"))?;

        let phone = normalize_phone(&user.phone)?;

        let mut text = format!(
            "🎫 *Pesanan Berhasil Dibuat!*\n\n\
             Kode Order: *{}*\n\
             Total: *Rp {}*\n\n\
             *Detail Tiket:*\n",
            order.order_code, order.total_amount
        );

        for item in &order.items {
            text.push_str(&format!(
                "• {} ({})\n  {}x tiket\n\n",
                item.event_name, item.variant_name, item.quantity
            ));
        }

        text.push_str("Segera lakukan pembayaran sebelum order expired.\nTerima kasih! 🙏");

        self.send_wa(&phone, &text).await
    }

    async fn send_order_paid_wa(
        &self,
        customer_id: &str,
        order: &OrderDetailResponse,
    ) -> anyhow::Result<()> {
        let user = self
            .user_repo
            .find_by_id(customer_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user {customer_id} not found"))?;

        let phone = normalize_phone(&user.phone)?;

        let mut text = format!(
            "✅ *Pembayaran Berhasil!*\n\n\
             Kode Order: *{}*\n\
             Total: *Rp {}*\n\
             Metode: *{}*\n\n\
             *Tiket Kamu:*\n",
            order.order_code,
            order.total_amount,
            order.payment_method.as_deref().unwrap_or("-"),
        );

        for item in &order.items {
            text.push_str(&format!(
                "• {} ({})\n  {}x tiket\n\n",
                item.event_name, item.variant_name, item.quantity
            ));
        }

        text.push_str("Tiketmu sudah aktif. Selamat menikmati event! 🎉");

        self.send_wa(&phone, &text).await
    }

    async fn send_wa(&self, phone: &str, text: &str) -> anyhow::Result<()> {
        let body = json!({
            "chatId":  phone,
            "text":    text,
            "session": self.waha.session,
        });

        let url = format!("{}/api/sendText", self.waha.base_url);
        let mut req = self.http.post(&url).json(&body);
        if !self.waha.api_key.is_empty() {
            req = req.header("X-Api-Key", &self.waha.api_key);
        }

        let res = req.send().await?;
        if !res.status().is_success() {
            let status = res.status();
            let body_text = res.text().await.unwrap_or_default();
            anyhow::bail!("WAHA error {status}: {body_text}");
        }

        Ok(())
    }
}
