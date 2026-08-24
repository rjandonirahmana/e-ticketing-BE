use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};

use crate::models::auth::Claims;

/// JwtService menyimpan EncodingKey + DecodingKey yang sudah di-pre-compute
/// dari secret. Sebelumnya secret disimpan sebagai String dan Key dibuat ulang
/// di setiap sign() + verify() — ini alokasi + CPU yang tidak perlu.
#[derive(Clone)]
pub struct JwtService {
    enc: EncodingKey,
    dec: DecodingKey,
    /// Umur token yang BENAR-BENAR dipasang ke klaim `exp`.
    ///
    /// Dulu `sign()` memakai 100 hari yang ditulis mati, sementara
    /// `JWT_EXPIRY_HOURS` dari konfigurasi hanya dipakai untuk mengisi field
    /// `expires_in` di respons login (`service::auth`). Jadi server memberi
    /// tahu klien "token ini berlaku 24 jam" sambil menerbitkan token yang
    /// sah selama seratus hari.
    ///
    /// Selisih itu bukan sekadar tidak rapi. Ia membuat satu-satunya angka yang
    /// bisa dipakai siapa pun untuk menalar masa berlaku sesi — termasuk saat
    /// menilai dampak token yang bocor — menjadi salah, dan salahnya ke arah
    /// yang meremehkan: 100× lebih panjang dari yang tertulis.
    expiry_hours: i64,
}

/// Umur token bila konfigurasinya tak masuk akal (nol/negatif). Dipilih pendek
/// dengan sengaja: kalau konfigurasi rusak, sesi yang terlalu singkat cuma
/// merepotkan, sedangkan sesi yang terlalu panjang adalah risiko.
const EXPIRY_FALLBACK_HOURS: i64 = 24;

impl JwtService {
    pub fn new(secret: &str, expiry_hours: i64) -> Self {
        Self {
            enc: EncodingKey::from_secret(secret.as_bytes()),
            // DecodingKey::from_secret mengkloning bytes internally — aman.
            dec: DecodingKey::from_secret(secret.as_bytes()),
            expiry_hours: if expiry_hours > 0 {
                expiry_hours
            } else {
                tracing::warn!(
                    expiry_hours,
                    "JWT_EXPIRY_HOURS tak masuk akal — memakai {EXPIRY_FALLBACK_HOURS} jam"
                );
                EXPIRY_FALLBACK_HOURS
            },
        }
    }

    /// Umur token dalam detik — satu-satunya sumber untuk `expires_in` yang
    /// dikirim ke klien, supaya angka yang dijanjikan dan yang diterbitkan
    /// mustahil berbeda lagi.
    pub fn expires_in_secs(&self) -> i64 {
        self.expiry_hours * 3600
    }

    pub fn sign(
        &self,
        user_id: &str,
        name: &str,
        phone: &str,
        role: &str,
    ) -> anyhow::Result<String> {
        let claims = Claims {
            user_id: user_id.to_string(),
            role: role.to_string(),
            name: name.to_string(),
            phone: phone.to_string(),
            exp: (chrono::Utc::now() + chrono::Duration::hours(self.expiry_hours))
                .timestamp(),
        };

        // Pakai pre-computed key — tidak ada alloc baru di sini.
        encode(&Header::default(), &claims, &self.enc).map_err(Into::into)
    }

    pub fn verify(&self, token: &str) -> anyhow::Result<Claims> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;

        decode::<Claims>(token, &self.dec, &validation)
            .map(|d| d.claims)
            .map_err(|e| {
                tracing::warn!("JWT verify failed: {:?}", e);
                e.into()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regresi: `sign()` pernah menulis mati 100 hari sambil mengabaikan
    /// `JWT_EXPIRY_HOURS`, sementara respons login melaporkan angka konfigurasi.
    /// Server menjanjikan 24 jam dan menerbitkan token seratus hari.
    ///
    /// Ini menguji yang PALING penting: yang dijanjikan == yang diterbitkan.
    #[test]
    fn exp_mengikuti_konfigurasi_bukan_angka_mati() {
        let svc = JwtService::new("rahasia-uji", 24);
        let token = svc.sign("u1", "Nama", "628123", "customer").unwrap();
        let claims = svc.verify(&token).unwrap();

        let sekarang = chrono::Utc::now().timestamp();
        let umur = claims.exp - sekarang;
        // 24 jam ± 1 menit (menampung waktu eksekusi uji).
        assert!(
            (24 * 3600 - 60..=24 * 3600 + 60).contains(&umur),
            "umur token {umur} detik, seharusnya ~{} detik",
            24 * 3600
        );
        assert_eq!(svc.expires_in_secs(), 24 * 3600);
    }

    /// `expires_in` yang dikirim ke klien harus SELALU sama dengan umur klaim.
    #[test]
    fn expires_in_konsisten_dengan_exp() {
        for jam in [1, 6, 24, 720] {
            let svc = JwtService::new("rahasia-uji", jam);
            let claims = svc.verify(&svc.sign("u", "n", "p", "customer").unwrap()).unwrap();
            let umur = claims.exp - chrono::Utc::now().timestamp();
            let dijanjikan = svc.expires_in_secs();
            assert!(
                (dijanjikan - umur).abs() <= 60,
                "jam={jam}: dijanjikan {dijanjikan}s, diterbitkan {umur}s",
            );
        }
    }

    /// Konfigurasi rusak (0/negatif) tak boleh menghasilkan token abadi atau
    /// token yang langsung kedaluwarsa — ia jatuh ke nilai aman.
    #[test]
    fn konfigurasi_tak_masuk_akal_jatuh_ke_bawaan() {
        for jam in [0, -5] {
            let svc = JwtService::new("rahasia-uji", jam);
            assert_eq!(svc.expires_in_secs(), EXPIRY_FALLBACK_HOURS * 3600);
        }
    }

    /// Token yang ditandatangani secret LAIN harus ditolak.
    #[test]
    fn secret_berbeda_ditolak() {
        let a = JwtService::new("secret-a", 24);
        let b = JwtService::new("secret-b", 24);
        let token = a.sign("u", "n", "p", "customer").unwrap();
        assert!(b.verify(&token).is_err());
    }
}
