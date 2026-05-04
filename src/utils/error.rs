use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::sync::{Arc, OnceLock};
use thiserror::Error;

use crate::service::telegram::TelegramService;

// ── Global Telegram notifier ───────────────────────────────────────────────────
// Di-init SATU kali di main.rs via `init_telegram_notifier(...)`.
// Setelah itu, setiap AppError 5xx otomatis kirim alert ke Telegram.

static TELEGRAM: OnceLock<Arc<TelegramService>> = OnceLock::new();

/// Panggil ini di main.rs setelah config di-load.
pub fn init_telegram_notifier(svc: Arc<TelegramService>) {
    let _ = TELEGRAM.set(svc);
}

/// Fire-and-forget: spawn tokio task, tidak block response ke client.
fn notify(status: u16, kind: &'static str, detail: String) {
    if let Some(tg) = TELEGRAM.get() {
        let tg = tg.clone();
        tokio::spawn(async move {
            tg.send_error_alert(status, kind, &detail).await;
        });
    }
}

// ── AppError ───────────────────────────────────────────────────────────────────

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
    Redis(#[from] redis::RedisError),

    #[error("Bcrypt error: {0}")]
    Bcrpyt(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            // ── 4xx — tidak perlu alert ──────────────────────────────────────
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::UnprocessableEntity(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),

            // ── 5xx — log full chain + kirim Telegram ────────────────────────
            AppError::Internal(e) => {
                // format!("{:#}", e) → tampilkan full error chain anyhow
                // contoh: "exec_drop failed\n  query: UPDATE ...\n\nCaused by:\n    column X does not exist"
                let detail = format!("{:#}", e);
                tracing::error!("Internal error:\n{}", detail);
                notify(500, "Internal", detail);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }

            AppError::Database(e) => {
                // tokio_postgres::Error punya info lengkap: message, detail, hint, where
                let detail = format_pg_error(e);
                tracing::error!("Database error:\n{}", detail);

                // Unique violation → 409 bukan 500, jangan alert
                if let Some(db_err) = e.as_db_error() {
                    if db_err.code() == &tokio_postgres::error::SqlState::UNIQUE_VIOLATION {
                        return (
                            StatusCode::CONFLICT,
                            Json(json!({ "error": "Resource already exists", "code": "CONFLICT" })),
                        )
                            .into_response();
                    }
                }

                notify(500, "Database", detail);
                (StatusCode::INTERNAL_SERVER_ERROR, "Database error".into())
            }

            AppError::Pool(e) => {
                let detail = format!("{:#}", e);
                tracing::error!("Pool error:\n{}", detail);
                notify(500, "ConnectionPool", detail);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Connection pool error".into(),
                )
            }

            AppError::Redis(e) => {
                let detail = format!("{:#}", e);
                tracing::error!("Redis error:\n{}", detail);
                notify(500, "Redis", detail);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal cache/queue error".into(),
                )
            }

            AppError::Bcrpyt(e) => {
                tracing::error!("Bcrypt error: {}", e);
                notify(500, "Bcrypt", e.clone());
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal error".into())
            }
        };

        (
            status,
            Json(json!({
                "error": message,
                "code":  status.as_u16()
            })),
        )
            .into_response()
    }
}

/// Format tokio_postgres::Error dengan semua field yang tersedia:
/// message, detail, hint, where (posisi query), schema, table, column.
fn format_pg_error(e: &tokio_postgres::Error) -> String {
    if let Some(db) = e.as_db_error() {
        let mut parts = vec![
            format!("message : {}", db.message()),
            format!("code    : {}", db.code().code()),
        ];
        if let Some(d) = db.detail() {
            parts.push(format!("detail  : {}", d));
        }
        if let Some(h) = db.hint() {
            parts.push(format!("hint    : {}", h));
        }
        if let Some(w) = db.where_() {
            parts.push(format!("where   : {}", w));
        }
        if let Some(s) = db.schema() {
            parts.push(format!("schema  : {}", s));
        }
        if let Some(t) = db.table() {
            parts.push(format!("table   : {}", t));
        }
        if let Some(c) = db.column() {
            parts.push(format!("column  : {}", c));
        }
        if let Some(r) = db.routine() {
            parts.push(format!("routine : {}", r));
        }
        parts.join("\n")
    } else {
        format!("{:#}", e)
    }
}

pub type AppResult<T> = Result<T, AppError>;
