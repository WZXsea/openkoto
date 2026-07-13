use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use unicode_normalization::UnicodeNormalization;
use url::Url;
use uuid::Uuid;

use crate::{
    auth::{ApiJson, AuthenticatedUser},
    error::AppError,
    routes::AppState,
};

const MAX_BULK_IDS: usize = 100;
const JOB_CREATE_FINGERPRINT_KEY: &str = "_openkoto_create_request_sha256";

#[derive(Debug, Clone, Serialize)]
pub struct MaterialTagDto {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, sqlx::FromRow)]
struct TagRow {
    id: Uuid,
    name: String,
    color: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
#[derive(Debug, Deserialize)]
pub struct CreateTag {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}
#[derive(Debug, Deserialize)]
pub struct PatchTag {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<Option<String>>,
}
#[derive(Debug, Deserialize)]
pub struct MergeTag {
    pub target_tag_id: Uuid,
}
#[derive(Debug, Deserialize)]
pub struct SetTags {
    #[serde(default)]
    pub tag_ids: Vec<Uuid>,
}
#[derive(Debug, Deserialize)]
pub struct BulkTags {
    pub ids: Vec<Uuid>,
    #[serde(default)]
    pub tag_ids: Vec<Uuid>,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadingProgressDto {
    pub material_id: String,
    pub reader_kind: String,
    pub locator: Value,
    pub progress_ratio: f64,
    pub status: String,
    pub last_opened_at: String,
    pub completed_at: Option<String>,
    pub updated_at: String,
}
#[derive(Debug, sqlx::FromRow)]
struct ProgressRow {
    material_id: Uuid,
    reader_kind: String,
    locator: Value,
    progress_ratio: f64,
    status: String,
    last_opened_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}
#[derive(Debug, Deserialize)]
pub struct PutProgress {
    pub reader_kind: String,
    #[serde(default = "empty_object")]
    pub locator: Value,
    pub progress_ratio: f64,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterialImportJobDto {
    pub id: String,
    pub source_kind: String,
    pub source_uri: Option<String>,
    pub normalized_source_url: Option<String>,
    pub file_id: Option<String>,
    pub input_hash: String,
    pub file_sha256: Option<String>,
    pub content_sha256: Option<String>,
    pub status: String,
    pub progress: f64,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub result_material_id: Option<String>,
    pub preview: Value,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}
#[derive(Debug, sqlx::FromRow)]
struct JobRow {
    id: Uuid,
    source_kind: String,
    source_uri: Option<String>,
    normalized_source_url: Option<String>,
    file_id: Option<Uuid>,
    input_hash: String,
    file_sha256: Option<String>,
    content_sha256: Option<String>,
    status: String,
    progress: f64,
    error_code: Option<String>,
    error_message: Option<String>,
    result_material_id: Option<Uuid>,
    preview: Value,
    metadata: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
}
#[derive(Debug, Deserialize)]
pub struct CreateJob {
    #[serde(default)]
    pub id: Option<Uuid>,
    pub source_kind: String,
    #[serde(default)]
    pub source_uri: Option<String>,
    #[serde(default)]
    pub file_id: Option<Uuid>,
    #[serde(default)]
    pub input_hash: Option<String>,
    #[serde(default)]
    pub file_sha256: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub content_sha256: Option<String>,
    #[serde(default = "empty_object")]
    pub preview: Value,
    #[serde(default = "empty_object")]
    pub metadata: Value,
}
#[derive(Debug, Deserialize)]
pub struct PatchJob {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub progress: Option<f64>,
    #[serde(default)]
    pub error_code: Option<Option<String>>,
    #[serde(default)]
    pub error_message: Option<Option<String>>,
    #[serde(default)]
    pub result_material_id: Option<Option<Uuid>>,
    #[serde(default)]
    pub preview: Option<Value>,
    #[serde(default)]
    pub metadata: Option<Value>,
}
#[derive(Debug, Default, Deserialize)]
pub struct ListJobs {
    pub status: Option<String>,
    pub limit: Option<String>,
    pub offset: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DuplicateRequest {
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub content_sha256: Option<String>,
    #[serde(default)]
    pub file_sha256: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct DuplicateResponse {
    pub duplicate: bool,
    pub normalized_source_url: Option<String>,
    pub content_sha256: Option<String>,
    pub file_sha256: Option<String>,
    pub matches: Vec<DuplicateMatch>,
}
#[derive(Debug, Serialize)]
pub struct DuplicateMatch {
    pub material_id: String,
    pub title: String,
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub matched_by: Vec<String>,
}
#[derive(Debug, sqlx::FromRow)]
struct DuplicateRow {
    id: Uuid,
    title: String,
    source_type: Option<String>,
    source_url: Option<String>,
    normalized_source_url: Option<String>,
    content: String,
    content_sha256: Option<String>,
    file_sha256: Option<String>,
    metadata_file_sha256: Option<String>,
}

pub async fn list_material_tags(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
) -> Result<Json<Vec<MaterialTagDto>>, AppError> {
    let rows = sqlx::query_as::<_, TagRow>("SELECT id,name,color,created_at,updated_at FROM material_tags WHERE user_id=$1 ORDER BY lower(name),id").bind(user.id).fetch_all(&s.pool).await?;
    Ok(Json(rows.into_iter().map(tag_dto).collect()))
}
pub async fn create_material_tag(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(p): ApiJson<CreateTag>,
) -> Result<Json<MaterialTagDto>, AppError> {
    let name = required(&p.name, "tag name is required")?;
    let color = p.color.and_then(opt);
    let row = sqlx::query_as::<_, TagRow>("INSERT INTO material_tags(id,user_id,name,color) VALUES($1,$2,$3,$4) RETURNING id,name,color,created_at,updated_at")
        .bind(Uuid::new_v4()).bind(user.id).bind(name).bind(color).fetch_one(&s.pool).await.map_err(tag_write_error)?;
    Ok(Json(tag_dto(row)))
}
pub async fn get_material_tag(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<MaterialTagDto>, AppError> {
    Ok(Json(tag_dto(tag_row(&s.pool, user.id, id).await?)))
}
pub async fn patch_material_tag(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(p): ApiJson<PatchTag>,
) -> Result<Json<MaterialTagDto>, AppError> {
    if p.name.is_none() && p.color.is_none() {
        return Err(AppError::bad_request(
            "invalid_material_tag",
            "at least one tag field is required",
        ));
    }
    let old = tag_row(&s.pool, user.id, id).await?;
    let name = match p.name {
        Some(v) => required(&v, "tag name is required")?,
        None => old.name,
    };
    let color = match p.color {
        Some(v) => v.and_then(opt),
        None => old.color,
    };
    let row=sqlx::query_as::<_,TagRow>("UPDATE material_tags SET name=$3,color=$4,updated_at=NOW() WHERE user_id=$1 AND id=$2 RETURNING id,name,color,created_at,updated_at")
        .bind(user.id).bind(id).bind(name).bind(color).fetch_one(&s.pool).await.map_err(tag_write_error)?;
    Ok(Json(tag_dto(row)))
}
pub async fn delete_material_tag(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let id = sqlx::query_scalar::<_, Uuid>(
        "DELETE FROM material_tags WHERE user_id=$1 AND id=$2 RETURNING id",
    )
    .bind(user.id)
    .bind(id)
    .fetch_optional(&s.pool)
    .await?;
    if id.is_none() {
        return Err(tag_missing());
    }
    Ok(Json(serde_json::json!({"deleted":true})))
}
pub async fn merge_material_tag(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(source): Path<Uuid>,
    ApiJson(p): ApiJson<MergeTag>,
) -> Result<Json<MaterialTagDto>, AppError> {
    if source == p.target_tag_id {
        return Err(AppError::bad_request(
            "invalid_material_tag_merge",
            "source and target tags must differ",
        ));
    }
    tag_row(&s.pool, user.id, source).await?;
    let target = tag_row(&s.pool, user.id, p.target_tag_id).await?;
    let mut tx = s.pool.begin().await?;
    sqlx::query("INSERT INTO material_tag_links(user_id,material_id,tag_id) SELECT user_id,material_id,$3 FROM material_tag_links WHERE user_id=$1 AND tag_id=$2 ON CONFLICT DO NOTHING").bind(user.id).bind(source).bind(p.target_tag_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM material_tags WHERE user_id=$1 AND id=$2")
        .bind(user.id)
        .bind(source)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(tag_dto(target)))
}
pub async fn get_material_tags(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(mid): Path<Uuid>,
) -> Result<Json<Vec<MaterialTagDto>>, AppError> {
    ensure_material(&s.pool, user.id, mid).await?;
    Ok(Json(fetch_material_tags(&s.pool, user.id, mid).await?))
}
pub async fn set_material_tags(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(mid): Path<Uuid>,
    ApiJson(p): ApiJson<SetTags>,
) -> Result<Json<Vec<MaterialTagDto>>, AppError> {
    let ids = ids(p.tag_ids, "tag_ids", true)?;
    ensure_material(&s.pool, user.id, mid).await?;
    ensure_tags(&s.pool, user.id, &ids).await?;
    let mut tx = s.pool.begin().await?;
    sqlx::query("DELETE FROM material_tag_links WHERE user_id=$1 AND material_id=$2")
        .bind(user.id)
        .bind(mid)
        .execute(&mut *tx)
        .await?;
    if !ids.is_empty() {
        sqlx::query(
            "INSERT INTO material_tag_links(user_id,material_id,tag_id) SELECT $1,$2,tag_id FROM unnest($3::uuid[]) AS tag_id",
        )
        .bind(user.id)
        .bind(mid)
        .bind(&ids)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Json(fetch_material_tags(&s.pool, user.id, mid).await?))
}
pub async fn bulk_material_tags(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(p): ApiJson<BulkTags>,
) -> Result<Json<Value>, AppError> {
    if !matches!(p.mode.as_str(), "add" | "remove" | "replace") {
        return Err(AppError::bad_request(
            "invalid_bulk_tags",
            "mode must be add, remove, or replace",
        ));
    }
    let mids = ids(p.ids, "ids", false)?;
    let tids = ids(p.tag_ids, "tag_ids", p.mode == "replace")?;
    ensure_materials(&s.pool, user.id, &mids).await?;
    ensure_tags(&s.pool, user.id, &tids).await?;
    let mut tx = s.pool.begin().await?;
    if p.mode == "replace" {
        sqlx::query("DELETE FROM material_tag_links WHERE user_id=$1 AND material_id=ANY($2)")
            .bind(user.id)
            .bind(&mids)
            .execute(&mut *tx)
            .await?;
    }
    if p.mode == "remove" {
        sqlx::query("DELETE FROM material_tag_links WHERE user_id=$1 AND material_id=ANY($2) AND tag_id=ANY($3)").bind(user.id).bind(&mids).bind(&tids).execute(&mut *tx).await?;
    } else if !tids.is_empty() {
        sqlx::query(
            "INSERT INTO material_tag_links(user_id,material_id,tag_id) SELECT $1,material_id,tag_id FROM unnest($2::uuid[]) AS material_id CROSS JOIN unnest($3::uuid[]) AS tag_id ON CONFLICT DO NOTHING",
        )
        .bind(user.id)
        .bind(&mids)
        .bind(&tids)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Json(serde_json::json!({"affected":mids.len()})))
}

pub async fn get_reading_progress(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(mid): Path<Uuid>,
) -> Result<Json<Option<ReadingProgressDto>>, AppError> {
    ensure_material(&s.pool, user.id, mid).await?;
    Ok(Json(fetch_reading_progress(&s.pool, user.id, mid).await?))
}
pub async fn upsert_reading_progress(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(mid): Path<Uuid>,
    ApiJson(p): ApiJson<PutProgress>,
) -> Result<Json<ReadingProgressDto>, AppError> {
    ensure_material(&s.pool, user.id, mid).await?;
    reader_kind(&p.reader_kind)?;
    ratio(p.progress_ratio, "progress_ratio")?;
    object(&p.locator, "locator")?;
    let status = p.status.unwrap_or_else(|| {
        if p.progress_ratio >= 1.0 {
            "completed".into()
        } else if p.progress_ratio > 0.0 {
            "reading".into()
        } else {
            "unread".into()
        }
    });
    reading_status(&status)?;
    let row=sqlx::query_as::<_,ProgressRow>(r#"INSERT INTO reading_progress(user_id,material_id,reader_kind,locator,progress_ratio,status,last_opened_at,completed_at,updated_at)
        VALUES($1,$2,$3,$4,$5,$6,NOW(),CASE WHEN $6='completed' THEN NOW() ELSE NULL END,NOW()) ON CONFLICT(user_id,material_id) DO UPDATE SET
        reader_kind=EXCLUDED.reader_kind,locator=EXCLUDED.locator,progress_ratio=EXCLUDED.progress_ratio,status=EXCLUDED.status,last_opened_at=NOW(),
        completed_at=CASE WHEN EXCLUDED.status='completed' THEN COALESCE(reading_progress.completed_at,NOW()) ELSE NULL END,updated_at=NOW()
        RETURNING material_id,reader_kind,locator,progress_ratio,status,last_opened_at,completed_at,updated_at"#)
        .bind(user.id).bind(mid).bind(p.reader_kind).bind(p.locator).bind(p.progress_ratio).bind(status).fetch_one(&s.pool).await?;
    Ok(Json(progress_dto(row)))
}

pub async fn list_material_import_jobs(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Query(q): Query<ListJobs>,
) -> Result<Json<Vec<MaterialImportJobDto>>, AppError> {
    if let Some(v) = q.status.as_deref() {
        import_status(v)?
    }
    let limit = parse_int(q.limit.as_deref(), "limit", 1, 200)?;
    let offset = parse_int(q.offset.as_deref(), "offset", 0, i64::MAX)?.unwrap_or(0);
    let mut b = QueryBuilder::<Postgres>::new(format!("{} WHERE user_id=", job_select()));
    b.push_bind(user.id);
    if let Some(v) = q.status.as_deref() {
        b.push(" AND status=").push_bind(v);
    }
    b.push(" ORDER BY created_at DESC,id DESC");
    if let Some(v) = limit {
        b.push(" LIMIT ").push_bind(v);
    }
    if offset > 0 {
        b.push(" OFFSET ").push_bind(offset);
    }
    let rows = b.build_query_as::<JobRow>().fetch_all(&s.pool).await?;
    Ok(Json(rows.into_iter().map(job_dto).collect()))
}
pub async fn create_material_import_job(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(p): ApiJson<CreateJob>,
) -> Result<Json<MaterialImportJobDto>, AppError> {
    source_kind(&p.source_kind)?;
    object(&p.preview, "preview")?;
    object(&p.metadata, "metadata")?;
    let stored_file_hash = match p.file_id {
        Some(fid) => Some(fetch_file_sha256(&s.pool, user.id, fid).await?),
        None => None,
    };
    let source_uri = p.source_uri.and_then(opt);
    let normalized = if matches!(p.source_kind.as_str(), "url" | "youtube") {
        normalize_source_url(source_uri.as_deref())?
    } else {
        None
    };
    if matches!(p.source_kind.as_str(), "url" | "youtube") && normalized.is_none() {
        return Err(AppError::bad_request(
            "invalid_source_url",
            "URL imports require an http or https source without credentials",
        ));
    }
    let requested_content_hash = validate_sha256(p.content_sha256.as_deref(), "content_sha256")?;
    let computed_content_hash = p.content.as_deref().and_then(content_sha256_hex);
    if requested_content_hash.is_some()
        && computed_content_hash.is_some()
        && requested_content_hash != computed_content_hash
    {
        return Err(AppError::bad_request(
            "content_hash_mismatch",
            "content_sha256 does not match the supplied content",
        ));
    }
    let content_hash = computed_content_hash.or(requested_content_hash);
    let requested_file_hash = validate_sha256(p.file_sha256.as_deref(), "file_sha256")?;
    if requested_file_hash.is_some()
        && stored_file_hash.is_some()
        && requested_file_hash != stored_file_hash
    {
        return Err(AppError::bad_request(
            "file_hash_mismatch",
            "file_sha256 does not match the stored file",
        ));
    }
    let file_hash = requested_file_hash.or(stored_file_hash);
    let requested_input_hash = validate_sha256(p.input_hash.as_deref(), "input_hash")?;
    let computed_input_hash = content_hash
        .clone()
        .or_else(|| file_hash.clone())
        .or_else(|| normalized.as_deref().map(sha256_hex));
    if requested_input_hash.is_some()
        && computed_input_hash.is_some()
        && requested_input_hash != computed_input_hash
    {
        return Err(AppError::bad_request(
            "input_hash_mismatch",
            "input_hash does not match the supplied import source",
        ));
    }
    let input_hash = computed_input_hash
        .or(requested_input_hash)
        .ok_or_else(|| {
            AppError::bad_request("invalid_import_job", "an import source is required")
        })?;
    if p.metadata.get(JOB_CREATE_FINGERPRINT_KEY).is_some() {
        return Err(AppError::bad_request(
            "reserved_import_metadata_key",
            format!("{JOB_CREATE_FINGERPRINT_KEY} is reserved for internal use"),
        ));
    }
    let create_fingerprint = job_create_fingerprint(
        &p.source_kind,
        source_uri.as_deref(),
        normalized.as_deref(),
        p.file_id,
        &input_hash,
        file_hash.as_deref(),
        content_hash.as_deref(),
        &p.preview,
        &p.metadata,
    );
    let mut stored_metadata = p.metadata.clone();
    stored_metadata
        .as_object_mut()
        .expect("metadata was validated as an object")
        .insert(
            JOB_CREATE_FINGERPRINT_KEY.to_string(),
            Value::String(create_fingerprint.clone()),
        );
    let job_id = p.id.unwrap_or_else(Uuid::new_v4);
    let row=sqlx::query_as::<_,JobRow>(r#"INSERT INTO material_import_jobs(id,user_id,source_kind,source_uri,normalized_source_url,file_id,input_hash,file_sha256,content_sha256,status,progress,preview,metadata)
        VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'queued',0,$10,$11)
        ON CONFLICT DO NOTHING
        RETURNING id,source_kind,source_uri,normalized_source_url,file_id,input_hash,file_sha256,content_sha256,status,progress,error_code,error_message,result_material_id,preview,metadata,created_at,updated_at,started_at,finished_at"#)
        .bind(job_id).bind(user.id).bind(&p.source_kind).bind(&source_uri).bind(&normalized).bind(p.file_id).bind(&input_hash).bind(&file_hash).bind(&content_hash).bind(&p.preview).bind(&stored_metadata).fetch_optional(&s.pool).await?;
    if let Some(row) = row {
        return Ok(Json(job_dto(row)));
    }

    let owner_id =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM material_import_jobs WHERE id=$1")
            .bind(job_id)
            .fetch_one(&s.pool)
            .await?;
    if owner_id != user.id {
        return Err(import_job_id_conflict());
    }
    let existing = job_row(&s.pool, user.id, job_id).await?;
    let same_request = job_create_fingerprint_from_metadata(&existing.metadata).map_or_else(
        || {
            existing.source_kind == p.source_kind
                && existing.source_uri == source_uri
                && existing.normalized_source_url == normalized
                && existing.file_id == p.file_id
                && existing.input_hash == input_hash
                && existing.file_sha256 == file_hash
                && existing.content_sha256 == content_hash
                && existing.preview == p.preview
                && existing.metadata == p.metadata
        },
        |fingerprint| fingerprint == create_fingerprint,
    );
    if !same_request {
        return Err(import_job_id_conflict());
    }
    Ok(Json(job_dto(existing)))
}
pub async fn get_material_import_job(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<MaterialImportJobDto>, AppError> {
    Ok(Json(job_dto(job_row(&s.pool, user.id, id).await?)))
}
pub async fn patch_material_import_job(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(p): ApiJson<PatchJob>,
) -> Result<Json<MaterialImportJobDto>, AppError> {
    let mut tx = s.pool.begin().await?;
    let old = sqlx::query_as::<_, JobRow>(&format!(
        "{} WHERE user_id=$1 AND id=$2 FOR UPDATE",
        job_select()
    ))
    .bind(user.id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(job_missing)?;
    let old_status = old.status.clone();
    let old_updated_at = old.updated_at;
    let status = p.status.clone().unwrap_or_else(|| old.status.clone());
    import_status(&status)?;
    if p.status.is_some() {
        transition(&old.status, &status)?;
    } else if finished_status(&old.status) {
        return Err(AppError::conflict(
            "terminal_import_job",
            "finished material import jobs are immutable",
        ));
    }
    let retrying = old_status == "failed_retryable" && status == "queued";
    let progress = next_import_progress(&old_status, &status, old.progress, p.progress)?;
    let result_mid = p.result_material_id.unwrap_or(old.result_material_id);
    let result_snapshot = if let Some(mid) = result_mid {
        Some(
            sqlx::query_scalar::<_, Value>(
                r#"SELECT jsonb_build_object(
                    'id',id,'title',title,'source_type',source_type,'source_url',source_url,
                    'created_at',created_at,'updated_at',updated_at
                ) FROM materials WHERE user_id=$1 AND id=$2"#,
            )
            .bind(user.id)
            .bind(mid)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| {
                AppError::not_found("material_not_found", "result material not found")
            })?,
        )
    } else {
        None
    };
    let preview = p.preview.unwrap_or(old.preview);
    let create_fingerprint = job_create_fingerprint_from_metadata(&old.metadata).map(str::to_owned);
    let mut metadata = p.metadata.unwrap_or(old.metadata);
    if let (Some(object), Some(fingerprint)) = (metadata.as_object_mut(), create_fingerprint) {
        object.insert(
            JOB_CREATE_FINGERPRINT_KEY.to_string(),
            Value::String(fingerprint),
        );
    }
    if status == "succeeded" {
        if let (Some(object), Some(snapshot)) = (metadata.as_object_mut(), result_snapshot) {
            object.insert("result_material_snapshot".to_string(), snapshot);
        }
    }
    object(&preview, "preview")?;
    object(&metadata, "metadata")?;
    if status == "committing" {
        validate_import_commit_intent(&metadata)?;
    }
    let mut ec = p.error_code.unwrap_or(old.error_code);
    let mut em = p.error_message.unwrap_or(old.error_message);
    if retrying {
        ec = None;
        em = None;
    }
    if status == "succeeded" && result_mid.is_none() {
        return Err(AppError::bad_request(
            "invalid_import_result",
            "succeeded material imports require result_material_id",
        ));
    }
    if matches!(status.as_str(), "failed_retryable" | "failed_terminal")
        && ec.as_deref().is_none_or(str::is_empty)
        && em.as_deref().is_none_or(str::is_empty)
    {
        return Err(AppError::bad_request(
            "invalid_import_failure",
            "failed material imports require an error code or message",
        ));
    }
    let started = if retrying {
        None
    } else if status == "validating" && old.started_at.is_none() {
        Some(Utc::now())
    } else {
        old.started_at
    };
    let finished = if finished_status(&status) {
        old.finished_at.or_else(|| Some(Utc::now()))
    } else {
        None
    };
    let row=sqlx::query_as::<_,JobRow>(r#"UPDATE material_import_jobs SET status=$3,progress=$4,error_code=$5,error_message=$6,result_material_id=$7,preview=$8,metadata=$9,started_at=$10,finished_at=$11,updated_at=NOW() WHERE user_id=$1 AND id=$2 AND status=$12 AND updated_at=$13
        RETURNING id,source_kind,source_uri,normalized_source_url,file_id,input_hash,file_sha256,content_sha256,status,progress,error_code,error_message,result_material_id,preview,metadata,created_at,updated_at,started_at,finished_at"#)
        .bind(user.id).bind(id).bind(status).bind(progress).bind(ec).bind(em).bind(result_mid).bind(preview).bind(metadata).bind(started).bind(finished).bind(old_status).bind(old_updated_at).fetch_optional(&mut *tx).await?
        .ok_or_else(|| AppError::conflict("import_job_changed", "material import job changed while it was being updated"))?;
    tx.commit().await?;
    Ok(Json(job_dto(row)))
}
pub async fn delete_material_import_job(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    job_row(&s.pool, user.id, id).await?;
    Err(AppError::conflict(
        "import_job_audit_preserved",
        "material import jobs are audit records; cancel active jobs instead of deleting them",
    ))
}

pub async fn check_material_duplicates(
    State(s): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(p): ApiJson<DuplicateRequest>,
) -> Result<Json<DuplicateResponse>, AppError> {
    let url = normalize_source_url(p.source_url.as_deref())?;
    let ch = match p.content_sha256.as_deref() {
        Some(v) => validate_sha256(Some(v), "content_sha256")?,
        None => p.content.as_deref().and_then(content_sha256_hex),
    };
    let fh = validate_sha256(p.file_sha256.as_deref(), "file_sha256")?;
    if url.is_none() && ch.is_none() && fh.is_none() {
        return Err(AppError::bad_request(
            "invalid_duplicate_check",
            "source_url, content, content_sha256, or file_sha256 is required",
        ));
    }
    let rows = duplicate_rows_pool(
        &s.pool,
        user.id,
        None,
        url.as_deref(),
        ch.as_deref(),
        fh.as_deref(),
    )
    .await?;
    let matches = duplicate_matches(rows, url.as_deref(), ch.as_deref(), fh.as_deref());
    Ok(Json(DuplicateResponse {
        duplicate: !matches.is_empty(),
        normalized_source_url: url,
        content_sha256: ch,
        file_sha256: fh,
        matches,
    }))
}

pub async fn lock_material_fingerprints(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    url: Option<&str>,
    content_hash: Option<&str>,
    file_hash: Option<&str>,
) -> Result<(), AppError> {
    let mut keys = Vec::new();
    if let Some(value) = url {
        keys.push(format!("{user_id}:url:{value}"));
    }
    if let Some(value) = content_hash {
        keys.push(format!("{user_id}:content:{value}"));
    }
    if let Some(value) = file_hash {
        keys.push(format!("{user_id}:file:{value}"));
    }
    keys.sort();
    keys.dedup();
    for key in keys {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(key)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub async fn ensure_no_material_duplicates_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    exclude_material_id: Option<Uuid>,
    url: Option<&str>,
    content_hash: Option<&str>,
    file_hash: Option<&str>,
) -> Result<(), AppError> {
    if url.is_none() && content_hash.is_none() && file_hash.is_none() {
        return Ok(());
    }
    let rows = duplicate_rows_tx(
        tx,
        user_id,
        exclude_material_id,
        url,
        content_hash,
        file_hash,
    )
    .await?;
    if duplicate_matches(rows, url, content_hash, file_hash).is_empty() {
        Ok(())
    } else {
        Err(AppError::conflict(
            "duplicate_material",
            "material duplicates an existing URL, content, or file",
        ))
    }
}

const DUPLICATE_ROWS_SQL: &str = r#"
    SELECT id,title,source_type,source_url,normalized_source_url,content,content_sha256,file_sha256,
           metadata->>'file_sha256' AS metadata_file_sha256
    FROM materials
    WHERE user_id=$1
      AND ($2::uuid IS NULL OR id<>$2)
      AND (
        ($3::text IS NOT NULL AND (
            normalized_source_url=$3 OR (normalized_source_url IS NULL AND source_url IS NOT NULL)
        ))
        OR ($4::text IS NOT NULL AND (
            content_sha256=$4 OR content_sha256 IS NULL
        ))
        OR ($5::text IS NOT NULL AND (
            file_sha256=$5 OR (file_sha256 IS NULL AND metadata->>'file_sha256' IS NOT NULL)
        ))
      )
    ORDER BY id
"#;

async fn duplicate_rows_pool(
    pool: &PgPool,
    user_id: Uuid,
    exclude_material_id: Option<Uuid>,
    url: Option<&str>,
    content_hash: Option<&str>,
    file_hash: Option<&str>,
) -> Result<Vec<DuplicateRow>, AppError> {
    Ok(sqlx::query_as::<_, DuplicateRow>(DUPLICATE_ROWS_SQL)
        .bind(user_id)
        .bind(exclude_material_id)
        .bind(url)
        .bind(content_hash)
        .bind(file_hash)
        .fetch_all(pool)
        .await?)
}

async fn duplicate_rows_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    exclude_material_id: Option<Uuid>,
    url: Option<&str>,
    content_hash: Option<&str>,
    file_hash: Option<&str>,
) -> Result<Vec<DuplicateRow>, AppError> {
    Ok(sqlx::query_as::<_, DuplicateRow>(DUPLICATE_ROWS_SQL)
        .bind(user_id)
        .bind(exclude_material_id)
        .bind(url)
        .bind(content_hash)
        .bind(file_hash)
        .fetch_all(&mut **tx)
        .await?)
}

fn duplicate_matches(
    rows: Vec<DuplicateRow>,
    url: Option<&str>,
    content_hash: Option<&str>,
    file_hash: Option<&str>,
) -> Vec<DuplicateMatch> {
    rows.into_iter()
        .filter_map(|row| {
            let mut matched_by = Vec::new();
            let row_url = row.normalized_source_url.or_else(|| {
                normalize_source_url(row.source_url.as_deref())
                    .ok()
                    .flatten()
            });
            if url.is_some() && row_url.as_deref() == url {
                matched_by.push("url".to_string());
            }
            let row_content_hash = row
                .content_sha256
                .or_else(|| content_sha256_hex(&row.content));
            if content_hash.is_some() && row_content_hash.as_deref() == content_hash {
                matched_by.push("content_hash".to_string());
            }
            let row_file_hash = row
                .file_sha256
                .or(row.metadata_file_sha256)
                .map(|value| value.to_ascii_lowercase());
            if file_hash.is_some() && row_file_hash.as_deref() == file_hash {
                matched_by.push("file_hash".to_string());
            }
            (!matched_by.is_empty()).then(|| DuplicateMatch {
                material_id: row.id.to_string(),
                title: row.title,
                source_type: row.source_type,
                source_url: row.source_url,
                matched_by,
            })
        })
        .collect()
}

pub async fn fetch_material_tags(
    pool: &PgPool,
    user_id: Uuid,
    mid: Uuid,
) -> Result<Vec<MaterialTagDto>, AppError> {
    let rows=sqlx::query_as::<_,TagRow>("SELECT t.id,t.name,t.color,t.created_at,t.updated_at FROM material_tags t JOIN material_tag_links l ON l.user_id=t.user_id AND l.tag_id=t.id WHERE t.user_id=$1 AND l.material_id=$2 ORDER BY lower(t.name),t.id").bind(user_id).bind(mid).fetch_all(pool).await?;
    Ok(rows.into_iter().map(tag_dto).collect())
}
pub async fn fetch_material_tags_bulk(
    pool: &PgPool,
    user_id: Uuid,
    mids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<MaterialTagDto>>, AppError> {
    if mids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows=sqlx::query_as::<_,(Uuid,Uuid,String,Option<String>,DateTime<Utc>,DateTime<Utc>)>("SELECT l.material_id,t.id,t.name,t.color,t.created_at,t.updated_at FROM material_tag_links l JOIN material_tags t ON t.user_id=l.user_id AND t.id=l.tag_id WHERE l.user_id=$1 AND l.material_id=ANY($2) ORDER BY l.material_id,lower(t.name),t.id").bind(user_id).bind(mids).fetch_all(pool).await?;
    let mut out = HashMap::new();
    for (mid, id, name, color, created, updated) in rows {
        out.entry(mid)
            .or_insert_with(Vec::new)
            .push(MaterialTagDto {
                id: id.to_string(),
                name,
                color,
                created_at: created.to_rfc3339(),
                updated_at: updated.to_rfc3339(),
            });
    }
    Ok(out)
}
pub async fn fetch_reading_progress(
    pool: &PgPool,
    user_id: Uuid,
    mid: Uuid,
) -> Result<Option<ReadingProgressDto>, AppError> {
    let row=sqlx::query_as::<_,ProgressRow>("SELECT material_id,reader_kind,locator,progress_ratio,status,last_opened_at,completed_at,updated_at FROM reading_progress WHERE user_id=$1 AND material_id=$2").bind(user_id).bind(mid).fetch_optional(pool).await?;
    Ok(row.map(progress_dto))
}
pub async fn fetch_reading_progress_bulk(
    pool: &PgPool,
    user_id: Uuid,
    mids: &[Uuid],
) -> Result<HashMap<Uuid, ReadingProgressDto>, AppError> {
    if mids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows=sqlx::query_as::<_,ProgressRow>("SELECT material_id,reader_kind,locator,progress_ratio,status,last_opened_at,completed_at,updated_at FROM reading_progress WHERE user_id=$1 AND material_id=ANY($2)").bind(user_id).bind(mids).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.material_id, progress_dto(r)))
        .collect())
}

pub fn normalize_source_url(value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let Ok(mut u) = Url::parse(v) else {
        // Local paths and historical opaque source identifiers are valid material metadata,
        // but they are intentionally excluded from network URL deduplication.
        return Ok(None);
    };
    if !matches!(u.scheme(), "http" | "https") {
        return Ok(None);
    }
    if !u.username().is_empty() || u.password().is_some() || u.host_str().is_none() {
        return Err(AppError::bad_request(
            "invalid_source_url",
            "source URL must be http or https and must not contain credentials",
        ));
    }
    u.set_fragment(None);
    if (u.scheme() == "http" && u.port() == Some(80))
        || (u.scheme() == "https" && u.port() == Some(443))
    {
        let _ = u.set_port(None);
    }
    let mut pairs = u.query_pairs().into_owned().collect::<Vec<_>>();
    pairs.sort();
    u.set_query(None);
    if !pairs.is_empty() {
        u.query_pairs_mut().extend_pairs(pairs);
    }
    Ok(Some(u.to_string()))
}
pub fn content_sha256_hex(v: &str) -> Option<String> {
    let n = v
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .nfc()
        .collect::<String>();
    if n.is_empty() {
        None
    } else {
        Some(hex::encode(Sha256::digest(n.as_bytes())))
    }
}
pub fn validate_sha256(v: Option<&str>, field: &'static str) -> Result<Option<String>, AppError> {
    let Some(v) = v.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let n = v.to_ascii_lowercase();
    if n.len() != 64 || !n.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(AppError::bad_request(
            "invalid_hash",
            format!("{field} must be a 64-character hexadecimal SHA-256"),
        ));
    }
    Ok(Some(n))
}

async fn tag_row(pool: &PgPool, uid: Uuid, id: Uuid) -> Result<TagRow, AppError> {
    sqlx::query_as::<_, TagRow>(
        "SELECT id,name,color,created_at,updated_at FROM material_tags WHERE user_id=$1 AND id=$2",
    )
    .bind(uid)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(tag_missing)
}
async fn job_row(pool: &PgPool, uid: Uuid, id: Uuid) -> Result<JobRow, AppError> {
    sqlx::query_as::<_, JobRow>(&format!("{} WHERE user_id=$1 AND id=$2", job_select()))
        .bind(uid)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(job_missing)
}
fn job_select() -> &'static str {
    "SELECT id,source_kind,source_uri,normalized_source_url,file_id,input_hash,file_sha256,content_sha256,status,progress,error_code,error_message,result_material_id,preview,metadata,created_at,updated_at,started_at,finished_at FROM material_import_jobs"
}
async fn ensure_material(pool: &PgPool, uid: Uuid, id: Uuid) -> Result<(), AppError> {
    if sqlx::query_scalar::<_, i64>("SELECT 1::BIGINT FROM materials WHERE user_id=$1 AND id=$2")
        .bind(uid)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .is_none()
    {
        return Err(AppError::not_found(
            "material_not_found",
            "material not found",
        ));
    }
    Ok(())
}
async fn ensure_materials(pool: &PgPool, uid: Uuid, values: &[Uuid]) -> Result<(), AppError> {
    let n = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM materials WHERE user_id=$1 AND id=ANY($2)",
    )
    .bind(uid)
    .bind(values)
    .fetch_one(pool)
    .await?;
    if n != values.len() as i64 {
        return Err(AppError::not_found(
            "material_not_found",
            "one or more materials were not found",
        ));
    }
    Ok(())
}
async fn ensure_tags(pool: &PgPool, uid: Uuid, values: &[Uuid]) -> Result<(), AppError> {
    if values.is_empty() {
        return Ok(());
    }
    let n = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM material_tags WHERE user_id=$1 AND id=ANY($2)",
    )
    .bind(uid)
    .bind(values)
    .fetch_one(pool)
    .await?;
    if n != values.len() as i64 {
        return Err(tag_missing());
    }
    Ok(())
}
async fn fetch_file_sha256(pool: &PgPool, uid: Uuid, id: Uuid) -> Result<String, AppError> {
    sqlx::query_scalar::<_, String>("SELECT sha256 FROM files WHERE user_id=$1 AND id=$2")
        .bind(uid)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::not_found("file_not_found", "file not found"))
}

fn sha256_hex(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn ids(values: Vec<Uuid>, field: &str, allow_empty: bool) -> Result<Vec<Uuid>, AppError> {
    if (!allow_empty && values.is_empty()) || values.len() > MAX_BULK_IDS {
        return Err(AppError::bad_request(
            "invalid_bulk_request",
            format!("{field} must contain between 1 and {MAX_BULK_IDS} values"),
        ));
    }
    let mut seen = HashSet::new();
    Ok(values.into_iter().filter(|v| seen.insert(*v)).collect())
}
fn required(v: &str, msg: &'static str) -> Result<String, AppError> {
    let v = v.trim();
    if v.is_empty() {
        Err(AppError::bad_request("invalid_material_tag", msg))
    } else {
        Ok(v.into())
    }
}
fn opt(v: String) -> Option<String> {
    let v = v.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.into())
    }
}
fn object(v: &Value, field: &str) -> Result<(), AppError> {
    if !v.is_object() {
        return Err(AppError::bad_request(
            "invalid_json_field",
            format!("{field} must be a JSON object"),
        ));
    }
    Ok(())
}
fn ratio(v: f64, field: &str) -> Result<(), AppError> {
    if !v.is_finite() || !(0.0..=1.0).contains(&v) {
        return Err(AppError::bad_request(
            "invalid_progress",
            format!("{field} must be between 0 and 1"),
        ));
    }
    Ok(())
}
fn reader_kind(v: &str) -> Result<(), AppError> {
    if !matches!(v, "article" | "pdf" | "epub" | "txt" | "media") {
        return Err(AppError::bad_request(
            "invalid_reader_kind",
            "reader_kind must be article, pdf, epub, txt, or media",
        ));
    }
    Ok(())
}
fn reading_status(v: &str) -> Result<(), AppError> {
    if !matches!(v, "unread" | "reading" | "completed") {
        return Err(AppError::bad_request(
            "invalid_reading_status",
            "status must be unread, reading, or completed",
        ));
    }
    Ok(())
}
fn source_kind(v: &str) -> Result<(), AppError> {
    if !matches!(
        v,
        "article" | "url" | "text_file" | "book" | "audio" | "video" | "subtitle" | "youtube"
    ) {
        return Err(AppError::bad_request(
            "invalid_source_kind",
            "unsupported material import source_kind",
        ));
    }
    Ok(())
}
fn import_status(v: &str) -> Result<(), AppError> {
    if !matches!(
        v,
        "queued"
            | "validating"
            | "parsing"
            | "preview_ready"
            | "committing"
            | "succeeded"
            | "failed_retryable"
            | "failed_terminal"
            | "cancelled"
    ) {
        return Err(AppError::bad_request(
            "invalid_import_status",
            "unsupported material import status",
        ));
    }
    Ok(())
}
fn transition(from: &str, to: &str) -> Result<(), AppError> {
    if from == to {
        return if finished_status(from) || from == "committing" {
            Err(AppError::conflict(
                if finished_status(from) {
                    "terminal_import_job"
                } else {
                    "import_job_already_committing"
                },
                if finished_status(from) {
                    "finished material import jobs are immutable"
                } else {
                    "material import job is already being committed"
                },
            ))
        } else {
            Ok(())
        };
    }
    let failure = matches!(to, "failed_retryable" | "failed_terminal" | "cancelled");
    let ok = match from {
        "queued" => to == "validating" || failure,
        "validating" => to == "parsing" || failure,
        "parsing" => to == "preview_ready" || failure,
        "preview_ready" => to == "committing" || failure,
        "committing" => matches!(to, "succeeded" | "failed_retryable" | "failed_terminal"),
        "failed_retryable" => matches!(to, "queued" | "cancelled"),
        _ => false,
    };
    if !ok {
        return Err(AppError::conflict(
            "invalid_import_status_transition",
            format!("cannot transition material import job from {from} to {to}"),
        ));
    }
    Ok(())
}
fn finished_status(v: &str) -> bool {
    matches!(
        v,
        "succeeded" | "failed_retryable" | "failed_terminal" | "cancelled"
    )
}
fn validate_import_commit_intent(metadata: &Value) -> Result<(), AppError> {
    let commit_kind = metadata
        .pointer("/import_commit/commit_kind")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let target_material_id = metadata
        .pointer("/import_commit/target_material_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    if commit_kind.is_none() || target_material_id.is_none() {
        return Err(AppError::bad_request(
            "invalid_import_commit_intent",
            "committing material imports require commit_kind and target_material_id",
        ));
    }
    Ok(())
}
fn next_import_progress(
    from: &str,
    to: &str,
    current: f64,
    requested: Option<f64>,
) -> Result<f64, AppError> {
    let retrying = from == "failed_retryable" && to == "queued";
    let progress = requested.unwrap_or(if retrying {
        0.0
    } else if to == "succeeded" {
        1.0
    } else {
        current
    });
    ratio(progress, "progress")?;
    if !retrying && progress < current {
        return Err(AppError::conflict(
            "import_progress_regression",
            "material import progress cannot move backwards",
        ));
    }
    Ok(progress)
}
fn parse_int(v: Option<&str>, field: &str, min: i64, max: i64) -> Result<Option<i64>, AppError> {
    let parsed = v
        .map(|v| {
            v.parse::<i64>().map_err(|_| {
                AppError::bad_request(
                    "invalid_import_query",
                    format!("{field} must be an integer"),
                )
            })
        })
        .transpose()?;
    if parsed.is_some_and(|v| !(min..=max).contains(&v)) {
        return Err(AppError::bad_request(
            "invalid_import_query",
            format!("{field} must be between {min} and {max}"),
        ));
    }
    Ok(parsed)
}
fn tag_dto(r: TagRow) -> MaterialTagDto {
    MaterialTagDto {
        id: r.id.to_string(),
        name: r.name,
        color: r.color,
        created_at: r.created_at.to_rfc3339(),
        updated_at: r.updated_at.to_rfc3339(),
    }
}
fn progress_dto(r: ProgressRow) -> ReadingProgressDto {
    ReadingProgressDto {
        material_id: r.material_id.to_string(),
        reader_kind: r.reader_kind,
        locator: r.locator,
        progress_ratio: r.progress_ratio,
        status: r.status,
        last_opened_at: r.last_opened_at.to_rfc3339(),
        completed_at: r.completed_at.map(|v| v.to_rfc3339()),
        updated_at: r.updated_at.to_rfc3339(),
    }
}
fn job_dto(r: JobRow) -> MaterialImportJobDto {
    let mut metadata = r.metadata;
    if let Some(object) = metadata.as_object_mut() {
        object.remove(JOB_CREATE_FINGERPRINT_KEY);
    }
    MaterialImportJobDto {
        id: r.id.to_string(),
        source_kind: r.source_kind,
        source_uri: r.source_uri,
        normalized_source_url: r.normalized_source_url,
        file_id: r.file_id.map(|v| v.to_string()),
        input_hash: r.input_hash,
        file_sha256: r.file_sha256,
        content_sha256: r.content_sha256,
        status: r.status,
        progress: r.progress,
        error_code: r.error_code,
        error_message: r.error_message,
        result_material_id: r.result_material_id.map(|v| v.to_string()),
        preview: r.preview,
        metadata,
        created_at: r.created_at.to_rfc3339(),
        updated_at: r.updated_at.to_rfc3339(),
        started_at: r.started_at.map(|v| v.to_rfc3339()),
        finished_at: r.finished_at.map(|v| v.to_rfc3339()),
    }
}
fn tag_write_error(e: sqlx::Error) -> AppError {
    if e.as_database_error()
        .and_then(|v| v.code())
        .is_some_and(|v| v == "23505")
    {
        AppError::conflict("material_tag_exists", "a tag with this name already exists")
    } else {
        AppError::from(e)
    }
}
fn tag_missing() -> AppError {
    AppError::not_found("material_tag_not_found", "material tag not found")
}
fn job_missing() -> AppError {
    AppError::not_found(
        "material_import_job_not_found",
        "material import job not found",
    )
}
fn import_job_id_conflict() -> AppError {
    AppError::conflict(
        "material_import_job_id_conflict",
        "material import job id is already bound to a different request",
    )
}
#[allow(clippy::too_many_arguments)]
fn job_create_fingerprint(
    source_kind: &str,
    source_uri: Option<&str>,
    normalized_source_url: Option<&str>,
    file_id: Option<Uuid>,
    input_hash: &str,
    file_sha256: Option<&str>,
    content_sha256: Option<&str>,
    preview: &Value,
    metadata: &Value,
) -> String {
    let request = serde_json::json!({
        "source_kind": source_kind,
        "source_uri": source_uri,
        "normalized_source_url": normalized_source_url,
        "file_id": file_id,
        "input_hash": input_hash,
        "file_sha256": file_sha256,
        "content_sha256": content_sha256,
        "preview": preview,
        "metadata": metadata,
    });
    sha256_hex(&request.to_string())
}
fn job_create_fingerprint_from_metadata(metadata: &Value) -> Option<&str> {
    metadata
        .get(JOB_CREATE_FINGERPRINT_KEY)
        .and_then(Value::as_str)
}
fn empty_object() -> Value {
    Value::Object(Default::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn url_and_hash_normalization() {
        assert_eq!(
            normalize_source_url(Some("HTTPS://Example.COM:443/a?b=2&a=1#x"))
                .unwrap()
                .unwrap(),
            "https://example.com/a?a=1&b=2"
        );
        assert_eq!(content_sha256_hex("a\r\nb"), content_sha256_hex("a\nb"));
        assert_eq!(content_sha256_hex("a   b\t c"), content_sha256_hex("a b c"));
        assert_eq!(
            content_sha256_hex("cafe\u{301}"),
            content_sha256_hex("caf\u{e9}")
        );
        assert_eq!(
            normalize_source_url(Some("file:///tmp/a.txt")).unwrap(),
            None
        );
        assert_eq!(
            normalize_source_url(Some("relative/source.txt")).unwrap(),
            None
        );
        assert!(normalize_source_url(Some("https://user@example.com/private")).is_err());
    }
    #[test]
    fn import_transitions() {
        for status in [
            "queued",
            "validating",
            "parsing",
            "preview_ready",
            "committing",
            "succeeded",
            "failed_retryable",
            "failed_terminal",
            "cancelled",
        ] {
            assert!(import_status(status).is_ok());
        }
        assert!(transition("queued", "validating").is_ok());
        assert!(transition("queued", "preview_ready").is_err());
        assert!(transition("failed_retryable", "queued").is_ok());
        assert!(transition("parsing", "failed_retryable").is_ok());
        assert!(transition("committing", "failed_terminal").is_ok());
        assert!(transition("committing", "cancelled").is_err());
        assert!(transition("preview_ready", "cancelled").is_ok());
        assert!(transition("succeeded", "queued").is_err());
        assert!(transition("succeeded", "succeeded").is_err());
    }

    #[test]
    fn import_retry_progress_defaults_to_zero() {
        assert_eq!(
            next_import_progress("failed_retryable", "queued", 0.8, None).unwrap(),
            0.0
        );
        assert_eq!(
            next_import_progress("failed_retryable", "queued", 0.8, Some(0.2)).unwrap(),
            0.2
        );
        assert!(next_import_progress("parsing", "preview_ready", 0.8, Some(0.2)).is_err());
    }
}
