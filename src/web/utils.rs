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

/// Kunci localStorage untuk identitas tamu. Satu kunci untuk seluruh aplikasi —
/// tamu yang sama harus dikenali sama di halaman produk, `/lives`, maupun PiP.
#[cfg(target_arch = "wasm32")]
const KUNCI_TAMU: &str = "pulse.tamu";

/// Identitas penonton TAMU yang bertahan antar-muat halaman: `(id, nama)`.
///
/// ── KENAPA HARUS DISIMPAN, BUKAN DIBUAT TIAP KALI ───────────────────────────
/// Menonton siaran tidak pernah memerlukan login — rute `subscribe` di
/// `live/api.rs` memang publik, dan itu disengaja karena siaran adalah umpan
/// yang menarik orang SEBELUM mereka punya akun.
///
/// Yang bermasalah adalah cara tamu diberi nama. Sebelumnya klien mengirim
/// `viewer_id: null`, dan server menjawabnya dengan UUID BARU pada setiap
/// permintaan subscribe (`api.rs`, `sub_id`). Akibatnya:
///
///   • Satu tamu yang me-refresh, kehilangan sinyal sebentar, atau berpindah
///     dari halaman produk ke `/lives` terhitung sebagai penonton BARU setiap
///     kali. Angka penonton yang dilihat merchant menggelembung tanpa ada orang
///     tambahan — dan angka itulah yang dipakai merchant untuk menilai apakah
///     siarannya berhasil.
///   • Semua tamu tampil sebagai "Anonim" yang identik, jadi merchant tak bisa
///     membedakan sepuluh orang dari satu orang yang me-refresh sepuluh kali.
///
/// Identitas yang disimpan menyelesaikan keduanya sekaligus, tanpa akun dan
/// tanpa satu pun data pribadi.
///
/// Nilainya TIDAK rahasia dan tak boleh dipakai untuk otorisasi apa pun: ia
/// hanya label tampilan. Siapa pun bisa menyuntingnya di localStorage, dan itu
/// tak apa-apa — yang paling bisa dilakukannya adalah mengganti nama samarannya
/// sendiri di daftar penonton.
///
/// Gagal membaca/menulis localStorage (mode privat di sebagian peramban
/// melemparnya) tidak dianggap galat: pemanggil tetap mendapat identitas yang
/// sah, hanya saja ia tak bertahan sesudah tab ditutup.
/// Padanan sisi server. Pemanggilnya (`components/live_stream.rs`) ada di dalam
/// badan komponen yang dikompilasi untuk KEDUA target, jadi fungsi ini harus ada
/// di dua-duanya — meski di server ia tak pernah benar-benar berjalan, karena
/// menyambung ke SFU baru terjadi sesudah hidrasi.
///
/// Nilainya sengaja BUKAN identitas yang tampak sah. Kalau ia mengembalikan
/// sesuatu seperti `("tamu_0", "Tamu-0000")`, dan suatu hari ada jalur SSR yang
/// tak sengaja memanggilnya, seluruh pengunjung akan berbagi satu identitas yang
/// sama tanpa ada yang menyadarinya — hitungan penonton runtuh jadi 1 dan tak
/// ada pesan galat yang menjelaskannya. String kosong membuat kegagalan itu
/// kelihatan.
#[cfg(not(target_arch = "wasm32"))]
pub fn identitas_tamu() -> (String, String) {
    (String::new(), String::new())
}

#[cfg(target_arch = "wasm32")]
pub fn identitas_tamu() -> (String, String) {
    let simpanan = web_sys::window().and_then(|w| w.local_storage().ok().flatten());

    if let Some(s) = &simpanan {
        if let Ok(Some(tersimpan)) = s.get_item(KUNCI_TAMU) {
            if let Some((id, nama)) = tersimpan.split_once('|') {
                if !id.is_empty() && !nama.is_empty() {
                    return (id.to_string(), nama.to_string());
                }
            }
        }
    }

    // Empat digit heksadesimal: cukup untuk membedakan penonton dalam satu
    // siaran, cukup pendek untuk dibaca sekilas di daftar penonton. Tabrakan
    // memang mungkin, dan konsekuensinya cuma dua tamu bernama sama.
    let acak = (js_sys::Math::random() * 65_536.0) as u32 & 0xFFFF;
    let nama = format!("Tamu-{acak:04X}");
    // Id memakai stempel waktu supaya tetap unik walau labelnya kebetulan sama.
    let id = format!("tamu_{:.0}_{acak:04x}", js_sys::Date::now());

    if let Some(s) = &simpanan {
        let _ = s.set_item(KUNCI_TAMU, &format!("{id}|{nama}"));
    }
    (id, nama)
}
