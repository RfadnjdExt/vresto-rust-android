use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;

pub type Res<T> = Result<T, AppError>;

#[derive(Debug)]
pub enum AppError {

    Validation(String),

    Business(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),

    Conflict(String),
    Internal(String),
}

impl AppError {
    pub fn validation(m: impl Into<String>) -> Self { Self::Validation(m.into()) }
    pub fn business(m: impl Into<String>) -> Self { Self::Business(m.into()) }
    pub fn not_found(m: impl Into<String>) -> Self { Self::NotFound(m.into()) }
    pub fn conflict(m: impl Into<String>) -> Self { Self::Conflict(m.into()) }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        match &e {
            rusqlite::Error::QueryReturnedNoRows => AppError::NotFound("Data tidak ditemukan".into()),
            rusqlite::Error::SqliteFailure(f, msg) if f.code == rusqlite::ErrorCode::ConstraintViolation => {
                AppError::Conflict(format!("Pelanggaran constraint: {}", msg.clone().unwrap_or_default()))
            }
            _ => {
                tracing::error!("database error: {e}");
                AppError::Internal("Kesalahan database".into())
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, msg) = match self {
            AppError::Validation(m) => (StatusCode::UNPROCESSABLE_ENTITY, "validation", m),
            AppError::Business(m) => (StatusCode::UNPROCESSABLE_ENTITY, "business", m),
            AppError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, "unauthorized", m),
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, "forbidden", m),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m),
            AppError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, "internal", m),
        };
        (status, Json(json!({ "success": false, "code": code, "message": msg }))).into_response()
    }
}
