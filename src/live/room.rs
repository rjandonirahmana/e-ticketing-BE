use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::sfu::SfuCommand;

/// Identitas penonton yang sedang menonton siaran. Dikirim klien saat subscribe
/// dan ditampilkan ke merchant ("siapa saja yang join"). `photo_url` opsional —
/// model user belum punya foto, jadi UI memakai avatar inisial sebagai fallback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewerInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub photo_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub room_id: String,
    pub merchant_id: String,
    pub merchant_name: String,
    pub event_slug: Option<String>,
    pub viewer_count: usize,
    pub started_at: i64,
    /// Daftar penonton yang sedang bergabung.
    #[serde(default)]
    pub viewers: Vec<ViewerInfo>,
}

pub struct LiveRoom {
    pub room_id: String,
    pub merchant_id: String,
    pub merchant_name: String,
    pub event_slug: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub cmd_tx: mpsc::Sender<SfuCommand>,
    subscribers: DashMap<String, ViewerInfo>,
}

impl LiveRoom {
    pub fn new(
        room_id: String,
        merchant_id: String,
        merchant_name: String,
        event_slug: Option<String>,
        cmd_tx: mpsc::Sender<SfuCommand>,
    ) -> Self {
        Self {
            room_id,
            merchant_id,
            merchant_name,
            event_slug,
            started_at: chrono::Utc::now(),
            cmd_tx,
            subscribers: DashMap::new(),
        }
    }

    pub fn viewer_count(&self) -> usize {
        self.subscribers.len()
    }

    pub fn add_subscriber(&self, id: &str, info: ViewerInfo) {
        self.subscribers.insert(id.to_string(), info);
    }

    pub fn remove_subscriber(&self, id: &str) {
        self.subscribers.remove(id);
    }

    pub fn viewers(&self) -> Vec<ViewerInfo> {
        self.subscribers.iter().map(|e| e.value().clone()).collect()
    }

    pub fn info(&self) -> RoomInfo {
        RoomInfo {
            room_id: self.room_id.clone(),
            merchant_id: self.merchant_id.clone(),
            merchant_name: self.merchant_name.clone(),
            event_slug: self.event_slug.clone(),
            viewer_count: self.viewer_count(),
            started_at: self.started_at.timestamp_millis(),
            viewers: self.viewers(),
        }
    }
}