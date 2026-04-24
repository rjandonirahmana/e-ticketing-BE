// ULID helpers — a couple of variants are kept available for future repos
// (e.g. nullable joins) even when not every consumer is wired yet.
#![allow(dead_code)]

use anyhow::Result;
use ulid::Ulid;

/// Generate a new ULID as canonical 26-char Crockford string.
pub fn new_ulid() -> String {
    Ulid::new().to_string()
}

/// Encode a ULID string into 16 raw bytes (for BYTEA params).
pub fn ulid_to_bytes(s: &str) -> Result<[u8; 16]> {
    s.parse::<Ulid>()
        .map(|u| u.to_bytes())
        .map_err(|e| anyhow::anyhow!("Invalid ULID '{}': {}", s, e))
}

/// Same as `ulid_to_bytes` but returns a `Vec<u8>` (handy for tokio-postgres bind).
pub fn ulid_to_vec(s: &str) -> Result<Vec<u8>> {
    ulid_to_bytes(s).map(|b| b.to_vec())
}

/// Accept either a 26-char ULID or a 32-char hex string and return the 16 raw bytes.
/// Useful when an id can come from a JWT (ULID) or from an old hex column.
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

/// Decode 16 raw bytes from BYTEA back into a ULID string.
pub fn bin_to_ulid(raw: Vec<u8>) -> Result<String> {
    let arr: [u8; 16] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("Expected 16 bytes for ULID"))?;
    Ok(Ulid::from_bytes(arr).to_string())
}

/// Optional variant of `bin_to_ulid` for nullable columns.
pub fn bin_to_ulid_opt(val: Option<Vec<u8>>) -> Result<Option<String>> {
    match val {
        None => Ok(None),
        Some(b) => Ok(Some(bin_to_ulid(b)?)),
    }
}
