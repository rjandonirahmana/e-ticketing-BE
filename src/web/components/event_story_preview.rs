// src/components/event_story_preview.rs
// Inline story preview untuk form Create Event.
// CSS-based (bukan canvas) supaya real-time tanpa async.

use leptos::prelude::*;

/// Inline 9:16 Instagram-frame preview.
/// Dipasang di MerchantCreateEventPage setelah cover upload.
/// Update otomatis saat title / cover / desc berubah.
#[component]
pub fn EventStoryPreviewInline(
    /// Signal judul event (f_name)
    title: Signal<String>,
    /// Signal blob URL cover (cover_preview, Option<String>)
    cover_url: Signal<Option<String>>,
    /// Signal deskripsi event (f_desc)
    description: Signal<String>,
    /// Callback saat user klik "Langsung Bagikan ke Cerita"
    /// Hanya muncul jika cover & title sudah diisi.
    #[prop(optional)]
    on_share_click: Option<Callback<()>>,
) -> impl IntoView {
    let show = RwSignal::new(false);

    // Potong deskripsi max 3 baris ~120 char untuk preview
    let short_desc = move || {
        let d = description.get();
        if d.len() > 120 {
            format!("{}…", &d[..117])
        } else {
            d
        }
    };

    let has_cover = move || cover_url.get().is_some();
    let has_title = move || !title.get().is_empty();
    let can_share = move || has_cover() && has_title();

    view! {
        <div class="esp-section">
            // Toggle button
            <button class="esp-toggle-btn" on:click=move |_| show.update(|v| *v = !*v)>
                <svg
                    width="14"
                    height="14"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2.5"
                    stroke-linecap="round"
                >
                    <rect x="2" y="3" width="20" height="14" rx="2" />
                    <line x1="8" y1="21" x2="16" y2="21" />
                    <line x1="12" y1="17" x2="12" y2="21" />
                </svg>
                {move || {
                    if show.get() { "Sembunyikan Preview Story" } else { "Preview Story Instagram" }
                }}
            </button>

            <Show when=move || show.get()>
                <div class="esp-wrapper">
                    // ── Phone frame ──────────────────────────────────────
                    <div class="esp-phone">
                        // Status bar notch simulasi
                        <div class="esp-notch">
                            <span class="esp-notch-time">"9:41"</span>
                            <div class="esp-notch-icons">
                                <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
                                    <path
                                        d="M1 6l4-4 4 4M1 18l4 4 4-4"
                                        stroke="currentColor"
                                        stroke-width="2"
                                        fill="none"
                                    />
                                </svg>
                                <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor">
                                    <rect x="2" y="7" width="20" height="11" rx="2" ry="2" />
                                    <path d="M22 11V13" stroke="white" stroke-width="2" />
                                </svg>
                            </div>
                        </div>

                        // Story canvas
                        <div class="esp-canvas">
                            // ── Cover zona 70% ────────────────────────────
                            <div class="esp-cover-zone">
                                {move || match cover_url.get() {
                                    Some(url) => {
                                        view! {
                                            <img src=url class="esp-cover-img" alt="Cover preview" />
                                        }
                                            .into_any()
                                    }
                                    None => {
                                        view! {
                                            <div class="esp-cover-empty">
                                                <svg
                                                    width="32"
                                                    height="32"
                                                    viewBox="0 0 24 24"
                                                    fill="none"
                                                    stroke="rgba(255,255,255,0.3)"
                                                    stroke-width="1.5"
                                                >
                                                    <rect x="3" y="3" width="18" height="18" rx="2" />
                                                    <circle cx="8.5" cy="8.5" r="1.5" />
                                                    <polyline points="21 15 16 10 5 21" />
                                                </svg>
                                                <span>"Upload cover dulu"</span>
                                            </div>
                                        }
                                            .into_any()
                                    }
                                }} // Vignette gradient bawah
                                <div class="esp-vignette"></div> // Separator neon
                                <div class="esp-sep"></div>
                            </div>

                            // ── Info panel 30% ────────────────────────────
                            <div class="esp-info">
                                // Badge LIVE PULSE
                                <div class="esp-badge">
                                    <span class="esp-badge-dot"></span>
                                    "LIVE PULSE"
                                </div>

                                // Judul event
                                <p class="esp-title">
                                    {move || {
                                        let t = title.get();
                                        if t.is_empty() {
                                            "Judul event kamu…".to_string()
                                        } else {
                                            t
                                        }
                                    }}
                                </p>

                                // Deskripsi (opsional)
                                {move || {
                                    let d = short_desc();
                                    (!d.is_empty()).then(|| view! { <p class="esp-desc">{d}</p> })
                                }}

                                // CTA
                                <div class="esp-cta">"TAP TO VIEW  →"</div>
                            </div>
                        </div>

                        // Progress bar simulasi di atas (Instagram style)
                        <div class="esp-progress-bar">
                            <div class="esp-progress-fill"></div>
                        </div>

                        // Header story (avatar + username)
                        <div class="esp-story-header">
                            <div class="esp-avatar-ring">
                                <div class="esp-avatar-inner"></div>
                            </div>
                            <span class="esp-username">"Kamu"</span>
                            <span class="esp-story-time">"Baru saja"</span>
                        </div>
                    </div>

                    // ── Share button (muncul jika cover + title siap) ─────
                    {move || {
                        can_share()
                            .then(|| {
                                let cb = on_share_click;
                                view! {
                                    <button
                                        class="esp-share-btn"
                                        on:click=move |_| {
                                            if let Some(c) = cb {
                                                c.run(());
                                            }
                                        }
                                    >
                                        <svg
                                            width="15"
                                            height="15"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2.5"
                                            stroke-linecap="round"
                                        >
                                            <circle cx="18" cy="5" r="3" />
                                            <circle cx="6" cy="12" r="3" />
                                            <circle cx="18" cy="19" r="3" />
                                            <line x1="8.59" y1="13.51" x2="15.42" y2="17.49" />
                                            <line x1="15.41" y1="6.51" x2="8.59" y2="10.49" />
                                        </svg>
                                        "Langsung Bagikan ke Cerita"
                                    </button>
                                }
                            })
                    }}

                    // Hint jika belum lengkap
                    {move || {
                        (!can_share())
                            .then(|| {
                                view! {
                                    <p class="esp-hint">
                                        {move || {
                                            if !has_cover() && !has_title() {
                                                "Isi judul dan upload cover untuk berbagi"
                                            } else if !has_cover() {
                                                "Upload cover untuk bisa berbagi ke cerita"
                                            } else {
                                                "Isi judul event untuk bisa berbagi ke cerita"
                                            }
                                        }}
                                    </p>
                                }
                            })
                    }}
                </div>
            </Show>
        </div>
    }
}
