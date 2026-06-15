use leptos::prelude::*;

#[component]
pub fn GridBackground() -> impl IntoView {
    view! {
        <div class="grid-bg" aria-hidden="true">
            <div class="grid-lines"></div>
            <div class="orb orb-1"></div>
            <div class="orb orb-2"></div>
        </div>
    }
}

#[component]
pub fn ErrorBanner(message: String) -> impl IntoView {
    view! {
        <div class="error-banner" role="alert">
            <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="none"
                stroke="#ff4f6b"
                stroke-width="2"
            >
                <circle cx="12" cy="12" r="10" />
                <line x1="12" y1="8" x2="12" y2="12" />
                <line x1="12" y1="16" x2="12.01" y2="16" />
            </svg>
            <span>{message}</span>
        </div>
    }
}

#[component]
pub fn KineticInput(
    id: &'static str,
    label: &'static str,
    #[prop(default = "text")] input_type: &'static str,
    value: RwSignal<String>,
    #[prop(optional)] placeholder: &'static str,
    #[prop(optional)] autocomplete: &'static str,
    #[prop(default = false)] disabled: bool,
) -> impl IntoView {
    let focused = RwSignal::new(false);
    let show_pass = RwSignal::new(false);
    let is_password = input_type == "password";

    let computed_type = move || {
        if is_password && show_pass.get() {
            "text".to_string()
        } else {
            input_type.to_string()
        }
    };

    let box_class = move || {
        if focused.get() {
            "field-box field-box--focused"
        } else {
            "field-box"
        }
    };

    view! {
        <div class="field-wrap">
            <label for=id class="field-label">
                {label}
            </label>
            <div class=box_class>
                <input
                    id=id
                    type=computed_type
                    placeholder=placeholder
                    autocomplete=autocomplete
                    disabled=disabled
                    class="field-input"
                    prop:value=move || value.get()
                    on:input=move |ev| value.set(event_target_value(&ev))
                    on:focus=move |_| focused.set(true)
                    on:blur=move |_| focused.set(false)
                />
                {if is_password {
                    view! {
                        <button
                            type="button"
                            class="pass-toggle"
                            tabindex="-1"
                            on:click=move |_| show_pass.update(|s| *s = !*s)
                        >
                            {move || {
                                if show_pass.get() {
                                    view! {
                                        <svg
                                            width="18"
                                            height="18"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2"
                                        >
                                            <path d="M17.94 17.94A10.07 10.07 0 0112 20c-7 0-11-8-11-8a18.45 18.45 0 015.06-5.94M9.9 4.24A9.12 9.12 0 0112 4c7 0 11 8 11 8a18.5 18.5 0 01-2.16 3.19m-6.72-1.07a3 3 0 11-4.24-4.24" />
                                            <line x1="1" y1="1" x2="23" y2="23" />
                                        </svg>
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <svg
                                            width="18"
                                            height="18"
                                            viewBox="0 0 24 24"
                                            fill="none"
                                            stroke="currentColor"
                                            stroke-width="2"
                                        >
                                            <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                                            <circle cx="12" cy="12" r="3" />
                                        </svg>
                                    }
                                        .into_any()
                                }
                            }}
                        </button>
                    }
                        .into_any()
                } else {
                    ().into_any()
                }}
            </div>
        </div>
    }
}

#[component]
pub fn EmptyState(
    icon: &'static str,
    title: &'static str,
    #[prop(into)] body: String,
    #[prop(optional)] cta_label: Option<&'static str>,
    #[prop(optional)] cta_href: Option<&'static str>,
) -> impl IntoView {
    view! {
        <div class="empty-state">
            <div class="empty-state-icon">{icon}</div>
            <div class="empty-state-title">{title}</div>
            <div class="empty-state-body">{body}</div>
            {cta_label
                .zip(cta_href)
                .map(|(label, href)| {
                    view! {
                        <a href=href class="empty-state-cta">
                            {label}
                        </a>
                    }
                })}
        </div>
    }
}
