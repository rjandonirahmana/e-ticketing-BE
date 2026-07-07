//! Stories store (web) — definisi tipe model kanonik berada di file ini.
//! Data di-fetch via server function `web::api::get_story_groups()` (jalur SSR).
use chrono::{DateTime, Utc};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::{Deserialize, Serialize};

// ── Model (definisi kanonik — sebelumnya di csr::state::stories) ───────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoryMediaType {
    Image,
    Video,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayType {
    Text,
    Sticker,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoryOverlay {
    pub id: String,
    pub overlay_type: OverlayType,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub font_size: Option<i32>,
    #[serde(default)]
    pub rotation: Option<f64>,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub z_index: i32,
    #[serde(default)]
    pub text_style: Option<String>,
    #[serde(default)]
    pub text_align: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoryItem {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub avatar_url: String,
    pub media_url: String,
    pub media_type: StoryMediaType,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub overlays: Vec<StoryOverlay>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(default)]
    pub viewed: bool,
    #[serde(default)]
    pub event_id: Option<String>,
    #[serde(default)]
    pub event_slug: Option<String>,
    #[serde(default)]
    pub event_title: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoryGroup {
    pub user_id: String,
    pub username: String,
    pub avatar_url: String,
    #[serde(default)]
    pub all_viewed: bool,
    pub stories: Vec<StoryItem>,
}

/// Preview ringan story pertama sebuah grup — dipakai face cube saat swipe
/// antar-user (drag horizontal ala Instagram).
#[derive(Clone, Debug, PartialEq)]
pub struct GroupPreview {
    pub username: String,
    pub avatar_url: String,
    pub media_url: String,
    pub is_video: bool,
}

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct StoriesCtx {
    pub groups: RwSignal<Vec<StoryGroup>>,
    pub loading: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub active_group: RwSignal<Option<usize>>,
    pub active_story_idx: RwSignal<usize>,
    pub progress: RwSignal<f64>,
    pub interval_handle: RwSignal<Option<i32>>,
    /// Story yang sedang di-upload (set true saat upload dimulai, false saat selesai/gagal).
    /// Dipakai StoryBar untuk menampilkan progress ring animasi di avatar user.
    pub uploading: RwSignal<bool>,
}

impl StoriesCtx {
    pub fn load(&self) {
        if is_server() { return; }
        // Cegah concurrent load — jika sedang loading, abaikan panggilan berikutnya.
        if self.loading.get_untracked() { return; }
        let ctx = *self;
        spawn_local(async move {
            ctx.loading.set(true);
            ctx.error.set(None);
            match crate::web::api::get_story_groups().await {
                Ok(groups) => ctx.groups.set(groups),
                Err(e) => ctx.error.set(Some(e.to_string())),
            }
            ctx.loading.set(false);
        });
    }

    pub fn open(&self, group_idx: usize) {
        let valid = self.groups.with(|g| {
            g.get(group_idx)
                .map(|gr| !gr.stories.is_empty())
                .unwrap_or(false)
        });
        if !valid {
            return;
        }
        self.clear_interval();
        self.active_group.set(Some(group_idx));
        self.active_story_idx.set(0);
        self.progress.set(0.0);
        self.mark_current_viewed();
    }

    pub fn close(&self) {
        self.clear_interval();
        self.active_group.set(None);
        self.active_story_idx.set(0);
        self.progress.set(0.0);
    }

    /// Buka viewer pada grup + indeks story tertentu — dipakai "Story Saya" di
    /// profil yang membuka satu grup pada story yang di-klik.
    pub fn open_at(&self, group_idx: usize, story_idx: usize) {
        let valid = self.groups.with_untracked(|g| {
            g.get(group_idx)
                .map(|gr| story_idx < gr.stories.len())
                .unwrap_or(false)
        });
        if !valid {
            return;
        }
        self.clear_interval();
        self.active_group.set(Some(group_idx));
        self.active_story_idx.set(story_idx);
        self.progress.set(0.0);
        self.mark_current_viewed();
    }

    /// Hapus story yang sedang tampil dari state lokal, lalu lanjut ke story
    /// berikutnya (atau tutup viewer bila grup jadi kosong). Penghapusan di
    /// server dilakukan terpisah lewat server fn `delete_my_story`.
    pub fn remove_current_story(&self) {
        let Some(gi) = self.active_group.get_untracked() else {
            return;
        };
        let si = self.active_story_idx.get_untracked();
        let mut should_close = false;
        let mut new_idx = si;
        self.groups.update(|groups| {
            if let Some(g) = groups.get_mut(gi) {
                if si < g.stories.len() {
                    g.stories.remove(si);
                }
                if g.stories.is_empty() {
                    should_close = true;
                } else if si >= g.stories.len() {
                    new_idx = g.stories.len() - 1;
                }
            }
        });
        if should_close {
            self.close();
        } else {
            self.progress.set(0.0);
            // set() memberi notifikasi → effect story-change re-run (RAF reset).
            self.active_story_idx.set(new_idx);
        }
    }

    pub fn next(&self) {
        let Some(gi) = self.active_group.get_untracked() else {
            return;
        };
        let groups = self.groups.get_untracked();
        let Some(group) = groups.get(gi) else {
            return;
        };
        let si = self.active_story_idx.get_untracked();

        if si + 1 < group.stories.len() {
            self.active_story_idx.set(si + 1);
            self.progress.set(0.0);
            self.mark_current_viewed();
        } else if gi + 1 < groups.len() {
            self.clear_interval();
            self.active_group.set(Some(gi + 1));
            self.active_story_idx.set(0);
            self.progress.set(0.0);
            self.mark_current_viewed();
        } else {
            self.close();
        }
    }

    pub fn prev(&self) {
        let Some(gi) = self.active_group.get_untracked() else {
            return;
        };
        let si = self.active_story_idx.get_untracked();
        if si > 0 {
            self.active_story_idx.set(si - 1);
            self.progress.set(0.0);
        } else if gi > 0 {
            self.clear_interval();
            let groups = self.groups.get_untracked();
            let prev_len = groups.get(gi - 1).map(|g| g.stories.len()).unwrap_or(1);
            self.active_group.set(Some(gi - 1));
            self.active_story_idx.set(prev_len.saturating_sub(1));
            self.progress.set(0.0);
        }
    }

    /// Pindah ke grup (user) BERIKUTNYA — dipakai swipe horizontal ala Instagram.
    /// Berbeda dengan `next()`: langsung lompat user walau story user saat ini
    /// masih tersisa. Di grup terakhir → tutup viewer (perilaku Instagram).
    pub fn next_group(&self) {
        let Some(gi) = self.active_group.get_untracked() else {
            return;
        };
        let len = self.groups.with_untracked(|g| g.len());
        if gi + 1 < len {
            self.clear_interval();
            self.active_group.set(Some(gi + 1));
            self.active_story_idx.set(0);
            self.progress.set(0.0);
            self.mark_current_viewed();
        } else {
            self.close();
        }
    }

    /// Pindah ke grup (user) SEBELUMNYA. Di grup pertama tidak melakukan apa-apa
    /// (viewer melakukan rubber-band snap back).
    pub fn prev_group(&self) {
        let Some(gi) = self.active_group.get_untracked() else {
            return;
        };
        if gi == 0 {
            return;
        }
        self.clear_interval();
        self.active_group.set(Some(gi - 1));
        self.active_story_idx.set(0);
        self.progress.set(0.0);
        self.mark_current_viewed();
    }

    pub fn has_next_group(&self) -> bool {
        let Some(gi) = self.active_group.get_untracked() else {
            return false;
        };
        self.groups.with_untracked(|g| gi + 1 < g.len())
    }

    pub fn has_prev_group(&self) -> bool {
        self.active_group
            .get_untracked()
            .map(|gi| gi > 0)
            .unwrap_or(false)
    }

    /// Data ringan untuk face cube tetangga saat drag (story pertama grup di
    /// offset ±1 dari grup aktif). Dipanggil reaktif — tracking `groups` dan
    /// `active_group` agar preview ikut ter-update.
    pub fn group_preview(&self, offset: isize) -> Option<GroupPreview> {
        let gi = self.active_group.get()? as isize + offset;
        if gi < 0 {
            return None;
        }
        self.groups.with(|g| {
            let group = g.get(gi as usize)?;
            let story = group.stories.first()?;
            Some(GroupPreview {
                username: group.username.clone(),
                avatar_url: group.avatar_url.clone(),
                media_url: story.media_url.clone(),
                is_video: matches!(story.media_type, StoryMediaType::Video),
            })
        })
    }

    pub fn set_interval_handle(&self, id: i32) {
        self.clear_interval();
        untrack(|| self.interval_handle.set(Some(id)));
    }

    pub fn clear_interval(&self) {
        let handle = untrack(|| self.interval_handle.get());
        if let Some(_id) = handle {
            #[cfg(target_arch = "wasm32")]
            if let Some(win) = web_sys::window() {
                win.clear_interval_with_handle(_id);
            }
            untrack(|| self.interval_handle.set(None));
        }
    }

    fn mark_current_viewed(&self) {
        let Some(gi) = self.active_group.get_untracked() else {
            return;
        };
        let si = self.active_story_idx.get_untracked();
        self.groups.update(|groups| {
            if let Some(group) = groups.get_mut(gi) {
                if let Some(story) = group.stories.get_mut(si) {
                    story.viewed = true;
                }
                group.all_viewed = group.stories.iter().all(|s| s.viewed);
            }
        });
    }

    pub fn current_story(&self) -> Option<StoryItem> {
        let gi = self.active_group.get()?;
        let si = self.active_story_idx.get();
        self.groups.with(|g| g.get(gi)?.stories.get(si).cloned())
    }

    pub fn with_current_story<T>(&self, f: impl FnOnce(&StoryItem) -> T) -> Option<T> {
        let gi = self.active_group.get()?;
        let si = self.active_story_idx.get();
        self.groups.with(|g| g.get(gi)?.stories.get(si).map(f))
    }

    pub fn next_story_url(&self) -> Option<String> {
        let gi = self.active_group.get()?;
        let si = self.active_story_idx.get();
        self.groups.with(|g| {
            let group = g.get(gi)?;
            if si + 1 < group.stories.len() {
                return group.stories.get(si + 1).map(|s| s.media_url.clone());
            }
            g.get(gi + 1)
                .and_then(|ng| ng.stories.first())
                .map(|s| s.media_url.clone())
        })
    }

    pub fn current_group_len(&self) -> usize {
        let Some(gi) = self.active_group.get() else {
            return 0;
        };
        self.groups
            .with(|g| g.get(gi).map(|gr| gr.stories.len()).unwrap_or(0))
    }
}

// ── Provider & hook ───────────────────────────────────────────────────────────

pub fn provide_stories_store() {
    let ctx = StoriesCtx {
        groups: RwSignal::new(Vec::new()),
        loading: RwSignal::new(false),
        error: RwSignal::new(None),
        active_group: RwSignal::new(None),
        active_story_idx: RwSignal::new(0),
        progress: RwSignal::new(0.0),
        interval_handle: RwSignal::new(None),
        uploading: RwSignal::new(false),
    };
    // Tidak auto-load: StoryBar (konsumen utama) memanggil ctx.load() sendiri
    // saat mount, sehingga data hanya di-fetch di halaman Explore dan Messages.
    provide_context(ctx);
}

pub fn use_stories_store() -> StoriesCtx {
    use_context::<StoriesCtx>()
        .expect("StoriesCtx not provided — pastikan provide_stories_store() dipanggil di App")
}

