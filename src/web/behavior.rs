//! behavior.rs — Pelacakan minat implisit (rekomendasi TANPA "like").
//!
//! Ide: setiap kali user membuka detail sebuah event, kita catat kategori event
//! itu sebagai sinyal minat. Skor per-kategori disimpan di **localStorage**
//! (per-device, hanya di browser) dan **di-decay** tiap kunjungan sehingga minat
//! terbaru lebih berbobot daripada yang lama. Section "Untuk Kamu" lalu mengambil
//! event dari kategori berskor tertinggi.
//!
//! Kenapa localStorage (bukan server)? Nol beban DB/write per page-view (penting
//! untuk VPS kecil), tanpa perlu login, dan privasi tetap di perangkat user.
//! Trade-off: tidak lintas-device & tidak bisa collaborative filtering — cukup
//! sebagai personalisasi v1 berbasis perilaku.

#[cfg(target_arch = "wasm32")]
const KEY: &str = "pulse_affinity_v1";

/// Faktor decay tiap view: skor lama dikali ini sebelum menambah minat baru.
#[cfg(target_arch = "wasm32")]
const DECAY: f64 = 0.92;

/// Catat bahwa user membuka event dengan kategori `categories`.
/// Semua skor di-decay, lalu kategori yang dilihat ditambah +1.
pub fn record_view(categories: &[String]) {
    #[cfg(target_arch = "wasm32")]
    {
        use std::collections::BTreeMap;
        let Some(store) = local_storage() else { return };
        let mut map: BTreeMap<String, f64> = load_map(&store);

        for v in map.values_mut() {
            *v *= DECAY;
        }
        for c in categories {
            let c = c.trim();
            if c.is_empty() {
                continue;
            }
            *map.entry(c.to_string()).or_insert(0.0) += 1.0;
        }
        // Buang skor yang sudah terlalu kecil agar map tak membengkak.
        map.retain(|_, v| *v > 0.05);

        if let Ok(s) = serde_json::to_string(&map) {
            let _ = store.set_item(KEY, &s);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = categories;
    }
}

/// Kategori dengan skor tertinggi (untuk rekomendasi). Kosong bila belum ada data
/// perilaku sama sekali.
pub fn top_categories(n: usize) -> Vec<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(store) = local_storage() else {
            return Vec::new();
        };
        let map = load_map(&store);
        let mut v: Vec<(String, f64)> = map.into_iter().collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        v.into_iter().take(n).map(|(k, _)| k).collect()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = n;
        Vec::new()
    }
}

#[cfg(target_arch = "wasm32")]
fn load_map(store: &web_sys::Storage) -> std::collections::BTreeMap<String, f64> {
    store
        .get_item(KEY)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}
