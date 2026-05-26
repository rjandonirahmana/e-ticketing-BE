// src/components/audio_pill.rs
use leptos::prelude::*;

#[component]
pub fn AudioPill(is_muted: RwSignal<bool>) -> impl IntoView {
    // Local state untuk visualizer dummy (bisa diganti real audio analyser)
    let is_playing = Signal::derive(move || !is_muted.get());

    view! {
        <button
            class="ap-root"
            aria-label=move || if is_muted.get() { "Unmute story" } else { "Mute story" }
            on:click=move |ev: leptos::ev::MouseEvent| {
                ev.stop_propagation();
                is_muted.update(|v| *v = !*v);
            }
        >
            <div class="ap-icon-wrap">
                // Speaker base
                <svg class="ap-speaker" width="18" height="18" viewBox="0 0 24 24"
                     fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
                    <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/>
                    <path d="M15.54 8.46a5 5 0 010 7.07"/>
                    <path d="M19.07 4.93a10 10 0 010 14.14"/>
                </svg>

                // Animated wave bars — CSS only, zero WASM cost
                <div class="ap-waves" class:ap-waves--active=move || is_playing.get()>
                    <span class="ap-bar"></span>
                    <span class="ap-bar"></span>
                    <span class="ap-bar"></span>
                </div>

                // Slash overlay saat muted
                <Show when=move || is_muted.get()>
                    <svg class="ap-slash" width="18" height="18" viewBox="0 0 24 24"
                         fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round">
                        <line x1="1" y1="1" x2="23" y2="23"/>
                    </svg>
                </Show>
            </div>

            <span class="ap-label">{move || if is_muted.get() { "Tap to unmute" } else { "Tap to mute" }}</span>
        </button>
    }
}
