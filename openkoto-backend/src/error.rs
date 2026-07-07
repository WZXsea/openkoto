use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

use crate::config::ConfigError;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{message}")]
    BadRequest { code: &'static str, message: String },
    #[error("{message}")]
    Unauthorized { code: &'static str, message: String },
    #[error("{message}")]
    Conflict { code: &'static str, message: String },
    #[error("{message}")]
    NotFound { code: &'static str, message: String },
    #[error("{message}")]
    Internal { code: &'static str, message: String },
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: &'static str,
    pub message: String,
}

impl AppError {
    pub fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::BadRequest {
            code,
            message: message.into(),
        }
    }

    pub fn unauthorized(code: &'static str, message: impl Into<String>) -> Self {
        Self::Unauthorized {
            code,
            message: message.into(),
        }
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::Conflict {
            code,
            message: message.into(),
        }
    }

    pub fn not_found(code: &'static str, message: impl Into<String>) -> Self {
        Self::NotFound {
            code,
            message: message.into(),
        }
    }

    pub fn internal(code: &'static str, message: impl Into<String>) -> Self {
        Self::Internal {
            code,
            message: message.into(),
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            AppError::BadRequest { .. } => StatusCode::BAD_REQUEST,
            AppError::Unauthorized { .. } => StatusCode::UNAUTHORIZED,
            AppError::Conflict { .. } => StatusCode::CONFLICT,
            AppError::NotFound { .. } => StatusCode::NOT_FOUND,
            AppError::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Database(_) | AppError::Migration(_) | AppError::Io(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    fn code(&self) -> &'static str {
        match self {
            AppError::BadRequest { code, .. }
            | AppError::Unauthorized { code, .. }
            | AppError::Conflict { code, .. }
            | AppError::NotFound { code, .. }
            | AppError::Internal { code, .. } => code,
            AppError::Config(_) => "config_error",
            AppError::Database(_) => "database_error",
            AppError::Migration(_) => "migration_error",
            AppError::Io(_) => "io_error",
        }
    }

    fn public_message(&self) -> String {
        match self {
            AppError::BadRequest { message, .. }
            | AppError::Unauthorized { message, .. }
            | AppError::Conflict { message, .. }
            | AppError::NotFound { message, .. }
            | AppError::Internal { message, .. } => message.clone(),
            AppError::Config(_) => "backend configuration error".to_string(),
            AppError::Database(_) => "database error".to_string(),
            AppError::Migration(_) => "database migration error".to_string(),
            AppError::Io(_) => "io error".to_string(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(ErrorBody {
            error: ErrorDetail {
                code: self.code(),
                message: self.public_message(),
            },
        });

        (status, body).into_response()
    }
}
