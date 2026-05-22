use anyhow::{bail, Result};
use std::sync::Arc;

use crate::{
    models::group_chat::{GroupMessage, GroupRoom, MemberRole, MsgType, TicketCard},
    repository::group_chat::GroupChatRepository,
    utils::ulid::new_ulid,
    ws::manager::WsManager,
    ws::proto::{WsEvent, WsMessage},
};

/// Limit pesan untuk customer (non-merchant) per group room
const CUSTOMER_MSG_LIMIT: i64 = 1;

pub struct GroupChatService {
    /// FIX: Arc<dyn GroupChatRepository> — konsisten dengan OrderService, AuthService, dll.
    /// Memungkinkan mock/test tanpa database, dan swap implementasi tanpa recompile service.
    pub repo: Arc<dyn GroupChatRepository>,
    pub ws_mgr: Arc<WsManager>,
}

impl GroupChatService {
    pub fn new(repo: Arc<dyn GroupChatRepository>, ws_mgr: Arc<WsManager>) -> Self {
        Self { repo, ws_mgr }
    }

    // ── Room ─────────────────────────────────────────────────────────────────

    /// Buat atau dapatkan room untuk sebuah event.
    /// Merchant yang buat event memanggil ini saat publish event.
    pub async fn get_or_create_room(
        &self,
        event_id: &str,
        event_name: &str,
        cover_url: Option<&str>,
        merchant_id: &str,
    ) -> Result<GroupRoom> {
        self.repo
            .upsert_event_room(event_id, event_name, cover_url, merchant_id)
            .await
    }

    pub async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<GroupRoom>> {
        self.repo.get_user_rooms(user_id).await
    }

    // ── Join room ─────────────────────────────────────────────────────────────

