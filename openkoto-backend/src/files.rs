use std::path::{Component, Path};

use axum::{
    body::Body,
    extract::{Multipart, Path as AxumPath, State},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, ETAG},
        HeaderMap, HeaderValue,
    },
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{auth::AuthenticatedUser, error::AppError, routes::AppState};

const MAX_UPLOAD_BYTES: usize = 200 * 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct FileDto {
    pub id: String,
    pub original_name: String,
    pub content_type: Option<String>,
    pub byte_size: i64,
    pub sha256: String,
    pub download_url: String,
    pub metadata: Value,
    pub created_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct FileRecord {
    id: Uuid,
    original_name: String,
    storage_path: String,
    content_type: Option<String>,
    byte_size: i64,
    sha256: String,
    metadata: Value,
    created_at: DateTime<Utc>,
}

struct PendingUpload {
    original_name: String,
    content_type: Option<String>,
    bytes: Vec<u8>,
    metadata: Value,
}

pub async fn upload_file(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    mut multipart: Multipart,
) -> Result<Json<FileDto>, AppError> {
    let mut upload: Option<PendingUpload> = None;
    let mut metadata = Value::Object(Default::default());

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::bad_request("invalid_multipart", "invalid multipart body"))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "metadata" => {
                let text = field
                    .text()
                    .await
                    .map_err(|_| AppError::bad_request("invalid_metadata", "invalid metadata"))?;
                metadata = serde_json::from_str(&text)
                    .map_err(|_| AppError::bad_request("invalid_metadata", "invalid metadata"))?;
            }
            "file" => {
                let original_name = sanitize_display_file_name(
                    field.file_name().unwrap_or("uploaded-file").to_string(),
                );
                let content_type = field.content_type().map(ToString::to_string);
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|_| AppError::bad_request("invalid_multipart", "invalid file body"))?;
                if bytes.len() > MAX_UPLOAD_BYTES {
                    return Err(AppError::bad_request(
                        "file_too_large",
                        "file exceeds the maximum upload size",
                    ));
                }
                upload = Some(PendingUpload {
                    original_name,
                    content_type,
                    bytes: bytes.to_vec(),
                    metadata: metadata.clone(),
                });
            }
            _ => {}
        }
    }

    let mut upload =
        upload.ok_or_else(|| AppError::bad_request("missing_file", "file is required"))?;
    upload.metadata = metadata;
    let file_id = Uuid::new_v4();
    let storage_path = format!("{}/{}", user.id, file_id);
    let absolute_path = state.config.file_storage_dir.join(&storage_path);
    let parent = absolute_path
        .parent()
        .ok_or_else(|| AppError::internal("file_storage_error", "invalid file storage path"))?;
    tokio::fs::create_dir_all(parent).await?;

    let sha256 = sha256_hex(&upload.bytes);
    tokio::fs::write(&absolute_path, &upload.bytes).await?;

    let record = sqlx::query_as::<_, FileRecord>(
        r#"
        INSERT INTO files (
            id, user_id, original_name, storage_path, content_type, byte_size, sha256, metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, original_name, storage_path, content_type, byte_size, sha256, metadata, created_at
        "#,
    )
    .bind(file_id)
    .bind(user.id)
    .bind(upload.original_name)
    .bind(&storage_path)
    .bind(upload.content_type)
    .bind(upload.bytes.len() as i64)
    .bind(sha256)
    .bind(upload.metadata)
    .fetch_one(&state.pool)
    .await;

    match record {
        Ok(record) => Ok(Json(file_from_record(record))),
        Err(error) => {
            let _ = tokio::fs::remove_file(&absolute_path).await;
            Err(AppError::Database(error))
        }
    }
}

pub async fn download_file(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    AxumPath(id): AxumPath<Uuid>,
) -> Result<Response, AppError> {
    let record = fetch_file(&state.pool, user.id, id).await?;
    validate_storage_path(&record.storage_path)?;
    let absolute_path = state.config.file_storage_dir.join(&record.storage_path);
    let bytes = tokio::fs::read(&absolute_path)
        .await
        .map_err(|_| AppError::not_found("file_not_found", "file not found"))?;

    let mut headers = HeaderMap::new();
    if let Some(content_type) = &record.content_type {
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_str(content_type)
                .map_err(|_| AppError::internal("invalid_file_header", "invalid file header"))?,
        );
    }
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&bytes.len().to_string())
            .map_err(|_| AppError::internal("invalid_file_header", "invalid file header"))?,
    );
    headers.insert(
        ETAG,
        HeaderValue::from_str(&format!("\"{}\"", record.sha256))
            .map_err(|_| AppError::internal("invalid_file_header", "invalid file header"))?,
    );
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"{}\"",
            escape_header_file_name(&record.original_name)
        ))
        .map_err(|_| AppError::internal("invalid_file_header", "invalid file header"))?,
    );

    Ok((headers, Body::from(bytes)).into_response())
}

async fn fetch_file(pool: &sqlx::PgPool, user_id: Uuid, id: Uuid) -> Result<FileRecord, AppError> {
    sqlx::query_as::<_, FileRecord>(
        r#"
        SELECT id, original_name, storage_path, content_type, byte_size, sha256, metadata, created_at
        FROM files
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("file_not_found", "file not found"))
}

fn file_from_record(record: FileRecord) -> FileDto {
    FileDto {
        id: record.id.to_string(),
        original_name: record.original_name,
        content_type: record.content_type,
        byte_size: record.byte_size,
        sha256: record.sha256,
        download_url: format!("/files/{}", record.id),
        metadata: record.metadata,
        created_at: record.created_at.to_rfc3339(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn sanitize_display_file_name(file_name: String) -> String {
    let file_name = file_name
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>()
        .replace(['/', '\\'], "_")
        .trim()
        .to_string();

    if file_name.is_empty() {
        "uploaded-file".to_string()
    } else {
        file_name
    }
}

fn escape_header_file_name(file_name: &str) -> String {
    file_name.replace(['"', '\r', '\n'], "_")
}

fn validate_storage_path(storage_path: &str) -> Result<(), AppError> {
    let path = Path::new(storage_path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AppError::internal(
            "file_storage_error",
            "invalid file storage path",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_file_name_is_not_a_path() {
        assert_eq!(
            sanitize_display_file_name("../paper.pdf".to_string()),
            ".._paper.pdf"
        );
        assert_eq!(sanitize_display_file_name("".to_string()), "uploaded-file");
    }

    #[test]
    fn rejects_non_relative_storage_paths() {
        assert!(validate_storage_path("user/file").is_ok());
        assert!(validate_storage_path("../file").is_err());
        assert!(validate_storage_path("/tmp/file").is_err());
    }
}
