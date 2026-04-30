use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};

use crate::models::auth::Claims;

/// JwtService menyimpan EncodingKey + DecodingKey yang sudah di-pre-compute
/// dari secret. Sebelumnya secret disimpan sebagai String dan Key dibuat ulang
/// di setiap sign() + verify() — ini alokasi + CPU yang tidak perlu.
#[derive(Clone)]
pub struct JwtService {
    enc: EncodingKey,
    dec: DecodingKey,
}

impl JwtService {
    pub fn new(secret: &str) -> Self {
        Self {
            enc: EncodingKey::from_secret(secret.as_bytes()),
            // DecodingKey::from_secret mengkloning bytes internally — aman.
            dec: DecodingKey::from_secret(secret.as_bytes()),
        }
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
            exp: (chrono::Utc::now() + chrono::Duration::days(100)).timestamp(),
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
