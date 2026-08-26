//! web/utils.rs — Formatting helpers (pengganti `csr::utils`).
//!
//! Murni fungsi sinkron tanpa I/O atau API browser → aman dipakai di SSR & WASM.

/// Format angka dengan pemisah ribuan gaya Indonesia (titik).
/// `1000000` → `"1.000.000"`. Menangani nilai negatif.
pub fn format_number(n: i64) -> String {
    let neg = n < 0;
    let digits = n.unsigned_abs().to_string();

    // Sisipkan '.' setiap 3 digit dari kanan.
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let len = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push('.');
        }
        out.push(*b as char);
    }

    if neg {
        format!("-{out}")
    } else {
        out
    }
}

/// Format Rupiah: `1000000` → `"Rp1.000.000"`.
pub fn format_idr(n: i64) -> String {
    format!("Rp{}", format_number(n))
}

// ── OpenStreetMap / Leaflet interop ──────────────────────────────────────────
// Implementasi JS (window.pulseMap*) di-inject di shell <head>. Helper ini
// memanggilnya via Reflect. Semua fungsi no-op di SSR (target non-wasm) sehingga
// halaman tetap kompilasi & render server-side tanpa peta.

/// Koordinat default bila product belum punya lokasi (Monas, Jakarta Pusat).
pub const DEFAULT_LAT: f64 = -6.2088;
pub const DEFAULT_LNG: f64 = 106.8456;

#[cfg(target_arch = "wasm32")]
fn call_js(name: &str, args: &js_sys::Array) {
    use wasm_bindgen::{JsCast, JsValue};
    if let Some(win) = web_sys::window() {
        if let Ok(f) = js_sys::Reflect::get(&win, &JsValue::from_str(name)) {
            if let Ok(func) = f.dyn_into::<js_sys::Function>() {
                let _ = func.apply(&JsValue::NULL, args);
            }
        }
    }
}

/// Inisialisasi peta picker interaktif (OpenStreetMap). Saat user klik/geser
/// marker, koordinat ditulis ke `<input id=lat_input_id>` & `<input id=lng_input_id>`
/// lalu di-dispatch product `input` — sehingga signal Leptos (lewat on:input) ikut
/// ter-update tanpa perlu callback closure dari Rust.
pub fn map_picker(map_id: &str, lat_input_id: &str, lng_input_id: &str, lat: f64, lng: f64) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;
        // Koordinat awal dioper EKSPLISIT (bukan dibaca dari value input yang bisa
        // belum ter-update saat edit) → pin selalu mendarat di lokasi yang benar.
        call_js(
            "pulseMapPicker",
            &js_sys::Array::of5(
                &JsValue::from_str(map_id),
                &JsValue::from_str(lat_input_id),
                &JsValue::from_str(lng_input_id),
                &JsValue::from_f64(lat),
                &JsValue::from_f64(lng),
            ),
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (map_id, lat_input_id, lng_input_id, lat, lng);
    }
}

/// Pindahkan center & marker peta picker (dipanggil saat user mengetik koordinat manual).
pub fn map_set(map_id: &str, lat: f64, lng: f64) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;
        call_js(
            "pulseMapSet",
            &js_sys::Array::of3(
                &JsValue::from_str(map_id),
                &JsValue::from_f64(lat),
                &JsValue::from_f64(lng),
            ),
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (map_id, lat, lng);
    }
}

/// Render peta read-only dengan satu marker + popup label (halaman lokasi).
pub fn map_viewer(map_id: &str, lat: f64, lng: f64, label: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;
        call_js(
            "pulseMapViewer",
            &js_sys::Array::of4(
                &JsValue::from_str(map_id),
                &JsValue::from_f64(lat),
                &JsValue::from_f64(lng),
                &JsValue::from_str(label),
            ),
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (map_id, lat, lng, label);
    }
}

/// Hancurkan instance peta (dipanggil di on_cleanup agar tidak bocor antar-navigasi SPA).
pub fn map_destroy(map_id: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;
        call_js("pulseMapDestroy", &js_sys::Array::of1(&JsValue::from_str(map_id)));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = map_id;
    }
}

/// Penanda sekali-pakai dari klien, dipakai sebagai kunci idempotensi checkout.
///
/// Dobel-klik dan retry jaringan mengirim kunci yang SAMA, sehingga server
/// mengembalikan order yang sudah ada alih-alih membuat order kedua. Nilainya
/// tak perlu rahasia maupun unik secara global — cukup unik per percobaan
/// checkout milik satu pengguna.
pub fn client_nonce() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        format!(
            "{:.0}-{:.0}",
            js_sys::Date::now(),
            js_sys::Math::random() * 1_000_000_000.0
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        String::new()
    }
}
