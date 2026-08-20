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
    /// connection_id → info penonton (satu user bisa punya beberapa koneksi/tab).
    subscribers: DashMap<String, ViewerInfo>,
    /// user_id → jumlah koneksi hidup. Menjaga `viewer_count()` tetap O(1) &
    /// ter-dedupe tanpa alokasi HashSet tiap panggilan (dipanggil di jalur join
    /// panas). CATATAN: AtomicUsize naif TIDAK bisa dipakai di sini karena akan
    /// menghitung ganda user multi-tab; refcount per-user inilah yang benar.
    unique_viewers: DashMap<String, u32>,
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
            unique_viewers: DashMap::new(),
        }
    }

    /// Jumlah penonton unik (dedupe by user id). Satu user dengan beberapa
    /// koneksi/tab dihitung sekali. O(1): baca panjang peta refcount per-user.
    pub fn viewer_count(&self) -> usize {
        self.unique_viewers.len()
    }

    pub fn add_subscriber(&self, id: &str, info: ViewerInfo) {
        // Pembuat live (merchant) tidak dihitung sebagai penonton.
        if info.id == self.merchant_id {
            return;
        }
        let user_id = info.id.clone();
        // Hanya bump refcount user bila ini koneksi BARU (bukan replace key sama).
        if self.subscribers.insert(id.to_string(), info).is_none() {
            *self.unique_viewers.entry(user_id).or_insert(0) += 1;
        }
    }

    pub fn remove_subscriber(&self, id: &str) {
        if let Some((_, info)) = self.subscribers.remove(id) {
            // Turunkan refcount user; hapus entri saat koneksi terakhir pergi.
            // Drop ref sebelum remove pada map yang sama → hindari deadlock DashMap.
            // `remove_if` menguji DAN menghapus di bawah kunci shard yang sama.
            // Versi sebelumnya melepas kunci (`drop(cnt)`) lalu menghapus tanpa
            // syarat: bila tepat di celah itu tab baru milik user yang sama
            // tersambung dan menaikkan hitungannya jadi 2, entrinya tetap
            // terhapus — penonton yang masih menonton hilang dari hitungan, dan
            // saat ia benar-benar pergi tak ada lagi entri untuk dikurangi.
            if self
                .unique_viewers
                .remove_if(&info.id, |_, cnt| *cnt <= 1)
                .is_none()
            {
                if let Some(mut cnt) = self.unique_viewers.get_mut(&info.id) {
                    *cnt -= 1;
                }
            }
        }
    }

    /// Daftar penonton unik (dedupe by user id).
    pub fn viewers(&self) -> Vec<ViewerInfo> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for e in self.subscribers.iter() {
            if seen.insert(e.value().id.clone()) {
                out.push(e.value().clone());
            }
        }
        out
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