use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Unprocessable entity: {0}")]
    UnprocessableEntity(String),

    #[error("Internal server error")]
    Internal(#[from] anyhow::Error),

    #[error("Database error: {0}")]
    Database(#[from] tokio_postgres::Error),

    #[error("Pool error: {0}")]
    Pool(#[from] deadpool_postgres::PoolError),

    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError), // Menambahkan redis error

    #[error("Redis error: {0}")]
    Bcrpyt(String), // Menambahkan redis error
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::UnprocessableEntity(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            AppError::Internal(e) => {
                tracing::error!("Internal error: {:?}", e.to_string());
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
            AppError::Database(e) => {
                tracing::error!("Database error: {:?}", e);
                // Check for unique violation
                if let Some(db_error) = e.as_db_error() {
                    if db_error.code() == &tokio_postgres::error::SqlState::UNIQUE_VIOLATION {
                        return (
                            StatusCode::CONFLICT,
                            Json(json!({ "error": "Resource already exists", "code": "CONFLICT" })),
                        )
                            .into_response();
                    }
                }
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error".into())
            }
            AppError::Pool(e) => {
                tracing::error!("Pool error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Connection pool error".into(),
                )
            }

            AppError::Redis(e) => {
                tracing::error!("Redis error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal cache/queue error".into(),
                )
            }
            AppError::Bcrpyt(e) => {
                tracing::error!("Bcrypt error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal cache/queue error".into(),
                )
            }
        };

        (
            status,
            Json(json!({
                "error": message,
                "code": status.as_u16()
            })),
        )
            .into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;
