use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;

pub const WEB: &str = "/";

/// Navigate helper — semua route tanpa prefix, langsung dari root.
///
/// nav("/login")       → navigate ke /login
/// nav("/")            → navigate ke /
/// nav("/events/123")  → navigate ke /events/123
pub fn use_nav() -> impl Fn(&str, NavigateOptions) + Clone {
    let navigate = use_navigate();
    move |path: &str, opts: NavigateOptions| {
        let full = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        };
        navigate(&full, opts);
    }
}
