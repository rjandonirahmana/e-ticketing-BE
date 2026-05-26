use leptos::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "light" => Some(Theme::Light),
            "dark" => Some(Theme::Dark),
            _ => None,
        }
    }
    pub fn toggled(&self) -> Self {
        match self {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        }
    }
}

#[derive(Clone, Copy)]
pub struct ThemeCtx {
    pub theme: RwSignal<Theme>,
}

const STORAGE_KEY: &str = "kinetic.theme";

fn read_initial_theme() -> Theme {
    let win = match web_sys::window() {
        Some(w) => w,
        None => return Theme::Dark,
    };

    if let Ok(Some(storage)) = win.local_storage() {
        if let Ok(Some(v)) = storage.get_item(STORAGE_KEY) {
            if let Some(t) = Theme::from_str(&v) {
                return t;
            }
        }
    }

    if let Ok(Some(media)) = win.match_media("(prefers-color-scheme: light)") {
        if media.matches() {
            return Theme::Light;
        }
    }
    Theme::Dark
}

fn apply_theme(theme: Theme) {
    if let Some(win) = web_sys::window() {
        if let Some(doc) = win.document() {
            if let Some(root) = doc.document_element() {
                let _ = root.set_attribute("data-theme", theme.as_str());
            }
        }
        if let Ok(Some(storage)) = win.local_storage() {
            let _ = storage.set_item(STORAGE_KEY, theme.as_str());
        }
    }
}

pub fn provide_theme() {
    let initial = read_initial_theme();
    let theme = RwSignal::new(initial);
    apply_theme(initial);

    Effect::new(move |_| {
        let t = theme.get();
        apply_theme(t);
    });

    provide_context(ThemeCtx { theme });
}

pub fn use_theme() -> ThemeCtx {
    use_context::<ThemeCtx>().expect("ThemeCtx not provided. Call provide_theme() in App.")
}

#[component]
pub fn ThemeToggle() -> impl IntoView {
    let ctx = use_theme();
    let theme = ctx.theme;

    let on_click = move |_| {
        theme.update(|t| *t = t.toggled());
    };

    let label = move || match theme.get() {
        Theme::Dark => "Switch to light mode",
        Theme::Light => "Switch to dark mode",
    };

    view! {
        <button
            type="button"
            class="theme-toggle"
            aria-label=label
            title=label
            on:click=on_click
        >
            {move || match theme.get() {
                Theme::Dark => view! {
                    // Sun icon — tampil saat mode dark (klik untuk pindah ke light)
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                        stroke="currentColor" stroke-width="2"
                        stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="12" cy="12" r="4"/>
                        <line x1="12" y1="2" x2="12" y2="5"/>
                        <line x1="12" y1="19" x2="12" y2="22"/>
                        <line x1="2" y1="12" x2="5" y2="12"/>
                        <line x1="19" y1="12" x2="22" y2="12"/>
                        <line x1="4.2" y1="4.2" x2="6.3" y2="6.3"/>
                        <line x1="17.7" y1="17.7" x2="19.8" y2="19.8"/>
                        <line x1="4.2" y1="19.8" x2="6.3" y2="17.7"/>
                        <line x1="17.7" y1="6.3" x2="19.8" y2="4.2"/>
                    </svg>
                }.into_any(),
                Theme::Light => view! {
                    // Moon icon — tampil saat mode light (klik untuk pindah ke dark)
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                        stroke="currentColor" stroke-width="2"
                        stroke-linecap="round" stroke-linejoin="round">
                        <path d="M21 12.8A9 9 0 1111.2 3a7 7 0 009.8 9.8z"/>
                    </svg>
                }.into_any(),
            }}
        </button>
    }
}
