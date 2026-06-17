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
        // SSR guard: spawn_local tidak tersedia di server
        if is_server() {
            return;
        }
        let ctx = *self;
        spawn_local(async move {
            ctx.loading.set(true);
            ctx.error.set(None);
            match crate::web::api::get_story_groups().await {
                Ok(groups) if !groups.is_empty() => ctx.groups.set(groups),
                Ok(_) => ctx.groups.set(mock_story_groups()),
                Err(_) => ctx.groups.set(mock_story_groups()),
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
    ctx.load();
    provide_context(ctx);
}

pub fn use_stories_store() -> StoriesCtx {
    use_context::<StoriesCtx>()
        .expect("StoriesCtx not provided — pastikan provide_stories_store() dipanggil di App")
}

// ── Mock story groups ─────────────────────────────────────────────────────────

fn mock_story_groups() -> Vec<StoryGroup> {
    use chrono::Duration;
    let now = chrono::Utc::now();

    vec![
        StoryGroup {
            user_id: "mock_1".into(),
            username: "ElectronicOasis".into(),
            avatar_url: "https://images.unsplash.com/photo-1507003211169-0a1dd7228f2d?w=100&q=80"
                .into(),
            all_viewed: false,
            stories: vec![
                StoryItem {
                    id: "ms1".into(),
                    user_id: "mock_1".into(),
                    username: "ElectronicOasis".into(),
                    avatar_url:
                        "https://images.unsplash.com/photo-1507003211169-0a1dd7228f2d?w=100&q=80"
                            .into(),
                    media_url:
                        "https://images.unsplash.com/photo-1470225620780-dba8ba36b745?w=800&q=85"
                            .into(),
                    media_type: StoryMediaType::Image,
                    filter: None,
                    overlays: vec![],
                    created_at: now - Duration::hours(2),
                    expires_at: now + Duration::hours(22),
                    viewed: false,
                    event_id: None,
                    event_slug: None,
                    event_title: None,
                },
                StoryItem {
                    id: "ms2".into(),
                    user_id: "mock_1".into(),
                    username: "ElectronicOasis".into(),
                    avatar_url:
                        "https://images.unsplash.com/photo-1507003211169-0a1dd7228f2d?w=100&q=80"
                            .into(),
                    media_url:
                        "https://images.unsplash.com/photo-1501386761578-eac5c94b800a?w=800&q=85"
                            .into(),
                    media_type: StoryMediaType::Image,
                    filter: Some("juno".into()),
                    overlays: vec![StoryOverlay {
                        id: "ov1".into(),
                        overlay_type: OverlayType::Text,
                        x: 50.0,
                        y: 20.0,
                        content: Some("LIVE TONIGHT 🎶".into()),
                        color: Some("#c8ff5e".into()),
                        font_size: Some(32),
                        rotation: Some(0.0),
                        emoji: None,
                        scale: Some(1.0),
                        z_index: 0,
                        text_style: Some("modern".into()),
                        text_align: Some("center".into()),
                    }],
                    created_at: now - Duration::minutes(45),
                    expires_at: now + Duration::hours(23),
                    viewed: false,
                    event_id: None,
                    event_slug: None,
                    event_title: None,
                },
            ],
        },
        StoryGroup {
            user_id: "mock_2".into(),
            username: "NeonPulse".into(),
            avatar_url: "https://images.unsplash.com/photo-1494790108755-2616b612b786?w=100&q=80"
                .into(),
            all_viewed: false,
            stories: vec![StoryItem {
                id: "ms3".into(),
                user_id: "mock_2".into(),
                username: "NeonPulse".into(),
                avatar_url:
                    "https://images.unsplash.com/photo-1494790108755-2616b612b786?w=100&q=80".into(),
                media_url:
                    "https://images.unsplash.com/photo-1514525253161-7a46d19cd819?w=800&q=85".into(),
                media_type: StoryMediaType::Image,
                filter: Some("clarendon".into()),
                overlays: vec![StoryOverlay {
                    id: "ov2".into(),
                    overlay_type: OverlayType::Sticker,
                    x: 75.0,
                    y: 35.0,
                    emoji: Some("🔥".into()),
                    scale: Some(1.5),
                    rotation: Some(-15.0),
                    content: None,
                    color: None,
                    font_size: None,
                    z_index: 0,
                    text_style: None,
                    text_align: None,
                }],
                created_at: now - Duration::minutes(20),
                expires_at: now + Duration::hours(23),
                viewed: false,
                event_id: None,
                event_slug: None,
                event_title: None,
            }],
        },
        StoryGroup {
            user_id: "mock_3".into(),
            username: "BassDrop".into(),
            avatar_url: "https://images.unsplash.com/photo-1500648767791-00dcc994a43e?w=100&q=80"
                .into(),
            all_viewed: true,
            stories: vec![StoryItem {
                id: "ms4".into(),
                user_id: "mock_3".into(),
                username: "BassDrop".into(),
                avatar_url:
                    "https://images.unsplash.com/photo-1500648767791-00dcc994a43e?w=100&q=80".into(),
                media_url:
                    "https://images.unsplash.com/photo-1493225457124-a3eb161ffa5f?w=800&q=85".into(),
                media_type: StoryMediaType::Image,
                filter: Some("moon".into()),
                overlays: vec![],
                created_at: now - Duration::hours(5),
                expires_at: now + Duration::hours(19),
                viewed: true,
                event_id: None,
                event_slug: None,
                event_title: None,
            }],
        },
        StoryGroup {
            user_id: "mock_4".into(),
            username: "VioletStage".into(),
            avatar_url: "https://images.unsplash.com/photo-1438761681033-6461ffad8d80?w=100&q=80"
                .into(),
            all_viewed: false,
            stories: vec![StoryItem {
                id: "ms5".into(),
                user_id: "mock_4".into(),
                username: "VioletStage".into(),
                avatar_url:
                    "https://images.unsplash.com/photo-1438761681033-6461ffad8d80?w=100&q=80".into(),
                media_url:
                    "https://images.unsplash.com/photo-1429962714451-bb934ecdc4ec?w=800&q=85".into(),
                media_type: StoryMediaType::Image,
                filter: None,
                overlays: vec![
                    StoryOverlay {
                        id: "ov3".into(),
                        overlay_type: OverlayType::Text,
                        x: 50.0,
                        y: 75.0,
                        content: Some("Stage 3 • Area Barat".into()),
                        color: Some("#ffffff".into()),
                        font_size: Some(24),
                        rotation: Some(0.0),
                        emoji: None,
                        scale: Some(1.0),
                        z_index: 0,
                        text_style: Some("classic".into()),
                        text_align: Some("left".into()),
                    },
                    StoryOverlay {
                        id: "ov4".into(),
                        overlay_type: OverlayType::Sticker,
                        x: 20.0,
                        y: 60.0,
                        emoji: Some("✨".into()),
                        scale: Some(1.2),
                        rotation: Some(10.0),
                        content: None,
                        color: None,
                        font_size: None,
                        z_index: 0,
                        text_style: None,
                        text_align: None,
                    },
                ],

                created_at: now - Duration::minutes(10),
                expires_at: now + Duration::hours(23),
                viewed: false,
                event_id: None,
                event_slug: None,
                event_title: None,
            }],
        },
        StoryGroup {
            user_id: "mock_5".into(),
            username: "SubFreq".into(),
            avatar_url: "https://images.unsplash.com/photo-1472099645785-5658abf4ff4e?w=100&q=80"
                .into(),
            all_viewed: false,
            stories: vec![
                StoryItem {
                    id: "ms6".into(),
                    user_id: "mock_5".into(),
                    username: "SubFreq".into(),
                    avatar_url:
                        "https://images.unsplash.com/photo-1472099645785-5658abf4ff4e?w=100&q=80"
                            .into(),
                    media_url:
                        "https://images.unsplash.com/photo-1516450360452-9312f5e86fc7?w=800&q=85"
                            .into(),
                    media_type: StoryMediaType::Image,
                    filter: Some("slumber".into()),
                    overlays: vec![],
                    created_at: now - Duration::hours(1),
                    expires_at: now + Duration::hours(23),
                    viewed: false,
                    event_id: None,
                    event_slug: None,
                    event_title: None,
                },
                StoryItem {
                    id: "ms2_video".into(),
                    user_id: "mock_1".into(),
                    username: "ElectronicOasis".into(),
                    avatar_url:
                        "https://images.unsplash.com/photo-1507003211169-0a1dd7228f2d?w=100&q=80"
                            .into(),

                    // ✅ Public CORS-friendly MP4
                    media_url:
                        "https://interactive-examples.mdn.mozilla.net/media/cc0-videos/flower.mp4"
                            .into(),

                    media_type: StoryMediaType::Video,

                    filter: Some("lofi".into()),

                    overlays: vec![
                        StoryOverlay {
                            id: "ov_video_1".into(),
                            overlay_type: OverlayType::Text,
                            x: 50.0,
                            y: 82.0,
                            content: Some("NIGHT DRIVE 🌃".into()),
                            color: Some("#ffffff".into()),
                            font_size: Some(30),
                            rotation: Some(0.0),
                            emoji: None,
                            scale: Some(1.0),
                            z_index: 1,
                            text_style: Some("modern".into()),
                            text_align: Some("center".into()),
                        },
                        StoryOverlay {
                            id: "ov_video_2".into(),
                            overlay_type: OverlayType::Sticker,
                            x: 82.0,
                            y: 18.0,
                            content: None,
                            color: None,
                            font_size: None,
                            rotation: Some(12.0),
                            emoji: Some("🚗".into()),
                            scale: Some(1.4),
                            z_index: 2,
                            text_style: None,
                            text_align: None,
                        },
                    ],

                    created_at: now - Duration::minutes(12),
                    expires_at: now + Duration::hours(23),
                    viewed: false,
                    event_id: None,
                    event_slug: None,
                    event_title: None,
                },
            ],
        },
    ]
}
