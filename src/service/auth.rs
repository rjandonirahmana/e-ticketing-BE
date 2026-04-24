use anyhow::anyhow;
use bcrypt::{hash, verify};
use std::sync::Arc;
use validator::Validate;

use crate::models::auth::AuthResponse;
use crate::models::users::{
    LoginRequest, RegisterRequest, UpdateProfileRequest, User, UserResponse, UserRole,
};
use crate::repository::user::UserRepository;
use crate::utils::error::{AppError, AppResult};
use crate::utils::jwt::JwtService;

pub struct AuthService {
    repo: Arc<dyn UserRepository>,
    jwt: JwtService,
    bcrypt_cost: u32,
    jwt_expiry_hours: i64,
}

impl AuthService {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        jwt: JwtService,
        bcrypt_cost: u32,
        jwt_expiry_hours: i64,
    ) -> Self {
        Self {
            repo,
            jwt,
            bcrypt_cost,
            jwt_expiry_hours,
        }
    }

    pub async fn register(&self, req: RegisterRequest, password: &str) -> AppResult<AuthResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        if password.len() < 5 {
            return Err(AppError::UnprocessableEntity(
                "Password must be at least 5 characters".into(),
            ));
        }

        // admin self-register is forbidden
        let role = match req.role.as_deref() {
            Some("merchant") => UserRole::Merchant,
            Some("customer") | None => UserRole::Customer,
            Some("admin") => {
                return Err(AppError::Forbidden(
                    "Admin accounts cannot self-register".into(),
                ));
            }
            Some(other) => {
                return Err(AppError::BadRequest(format!("Unknown role '{}'", other)));
            }
        };

        // duplicate email guard
        if self
            .repo
            .find_by_email_with_password(&req.email)
            .await?
            .is_some()
        {
            return Err(AppError::Conflict("Email already registered".into()));
        }

        let hashed =
            hash(password, self.bcrypt_cost).map_err(|e| AppError::Internal(anyhow!(e)))?;
        let user = self.repo.create(&req, &hashed, role).await?;

        Ok(self.build_auth_response(user))
    }

    pub async fn login(&self, req: LoginRequest) -> AppResult<AuthResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;

        let found = self.repo.find_by_email_with_password(&req.email).await?;
        let Some(record) = found else {
            return Err(AppError::Unauthorized("Invalid email or password".into()));
        };

        let ok = verify(&req.password, &record.password_hash)
            .map_err(|e| AppError::Internal(anyhow!(e)))?;
        if !ok {
            return Err(AppError::Unauthorized("Invalid email or password".into()));
        }

        Ok(self.build_auth_response(record.user))
    }

    pub async fn me(&self, user_id: &str) -> AppResult<UserResponse> {
        let user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;
        Ok(user.into())
    }

    pub async fn update_profile(
        &self,
        user_id: &str,
        req: UpdateProfileRequest,
    ) -> AppResult<UserResponse> {
        req.validate()
            .map_err(|e| AppError::UnprocessableEntity(format!("{e}")))?;
        self.repo
            .update_profile(user_id, req.name.as_deref(), req.phone.as_deref())
            .await?;
        self.me(user_id).await
    }

    fn build_auth_response(&self, user: User) -> AuthResponse {
        let token = self
            .jwt
            .sign(&user.id, &user.email, &user.role.to_string())
            .expect("sign jwt");
        AuthResponse {
            access_token: token,
            token_type: "Bearer".into(),
            expires_in: self.jwt_expiry_hours * 3600,
            user: user.into(),
        }
    }
}
