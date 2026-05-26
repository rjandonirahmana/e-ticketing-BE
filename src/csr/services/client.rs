use gloo_net::http::{Request, RequestBuilder};
use gloo_storage::{LocalStorage, Storage};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt;

pub const TOKEN_KEY: &str = "kinetic_access_token";
pub const REFRESH_KEY: &str = "kinetic_refresh_token";
pub const USER_KEY: &str = "kinetic_user";

fn api_base() -> &'static str {
    option_env!("KINETIC_API_BASE_URL").unwrap_or("/api")
}

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.status, self.message)
    }
}

impl ApiError {
    pub fn network(msg: impl Into<String>) -> Self {
        Self { status: 0, message: msg.into() }
    }
    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self { status: 501, message: msg.into() }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Internal JWT — shared secret antara FE Leptos dan BE Axum.
//
// Secret di-embed di compile-time via INTERNAL_JWT_SECRET env var.
// Di production, set ke nilai acak minimal 32 karakter di .env dan
// build script. Ini memastikan hanya binary FE yang bisa memanggil API.
//
// Implementasi SHA-256 dan HMAC-SHA256 murni Rust — tidak butuh dependency
// tambahan, kompatibel WASM/no_std.
// ═══════════════════════════════════════════════════════════════════════════════

// ── SHA-256 (pure Rust, WASM-safe) ───────────────────────────────────────────

#[allow(clippy::many_single_char_names)]
fn sha256(data: &[u8]) -> [u8; 32] {
    #[rustfmt::skip]
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
        0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
        0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
        0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];

    // Padding
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i*4], chunk[i*4+1], chunk[i*4+2], chunk[i*4+3]]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e;
            e = d.wrapping_add(t1);
            d = c; c = b; b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, &v) in h.iter().enumerate() {
        out[i*4..i*4+4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

// ── HMAC-SHA256 ───────────────────────────────────────────────────────────────

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let k: Vec<u8> = if key.len() > 64 { sha256(key).to_vec() } else { key.to_vec() };
    let mut k64 = [0u8; 64];
    k64[..k.len()].copy_from_slice(&k);

    let mut ipad_data = Vec::with_capacity(64 + data.len());
    let mut opad_data = Vec::with_capacity(96);
    for &b in &k64 { ipad_data.push(b ^ 0x36); }
    ipad_data.extend_from_slice(data);
    let inner = sha256(&ipad_data);
    for &b in &k64 { opad_data.push(b ^ 0x5c); }
    opad_data.extend_from_slice(&inner);
    sha256(&opad_data)
}

// ── Base64url encode (no padding) ─────────────────────────────────────────────

fn b64url(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity((data.len() * 4 + 2) / 3);
    let mut i = 0usize;
    while i + 2 < data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i+1] as u32) << 8) | (data[i+2] as u32);
        out.push(T[(n>>18 & 0x3f) as usize] as char);
        out.push(T[(n>>12 & 0x3f) as usize] as char);
        out.push(T[(n>>6  & 0x3f) as usize] as char);
        out.push(T[(n     & 0x3f) as usize] as char);
        i += 3;
    }
    match data.len() - i {
        1 => {
            let n = (data[i] as u32) << 16;
            out.push(T[(n>>18 & 0x3f) as usize] as char);
            out.push(T[(n>>12 & 0x3f) as usize] as char);
        }
        2 => {
            let n = ((data[i] as u32) << 16) | ((data[i+1] as u32) << 8);
            out.push(T[(n>>18 & 0x3f) as usize] as char);
            out.push(T[(n>>12 & 0x3f) as usize] as char);
            out.push(T[(n>>6  & 0x3f) as usize] as char);
        }
        _ => {}
    }
    out
}

/// Buat JWT internal HS256 yang di-sign dengan INTERNAL_JWT_SECRET.
/// Dipanggil untuk setiap request HTTP ke backend.
///
/// Token expire dalam 5 menit — cukup untuk satu request + response.
/// FE dan BE harus pakai secret yang SAMA (set via env `INTERNAL_JWT_SECRET`).
fn make_internal_token() -> String {
    // Secret di-embed saat compile: `INTERNAL_JWT_SECRET=xxx cargo build`
    // Fallback ke string dev default jika env var tidak di-set saat build.
    let secret = option_env!("INTERNAL_JWT_SECRET")
        .unwrap_or("kinetic-internal-dev-secret-changeme-in-production-seulgi");

    // Ambil timestamp dari JS runtime (WASM tidak punya std::time::SystemTime)
    let now_ms = js_sys::Date::now();
    let now_sec = (now_ms / 1000.0) as i64;
    let exp = now_sec + 300; // valid 5 menit

    let header = b64url(br#"{"alg":"HS256","typ":"JWT"}"#);
    let claims_str = format!(r#"{{"iss":"kinetic-fe","iat":{},"exp":{}}}"#, now_sec, exp);
    let claims = b64url(claims_str.as_bytes());

    let signing_input = format!("{}.{}", header, claims);
    let sig = hmac_sha256(secret.as_bytes(), signing_input.as_bytes());
    format!("{}.{}", signing_input, b64url(&sig))
}

// ── URL builder ───────────────────────────────────────────────────────────────

fn build_url(path: &str) -> String {
    let base = api_base().trim_end_matches('/');
    let mut url = String::with_capacity(base.len() + path.len() + 1);
    url.push_str(base);
    if !path.starts_with('/') { url.push('/'); }
    url.push_str(path);
    url
}

fn current_token() -> Option<String> {
    LocalStorage::get::<String>(TOKEN_KEY).ok()
}

/// Terapkan semua header yang diperlukan ke setiap request:
/// 1. `X-App-Token` — internal JWT (selalu, untuk validasi asal FE)
/// 2. `Authorization: Bearer <user_jwt>` — jika with_auth=true dan user login
fn apply_headers(builder: RequestBuilder, with_auth: bool) -> RequestBuilder {
    let builder = builder.header("X-App-Token", &make_internal_token());
    if with_auth {
        if let Some(tok) = current_token() {
            let bearer = format!("Bearer {}", tok);
            return builder.header("Authorization", &bearer);
        }
    }
    builder
}

// ── Core HTTP helpers (non-generic) ──────────────────────────────────────────

async fn send_and_read(req: gloo_net::http::Request) -> Result<String, ApiError> {
    let res = req.send().await.map_err(|e| ApiError::network(e.to_string()))?;
    let status = res.status();
    let text = res.text().await.unwrap_or_default();

    if !(200..300).contains(&status) {
        let message = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("message")
                    .or_else(|| v.get("error"))
                    .and_then(|m| m.as_str())
                    .map(String::from)
            })
            .unwrap_or_else(|| {
                if text.is_empty() {
                    format!("HTTP {}", status)
                } else {
                    text.clone()
                }
            });
        return Err(ApiError { status, message });
    }
    Ok(text)
}

