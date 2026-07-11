//! web/rtc.rs — Sumber tunggal daftar ICE server untuk semua WebRTC client
//! (live publisher, live viewer, dan meet mesh).
//!
//! Server mengekspos daftar ICE (STUN + TURN dari env) di `GET /api/rtc/ice`.
//! Browser tidak bisa membaca env server, jadi creds TURN diambil dari endpoint
//! ini saat akan membuat `RtcPeerConnection`. Bila fetch gagal, fallback ke
//! STUN publik (cukup untuk LAN/demo). Lihat `TURN_*` di `.env.example`.

use wasm_bindgen::prelude::*;

/// Daftar STUN-only sebagai fallback (dipakai bila endpoint gagal/ kosong).
pub fn ice_fallback() -> js_sys::Array {
    let urls = js_sys::Array::new();
    urls.push(&JsValue::from_str("stun:stun.l.google.com:19302"));
    urls.push(&JsValue::from_str("stun:stun1.l.google.com:19302"));
    let server = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&server, &JsValue::from_str("urls"), &urls);
    let servers = js_sys::Array::new();
    servers.push(&server);
    servers
}

/// Bangun `js_sys::Array` ICE server dari payload JSON server
/// (`{ "data": [ { "urls": [...], "username"?, "credential"? }, ... ] }`).
fn build_from_json(v: &serde_json::Value) -> Option<js_sys::Array> {
    let list = v.get("data")?.as_array()?;
    if list.is_empty() {
        return None;
    }
    let servers = js_sys::Array::new();
    for entry in list {
        let urls_json = entry.get("urls").and_then(|u| u.as_array())?;
        let urls = js_sys::Array::new();
        for u in urls_json {
            if let Some(s) = u.as_str() {
                urls.push(&JsValue::from_str(s));
            }
        }
        let obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("urls"), &urls);
        if let Some(user) = entry.get("username").and_then(|x| x.as_str()) {
            let _ = js_sys::Reflect::set(&obj, &JsValue::from_str("username"), &JsValue::from_str(user));
        }
        if let Some(cred) = entry.get("credential").and_then(|x| x.as_str()) {
            let _ =
                js_sys::Reflect::set(&obj, &JsValue::from_str("credential"), &JsValue::from_str(cred));
        }
        servers.push(&obj);
    }
    Some(servers)
}

/// Ambil daftar ICE server (STUN + TURN) dari server; fallback STUN bila gagal.
pub async fn fetch_ice_servers() -> js_sys::Array {
    let resp = match gloo_net::http::Request::get("/api/rtc/ice").send().await {
        Ok(r) => r,
        Err(_) => return ice_fallback(),
    };
    match resp.json::<serde_json::Value>().await {
        Ok(v) => build_from_json(&v).unwrap_or_else(ice_fallback),
        Err(_) => ice_fallback(),
    }
}

// ─── Izin kamera & mikrofon ──────────────────────────────────────────────────

/// Klasifikasi kegagalan `getUserMedia` agar UI bisa memberi panduan yang tepat
/// (bukan sekadar "ditolak"). Dipetakan dari `DOMException.name`.
#[derive(Clone, Debug, PartialEq)]
pub enum MediaError {
    /// Izin ditolak / diblokir (NotAllowedError / SecurityError).
    /// `permanent` = browser TIDAK akan bertanya lagi (state Permissions API
    /// "denied") → satu-satunya jalan adalah lewat pengaturan situs; `android`
    /// menentukan instruksi pengaturan mana yang ditampilkan. Web TIDAK bisa
    /// membuka pengaturan OS/browser secara programatik — panduan langkah
    /// eksplisit adalah yang terbaik yang mungkin.
    PermissionDenied { permanent: bool, android: bool },
    /// Tak ada kamera/mic terdeteksi (NotFoundError / OverconstrainedError).
    NoDevice,
    /// Perangkat dipakai aplikasi lain (NotReadableError / TrackStartError).
    InUse,
    /// Lainnya (tak didukung / konteks tak aman / dsb).
    Other(String),
}

impl MediaError {
    pub fn is_permission_denied(&self) -> bool {
        matches!(self, MediaError::PermissionDenied { .. })
    }

    /// Pesan siap-tampil (Bahasa Indonesia) berikut panduan tindakan.
    pub fn user_message(&self) -> String {
        match self {
            MediaError::PermissionDenied { permanent: true, android: true } =>
                "Izin kamera & mikrofon DIBLOKIR oleh browser. Buka: menu ⋮ (kanan atas) → Setelan → Setelan situs → Kamera & Mikrofon → pilih situs ini → Izinkan. Atau ketuk ikon 🔒/ⓘ di samping alamat → Izin → aktifkan Kamera & Mikrofon. Setelah itu tekan Coba Lagi.".into(),
            MediaError::PermissionDenied { permanent: true, android: false } =>
                "Izin kamera & mikrofon DIBLOKIR. Klik ikon gembok/kamera di address bar → ubah Kamera & Mikrofon menjadi \"Izinkan\", lalu tekan Coba Lagi.".into(),
            MediaError::PermissionDenied { permanent: false, .. } =>
                "Akses kamera & mikrofon dibutuhkan untuk fitur ini. Tekan Coba Lagi lalu pilih \"Izinkan\" saat browser bertanya.".into(),
            MediaError::NoDevice => "Kamera atau mikrofon tidak terdeteksi. Pastikan perangkat terpasang, lalu Coba Lagi.".into(),
            MediaError::InUse => "Kamera/mikrofon sedang dipakai aplikasi lain (mis. Zoom/Meet). Tutup aplikasi itu, lalu Coba Lagi.".into(),
            MediaError::Other(m) => format!("Gagal mengakses kamera/mikrofon: {m}"),
        }
    }
}

