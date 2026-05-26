/// Story creator — halaman berat WASM (canvas, audio, drag-drop).
/// SSR hanya me-render shell; interaksi aktif setelah hydration selesai.
use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::app::AuthResource;

#[component]
pub fn StoryPage() -> impl IntoView {
    let auth = use_context::<AuthResource>();
    let is_logged_in = move || {
        auth.map(|a| a.get().and_then(|r| r.ok()).flatten().is_some())
            .unwrap_or(false)
    };

    // Client-side redirect jika belum login
    if !is_logged_in() {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(win) = web_sys::window() {
                let _ = win.location().replace("/login");
            }
        }
    }

    view! {
        <div class="page story-page">
            <header class="page-header">
                <A href="/explore" attr:class="back-btn">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="page-logo">"STORY STUDIO"</span>
                <div style="width:36px"></div>
            </header>

            // Canvas area
            <div class="story-canvas-wrap">
                <div class="story-canvas-inner" id="story-canvas">
                    // Background gradient placeholder
                    <div class="story-bg-preview"></div>

                    // Center prompt
                    <div class="story-center-hint">
                        <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" opacity="0.4">
                            <rect x="3" y="3" width="18" height="18" rx="2"/>
                            <path d="M3 9h18M9 21V9"/>
                        </svg>
                        <p>"Tap to add media or text to your story"</p>
                    </div>
                </div>
            </div>

            // Bottom toolbar
            <div class="story-toolbar">
                <button class="story-tool-btn" id="tool-text">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <polyline points="4 7 4 4 20 4 20 7"/>
                        <line x1="9" y1="20" x2="15" y2="20"/>
                        <line x1="12" y1="4" x2="12" y2="20"/>
                    </svg>
                    <span>"Text"</span>
                </button>
                <button class="story-tool-btn" id="tool-image">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <rect x="3" y="3" width="18" height="18" rx="2"/>
                        <circle cx="8.5" cy="8.5" r="1.5"/>
                        <polyline points="21 15 16 10 5 21"/>
                    </svg>
                    <span>"Image"</span>
                </button>
                <button class="story-tool-btn story-tool-btn--accent" id="tool-music">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M9 18V5l12-2v13"/>
                        <circle cx="6" cy="18" r="3"/>
                        <circle cx="18" cy="16" r="3"/>
                    </svg>
                    <span>"Music"</span>
                </button>
                <button class="story-tool-btn" id="tool-sticker">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <circle cx="12" cy="12" r="10"/>
                        <path d="M8 14s1.5 2 4 2 4-2 4-2"/>
                        <line x1="9" y1="9" x2="9.01" y2="9"/>
                        <line x1="15" y1="9" x2="15.01" y2="9"/>
                    </svg>
                    <span>"Sticker"</span>
                </button>
                <button class="story-tool-btn story-tool-btn--post" id="tool-post">
                    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <line x1="22" y1="2" x2="11" y2="13"/>
                        <polygon points="22 2 15 22 11 13 2 9 22 2"/>
                    </svg>
                    <span>"Post"</span>
                </button>
            </div>

            // Color palette
            <div class="story-palette" id="story-palette">
                {["#c8ff5e","#ff6b6b","#4f6bff","#ffd166","#06d6a0","#ffffff","#000000"]
                    .iter()
                    .map(|color| {
                        let c = *color;
                        view! {
                            <button
                                class="story-color-swatch"
                                style=format!("background:{c}")
                                data-color=c
                            ></button>
                        }
                    })
                    .collect_view()}
            </div>

            <style>
                "
                .story-page { min-height:100vh; display:flex; flex-direction:column; background:#0a0a18; overflow:hidden; }
                .story-canvas-wrap { flex:1; display:flex; align-items:center; justify-content:center; padding:16px; }
                .story-canvas-inner { position:relative; width:min(360px,100%); aspect-ratio:9/16; border-radius:20px; overflow:hidden; background:#141428; border:1px solid rgba(255,255,255,0.08); touch-action:none; }
                .story-bg-preview { position:absolute; inset:0; background:linear-gradient(135deg,#0d0d2a 0%,#1a0a3a 50%,#0a1a2a 100%); }
                .story-center-hint { position:absolute; inset:0; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:12px; color:rgba(255,255,255,0.3); text-align:center; padding:24px; pointer-events:none; }
                .story-center-hint p { font-size:13px; line-height:1.5; }
                .story-toolbar { display:flex; justify-content:space-around; align-items:center; padding:16px 8px; background:rgba(255,255,255,0.03); border-top:1px solid rgba(255,255,255,0.06); }
                .story-tool-btn { display:flex; flex-direction:column; align-items:center; gap:4px; background:none; border:none; color:#8888aa; cursor:pointer; padding:8px 12px; border-radius:12px; font-size:10px; letter-spacing:0.5px; transition:all 0.2s; }
                .story-tool-btn:active { background:rgba(255,255,255,0.06); color:#fff; }
                .story-tool-btn--accent { color:#c8ff5e; }
                .story-tool-btn--post { color:#4f6bff; }
                .story-palette { display:flex; justify-content:center; gap:10px; padding:12px 16px; }
                .story-color-swatch { width:28px; height:28px; border-radius:50%; border:2px solid rgba(255,255,255,0.15); cursor:pointer; transition:transform 0.15s; }
                .story-color-swatch:active { transform:scale(1.2); }
                "
            </style>
        </div>
    }
}
