use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::{
    auth::{ApiJson, AuthenticatedUser},
    error::AppError,
    material_library::{
        content_sha256_hex, ensure_no_material_duplicates_in_tx, fetch_material_tags,
        fetch_material_tags_bulk, fetch_reading_progress, fetch_reading_progress_bulk,
        lock_material_fingerprints, normalize_source_url, validate_sha256, MaterialTagDto,
        ReadingProgressDto,
    },
    routes::AppState,
};

#[derive(Debug, Default, Deserialize)]
pub struct ListMaterialsQuery {
    pub query: Option<String>,
    pub source_type: Option<String>,
    pub tag: Option<String>,
    pub reading_status: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<String>,
    pub offset: Option<String>,
    pub include_archived: Option<String>,
    pub created_from: Option<String>,
    pub created_to: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateMaterialRequest {
    pub id: Option<Uuid>,
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub media_path: Option<String>,
    #[serde(default)]
    pub book_path: Option<String>,
    #[serde(default)]
    pub book_type: Option<String>,
    #[serde(default)]
    pub translated: Option<bool>,
    #[serde(default)]
    pub active_mind_map_artifact_id: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub file_sha256: Option<String>,
    #[serde(default)]
    pub duplicate_policy: Option<String>,
    #[serde(default)]
    pub segments: Option<Vec<MaterialSegmentInput>>,
}

#[derive(Debug, Deserialize)]
pub struct PatchMaterialRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub source_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub source_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub media_path: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub book_path: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub book_type: Option<Option<String>>,
    #[serde(default)]
    pub translated: Option<bool>,
    #[serde(default)]
    pub active_mind_map_artifact_id: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub file_sha256: Option<String>,
    #[serde(default)]
    pub segments: Option<Vec<MaterialSegmentInput>>,
}

fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct BulkMaterialIdsRequest {
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MaterialSegmentInput {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, rename = "order")]
    pub order: Option<i32>,
    pub text: String,
    #[serde(default)]
    pub reading_text: Option<String>,
    #[serde(default)]
    pub translation: Option<String>,
    #[serde(default)]
    pub explanation: Option<Value>,
    #[serde(default)]
    pub start_time: Option<f64>,
    #[serde(default)]
    pub end_time: Option<f64>,
    #[serde(default)]
    pub is_new_paragraph: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArticleDto {
    pub id: String,
    pub title: String,
    pub content: String,
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub media_path: Option<String>,
    pub book_path: Option<String>,
    pub book_type: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub content_updated_at: String,
    pub current_revision: i64,
    pub content_sha256: Option<String>,
    pub editable_source_material_id: Option<String>,
    pub translated: bool,
    pub active_mind_map_artifact_id: Option<String>,
    pub segments: Vec<ArticleSegmentDto>,
    pub metadata: Value,
    pub tags: Vec<MaterialTagDto>,
    pub reading_progress: Option<ReadingProgressDto>,
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArticleSegmentDto {
    pub id: String,
    pub article_id: String,
    #[serde(rename = "order")]
    pub order: i32,
    pub text: String,
    pub text_sha256: Option<String>,
    pub reading_text: Option<String>,
    pub translation: Option<String>,
    pub explanation: Option<Value>,
    pub start_time: Option<f64>,
    pub end_time: Option<f64>,
    pub created_at: String,
    pub is_new_paragraph: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct MaterialRecord {
    id: Uuid,
    title: String,
    content: String,
    source_type: Option<String>,
    source_url: Option<String>,
    media_path: Option<String>,
    book_path: Option<String>,
    book_type: Option<String>,
    translated: bool,
    active_mind_map_artifact_id: Option<String>,
    metadata: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    content_updated_at: DateTime<Utc>,
    current_revision: i64,
    editable_source_material_id: Option<Uuid>,
    normalized_source_url: Option<String>,
    content_sha256: Option<String>,
    file_sha256: Option<String>,
    archived_at: Option<DateTime<Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct SegmentRecord {
    id: Uuid,
    material_id: Uuid,
    segment_order: i32,
    text: String,
    text_sha256: Option<String>,
    reading_text: Option<String>,
    translation: Option<String>,
    explanation: Option<Value>,
    start_time: Option<f64>,
    end_time: Option<f64>,
    is_new_paragraph: bool,
    created_at: DateTime<Utc>,
}

pub async fn list_materials(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Query(query): Query<ListMaterialsQuery>,
) -> Result<Json<Vec<ArticleDto>>, AppError> {
    let tag_id = query
        .tag
        .as_deref()
        .map(|value| {
            Uuid::parse_str(value)
                .map_err(|_| AppError::bad_request("invalid_material_query", "tag must be a UUID"))
        })
        .transpose()?;
    let limit = parse_bounded_query_integer(query.limit.as_deref(), "limit", 1, 200)?;
    let offset =
        parse_bounded_query_integer(query.offset.as_deref(), "offset", 0, i64::MAX)?.unwrap_or(0);
    let reading_status = query.reading_status.as_deref();
    if reading_status
        .is_some_and(|value| !matches!(value, "unread" | "reading" | "completed" | "archived"))
    {
        return Err(AppError::bad_request(
            "invalid_material_query",
            "reading_status must be unread, reading, completed, or archived",
        ));
    }
    let include_archived =
        parse_query_bool(query.include_archived.as_deref(), "include_archived")?.unwrap_or(false);
    let created_from = parse_query_timestamp(query.created_from.as_deref(), "created_from")?;
    let created_to = parse_query_timestamp(query.created_to.as_deref(), "created_to")?;
    if let Some(source_type) = query.source_type.as_deref() {
        validate_source_type(source_type)?;
    }
    let sort = query.sort.as_deref().unwrap_or("created_at_desc");
    let order_by = match sort {
        "created_at_desc" => "m.created_at DESC, m.id DESC",
        "created_at_asc" => "m.created_at ASC, m.id ASC",
        "updated_at_desc" => "m.updated_at DESC, m.id DESC",
        "updated_at_asc" => "m.updated_at ASC, m.id ASC",
        "title_asc" => "lower(m.title) ASC, m.id ASC",
        "title_desc" => "lower(m.title) DESC, m.id DESC",
        "last_opened_at_desc" | "last_read_at_desc" => {
            "rp.last_opened_at DESC NULLS LAST, m.created_at DESC, m.id DESC"
        }
        "progress_desc" => "rp.progress_ratio DESC NULLS LAST, m.created_at DESC, m.id DESC",
        "progress_asc" => "rp.progress_ratio ASC NULLS FIRST, m.created_at DESC, m.id DESC",
        _ => {
            return Err(AppError::bad_request(
                "invalid_material_query",
                "unsupported material sort",
            ))
        }
    };

    let mut builder = QueryBuilder::<Postgres>::new(
        r#"
        SELECT m.id, m.title, m.content, m.source_type, m.source_url, m.media_path,
               m.book_path, m.book_type, m.translated, m.active_mind_map_artifact_id,
               m.metadata, m.created_at, m.updated_at, m.normalized_source_url,
               m.content_sha256, m.file_sha256, m.archived_at,
               m.content_updated_at, m.current_revision, m.editable_source_material_id
        FROM materials m
        LEFT JOIN reading_progress rp
          ON rp.user_id = m.user_id AND rp.material_id = m.id
        WHERE m.user_id =
        "#,
    );
    builder.push_bind(user.id);
    if reading_status == Some("archived") {
        builder.push(" AND m.archived_at IS NOT NULL");
    } else if !include_archived {
        builder.push(" AND m.archived_at IS NULL");
    }
    if let Some(value) = query
        .query
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        let pattern = format!("%{value}%");
        builder
            .push(" AND (m.title ILIKE ")
            .push_bind(pattern.clone());
        builder
            .push(" OR m.content ILIKE ")
            .push_bind(pattern.clone());
        builder
            .push(" OR COALESCE(m.source_url, '') ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
    if let Some(source_type) = query.source_type.as_deref() {
        builder.push(" AND m.source_type = ").push_bind(source_type);
    }
    if let Some(tag_id) = tag_id {
        builder.push(
            " AND EXISTS (SELECT 1 FROM material_tag_links mtl \
             WHERE mtl.user_id = m.user_id AND mtl.material_id = m.id AND mtl.tag_id = ",
        );
        builder.push_bind(tag_id).push(")");
    }
    if let Some(created_from) = created_from {
        builder
            .push(" AND m.created_at >= ")
            .push_bind(created_from);
    }
    if let Some(created_to) = created_to {
        builder.push(" AND m.created_at <= ").push_bind(created_to);
    }
    if let Some(status) = reading_status {
        if status == "archived" {
            // The archive predicate above fully defines this virtual reading status.
        } else if status == "unread" {
            builder.push(" AND COALESCE(rp.status, 'unread') = 'unread'");
        } else {
            builder.push(" AND rp.status = ").push_bind(status);
        }
    }
    builder.push(" ORDER BY ").push(order_by);
    if let Some(limit) = limit {
        builder.push(" LIMIT ").push_bind(limit);
    }
    if offset > 0 {
        builder.push(" OFFSET ").push_bind(offset);
    }

    let records = builder
        .build_query_as::<MaterialRecord>()
        .fetch_all(&state.pool)
        .await?;

    let material_ids = records.iter().map(|record| record.id).collect::<Vec<_>>();
    let (mut segments_by_material, mut tags_by_material, mut progress_by_material) = tokio::try_join!(
        fetch_segments_bulk(&state.pool, user.id, &material_ids),
        fetch_material_tags_bulk(&state.pool, user.id, &material_ids),
        fetch_reading_progress_bulk(&state.pool, user.id, &material_ids),
    )?;
    let mut materials = Vec::with_capacity(records.len());
    for record in records {
        let material_id = record.id;
        let segments = segments_by_material
            .remove(&material_id)
            .unwrap_or_default();
        let tags = tags_by_material.remove(&material_id).unwrap_or_default();
        let progress = progress_by_material.remove(&material_id);
        materials.push(article_from_records(record, segments, tags, progress));
    }

    Ok(Json(materials))
}

pub async fn create_material(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<CreateMaterialRequest>,
) -> Result<Json<ArticleDto>, AppError> {
    validate_title(&payload.title)?;

    let material_id = payload.id.unwrap_or_else(Uuid::new_v4);
    let metadata = payload
        .metadata
        .unwrap_or_else(|| Value::Object(Default::default()));
    let translated = payload.translated.unwrap_or(false);
    let source_type = payload
        .source_type
        .or_else(|| Some("article".to_string()))
        .filter(|value| !value.trim().is_empty());
    if let Some(source_type) = source_type.as_deref() {
        validate_source_type(source_type)?;
    }
    let normalized_source_url = normalize_source_url(payload.source_url.as_deref())?;
    let content_sha256 = content_sha256_hex(&payload.content);
    let file_sha256 = validate_sha256(payload.file_sha256.as_deref(), "file_sha256")?;
    let duplicate_policy = payload.duplicate_policy.as_deref().unwrap_or("reject");
    if !matches!(duplicate_policy, "reject" | "keep_copy") {
        return Err(AppError::bad_request(
            "invalid_duplicate_policy",
            "duplicate_policy must be reject or keep_copy",
        ));
    }
    let segment_inputs = payload
        .segments
        .unwrap_or_else(|| create_segments_from_content(&payload.content));

    let mut tx = state.pool.begin().await?;
    if duplicate_policy == "reject" {
        lock_material_fingerprints(
            &mut tx,
            user.id,
            normalized_source_url.as_deref(),
            content_sha256.as_deref(),
            file_sha256.as_deref(),
        )
        .await?;
        ensure_no_material_duplicates_in_tx(
            &mut tx,
            user.id,
            None,
            normalized_source_url.as_deref(),
            content_sha256.as_deref(),
            file_sha256.as_deref(),
        )
        .await?;
    }
    let record = sqlx::query_as::<_, MaterialRecord>(
        r#"
        INSERT INTO materials (
            id, user_id, title, content, source_type, source_url, media_path, book_path,
            book_type, translated, active_mind_map_artifact_id, metadata,
            normalized_source_url, content_sha256, file_sha256
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        RETURNING id, title, content, source_type, source_url, media_path, book_path, book_type,
                  translated, active_mind_map_artifact_id, metadata, created_at, updated_at,
                  normalized_source_url, content_sha256, file_sha256, archived_at,
                  content_updated_at, current_revision, editable_source_material_id
        "#,
    )
    .bind(material_id)
    .bind(user.id)
    .bind(payload.title.trim())
    .bind(payload.content)
    .bind(source_type)
    .bind(payload.source_url)
    .bind(payload.media_path)
    .bind(payload.book_path)
    .bind(payload.book_type)
    .bind(translated)
    .bind(payload.active_mind_map_artifact_id)
    .bind(metadata)
    .bind(normalized_source_url)
    .bind(content_sha256)
    .bind(file_sha256)
    .fetch_one(&mut *tx)
    .await?;

    insert_segments(&mut tx, user.id, record.id, segment_inputs).await?;
    tx.commit().await?;
    crate::document_editing::initialize_document(&state.pool, user.id, record.id).await?;

    let record = fetch_material(&state.pool, user.id, record.id).await?;
    let segments = fetch_segments(&state.pool, user.id, record.id).await?;
    let tags = fetch_material_tags(&state.pool, user.id, record.id).await?;
    let progress = fetch_reading_progress(&state.pool, user.id, record.id).await?;
    Ok(Json(article_from_records(record, segments, tags, progress)))
}

pub async fn get_material(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ArticleDto>, AppError> {
    let record = fetch_material(&state.pool, user.id, id).await?;
    let segments = fetch_segments(&state.pool, user.id, id).await?;
    let tags = fetch_material_tags(&state.pool, user.id, id).await?;
    let progress = fetch_reading_progress(&state.pool, user.id, id).await?;

    Ok(Json(article_from_records(record, segments, tags, progress)))
}

pub async fn patch_material(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(payload): ApiJson<PatchMaterialRequest>,
) -> Result<Json<ArticleDto>, AppError> {
    let existing = fetch_material(&state.pool, user.id, id).await?;
    let attempts_document_write = payload.content.is_some() || payload.segments.is_some();
    let existing_is_media = matches!(
        existing.source_type.as_deref(),
        Some("youtube" | "local_video" | "audio")
    );
    let requested_source_remains_media = payload.source_type.as_ref().is_none_or(|source_type| {
        matches!(
            source_type.as_deref(),
            Some("youtube" | "local_video" | "audio")
        )
    });
    let allows_media_subtitle_compat = existing_is_media && requested_source_remains_media;
    if attempts_document_write && !allows_media_subtitle_compat {
        return Err(AppError::bad_request(
            "deprecated_document_write",
            "text document content must be changed through /materials/{id}/document preview and commit endpoints",
        ));
    }
    let updates_content = payload.content.is_some();
    let existing_normalized_source_url = existing
        .normalized_source_url
        .clone()
        .or(normalize_source_url(existing.source_url.as_deref())?);
    let existing_content_sha256 = existing
        .content_sha256
        .clone()
        .or_else(|| content_sha256_hex(&existing.content));
    let existing_file_sha256 = existing.file_sha256.clone();

    if let Some(title) = &payload.title {
        validate_title(title)?;
    }

    let title = payload.title.unwrap_or(existing.title);
    let content = payload.content.unwrap_or(existing.content);
    let source_type = payload.source_type.unwrap_or(existing.source_type);
    if let Some(source_type) = source_type.as_deref() {
        validate_source_type(source_type)?;
    }
    let source_url = payload.source_url.unwrap_or(existing.source_url);
    let normalized_source_url = normalize_source_url(source_url.as_deref())?;
    let content_sha256 = content_sha256_hex(&content);
    let file_sha256 = match payload.file_sha256.as_deref() {
        Some(value) => validate_sha256(Some(value), "file_sha256")?,
        None => existing.file_sha256,
    };
    let media_path = payload.media_path.unwrap_or(existing.media_path);
    let book_path = payload.book_path.unwrap_or(existing.book_path);
    let book_type = payload.book_type.unwrap_or(existing.book_type);
    let translated = payload.translated.unwrap_or(existing.translated);
    let active_mind_map_artifact_id = payload
        .active_mind_map_artifact_id
        .or(existing.active_mind_map_artifact_id);
    let metadata = payload.metadata.unwrap_or(existing.metadata);
    let fingerprints_changed = normalized_source_url != existing_normalized_source_url
        || content_sha256 != existing_content_sha256
        || file_sha256 != existing_file_sha256;

    let mut tx = state.pool.begin().await?;
    if fingerprints_changed {
        lock_material_fingerprints(
            &mut tx,
            user.id,
            normalized_source_url.as_deref(),
            content_sha256.as_deref(),
            file_sha256.as_deref(),
        )
        .await?;
        ensure_no_material_duplicates_in_tx(
            &mut tx,
            user.id,
            Some(id),
            normalized_source_url.as_deref(),
            content_sha256.as_deref(),
            file_sha256.as_deref(),
        )
        .await?;
    }
    let record = sqlx::query_as::<_, MaterialRecord>(
        r#"
        UPDATE materials
        SET title = $3,
            content = $4,
            source_type = $5,
            source_url = $6,
            media_path = $7,
            book_path = $8,
            book_type = $9,
            translated = $10,
            active_mind_map_artifact_id = $11,
            metadata = $12,
            normalized_source_url = $13,
            content_sha256 = $14,
            file_sha256 = $15,
            content_updated_at = CASE WHEN $16 THEN NOW() ELSE content_updated_at END,
            updated_at = NOW()
        WHERE id = $1 AND user_id = $2
        RETURNING id, title, content, source_type, source_url, media_path, book_path, book_type,
                  translated, active_mind_map_artifact_id, metadata, created_at, updated_at,
                  normalized_source_url, content_sha256, file_sha256, archived_at,
                  content_updated_at, current_revision, editable_source_material_id
        "#,
    )
    .bind(id)
    .bind(user.id)
    .bind(title.trim())
    .bind(content)
    .bind(source_type)
    .bind(source_url)
    .bind(media_path)
    .bind(book_path)
    .bind(book_type)
    .bind(translated)
    .bind(active_mind_map_artifact_id)
    .bind(metadata)
    .bind(normalized_source_url)
    .bind(content_sha256)
    .bind(file_sha256)
    .bind(updates_content)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE learning_items
        SET source_material_title_snapshot = $3,
            source_type_snapshot = $4
        WHERE user_id = $1 AND material_id = $2
        "#,
    )
    .bind(user.id)
    .bind(record.id)
    .bind(&record.title)
    .bind(&record.source_type)
    .execute(&mut *tx)
    .await?;

    if let Some(segments) = payload.segments {
        replace_segments_compat(&mut tx, user.id, record.id, segments).await?;
    }

    tx.commit().await?;

    let segments = fetch_segments(&state.pool, user.id, record.id).await?;
    let tags = fetch_material_tags(&state.pool, user.id, record.id).await?;
    let progress = fetch_reading_progress(&state.pool, user.id, record.id).await?;
    Ok(Json(article_from_records(record, segments, tags, progress)))
}

pub async fn bulk_archive_materials(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<BulkMaterialIdsRequest>,
) -> Result<Json<Value>, AppError> {
    let ids = validate_bulk_ids(payload.ids)?;
    let result = sqlx::query(
        "UPDATE materials SET archived_at = COALESCE(archived_at, NOW()), updated_at = NOW() \
         WHERE user_id = $1 AND id = ANY($2)",
    )
    .bind(user.id)
    .bind(&ids)
    .execute(&state.pool)
    .await?;
    Ok(Json(
        serde_json::json!({ "affected": result.rows_affected() }),
    ))
}

pub async fn bulk_unarchive_materials(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<BulkMaterialIdsRequest>,
) -> Result<Json<Value>, AppError> {
    let ids = validate_bulk_ids(payload.ids)?;
    let result = sqlx::query(
        "UPDATE materials SET archived_at = NULL, updated_at = NOW() \
         WHERE user_id = $1 AND id = ANY($2)",
    )
    .bind(user.id)
    .bind(&ids)
    .execute(&state.pool)
    .await?;
    Ok(Json(
        serde_json::json!({ "affected": result.rows_affected() }),
    ))
}

pub async fn bulk_delete_materials(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<BulkMaterialIdsRequest>,
) -> Result<Json<Value>, AppError> {
    let ids = validate_bulk_ids(payload.ids)?;
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        r#"
        UPDATE learning_items li
        SET source_material_title_snapshot = m.title,
            source_type_snapshot = m.source_type
        FROM materials m
        WHERE li.user_id = $1 AND li.material_id = m.id AND m.id = ANY($2)
        "#,
    )
    .bind(user.id)
    .bind(&ids)
    .execute(&mut *tx)
    .await?;
    let result = sqlx::query("DELETE FROM materials WHERE user_id = $1 AND id = ANY($2)")
        .bind(user.id)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(
        serde_json::json!({ "affected": result.rows_affected() }),
    ))
}

pub async fn delete_material(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        r#"
        UPDATE learning_items li
        SET source_material_title_snapshot = m.title,
            source_type_snapshot = m.source_type
        FROM materials m
        WHERE li.user_id = $1 AND li.material_id = m.id AND m.id = $2
        "#,
    )
    .bind(user.id)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let deleted = sqlx::query_scalar::<_, Uuid>(
        r#"
        DELETE FROM materials
        WHERE id = $1 AND user_id = $2
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await?;

    if deleted.is_none() {
        return Err(material_not_found());
    }
    tx.commit().await?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn fetch_material(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<MaterialRecord, AppError> {
    sqlx::query_as::<_, MaterialRecord>(
        r#"
        SELECT id, title, content, source_type, source_url, media_path, book_path, book_type,
               translated, active_mind_map_artifact_id, metadata, created_at, updated_at,
               normalized_source_url, content_sha256, file_sha256, archived_at,
               content_updated_at, current_revision, editable_source_material_id
        FROM materials
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(material_not_found)
}

async fn fetch_segments(
    pool: &PgPool,
    user_id: Uuid,
    material_id: Uuid,
) -> Result<Vec<SegmentRecord>, AppError> {
    sqlx::query_as::<_, SegmentRecord>(
        r#"
        SELECT id, material_id, segment_order, text, text_sha256, reading_text, translation, explanation,
               start_time, end_time, is_new_paragraph, created_at
        FROM material_segments
        WHERE user_id = $1 AND material_id = $2 AND deleted_at IS NULL
        ORDER BY segment_order ASC
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

async fn fetch_segments_bulk(
    pool: &PgPool,
    user_id: Uuid,
    material_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<SegmentRecord>>, AppError> {
    if material_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let records = sqlx::query_as::<_, SegmentRecord>(
        r#"
        SELECT id, material_id, segment_order, text, text_sha256, reading_text, translation, explanation,
               start_time, end_time, is_new_paragraph, created_at
        FROM material_segments
        WHERE user_id = $1 AND material_id = ANY($2) AND deleted_at IS NULL
        ORDER BY material_id, segment_order ASC
        "#,
    )
    .bind(user_id)
    .bind(material_ids)
    .fetch_all(pool)
    .await?;
    let mut grouped = HashMap::<Uuid, Vec<SegmentRecord>>::new();
    for record in records {
        grouped.entry(record.material_id).or_default().push(record);
    }
    Ok(grouped)
}

async fn insert_segments(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    material_id: Uuid,
    segments: Vec<MaterialSegmentInput>,
) -> Result<(), AppError> {
    for (index, segment) in segments.into_iter().enumerate() {
        let text = segment.text.trim().to_string();
        if text.is_empty() {
            return Err(AppError::bad_request(
                "invalid_segment",
                "segment text must not be empty",
            ));
        }

        sqlx::query(
            r#"
            INSERT INTO material_segments (
                id, user_id, material_id, segment_order, text, reading_text, translation,
                explanation, start_time, end_time, is_new_paragraph
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
        )
        .bind(
            segment
                .id
                .as_deref()
                .and_then(|id| Uuid::parse_str(id).ok())
                .unwrap_or_else(Uuid::new_v4),
        )
        .bind(user_id)
        .bind(material_id)
        .bind(segment.order.unwrap_or(index as i32))
        .bind(text)
        .bind(segment.reading_text)
        .bind(segment.translation)
        .bind(segment.explanation)
        .bind(segment.start_time)
        .bind(segment.end_time)
        .bind(segment.is_new_paragraph.unwrap_or(index == 0))
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn replace_segments_compat(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    material_id: Uuid,
    segments: Vec<MaterialSegmentInput>,
) -> Result<(), AppError> {
    let existing_blocks = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id FROM material_blocks
        WHERE user_id = $1 AND material_id = $2 AND deleted_at IS NULL
        ORDER BY block_order
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_all(&mut **tx)
    .await?;

    sqlx::query(
        "UPDATE material_blocks SET block_order = block_order + 1000000 WHERE user_id = $1 AND material_id = $2 AND deleted_at IS NULL",
    )
    .bind(user_id)
    .bind(material_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE material_segments SET segment_order = segment_order + 1000000 WHERE user_id = $1 AND material_id = $2 AND deleted_at IS NULL",
    )
    .bind(user_id)
    .bind(material_id)
    .execute(&mut **tx)
    .await?;

    let mut live_blocks = Vec::<Uuid>::new();
    let mut live_segments = Vec::<Uuid>::new();
    let mut paragraph_index = 0usize;
    let mut block_segment_order = 0i32;
    let mut current_block_id = None;
    for (index, segment) in segments.into_iter().enumerate() {
        let text = segment.text.trim().to_string();
        if text.is_empty() {
            return Err(AppError::bad_request(
                "invalid_segment",
                "segment text must not be empty",
            ));
        }
        if index == 0 || segment.is_new_paragraph.unwrap_or(false) {
            block_segment_order = 0;
            let block_id = existing_blocks
                .get(paragraph_index)
                .copied()
                .unwrap_or_else(Uuid::new_v4);
            let block_row = sqlx::query_scalar::<_, Uuid>(
                r#"
                INSERT INTO material_blocks (
                    id, user_id, material_id, block_order, block_type, attrs, deleted_at
                ) VALUES ($1, $2, $3, $4, 'paragraph', '{}'::jsonb, NULL)
                ON CONFLICT (id) DO UPDATE
                SET block_order = EXCLUDED.block_order, deleted_at = NULL, updated_at = NOW()
                WHERE material_blocks.user_id = EXCLUDED.user_id
                  AND material_blocks.material_id = EXCLUDED.material_id
                RETURNING id
                "#,
            )
            .bind(block_id)
            .bind(user_id)
            .bind(material_id)
            .bind(paragraph_index as i32)
            .fetch_optional(&mut **tx)
            .await?;
            if block_row.is_none() {
                return Err(AppError::conflict(
                    "document_block_id_conflict",
                    "block id already belongs to another material",
                ));
            }
            live_blocks.push(block_id);
            current_block_id = Some(block_id);
            paragraph_index += 1;
        }
        let block_id = current_block_id.expect("first segment initializes a block");
        let segment_id = segment
            .id
            .as_deref()
            .and_then(|id| Uuid::parse_str(id).ok())
            .unwrap_or_else(Uuid::new_v4);
        let text_hash = crate::document_editing::source_text_sha256(&text);
        let reading_hash = segment.reading_text.as_ref().map(|_| text_hash.clone());
        let translation_hash = segment.translation.as_ref().map(|_| text_hash.clone());
        let explanation_hash = segment.explanation.as_ref().map(|_| text_hash.clone());
        let segment_row = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO material_segments (
                id, user_id, material_id, block_id, segment_order, block_segment_order,
                text, reading_text, translation, explanation, start_time, end_time,
                is_new_paragraph, text_sha256, reading_source_sha256,
                translation_source_sha256, explanation_source_sha256, deleted_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                $14, $15, $16, $17, NULL
            )
            ON CONFLICT (id) DO UPDATE
            SET block_id = EXCLUDED.block_id, segment_order = EXCLUDED.segment_order,
                block_segment_order = EXCLUDED.block_segment_order, text = EXCLUDED.text,
                reading_text = EXCLUDED.reading_text, translation = EXCLUDED.translation,
                explanation = EXCLUDED.explanation, start_time = EXCLUDED.start_time,
                end_time = EXCLUDED.end_time, is_new_paragraph = EXCLUDED.is_new_paragraph,
                text_sha256 = EXCLUDED.text_sha256,
                reading_source_sha256 = EXCLUDED.reading_source_sha256,
                translation_source_sha256 = EXCLUDED.translation_source_sha256,
                explanation_source_sha256 = EXCLUDED.explanation_source_sha256,
                deleted_at = NULL, updated_at = NOW()
            WHERE material_segments.user_id = EXCLUDED.user_id
              AND material_segments.material_id = EXCLUDED.material_id
            RETURNING id
            "#,
        )
        .bind(segment_id)
        .bind(user_id)
        .bind(material_id)
        .bind(block_id)
        .bind(index as i32)
        .bind(block_segment_order)
        .bind(text)
        .bind(segment.reading_text)
        .bind(segment.translation)
        .bind(segment.explanation)
        .bind(segment.start_time)
        .bind(segment.end_time)
        .bind(block_segment_order == 0)
        .bind(text_hash)
        .bind(reading_hash)
        .bind(translation_hash)
        .bind(explanation_hash)
        .fetch_optional(&mut **tx)
        .await?;
        if segment_row.is_none() {
            return Err(AppError::conflict(
                "document_segment_id_conflict",
                "segment id already belongs to another material",
            ));
        }
        live_segments.push(segment_id);
        block_segment_order += 1;
    }

    sqlx::query(
        "UPDATE material_segments SET deleted_at = NOW(), updated_at = NOW() WHERE user_id = $1 AND material_id = $2 AND deleted_at IS NULL AND NOT (id = ANY($3))",
    )
    .bind(user_id)
    .bind(material_id)
    .bind(&live_segments)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE material_blocks SET deleted_at = NOW(), updated_at = NOW() WHERE user_id = $1 AND material_id = $2 AND deleted_at IS NULL AND NOT (id = ANY($3))",
    )
    .bind(user_id)
    .bind(material_id)
    .bind(&live_blocks)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn article_from_records(
    record: MaterialRecord,
    segments: Vec<SegmentRecord>,
    tags: Vec<MaterialTagDto>,
    reading_progress: Option<ReadingProgressDto>,
) -> ArticleDto {
    ArticleDto {
        id: record.id.to_string(),
        title: record.title,
        content: record.content,
        source_type: record.source_type,
        source_url: record.source_url,
        media_path: record.media_path,
        book_path: record.book_path,
        book_type: record.book_type,
        created_at: record.created_at.to_rfc3339(),
        updated_at: record.updated_at.to_rfc3339(),
        content_updated_at: record.content_updated_at.to_rfc3339(),
        current_revision: record.current_revision,
        content_sha256: record.content_sha256,
        editable_source_material_id: record.editable_source_material_id.map(|id| id.to_string()),
        translated: record.translated,
        active_mind_map_artifact_id: record.active_mind_map_artifact_id,
        segments: segments.into_iter().map(segment_from_record).collect(),
        metadata: record.metadata,
        tags,
        reading_progress,
        archived_at: record.archived_at.map(|value| value.to_rfc3339()),
    }
}

fn segment_from_record(record: SegmentRecord) -> ArticleSegmentDto {
    let text_sha256 = record
        .text_sha256
        .or_else(|| Some(crate::document_editing::source_text_sha256(&record.text)));
    ArticleSegmentDto {
        id: record.id.to_string(),
        article_id: record.material_id.to_string(),
        order: record.segment_order,
        text: record.text,
        text_sha256,
        reading_text: record.reading_text,
        translation: record.translation,
        explanation: record.explanation,
        start_time: record.start_time,
        end_time: record.end_time,
        created_at: record.created_at.to_rfc3339(),
        is_new_paragraph: record.is_new_paragraph,
    }
}

fn validate_title(title: &str) -> Result<(), AppError> {
    if title.trim().is_empty() {
        return Err(AppError::bad_request(
            "invalid_title",
            "material title must not be empty",
        ));
    }

    Ok(())
}

fn validate_source_type(source_type: &str) -> Result<(), AppError> {
    if !matches!(
        source_type,
        "article" | "web" | "text_file" | "youtube" | "local_video" | "audio" | "book"
    ) {
        return Err(AppError::bad_request(
            "invalid_source_type",
            "unsupported material source_type",
        ));
    }
    Ok(())
}

fn parse_bounded_query_integer(
    value: Option<&str>,
    field: &str,
    minimum: i64,
    maximum: i64,
) -> Result<Option<i64>, AppError> {
    value
        .map(|value| {
            value.parse::<i64>().map_err(|_| {
                AppError::bad_request(
                    "invalid_material_query",
                    format!("{field} must be an integer"),
                )
            })
        })
        .transpose()?
        .map(|value| {
            if (minimum..=maximum).contains(&value) {
                Ok(value)
            } else {
                Err(AppError::bad_request(
                    "invalid_material_query",
                    format!("{field} must be between {minimum} and {maximum}"),
                ))
            }
        })
        .transpose()
}

fn parse_query_bool(value: Option<&str>, field: &str) -> Result<Option<bool>, AppError> {
    value
        .map(|value| match value {
            "true" | "1" => Ok(true),
            "false" | "0" => Ok(false),
            _ => Err(AppError::bad_request(
                "invalid_material_query",
                format!("{field} must be true or false"),
            )),
        })
        .transpose()
}

fn parse_query_timestamp(
    value: Option<&str>,
    field: &str,
) -> Result<Option<DateTime<Utc>>, AppError> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|value| value.with_timezone(&Utc))
                .map_err(|_| {
                    AppError::bad_request(
                        "invalid_material_query",
                        format!("{field} must be an RFC3339 timestamp"),
                    )
                })
        })
        .transpose()
}

fn validate_bulk_ids(ids: Vec<Uuid>) -> Result<Vec<Uuid>, AppError> {
    if ids.is_empty() || ids.len() > 100 {
        return Err(AppError::bad_request(
            "invalid_material_ids",
            "ids must contain between 1 and 100 values",
        ));
    }
    let mut seen = HashSet::with_capacity(ids.len());
    let unique = ids
        .into_iter()
        .filter(|id| seen.insert(*id))
        .collect::<Vec<_>>();
    Ok(unique)
}

fn material_not_found() -> AppError {
    AppError::not_found("material_not_found", "material not found")
}

fn create_segments_from_content(content: &str) -> Vec<MaterialSegmentInput> {
    let mut segments = Vec::new();

    for paragraph in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let sentences = split_into_sentences(paragraph);
        for (sentence_index, sentence) in sentences.into_iter().enumerate() {
            let text = sentence.trim().to_string();
            if text.is_empty() {
                continue;
            }

            segments.push(MaterialSegmentInput {
                id: Some(Uuid::new_v4().to_string()),
                order: Some(segments.len() as i32),
                text,
                reading_text: None,
                translation: None,
                explanation: None,
                start_time: None,
                end_time: None,
                is_new_paragraph: Some(sentence_index == 0),
            });
        }
    }

    if segments.is_empty() && !content.trim().is_empty() {
        segments.push(MaterialSegmentInput {
            id: Some(Uuid::new_v4().to_string()),
            order: Some(0),
            text: content.trim().to_string(),
            reading_text: None,
            translation: None,
            explanation: None,
            start_time: None,
            end_time: None,
            is_new_paragraph: Some(true),
        });
    }

    segments
}

fn split_into_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        current.push(c);
        let is_sentence_end = c == '。'
            || c == '？'
            || c == '！'
            || c == '?'
            || c == '!'
            || (c == '.' && !is_abbreviation(&chars, i));

        if is_sentence_end {
            if i + 1 < chars.len() {
                let next = chars[i + 1];
                if next == '"' || next == '\'' || next == '\u{2019}' || next == ')' || next == '）'
                {
                    i += 1;
                    current.push(next);
                }
            }
            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                sentences.push(trimmed);
            }
            current.clear();
        }

        i += 1;
    }

    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }

    if sentences.is_empty() && !text.trim().is_empty() {
        sentences.push(text.trim().to_string());
    }

    sentences
}

fn is_abbreviation(chars: &[char], pos: usize) -> bool {
    if pos + 1 < chars.len() && chars[pos + 1].is_alphabetic() {
        return true;
    }

    let mut word = String::new();
    let mut index = pos as i32 - 1;
    while index >= 0 && chars[index as usize].is_alphabetic() {
        word.insert(0, chars[index as usize]);
        index -= 1;
    }

    let abbreviations = [
        "mr", "mrs", "ms", "dr", "jr", "sr", "vs", "etc", "inc", "ltd", "no", "st", "ave", "rd",
    ];
    let word_lower = word.to_lowercase();

    abbreviations.contains(&word_lower.as_str())
        || (word.len() == 1 && word.chars().next().is_some_and(char::is_uppercase))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentence_split_preserves_abbreviations() {
        let segments =
            create_segments_from_content("Dr. Smith reviewed it. It worked.\nSecond paragraph");

        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].text, "Dr. Smith reviewed it.");
        assert!(segments[0].is_new_paragraph.unwrap());
        assert!(!segments[1].is_new_paragraph.unwrap());
        assert!(segments[2].is_new_paragraph.unwrap());
    }
}