/// Status izin via Permissions API (`navigator.permissions.query`) — lewat
/// Reflect agar tak butuh feature web-sys tambahan. `name`: "camera" / "microphone".
/// None bila API tak tersedia (Safari lama) / query gagal.
async fn permission_state(name: &str) -> Option<String> {
    let nav = web_sys::window()?.navigator();
    let perms = js_sys::Reflect::get(nav.as_ref(), &JsValue::from_str("permissions")).ok()?;
    if perms.is_undefined() || perms.is_null() {
        return None;
    }
    let query = js_sys::Reflect::get(&perms, &JsValue::from_str("query"))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    let desc = js_sys::Object::new();
    js_sys::Reflect::set(&desc, &JsValue::from_str("name"), &JsValue::from_str(name)).ok()?;
    let promise = query
        .call1(&perms, &desc)
        .ok()?
        .dyn_into::<js_sys::Promise>()
        .ok()?;
    let status = wasm_bindgen_futures::JsFuture::from(promise).await.ok()?;
    js_sys::Reflect::get(&status, &JsValue::from_str("state"))
        .ok()?
        .as_string()
}

/// Apakah user agent Android (untuk memilih instruksi pengaturan yang tepat).
fn is_android() -> bool {
    web_sys::window()
        .map(|w| w.navigator().user_agent().unwrap_or_default().contains("Android"))
        .unwrap_or(false)
}

/// Baca `name` dari DOMException hasil reject `getUserMedia` (via Reflect — tak
/// bergantung pada cast DomException yang bisa gagal di sebagian browser).
fn classify_media_error(err: &JsValue) -> MediaError {
    let name = js_sys::Reflect::get(err, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    match name.as_str() {
        "NotAllowedError" | "SecurityError" | "PermissionDeniedError" => {
            // `permanent` dilengkapi pemanggil (butuh query Permissions API async).
            MediaError::PermissionDenied { permanent: false, android: is_android() }
        }
        "NotFoundError" | "OverconstrainedError" | "DevicesNotFoundError" => MediaError::NoDevice,
        "NotReadableError" | "TrackStartError" | "AbortError" => MediaError::InUse,
        other if !other.is_empty() => MediaError::Other(other.to_string()),
        _ => MediaError::Other(format!("{err:?}")),
    }
}

/// Lengkapi [`MediaError::PermissionDenied`] dengan status `permanent` dari
/// Permissions API: state "denied" = blokir permanen (getUserMedia langsung
/// gagal TANPA dialog — kasus umum di Android setelah sekali menolak) → user
/// HARUS lewat pengaturan situs; selain itu retry akan memunculkan dialog lagi.
async fn enrich_permission_denied(err: MediaError) -> MediaError {
    match err {
        MediaError::PermissionDenied { android, .. } => {
            let cam = permission_state("camera").await;
            let mic = permission_state("microphone").await;
            let permanent = cam.as_deref() == Some("denied") || mic.as_deref() == Some("denied");
            MediaError::PermissionDenied { permanent, android }
        }
        e => e,
    }
}

/// Minta izin kamera + mikrofon (video ideal 1280×720 + audio). Sumber tunggal
/// untuk publisher live streaming dan meet — mengembalikan `MediaStream` bila
/// diizinkan, atau [`MediaError`] terklasifikasi bila gagal sehingga UI dapat
/// menampilkan panduan izin + tombol "Coba Lagi".
pub async fn request_camera_mic() -> Result<web_sys::MediaStream, MediaError> {
    let window = web_sys::window().ok_or_else(|| MediaError::Other("no window".into()))?;
    let media_devices = window
        .navigator()
        .media_devices()
        .map_err(|_| MediaError::Other("MediaDevices tak didukung (butuh HTTPS)".into()))?;

    let constraints = web_sys::MediaStreamConstraints::new();
    let video = web_sys::MediaTrackConstraints::new();
    // Nilai bare = "ideal" (bukan exact) → tak gagal di kamera non-720p.
    video.set_width(&JsValue::from_f64(1280.0));
    video.set_height(&JsValue::from_f64(720.0));
    constraints.set_audio(&JsValue::TRUE);
    constraints.set_video(&video.into());

    let promise = match media_devices.get_user_media_with_constraints(&constraints) {
        Ok(p) => p,
        Err(e) => return Err(enrich_permission_denied(classify_media_error(&e)).await),
    };
    match wasm_bindgen_futures::JsFuture::from(promise).await {
        Ok(val) => Ok(web_sys::MediaStream::from(val)),
        Err(e) => Err(enrich_permission_denied(classify_media_error(&e)).await),
    }
}