    /// Join room by room_id — dipanggil dari REST handler POST /chat/rooms/:id/join.
    ///
    /// FIX P2: Konsolidasi join logic + system message ke service layer.
    /// Sebelumnya ws/routes.rs join_room handler langsung call repo.add_member()
    /// tanpa kirim system message — inkonsisten dengan auto_join_after_payment()
    /// yang selalu kirim system message.
    ///
    /// Return: room yang di-join (untuk response JSON)
    pub async fn join_room(
        &self,
        room_id: &str,
        user_id: &str,
        user_name: &str,
    ) -> Result<GroupRoom> {
        let room = self
            .repo
            .find_by_id(room_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Room not found"))?;

        // Idempotent — jika sudah member, return OK tanpa duplikat system msg
        if self.repo.is_member(room_id, user_id).await? {
            return Ok(room);
        }

        self.repo
            .add_member(room_id, user_id, MemberRole::Member)
            .await?;

        // System message — konsisten dengan auto_join_after_payment
        let sys = self.build_system_msg(
            room_id,
            &format!("{user_name} bergabung ke grup"),
        );
        self.repo.save_message(&sys).await?;
        self.fanout(room_id, &sys).await;

        tracing::info!(user_id, room_id, "User joined room");
        Ok(room)
    }

    // ── Auto-join setelah bayar event ─────────────────────────────────────────

    /// Dipanggil oleh OrderService setelah `mark_paid_and_issue_tickets` sukses.
    /// event_id diambil dari order items.
    pub async fn auto_join_after_payment(
        &self,
        event_id: &str,
        user_id: &str,
        user_name: &str,
    ) -> Result<()> {
        let room = match self.repo.find_by_event(event_id).await? {
            Some(r) => r,
            None => {
                // Room belum dibuat merchant — buat dengan nama placeholder
                tracing::warn!(
                    event_id,
                    user_id,
                    "No group room found for event, skipping auto-join"
                );
                return Ok(());
            }
        };

        // Jika sudah member, skip
        if self.repo.is_member(&room.id, user_id).await? {
            return Ok(());
        }

        self.repo
            .add_member(&room.id, user_id, MemberRole::Member)
            .await?;

        // Kirim system message ke room
        let sys = self.build_system_msg(
            &room.id,
            &format!("{user_name} bergabung ke grup setelah membeli tiket"),
        );
        self.repo.save_message(&sys).await?;
        self.fanout(&room.id, &sys).await;

        tracing::info!(
            user_id, room_id = %room.id, event_id,
            "User auto-joined event group after payment"
        );
        Ok(())
    }

    // ── Messaging ─────────────────────────────────────────────────────────────

    /// Kirim pesan teks.
    /// `role` = Claims.role ("customer" | "merchant" | "admin")
    pub async fn send_text(
        &self,
        room_id: &str,
        sender_id: &str,
        sender_name: &str,
        role: &str,
        content: &str,
    ) -> Result<GroupMessage> {
        if content.trim().is_empty() {
            bail!("Pesan tidak boleh kosong");
        }

        let msg = GroupMessage {
            id: new_ulid(),
            room_id: room_id.to_string(),
            sender_id: sender_id.to_string(),
            sender_name: sender_name.to_string(),
            msg_type: MsgType::Text,
            content: content.to_string(),
            media_url: None,
            ticket_card: None,
            sent_at: chrono::Utc::now(),
            is_system: false,
        };

        self.authorize_and_save(&msg, role).await?;
        self.fanout(room_id, &msg).await;
        Ok(msg)
    }

    /// Share ticket card ke grup.
    /// Juga menggunakan send-limit yang sama dengan pesan teks.
    pub async fn share_ticket(
        &self,
        room_id: &str,
        sender_id: &str,
        sender_name: &str,
        role: &str,
        ticket: TicketCard,
        caption: &str,
    ) -> Result<GroupMessage> {
        let msg = GroupMessage {
            id: new_ulid(),
            room_id: room_id.to_string(),
            sender_id: sender_id.to_string(),
            sender_name: sender_name.to_string(),
            msg_type: MsgType::SharedTicket,
            content: caption.to_string(),
            media_url: None,
            ticket_card: Some(ticket),
            sent_at: chrono::Utc::now(),
            is_system: false,
        };

        self.authorize_and_save(&msg, role).await?;
        self.fanout(room_id, &msg).await;
        Ok(msg)
    }

    pub async fn get_history(
        &self,
        room_id: &str,
        user_id: &str,
        limit: i64,
        before_id: Option<&str>,
    ) -> Result<(Vec<GroupMessage>, bool)> {
        if !self.repo.is_member(room_id, user_id).await? {
            bail!("Bukan member room ini");
        }
        let limit = limit.clamp(1, 100);
        self.repo.get_history(room_id, limit, before_id).await
    }

    /// Berapa pesan yang sudah dikirim user di room ini (untuk UI hint)
    pub async fn sent_count(&self, room_id: &str, user_id: &str) -> Result<i64> {
        self.repo.count_user_messages(room_id, user_id).await
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    /// Enforce aturan:
    /// - Customer: maks CUSTOMER_MSG_LIMIT pesan per room — dicek secara ATOMIC di DB
    /// - Merchant/Admin: unlimited, langsung save
    async fn authorize_and_save(&self, msg: &GroupMessage, role: &str) -> Result<()> {
        if !self.repo.is_member(&msg.room_id, &msg.sender_id).await? {
            bail!("Bukan member room ini");
        }

        if role == "merchant" || role == "admin" {
            // Unlimited — langsung insert tanpa limit check
            self.repo.save_message(msg).await?;
            return Ok(());
        }

        // FIX: Customer — gunakan atomic CTE insert yang menggabungkan
        // count check + insert dalam satu query. Tidak ada race condition
        // antara check dan insert seperti pola count lalu insert terpisah.
        let inserted = self.repo.save_message_if_under_limit(msg).await?;

        if !inserted {
            bail!(
                "Customer hanya bisa mengirim {} pesan per grup. \
                 Upgrade ke merchant untuk koordinasi tak terbatas.",
                CUSTOMER_MSG_LIMIT
            );
        }

        Ok(())
    }

    fn build_system_msg(&self, room_id: &str, content: &str) -> GroupMessage {
        GroupMessage {
            id: new_ulid(),
            room_id: room_id.to_string(),
            sender_id: "00000000000000000000000000".to_string(),
            sender_name: "System Pulse".to_string(),
            msg_type: MsgType::System,
            content: content.to_string(),
            media_url: None,
            ticket_card: None,
            sent_at: chrono::Utc::now(),
            is_system: true,
        }
    }

    /// FIX: Tidak ada tokio::spawn — await langsung.
    /// Spawn tanpa bound menyebabkan task buildup pada load tinggi (Redis retry
    /// 3× × 200ms worst-case = ~350ms per fanout, 1k msg/s = ~350 outstanding tasks).
    /// broadcast_room local-delivery via try_send (non-blocking), Redis 1 publish.
    /// Latency tambahan ke caller hanya μs untuk local + ~1ms untuk Redis — acceptable.
    async fn fanout(&self, room_id: &str, msg: &GroupMessage) {
        let event = WsEvent::NewMessage(Box::new(WsMessage::from_model(msg)));
        self.ws_mgr.broadcast_room(room_id, event).await;
    }
}
