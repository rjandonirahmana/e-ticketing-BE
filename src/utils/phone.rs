use anyhow::{anyhow, Result};

/// Normalisasi nomor HP Indonesia ke format WAHA (`62xxxx@c.us`).
pub fn normalize_phone(phone: &str) -> Result<String> {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();

    let normalized = if digits.starts_with("08") {
        format!("62{}", &digits[1..])
    } else if digits.starts_with("62") {
        digits
    } else {
        return Err(anyhow!("Nomor HP harus diawali dengan '08' atau '+62'"));
    };

    Ok(format!("{}@c.us", normalized))
}
