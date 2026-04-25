use serde::{Deserialize, Serialize};

use crate::models::users::UserResponse;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub user_id: String, // user id (ulid hex)
    pub phone: String,
    pub role: String,
    pub name: String,
    pub exp: i64,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64, // seconds
    pub user: UserResponse,
}
