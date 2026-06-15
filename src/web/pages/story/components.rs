// ═══════════════════════════════════════════════════════════════════════════════
//  STORY — Leaf components: TombolAlat, TombolWarna, PanelGeser
// ═══════════════════════════════════════════════════════════════════════════════

use leptos::prelude::*;

use super::types::Alat;

#[component]
pub(super) fn TombolAlat(
    aktif: RwSignal<Alat>,
    target: Alat,
    label: &'static str,
    on_click: impl Fn(leptos::ev::MouseEvent) + 'static,
    children: Children,
) -> impl IntoView {
    view! {
        <button class="sc-tombol-alat"
            class:sc-tombol-alat--aktif=move || aktif.get() == target
            on:click=on_click aria-label=label title=label>
            {children()}
        </button>
    }
}

#[component]
pub(super) fn TombolWarna(warna: String, dipilih: RwSignal<String>) -> impl IntoView {
    let wk = warna.clone(); let wc = warna.clone();
    view! {
        <button class="sc-tombol-warna"
            class:sc-tombol-warna--aktif=move || dipilih.get() == wc
            style=format!("background-color:{}", warna)
            on:click=move |_| dipilih.set(wk.clone())
            aria-label=format!("Pilih warna {}", warna.clone()) />
    }
}

#[component]
pub(super) fn PanelGeser(
    #[prop(default = "")] judul: &'static str,
    on_tutup: impl Fn() + 'static,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="sc-panel-geser" role="dialog" aria-modal="false"
            aria-label=format!("Panel {}", judul)>
            <div class="sc-panel-header">
                <div class="sc-garis-pemisah"></div>
                <Show when=move || !judul.is_empty()>
                    <div class="sc-judul-panel">{judul}</div>
                </Show>
                <button class="sc-tombol-tutup-panel" aria-label="Tutup panel"
                    on:click=move |_| on_tutup()>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <line x1="18" y1="6" x2="6" y2="18"/>
                        <line x1="6"  y1="6" x2="18" y2="18"/>
                    </svg>
                </button>
            </div>
            {children()}
        </div>
    }
}
