use reqwest::Client as HttpClient;
use serde_json::json;
use std::sync::Arc;
use tracing;

use crate::config::config::WahaConfig;
use crate::models::orders::OrderDetailResponse;
use crate::repository::user::UserRepository;
use crate::utils::error::notify_background_error;
use crate::utils::phone::normalize_phone;

#[derive(Clone)]
pub struct NotificationService {
    http: HttpClient,
    waha: Arc<WahaConfig>,
    user_repo: Arc<dyn UserRepository>,
}

impl NotificationService {
    pub fn new(
        http: HttpClient,
        waha: Arc<WahaConfig>,
        user_repo: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            http,
            waha,
            user_repo,
        }
    }

    // ── Fire-and-forget spawners ──────────────────────────────────────────────

    /// Kirim notifikasi WA saat order berhasil dibuat.
    /// Fire-and-forget: tidak blocking response API.
    pub fn notify_order_created(&self, customer_id: String, order: OrderDetailResponse) {
        let svc = self.clone();
        tokio::spawn(async move {
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
                // BUG FIX: forward error ke Telegram admin alert.
                // Sebelumnya hanya tracing::error — tidak ada visibilitas saat WA down.
                notify_background_error("WA_OrderCreated", detail);
            } else {
                tracing::info!(
                    customer_id,
                    order_id = %order.id,
                    "WA order-created notification sent"
                );
            }
        });
    }

    /// Kirim notifikasi WA saat pembayaran berhasil.
    /// Fire-and-forget: tidak blocking response API.
    ///
    /// BUG FIX: sebelumnya pay() tidak mengirim notifikasi apapun ke customer.
    pub fn notify_order_paid(&self, customer_id: String, order: OrderDetailResponse) {
        let svc = self.clone();
        tokio::spawn(async move {
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
                tracing::info!(
                    customer_id,
                    order_id = %order.id,
                    "WA order-paid notification sent"
                );
            }
        });
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

    /// Helper: kirim pesan ke WAHA API.
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
