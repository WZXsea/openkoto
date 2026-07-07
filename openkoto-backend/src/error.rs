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
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Database(_) | AppError::Migration(_) | AppError::Io(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    fn code(&self) -> &'static str {
        match self {
            AppError::Config(_) => "config_error",
            AppError::Database(_) => "database_error",
            AppError::Migration(_) => "migration_error",
            AppError::Io(_) => "io_error",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = Json(ErrorBody {
            error: ErrorDetail {
                code: self.code(),
                message: self.to_string(),
            },
        });

        (status, body).into_response()
    }
}
