//! web/hooks.rs — Hooks UI & auth (pengganti `csr::hooks`).
//!
//! - `provide_theme()`  : sediakan ThemeCtx + sinkronisasi `data-theme`/localStorage.
//!                        SSR-safe: kode web_sys hanya dikompilasi di target wasm32.
//! - `ThemeToggle`      : tombol toggle dark/light.
//! - `use_auth()`       : turunan dari `AuthResource` (lihat web::app) sebagai
//!                        profil ringkas dengan `role` dan `membership_tier`.

use leptos::prelude::*;

// ── Theme ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct ThemeCtx {
    /// "dark" | "light"
    pub theme: RwSignal<String>,
}

impl ThemeCtx {
    pub fn is_dark(&self) -> bool {
        self.theme.with(|t| t == "dark")
    }

    pub fn toggle(&self) {
        let next = if self.is_dark() { "light" } else { "dark" };
        self.theme.set(next.to_string());
    }
}

/// Sediakan ThemeCtx + Effect yang menulis `data-theme` ke <html> dan localStorage.
///
/// SSR-safe: seluruh akses `web_sys` dipagari `#[cfg(target_arch = "wasm32")]`,
/// jadi tidak pernah dipanggil di server (akan panik di target non-wasm).
pub fn provide_theme() {
    // Default mengikuti shell() SSR: data-theme="dark".
    let theme = RwSignal::new("dark".to_string());
    provide_context(ThemeCtx { theme });

    // Semua akses browser (web_sys) HANYA dikompilasi & dijalankan di target WASM.
    // Di server (SSR / generate_route_list), blok ini dihapus sepenuhnya sehingga
    // `web_sys::window()` tidak pernah dipanggil (panik di non-wasm).
    #[cfg(target_arch = "wasm32")]
    {
        // Effect berjalan setelah hydration (bukan saat render awal), jadi:
        //  - run pertama: baca preferensi tersimpan (hindari hydration mismatch),
        //  - tiap perubahan: sinkronkan <html data-theme> + localStorage.
        Effect::new(move |prev: Option<()>| {
            if prev.is_none() {
                if let Some(win) = web_sys::window() {
                    if let Ok(Some(storage)) = win.local_storage() {
                        if let Ok(Some(saved)) = storage.get_item("kinetic.theme") {
                            if saved == "light" || saved == "dark" {
                                theme.set(saved);
                            }
                        }
                    }
                }
            }

            let current = theme.get();
            if let Some(win) = web_sys::window() {
                if let Some(doc) = win.document() {
                    if let Some(html) = doc.document_element() {
                        let _ = html.set_attribute("data-theme", &current);
                    }
                }
                // Sinkronkan <meta name="theme-color"> (helper JS di shell.rs) agar
                // warna chrome browser (address bar mobile) ikut tema.
                {
                    use wasm_bindgen::JsCast;
                    if let Ok(f) = js_sys::Reflect::get(
                        &win,
                        &wasm_bindgen::JsValue::from_str("__setThemeColor"),
                    ) {
                        if let Ok(func) = f.dyn_into::<js_sys::Function>() {
                            let _ = func.call1(
                                &wasm_bindgen::JsValue::NULL,
                                &wasm_bindgen::JsValue::from_str(&current),
                            );
                        }
                    }
                }
                if let Ok(Some(storage)) = win.local_storage() {
                    let _ = storage.set_item("kinetic.theme", &current);
                }
            }
        });
    }
}

pub fn use_theme() -> ThemeCtx {
    use_context::<ThemeCtx>().expect("ThemeCtx not provided — panggil provide_theme() di App")
}

#[component]
pub fn ThemeToggle() -> impl IntoView {
    let theme = use_theme();
    view! {
        <button
            class="theme-toggle icon-btn"
            type="button"
            aria-label="Toggle tema"
            on:click=move |_| theme.toggle()
        >
            {move || {
                if theme.is_dark() {
                    // Ikon bulan (dark aktif)
                    view! {
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                            stroke="currentColor" stroke-width="2" stroke-linecap="round"
                            stroke-linejoin="round">
                            <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
                        </svg>
                    }
                        .into_any()
                } else {
                    // Ikon matahari (light aktif)
                    view! {
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                            stroke="currentColor" stroke-width="2" stroke-linecap="round"
                            stroke-linejoin="round">
                            <circle cx="12" cy="12" r="5" />
                            <line x1="12" y1="1" x2="12" y2="3" />
                            <line x1="12" y1="21" x2="12" y2="23" />
                            <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
                            <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
                            <line x1="1" y1="12" x2="3" y2="12" />
                            <line x1="21" y1="12" x2="23" y2="12" />
                            <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
                            <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
                        </svg>
                    }
                        .into_any()
                }
            }}
        </button>
    }
}

// ── Auth ──────────────────────────────────────────────────────────────────────

/// Profil ringkas untuk pengecekan peran di UI.
///
/// CATATAN: backend `UserResponse` tidak mengirim `membership_tier`. Di sini
/// nilainya diturunkan dari `role` (role "merchant" → tier "MERCHANT").
/// Jika backend Anda punya tier asli, tambahkan field-nya di `UserResponse`
/// (web::models) lalu sesuaikan pemetaan di `to_profile()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthProfile {
    pub id: String,
    pub name: String,
    pub role: String,
    pub membership_tier: String,
}

#[derive(Clone, Copy)]
pub struct AuthCtx {
    /// Reaktif: `None` saat belum login / belum ter-resolve.
    pub user: Signal<Option<AuthProfile>>,
}

fn to_profile(u: &crate::web::models::UserResponse) -> AuthProfile {
    let membership_tier = if u.role.eq_ignore_ascii_case("merchant") {
        "MERCHANT".to_string()
    } else {
        "FREE".to_string()
    };
    AuthProfile {
        id: u.id.clone(),
        name: u.name.clone(),
        role: u.role.clone(),
        membership_tier,
    }
}

/// Hook auth: turunkan profil dari `AuthResource` yang di-provide di App.
///
/// Hanya membaca resource di dalam Effect (konteks valid) — tidak ada akses di luar
/// Suspense/Transition/effect untuk menghindari hydration mismatch warning.
pub fn use_auth() -> AuthCtx {
    let auth = use_context::<crate::web::app::AuthResource>()
        .expect("AuthResource not provided — pastikan dipakai di dalam App");

    // Inisialisasi dengan None; Effect akan mengisi nilainya setelah hydration selesai.
    // Tidak ada get_untracked() di sini — menghindari "reading resource in hydrate mode" warning.
    let user: RwSignal<Option<AuthProfile>> = RwSignal::new(None);

    Effect::new(move |_| {
        user.set(
            auth.get()
                .and_then(|res| res.ok())
                .flatten()
                .as_ref()
                .map(to_profile),
        );
    });

    AuthCtx { user: user.into() }
}
