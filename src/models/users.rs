use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    Customer,
    Merchant,
    Admin,
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserRole::Customer => write!(f, "customer"),
            UserRole::Merchant => write!(f, "merchant"),
            UserRole::Admin => write!(f, "admin"),
        }
    }
}

impl From<&str> for UserRole {
    fn from(s: &str) -> Self {
        match s {
            "merchant" => UserRole::Merchant,
            "admin" => UserRole::Admin,
            _ => UserRole::Customer,
        }
    }
}

/// Full user record from DB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String, // ULID as hex string
    pub email: String,
    pub name: String,
    pub phone: Option<String>,
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request: Register new user
#[derive(Debug, Deserialize, Validate)]
pub struct RegisterRequest {
    #[validate(email(message = "Invalid email format"))]
    pub email: String,

    #[validate(length(min = 2, max = 255, message = "Name must be 2-255 characters"))]
    pub name: String,

    pub phone: Option<String>,

    /// "customer" or "merchant" (admin cannot self-register)
    pub role: Option<String>,
}

/// Request: Login
#[derive(Debug, Deserialize, Validate)]
pub struct LoginRequest {
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 5, message = "Password must be at least 5 characters"))]
    pub password: String,
}

/// Request: Update profile
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateProfileRequest {
    #[validate(length(min = 2, max = 255))]
    pub name: Option<String>,
    pub phone: Option<String>,
}

/// Response: user info (no password)
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    pub phone: Option<String>,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

impl From<User> for UserResponse {
    fn from(u: User) -> Self {
        Self {
            id: u.id,
            email: u.email,
            name: u.name,
            phone: u.phone,
            role: u.role.to_string(),
            created_at: u.created_at,
        }
    }
}
