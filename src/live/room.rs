use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::sfu::SfuCommand;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub room_id: String,
    pub merchant_id: String,
    pub merchant_name: String,
    pub event_slug: Option<String>,
    pub viewer_count: usize,
    pub started_at: i64,
}

pub struct LiveRoom {
    pub room_id: String,
    pub merchant_id: String,
    pub merchant_name: String,
    pub event_slug: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub cmd_tx: mpsc::Sender<SfuCommand>,
    subscriber_ids: DashMap<String, ()>,
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
            subscriber_ids: DashMap::new(),
        }
    }

    pub fn viewer_count(&self) -> usize {
        self.subscriber_ids.len()
    }

    pub fn add_subscriber(&self, id: &str) {
        self.subscriber_ids.insert(id.to_string(), ());
    }

    pub fn remove_subscriber(&self, id: &str) {
        self.subscriber_ids.remove(id);
    }

    pub fn info(&self) -> RoomInfo {
        RoomInfo {
            room_id: self.room_id.clone(),
            merchant_id: self.merchant_id.clone(),
            merchant_name: self.merchant_name.clone(),
            event_slug: self.event_slug.clone(),
            viewer_count: self.viewer_count(),
            started_at: self.started_at.timestamp_millis(),
        }
    }
}