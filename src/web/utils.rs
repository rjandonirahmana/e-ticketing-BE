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
