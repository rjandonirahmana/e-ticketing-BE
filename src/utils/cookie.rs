//! cookie.rs — Membaca satu nilai dari header `Cookie`.
//!
//! Dulu ada TIGA salinan: `middleware/auth.rs`, `meet/api.rs`, dan
//! `web/api/upload.rs` (yang bernama `cookie_value`, sehingga luput dari
//! pencarian berdasarkan nama). Ketiganya menerapkan aturan yang sama, dan
//! aturan itu punya sudut-sudut yang mudah salah kalau ditulis ulang: spasi
//! sesudah titik koma, nilai kosong yang harus dianggap tak ada, dan nama yang
//! merupakan awalan dari nama lain (`token` vs `token_lama`).

use axum::http::header;

/// Nilai cookie `name`, atau `None` bila tak ada / kosong.
pub fn nilai(headers: &header::HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').map(str::trim).find_map(|p| {
        // `{name}=` lengkap dengan tanda sama dengan — tanpa itu, cookie
        // bernama `pulse_token_lama` akan menjawab permintaan `pulse_token`.
        p.strip_prefix(&format!("{name}="))
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(String::from)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::{HeaderMap, HeaderValue, COOKIE};

    fn h(v: &str) -> HeaderMap {
        let mut m = HeaderMap::new();
        m.insert(COOKIE, HeaderValue::from_str(v).unwrap());
        m
    }

    #[test]
    fn membaca_nilai() {
        assert_eq!(nilai(&h("a=1; b=2"), "b"), Some("2".into()));
    }

    #[test]
    fn spasi_sesudah_titik_koma_diabaikan() {
        assert_eq!(nilai(&h("a=1;   b=2"), "b"), Some("2".into()));
    }

    #[test]
    fn nilai_kosong_dianggap_tak_ada() {
        assert_eq!(nilai(&h("a=1; b="), "b"), None);
    }

    /// Sudut yang paling mudah salah saat ditulis ulang.
    #[test]
    fn nama_yang_jadi_awalan_nama_lain_tak_tertukar() {
        assert_eq!(nilai(&h("pulse_token_lama=x"), "pulse_token"), None);
        assert_eq!(nilai(&h("pulse_token_lama=x; pulse_token=y"), "pulse_token"), Some("y".into()));
    }

    #[test]
    fn tanpa_header_cookie() {
        assert_eq!(nilai(&HeaderMap::new(), "a"), None);
    }
}
