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

#[cfg(test)]
mod tests {
    use super::*;

    /// Teks → byte → teks harus utuh. Seluruh id di database disimpan sebagai
    /// BYTEA 16 byte dan ditampilkan sebagai teks; kalau perjalanan itu tak
    /// setia, id yang dibaca aplikasi bukan id yang tersimpan.
    #[test]
    fn bolak_balik_utuh() {
        let a = new_ulid();
        assert_eq!(a.len(), 26);
        let bytes = ulid_to_vec(&a).unwrap();
        assert_eq!(bytes.len(), 16);
        assert_eq!(bin_to_ulid(bytes).unwrap(), a);
    }

    /// `id_to_vec` menerima DUA bentuk yang beredar di sistem ini: ULID 26
    /// karakter dan hex 32 karakter. Keduanya harus menghasilkan byte yang sama
    /// untuk id yang sama — kalau tidak, satu baris bisa "tak ditemukan" hanya
    /// karena pemanggilnya memakai bentuk yang berbeda.
    #[test]
    fn dua_bentuk_id_menghasilkan_byte_sama() {
        let ulid = new_ulid();
        let hex = ulid_to_hex(&ulid).unwrap();
        assert_eq!(hex.len(), 32);
        assert_eq!(id_to_vec(&ulid).unwrap(), id_to_vec(&hex).unwrap());
    }

    /// Id cacat dari luar (URL, form) DITOLAK, bukan diam-diam berubah jadi id
    /// lain yang kebetulan sah.
    #[test]
    fn id_cacat_ditolak() {
        for jahat in ["", "bukan-id", "123", &"f".repeat(31), &"f".repeat(33)] {
            assert!(id_to_vec(jahat).is_err(), "'{jahat}' seharusnya ditolak");
        }
        assert!(bin_to_ulid(vec![0u8; 4]).is_err(), "byte kurang dari 16 harus ditolak");
    }

    /// ULID harus urut-waktu: itu satu-satunya alasan memilihnya ketimbang
    /// UUIDv4 acak, karena index primary key jadi tetap append-only.
    #[test]
    fn urut_waktu() {
        let a = new_ulid();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = new_ulid();
        assert!(a < b, "{a} seharusnya lebih kecil dari {b}");
    }
}
