//! Utilitas bersama — fungsi kecil yang dipakai di banyak modul.

/// Format angka IDR dengan pemisah titik ribuan.
/// Contoh: `1_200_000` → `"Rp1.200.000"`
pub fn format_idr(amount: i64) -> String {
    let n = amount.abs();
    let s = n.to_string();
    let chars: Vec<char> = s.chars().rev().collect();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push('.');
        }
        out.push(*c);
    }
    let formatted: String = out.chars().rev().collect();
    if amount < 0 {
        format!("-Rp{formatted}")
    } else {
        format!("Rp{formatted}")
    }
}

/// Format angka biasa dengan pemisah titik (untuk harga tanpa prefiks "Rp").
pub fn format_number(n: i64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}
