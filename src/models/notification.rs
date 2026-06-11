//! models/notification.rs
//!
//! Skema BARU: notifikasi bersifat *polymorphic*.
//!   - `kind`      → diskriminator ("order" | "ticket" | "story" | ...)
//!   - `target_id` → ULID baris yang dirujuk, tabelnya ditentukan `kind`:
//!         kind = "order"  → orders.id
//!         kind = "ticket" → tickets.id
//!         kind = "story"  → stories.id
//!
//! Saat listing, repo ikut menarik data target (`target`) sesuai `kind`
//! supaya UI bisa render kartu tanpa request tambahan.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Nilai kanonik untuk kolom `notifications.kind`.
pub mod kind {
    pub const ORDER: &str = "order";
    pub const TICKET: &str = "ticket";
    pub const STORY: &str = "story";
}

/// Data target yang di-embed ke notifikasi untuk keperluan UI.
/// Bentuknya berbeda per `kind` (internally-tagged via field "kind").
///
/// JSON contoh:
///   { "kind":"order", "order_code":"KN...", "status":"paid", ... }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotificationTarget {
    Story {
        media_url: String,
        /// stories.event_slug (None bila story tidak terkait event)
        slug: Option<String>,
        event_title: Option<String>,
    },
    Order {
        order_code: String,
        status: Option<String>,
        /// string untuk menjaga presisi desimal (mis. "350000.00")
        total_amount: String,
        payment_method: Option<String>,
        expired_at: Option<DateTime<Utc>>,
    },
    Ticket {
        ticket_code: String,
        status: Option<String>,
        used_at: Option<DateTime<Utc>>,
        variant_name: Option<String>,
        event_name: Option<String>,
        event_date: Option<DateTime<Utc>>,
        venue: Option<String>,
        event_slug: Option<String>,
    },
}

/// Representasi 1 baris `notifications` untuk dikirim ke client.
#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    pub id: String,
    pub user_id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub is_read: bool,
    /// ULID target. Tabel tujuan ditentukan oleh `kind`.
    pub target_id: Option<String>,
    /// Label ringkas. Hanya diisi di endpoint DETAIL (bukan list).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_title: Option<String>,
    /// Data target lengkap sesuai `kind`. Hanya diisi di endpoint DETAIL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<NotificationTarget>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Payload untuk membuat notifikasi baru.
///
/// Pakai constructor (`order`, `ticket`, `story`) supaya pasangan
/// kind↔target_id selalu konsisten.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateNotificationInput {
    pub user_id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub target_id: Option<String>,
}

impl CreateNotificationInput {
    /// ORDER (pesanan dibuat / menunggu bayar). `target_id` → orders.id
    pub fn order(
        user_id: impl Into<String>,
        order_id: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            kind: kind::ORDER.into(),
            title: title.into(),
            body: body.into(),
            target_id: Some(order_id.into()),
        }
    }

    /// TICKET (pembayaran sukses → tiket terbit). `target_id` → tickets.id
    pub fn ticket(
        user_id: impl Into<String>,
        ticket_id: impl Into<String>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            kind: kind::TICKET.into(),
            title: title.into(),
            body: body.into(),
            target_id: Some(ticket_id.into()),
        }
    }

    /// STORY (story berhasil diunggah). `target_id` → stories.id
    pub fn story(
        user_id: impl Into<String>,
        story_id: Option<String>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            kind: kind::STORY.into(),
            title: title.into(),
            body: body.into(),
            target_id: story_id,
        }
    }
}