#[inline]
fn from_json<T: DeserializeOwned>(text: &str) -> Result<T, ApiError> {
    if text.trim().is_empty() {
        serde_json::from_value::<T>(serde_json::Value::Null)
            .map_err(|e| ApiError::network(format!("decode-empty: {}", e)))
    } else {
        serde_json::from_str::<T>(text)
            .map_err(|e| ApiError::network(format!("decode: {}", e)))
    }
}

fn make_get(path: &str, auth: bool) -> Result<gloo_net::http::Request, ApiError> {
    apply_headers(Request::get(&build_url(path)), auth)
        .build()
        .map_err(|e| ApiError::network(e.to_string()))
}

fn make_delete(path: &str) -> Result<gloo_net::http::Request, ApiError> {
    apply_headers(Request::delete(&build_url(path)), true)
        .build()
        .map_err(|e| ApiError::network(e.to_string()))
}

fn make_json_body<T: Serialize>(v: &T) -> Result<String, ApiError> {
    serde_json::to_string(v).map_err(|e| ApiError::network(e.to_string()))
}

fn make_post_json(
    path: &str,
    method: &str,
    body_json: &str,
    auth: bool,
) -> Result<gloo_net::http::Request, ApiError> {
    let url = build_url(path);
    let builder = match method {
        "PUT" => Request::put(&url),
        _ => Request::post(&url),
    };
    apply_headers(builder.header("Content-Type", "application/json"), auth)
        .body(body_json)
        .map_err(|e| ApiError::network(e.to_string()))
}

// ── Public API ────────────────────────────────────────────────────────────────

pub async fn get_public<TRes: DeserializeOwned>(path: &str) -> Result<TRes, ApiError> {
    let req = make_get(path, false)?;
    from_json(&send_and_read(req).await?)
}

pub async fn get_private<TRes: DeserializeOwned>(path: &str) -> Result<TRes, ApiError> {
    let req = make_get(path, true)?;
    from_json(&send_and_read(req).await?)
}

pub async fn post_public<TReq: Serialize, TRes: DeserializeOwned>(
    path: &str,
    body: &TReq,
) -> Result<TRes, ApiError> {
    let body_str = make_json_body(body)?;
    let req = make_post_json(path, "POST", &body_str, false)?;
    from_json(&send_and_read(req).await?)
}

pub async fn post_private<TReq: Serialize, TRes: DeserializeOwned>(
    path: &str,
    body: &TReq,
) -> Result<TRes, ApiError> {
    let body_str = make_json_body(body)?;
    let req = make_post_json(path, "POST", &body_str, true)?;
    from_json(&send_and_read(req).await?)
}

pub async fn put_private<TReq: Serialize, TRes: DeserializeOwned>(
    path: &str,
    body: &TReq,
) -> Result<TRes, ApiError> {
    let body_str = make_json_body(body)?;
    let req = make_post_json(path, "PUT", &body_str, true)?;
    from_json(&send_and_read(req).await?)
}

pub async fn delete_private<TRes: DeserializeOwned>(path: &str) -> Result<TRes, ApiError> {
    let req = make_delete(path)?;
    from_json(&send_and_read(req).await?)
}

pub async fn post_multipart_private<TRes: DeserializeOwned>(
    path: &str,
    form: web_sys::FormData,
) -> Result<TRes, ApiError> {
    let url = build_url(path);
    let req = apply_headers(Request::post(&url), true)
        .body(wasm_bindgen::JsValue::from(form))
        .map_err(|e| ApiError::network(e.to_string()))?;
    from_json(&send_and_read(req).await?)
}

pub async fn put_multipart_private<TRes: DeserializeOwned>(
    path: &str,
    form: web_sys::FormData,
) -> Result<TRes, ApiError> {
    let url = build_url(path);
    let req = apply_headers(Request::put(&url), true)
        .body(wasm_bindgen::JsValue::from(form))
        .map_err(|e| ApiError::network(e.to_string()))?;
    from_json(&send_and_read(req).await?)
}
