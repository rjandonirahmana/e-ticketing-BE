use anyhow::{Context, Result};
use ulid::Ulid;

pub fn new_ulid() -> String {
    // `generate()`, bukan `new()`: ulid 3.0 mengganti namanya. Keduanya sama —
    // stempel waktu sekarang + 80 bit acak.
    Ulid::generate().to_string()
}

pub fn ulid_to_bytes(s: &str) -> Result<[u8; 16]> {
    s.parse::<Ulid>()
        .map(|u| u.to_bytes())
        .map_err(|e| anyhow::anyhow!("Invalid ULID '{}': {}", s, e))
}

/// FIX: Kembalikan stack array [u8; 16] — tidak ada heap alloc.
/// Gunakan di repository hot path (save_message, insert msg, dll).
/// tokio-postgres ToSql menerima &[u8] yang bisa di-coerce dari &[u8; 16].
pub fn ulid_to_arr(s: &str) -> Result<[u8; 16]> {
    ulid_to_bytes(s)
}

/// Untuk bind ke tokio-postgres BYTEA parameter.
/// Kembalikan Vec<u8> — gunakan di non-hot-path atau saat ownership diperlukan.
pub fn ulid_to_vec(s: &str) -> Result<Vec<u8>> {
    ulid_to_bytes(s).map(|b| b.to_vec())
}

/// Handle kedua format id:
///   - ULID 26 char  (misal "01ARZ3NDEKTSV4RRFFQ69G5FAV") → parse via ulid_to_vec
///   - Hex  32 char  (misal "019d4942f47000ee70983c1090bc616b") → decode via hex::decode
///
/// Gunakan ini di repository yang menerima id dari JWT / request luar.
pub fn id_to_vec(s: &str) -> Result<Vec<u8>> {
    match s.len() {
        26 => ulid_to_vec(s),
        32 => hex::decode(s).map_err(|e| anyhow::anyhow!("Invalid hex id '{}': {}", s, e)),
        n => anyhow::bail!(
            "Invalid id '{}': expected 26-char ULID or 32-char hex, got {} chars",
            s,
            n
        ),
    }
}

pub fn ulid_to_hex(s: &str) -> Result<String> {
    let bytes = ulid_to_bytes(s)?;
    Ok(hex::encode(bytes))
}

pub fn bin_to_ulid(raw: Vec<u8>) -> Result<String> {
    let arr: [u8; 16] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("Expected 16 bytes for ULID"))?;
    Ok(Ulid::from_bytes(arr).to_string())
}

/// Versi borrow dari bin_to_ulid — tidak perlu clone Vec<u8>.
/// Gunakan ini di error path atau tempat yang hanya punya &[u8].
pub fn bin_to_ulid_ref(raw: &[u8]) -> Result<String> {
    let arr: [u8; 16] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("Expected 16 bytes for ULID"))?;
    Ok(Ulid::from_bytes(arr).to_string())
}

pub fn hex_to_ulid(hex: &str) -> Result<String> {
    let bytes = hex::decode(hex).context("hex_to_ulid: invalid hex")?;
    bin_to_ulid(bytes)
}

pub fn bin_to_ulid_opt(val: Option<Vec<u8>>) -> Result<Option<String>> {
    match val {
        None => Ok(None),
        Some(b) => Ok(Some(bin_to_ulid(b)?)),
    }
}

pub fn mime_to_type(mime: &str) -> &'static str {
    if mime.starts_with("image/") {
        "image"
    } else if mime.starts_with("video/") {
        "video"
    } else if mime.starts_with("audio/") {
        "audio"
    } else {
        "file"
    }
}
