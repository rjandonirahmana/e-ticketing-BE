use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Satu baris `notifications` — mirror langsung dari skema DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub user_id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub is_read: bool,
    /// Opsional — diisi jika notif berkaitan dengan order tertentu.
    pub order_id: Option<String>,
    /// Opsional — diisi jika notif berkaitan dengan tiket tertentu.
    pub ticket_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Payload untuk membuat notifikasi baru (dari service layer).
#[derive(Debug, Clone)]
pub struct CreateNotificationInput {
    pub user_id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub order_id: Option<String>,
    pub ticket_id: Option<String>,
}
