use std::collections::HashSet;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::{
    auth::{ApiJson, AuthenticatedUser},
    error::AppError,
    learning_activity,
    routes::AppState,
};

const LOCATOR_VERSION: i64 = 1;
const DEFAULT_LIST_LIMIT: i64 = 100;
const MAX_LIST_LIMIT: i64 = 500;

#[derive(Debug, Clone, Serialize)]
pub struct AnnotationDto {
    pub id: String,
    pub material_id: String,
    pub segment_id: Option<String>,
    pub kind: String,
    pub locator: Value,
    pub source_text: String,
    pub material_revision: Option<String>,
    pub content_sha256: Option<String>,
    pub color: Option<String>,
    pub note: Option<String>,
    pub tags: Vec<String>,
    pub learning_item_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListAnnotationsQuery {
    pub material_id: Option<Uuid>,
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub q: Option<String>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateAnnotationRequest {
    #[serde(default)]
    pub id: Option<Uuid>,
    pub material_id: Uuid,
    #[serde(default)]
    pub segment_id: Option<Uuid>,
    pub kind: String,
    pub locator: Value,
    pub source_text: String,
    #[serde(default)]
    pub material_revision: Option<String>,
    #[serde(default)]
    pub content_sha256: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub learning_item_id: Option<Uuid>,
    pub client_request_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchAnnotationRequest {
    #[serde(default)]
    pub material_id: Option<Uuid>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub segment_id: Option<Option<Uuid>>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub locator: Option<Value>,
    #[serde(default)]
    pub source_text: Option<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub material_revision: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub content_sha256: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub color: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub note: Option<Option<String>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub learning_item_id: Option<Option<Uuid>>,
}

fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Serialize)]
pub struct DeleteAnnotationResponse {
    pub deleted: bool,
}

#[derive(Debug, Serialize)]
pub struct ConvertAnnotationResponse {
    pub annotation: AnnotationDto,
    pub learning_item: ConvertedLearningItemDto,
}

#[derive(Debug, Serialize)]
pub struct ConvertedLearningItemDto {
    pub id: String,
    pub material_id: Option<String>,
    pub segment_id: Option<String>,
    pub item_type: String,
    pub text: String,
    pub source_sentence: String,
    pub tags: Vec<String>,
    pub status: String,
    pub review_state: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct AnnotationRecord {
    id: Uuid,
    material_id: Uuid,
    segment_id: Option<Uuid>,
    kind: String,
    locator: Value,
    source_text: String,
    material_revision: Option<String>,
    content_sha256: Option<String>,
    color: Option<String>,
    note: Option<String>,
    tags: Value,
    learning_item_id: Option<Uuid>,
    idempotency_payload_sha256: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct MaterialSnapshot {
    content_sha256: Option<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct ConvertedLearningItemRecord {
    id: Uuid,
    material_id: Option<Uuid>,
    segment_id: Option<Uuid>,
    item_type: String,
    text: String,
    source_sentence: String,
    tags: Value,
    status: String,
    review_state: Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

pub async fn list_annotations(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Query(query): Query<ListAnnotationsQuery>,
) -> Result<Json<Vec<AnnotationDto>>, AppError> {
    if let Some(kind) = query.kind.as_deref() {
        validate_kind(kind)?;
    }
    let tag = normalize_query_value(query.tag, "tag")?;
    let search = normalize_query_value(query.q, "q")?;
    let limit = query
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);

    let mut builder = QueryBuilder::<Postgres>::new(annotation_select());
    builder.push(" WHERE a.user_id = ").push_bind(user.id);
    if let Some(material_id) = query.material_id {
        builder.push(" AND a.material_id = ").push_bind(material_id);
    }
    if let Some(kind) = query.kind {
        builder.push(" AND a.kind = ").push_bind(kind);
    }
    if let Some(tag) = tag {
        builder.push(" AND a.tags ? ").push_bind(tag);
    }
    if let Some(search) = search {
        let pattern = format!("%{search}%");
        builder
            .push(" AND (a.source_text ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR COALESCE(a.note, '') ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
    if let Some(created_after) = query.created_after {
        builder
            .push(" AND a.created_at >= ")
            .push_bind(created_after);
    }
    if let Some(created_before) = query.created_before {
        builder
            .push(" AND a.created_at <= ")
            .push_bind(created_before);
    }
    builder
        .push(" ORDER BY a.updated_at DESC, a.id DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let records = builder
        .build_query_as::<AnnotationRecord>()
        .fetch_all(&state.pool)
        .await?;
    Ok(Json(records.into_iter().map(annotation_to_dto).collect()))
}

pub async fn create_annotation(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(mut payload): ApiJson<CreateAnnotationRequest>,
) -> Result<Json<AnnotationDto>, AppError> {
    normalize_create_request(&mut payload)?;
    let payload_hash = sha256_json(&payload)?;

    if let Some(existing) =
        fetch_annotation_by_client_request_id(&state.pool, user.id, &payload.client_request_id)
            .await?
    {
        ensure_idempotent_payload(&existing, &payload_hash)?;
        return Ok(Json(annotation_to_dto(existing)));
    }

    let locator_segment_id = locator_segment_id(&payload.locator)?;
    let segment_id = reconcile_segment_id(payload.segment_id, locator_segment_id)?;
    let material = fetch_material_snapshot(&state.pool, user.id, payload.material_id).await?;
    validate_segment(&state.pool, user.id, payload.material_id, segment_id).await?;
    validate_learning_item(&state.pool, user.id, payload.learning_item_id).await?;
    let (material_revision, content_sha256) = resolve_material_identity(
        payload.material_revision,
        payload.content_sha256,
        &payload.locator,
        &material,
    )?;

    let inserted = sqlx::query_as::<_, AnnotationRecord>(&format!(
        r#"
        INSERT INTO annotations (
            id, user_id, material_id, segment_id, kind, locator, source_text,
            material_revision, content_sha256, color, note, tags, learning_item_id,
            client_request_id, idempotency_payload_sha256
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15
        )
        ON CONFLICT (user_id, client_request_id) WHERE client_request_id IS NOT NULL
        DO NOTHING
        RETURNING {}
        "#,
        annotation_columns()
    ))
    .bind(payload.id.unwrap_or_else(Uuid::new_v4))
    .bind(user.id)
    .bind(payload.material_id)
    .bind(segment_id)
    .bind(payload.kind)
    .bind(payload.locator)
    .bind(payload.source_text)
    .bind(material_revision)
    .bind(content_sha256)
    .bind(payload.color)
    .bind(payload.note)
    .bind(strings_to_json(&payload.tags))
    .bind(payload.learning_item_id)
    .bind(&payload.client_request_id)
    .bind(&payload_hash)
    .fetch_optional(&state.pool)
    .await;

    let record = match inserted {
        Ok(Some(record)) => record,
        Ok(None) => {
            let existing = fetch_annotation_by_client_request_id(
                &state.pool,
                user.id,
                &payload.client_request_id,
            )
            .await?
            .ok_or_else(|| {
                AppError::conflict("annotation_conflict", "annotation could not be created")
            })?;
            ensure_idempotent_payload(&existing, &payload_hash)?;
            existing
        }
        Err(error) if is_unique_violation(&error) => {
            return Err(AppError::conflict(
                "annotation_id_conflict",
                "annotation id already exists",
            ));
        }
        Err(error) => return Err(error.into()),
    };

    Ok(Json(annotation_to_dto(record)))
}

pub async fn get_annotation(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<AnnotationDto>, AppError> {
    let record = fetch_annotation(&state.pool, user.id, id).await?;
    Ok(Json(annotation_to_dto(record)))
}

pub async fn patch_annotation(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(payload): ApiJson<PatchAnnotationRequest>,
) -> Result<Json<AnnotationDto>, AppError> {
    let existing = fetch_annotation(&state.pool, user.id, id).await?;
    let material_id = payload.material_id.unwrap_or(existing.material_id);
    let kind = payload.kind.unwrap_or(existing.kind);
    validate_kind(&kind)?;
    let mut locator = payload.locator.unwrap_or(existing.locator);
    validate_locator(&mut locator)?;
    let requested_segment_id = payload.segment_id.unwrap_or(existing.segment_id);
    let segment_id = reconcile_segment_id(requested_segment_id, locator_segment_id(&locator)?)?;
    let source_text = payload.source_text.unwrap_or(existing.source_text);
    validate_source_text(&kind, &source_text)?;
    let material_revision = normalize_patch_string(
        payload.material_revision,
        existing.material_revision,
        "material_revision",
    )?;
    let content_sha256 = normalize_patch_hash(payload.content_sha256, existing.content_sha256)?;
    ensure_locator_identity_matches(
        &locator,
        material_revision.as_deref(),
        content_sha256.as_deref(),
    )?;
    let color = normalize_patch_string(payload.color, existing.color, "color")?;
    let note = normalize_patch_string(payload.note, existing.note, "note")?;
    let tags = payload
        .tags
        .map(normalize_tags)
        .transpose()?
        .map_or(existing.tags, |tags| strings_to_json(&tags));
    let learning_item_id = payload
        .learning_item_id
        .unwrap_or(existing.learning_item_id);

    fetch_material_snapshot(&state.pool, user.id, material_id).await?;
    validate_segment(&state.pool, user.id, material_id, segment_id).await?;
    validate_learning_item(&state.pool, user.id, learning_item_id).await?;

    let record = sqlx::query_as::<_, AnnotationRecord>(&format!(
        r#"
        UPDATE annotations
        SET material_id = $3, segment_id = $4, kind = $5, locator = $6,
            source_text = $7, material_revision = $8, content_sha256 = $9,
            color = $10, note = $11, tags = $12, learning_item_id = $13,
            updated_at = NOW()
        WHERE id = $1 AND user_id = $2
        RETURNING {}
        "#,
        annotation_columns()
    ))
    .bind(id)
    .bind(user.id)
    .bind(material_id)
    .bind(segment_id)
    .bind(kind)
    .bind(locator)
    .bind(source_text)
    .bind(material_revision)
    .bind(content_sha256)
    .bind(color)
    .bind(note)
    .bind(tags)
    .bind(learning_item_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(annotation_not_found)?;

    Ok(Json(annotation_to_dto(record)))
}

pub async fn delete_annotation(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteAnnotationResponse>, AppError> {
    let deleted = sqlx::query_scalar::<_, Uuid>(
        "DELETE FROM annotations WHERE id = $1 AND user_id = $2 RETURNING id",
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await?;
    if deleted.is_none() {
        return Err(annotation_not_found());
    }
    Ok(Json(DeleteAnnotationResponse { deleted: true }))
}

pub async fn convert_to_learning_item(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ConvertAnnotationResponse>, AppError> {
    let mut tx = state.pool.begin().await?;
    let mut annotation = fetch_annotation_for_update(&mut tx, user.id, id).await?;

    let learning_item = if let Some(learning_item_id) = annotation.learning_item_id {
        fetch_converted_learning_item(&mut tx, user.id, learning_item_id).await?
    } else {
        let text = conversion_text(&annotation)?;
        let item_type = conversion_item_type(&annotation.kind, &text);
        let source_sentence = fetch_conversion_source_sentence(
            &mut tx,
            user.id,
            annotation.segment_id,
            &annotation.source_text,
        )
        .await?;
        let review_state = json!({
            "origin": "annotation",
            "annotation_id": annotation.id,
            "annotation_kind": annotation.kind.clone(),
            "locator": annotation.locator.clone(),
            "material_revision": annotation.material_revision.clone(),
            "content_sha256": annotation.content_sha256.clone(),
            "color": annotation.color.clone(),
            "note": annotation.note.clone(),
        });
        let learning_item_id = Uuid::new_v4();
        let dedupe_key = sha256_text(&format!("annotation-conversion:v1:{}", annotation.id));
        let normalized_text = normalize_learning_text(&text);

        sqlx::query(
            r#"
            INSERT INTO learning_items (
                id, user_id, material_id, segment_id, item_type, text, normalized_text,
                source_sentence, collocations, examples, tags, status, priority,
                review_state, dedupe_key
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, '[]'::jsonb, '[]'::jsonb,
                $9, 'candidate', 0, $10, $11
            )
            "#,
        )
        .bind(learning_item_id)
        .bind(user.id)
        .bind(annotation.material_id)
        .bind(annotation.segment_id)
        .bind(item_type)
        .bind(text)
        .bind(normalized_text)
        .bind(source_sentence)
        .bind(annotation.tags.clone())
        .bind(review_state)
        .bind(dedupe_key)
        .execute(&mut *tx)
        .await?;

        learning_activity::record_event_tx(
            &mut tx,
            user.id,
            Some(learning_item_id),
            Some(annotation.material_id),
            "create",
            json!({
                "origin": "annotation",
                "annotation_id": annotation.id,
                "annotation_kind": annotation.kind,
            }),
            Some(format!(
                "create:annotation:{}:{learning_item_id}",
                annotation.id
            )),
        )
        .await?;

        sqlx::query(
            r#"
            UPDATE annotations
            SET learning_item_id = $3, updated_at = NOW()
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(annotation.id)
        .bind(user.id)
        .bind(learning_item_id)
        .execute(&mut *tx)
        .await?;
        annotation.learning_item_id = Some(learning_item_id);
        annotation.updated_at = Utc::now();

        fetch_converted_learning_item(&mut tx, user.id, learning_item_id).await?
    };

    tx.commit().await?;
    Ok(Json(ConvertAnnotationResponse {
        annotation: annotation_to_dto(annotation),
        learning_item: converted_learning_item_to_dto(learning_item),
    }))
}

fn annotation_select() -> &'static str {
    "SELECT a.id, a.material_id, a.segment_id, a.kind, a.locator, a.source_text, \
     a.material_revision, a.content_sha256, a.color, a.note, a.tags, \
     a.learning_item_id, a.idempotency_payload_sha256, \
     a.created_at, a.updated_at FROM annotations a"
}

fn annotation_columns() -> &'static str {
    "id, material_id, segment_id, kind, locator, source_text, material_revision, \
     content_sha256, color, note, tags, learning_item_id, \
     idempotency_payload_sha256, created_at, updated_at"
}

async fn fetch_annotation(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<AnnotationRecord, AppError> {
    sqlx::query_as::<_, AnnotationRecord>(&format!(
        "{} WHERE a.user_id = $1 AND a.id = $2",
        annotation_select()
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(annotation_not_found)
}

async fn fetch_annotation_by_client_request_id(
    pool: &PgPool,
    user_id: Uuid,
    client_request_id: &str,
) -> Result<Option<AnnotationRecord>, AppError> {
    Ok(sqlx::query_as::<_, AnnotationRecord>(&format!(
        "{} WHERE a.user_id = $1 AND a.client_request_id = $2",
        annotation_select()
    ))
    .bind(user_id)
    .bind(client_request_id)
    .fetch_optional(pool)
    .await?)
}

async fn fetch_annotation_for_update(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: Uuid,
) -> Result<AnnotationRecord, AppError> {
    sqlx::query_as::<_, AnnotationRecord>(&format!(
        "{} WHERE a.user_id = $1 AND a.id = $2 FOR UPDATE OF a",
        annotation_select()
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(annotation_not_found)
}

async fn fetch_material_snapshot(
    pool: &PgPool,
    user_id: Uuid,
    material_id: Uuid,
) -> Result<MaterialSnapshot, AppError> {
    sqlx::query_as::<_, MaterialSnapshot>(
        "SELECT content_sha256, updated_at FROM materials WHERE user_id = $1 AND id = $2",
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("material_not_found", "material not found"))
}

async fn validate_segment(
    pool: &PgPool,
    user_id: Uuid,
    material_id: Uuid,
    segment_id: Option<Uuid>,
) -> Result<(), AppError> {
    let Some(segment_id) = segment_id else {
        return Ok(());
    };
    let exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM material_segments
            WHERE user_id = $1 AND material_id = $2 AND id = $3 AND deleted_at IS NULL
        )
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .bind(segment_id)
    .fetch_one(pool)
    .await?;
    if !exists {
        return Err(AppError::not_found(
            "segment_not_found",
            "segment not found for material",
        ));
    }
    Ok(())
}

async fn validate_learning_item(
    pool: &PgPool,
    user_id: Uuid,
    learning_item_id: Option<Uuid>,
) -> Result<(), AppError> {
    let Some(learning_item_id) = learning_item_id else {
        return Ok(());
    };
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM learning_items WHERE user_id = $1 AND id = $2)",
    )
    .bind(user_id)
    .bind(learning_item_id)
    .fetch_one(pool)
    .await?;
    if !exists {
        return Err(AppError::not_found(
            "learning_item_not_found",
            "learning item not found",
        ));
    }
    Ok(())
}

fn normalize_create_request(payload: &mut CreateAnnotationRequest) -> Result<(), AppError> {
    payload.kind = payload.kind.trim().to_ascii_lowercase();
    validate_kind(&payload.kind)?;
    validate_locator(&mut payload.locator)?;
    validate_source_text(&payload.kind, &payload.source_text)?;
    payload.material_revision =
        normalize_optional_string(payload.material_revision.take(), "material_revision")?;
    payload.content_sha256 = normalize_optional_hash(payload.content_sha256.take())?;
    payload.color = normalize_optional_string(payload.color.take(), "color")?;
    payload.note = normalize_optional_string(payload.note.take(), "note")?;
    payload.tags = normalize_tags(std::mem::take(&mut payload.tags))?;
    payload.client_request_id = payload.client_request_id.trim().to_string();
    if payload.client_request_id.is_empty() {
        return Err(AppError::bad_request(
            "invalid_annotation",
            "client_request_id must not be empty",
        ));
    }
    if payload.client_request_id.len() > 255 {
        return Err(AppError::bad_request(
            "invalid_annotation",
            "client_request_id must not exceed 255 characters",
        ));
    }
    Ok(())
}

fn validate_kind(kind: &str) -> Result<(), AppError> {
    if matches!(
        kind,
        "highlight" | "excerpt" | "note" | "vocabulary" | "grammar"
    ) {
        Ok(())
    } else {
        Err(AppError::bad_request(
            "invalid_annotation_kind",
            "kind must be highlight, excerpt, note, vocabulary, or grammar",
        ))
    }
}

fn validate_source_text(kind: &str, source_text: &str) -> Result<(), AppError> {
    if kind != "note" && source_text.trim().is_empty() {
        return Err(AppError::bad_request(
            "invalid_annotation",
            "source_text must not be empty for this annotation kind",
        ));
    }
    Ok(())
}

fn validate_locator(locator: &mut Value) -> Result<(), AppError> {
    let object = locator
        .as_object_mut()
        .ok_or_else(|| invalid_locator("locator must be an object"))?;
    match object.get("version") {
        None => {
            object.insert("version".to_string(), Value::from(LOCATOR_VERSION));
        }
        Some(value) if value.as_i64() == Some(LOCATOR_VERSION) => {}
        Some(_) => return Err(invalid_locator("unsupported locator version")),
    }

    validate_optional_non_empty_string(object, "material_revision")?;
    if let Some(hash) = object.get_mut("content_sha256") {
        let value = hash
            .as_str()
            .ok_or_else(|| invalid_locator("content_sha256 must be a string"))?;
        let normalized = validate_hash(value, "locator content_sha256")?;
        *hash = Value::String(normalized);
    }
    if let Some(quote) = object.get("quote") {
        validate_quote(quote)?;
    }

    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_locator("locator kind must be a string"))?;
    let allowed = match kind {
        "segment" => {
            validate_reader_kind(object, &["article", "txt"])?;
            let order = required_integer(object, "segment_order")?;
            let total = optional_integer(object, "total_segments")?;
            if order < 0 || total.is_some_and(|total| total < 1 || order >= total) {
                return Err(invalid_locator("segment locator range is invalid"));
            }
            validate_optional_uuid_string(object, "segment_id")?;
            common_keys(&["segment_order", "total_segments", "segment_id"])
        }
        "text_range" => {
            validate_reader_kind(object, &["article", "txt", "pdf"])?;
            let start = required_integer(object, "start_offset")?;
            let end = required_integer(object, "end_offset")?;
            if start < 0 || start >= end {
                return Err(invalid_locator("text range must have a positive range"));
            }
            if !object.contains_key("quote") {
                return Err(invalid_locator("text_range locator requires quote"));
            }
            validate_optional_uuid_string(object, "segment_id")?;
            optional_non_negative_integer(object, "segment_order")?;
            validate_optional_page_fields(object)?;
            if object.get("reader_kind").and_then(Value::as_str) == Some("pdf")
                && !object.contains_key("page")
            {
                return Err(invalid_locator("PDF text_range locator requires page"));
            }
            common_keys(&[
                "segment_id",
                "segment_order",
                "page",
                "total_pages",
                "start_offset",
                "end_offset",
            ])
        }
        "page" => {
            validate_reader_kind(object, &["pdf"])?;
            validate_page_fields(object)?;
            common_keys(&["page", "total_pages"])
        }
        "epub_cfi" | "cfi" => {
            validate_reader_kind(object, &["epub"])?;
            required_non_empty_string(object, "cfi")?;
            common_keys(&["cfi"])
        }
        "time" | "time_range" => {
            validate_reader_kind(object, &["media"])?;
            let current = required_finite_number(object, "current_time")?;
            if current < 0.0 {
                return Err(invalid_locator("current_time must be non-negative"));
            }
            if let Some(end) = optional_finite_number(object, "end_time")? {
                if end < current {
                    return Err(invalid_locator("end_time must not precede current_time"));
                }
            }
            if let Some(duration) = optional_finite_number(object, "duration")? {
                if duration < 0.0
                    || current > duration
                    || optional_finite_number(object, "end_time")?.is_some_and(|end| end > duration)
                {
                    return Err(invalid_locator("time locator exceeds duration"));
                }
            }
            validate_optional_uuid_string(object, "segment_id")?;
            common_keys(&["current_time", "end_time", "duration", "segment_id"])
        }
        _ => return Err(invalid_locator("unsupported locator kind")),
    };
    reject_unknown_keys(object, &allowed)?;
    Ok(())
}

fn common_keys(anchor_keys: &[&'static str]) -> HashSet<&'static str> {
    [
        "version",
        "kind",
        "material_revision",
        "content_sha256",
        "quote",
        "reader_kind",
    ]
    .into_iter()
    .chain(anchor_keys.iter().copied())
    .collect()
}

fn reject_unknown_keys(
    object: &Map<String, Value>,
    allowed: &HashSet<&str>,
) -> Result<(), AppError> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(key.as_str())) {
        return Err(invalid_locator(format!("unsupported locator field: {key}")));
    }
    Ok(())
}

fn validate_quote(value: &Value) -> Result<(), AppError> {
    let quote = value
        .as_object()
        .ok_or_else(|| invalid_locator("quote must be an object"))?;
    let exact = quote
        .get("exact")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_locator("quote.exact must be a string"))?;
    if exact.trim().is_empty() {
        return Err(invalid_locator("quote.exact must not be empty"));
    }
    for key in ["prefix", "suffix"] {
        if quote.get(key).is_some_and(|value| !value.is_string()) {
            return Err(invalid_locator(format!("quote.{key} must be a string")));
        }
    }
    if let Some(key) = quote
        .keys()
        .find(|key| !matches!(key.as_str(), "exact" | "prefix" | "suffix"))
    {
        return Err(invalid_locator(format!("unsupported quote field: {key}")));
    }
    Ok(())
}

fn validate_reader_kind(object: &Map<String, Value>, allowed: &[&str]) -> Result<(), AppError> {
    let Some(value) = object.get("reader_kind") else {
        return Ok(());
    };
    let reader_kind = value
        .as_str()
        .ok_or_else(|| invalid_locator("reader_kind must be a string"))?;
    if !allowed.contains(&reader_kind) {
        return Err(invalid_locator(
            "reader_kind is incompatible with locator kind",
        ));
    }
    Ok(())
}

fn validate_page_fields(object: &Map<String, Value>) -> Result<(), AppError> {
    let page = required_integer(object, "page")?;
    if page < 1 {
        return Err(invalid_locator("page must be at least 1"));
    }
    if let Some(total) = optional_integer(object, "total_pages")? {
        if total < page {
            return Err(invalid_locator("total_pages must be at least page"));
        }
    }
    Ok(())
}

fn validate_optional_page_fields(object: &Map<String, Value>) -> Result<(), AppError> {
    if object.contains_key("page") || object.contains_key("total_pages") {
        validate_page_fields(object)?;
    }
    Ok(())
}

fn locator_segment_id(locator: &Value) -> Result<Option<Uuid>, AppError> {
    locator
        .get("segment_id")
        .and_then(Value::as_str)
        .map(|value| {
            Uuid::parse_str(value).map_err(|_| invalid_locator("segment_id must be a UUID"))
        })
        .transpose()
}

fn reconcile_segment_id(
    explicit: Option<Uuid>,
    locator: Option<Uuid>,
) -> Result<Option<Uuid>, AppError> {
    match (explicit, locator) {
        (Some(left), Some(right)) if left != right => Err(invalid_locator(
            "locator segment_id must match annotation segment_id",
        )),
        (Some(value), _) | (_, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

fn resolve_material_identity(
    requested_revision: Option<String>,
    requested_hash: Option<String>,
    locator: &Value,
    material: &MaterialSnapshot,
) -> Result<(Option<String>, Option<String>), AppError> {
    let locator_revision = locator.get("material_revision").and_then(Value::as_str);
    let locator_hash = locator.get("content_sha256").and_then(Value::as_str);
    if requested_revision
        .as_deref()
        .zip(locator_revision)
        .is_some_and(|(left, right)| left != right)
    {
        return Err(invalid_locator("material_revision conflicts with locator"));
    }
    if requested_hash
        .as_deref()
        .zip(locator_hash)
        .is_some_and(|(left, right)| !left.eq_ignore_ascii_case(right))
    {
        return Err(invalid_locator("content_sha256 conflicts with locator"));
    }
    Ok((
        requested_revision
            .or_else(|| locator_revision.map(str::to_string))
            .or_else(|| {
                Some(
                    material
                        .updated_at
                        .to_rfc3339_opts(SecondsFormat::Micros, true),
                )
            }),
        requested_hash
            .or_else(|| locator_hash.map(str::to_ascii_lowercase))
            .or_else(|| material.content_sha256.clone()),
    ))
}

fn ensure_locator_identity_matches(
    locator: &Value,
    revision: Option<&str>,
    hash: Option<&str>,
) -> Result<(), AppError> {
    if locator
        .get("material_revision")
        .and_then(Value::as_str)
        .zip(revision)
        .is_some_and(|(left, right)| left != right)
    {
        return Err(invalid_locator("material_revision conflicts with locator"));
    }
    if locator
        .get("content_sha256")
        .and_then(Value::as_str)
        .zip(hash)
        .is_some_and(|(left, right)| !left.eq_ignore_ascii_case(right))
    {
        return Err(invalid_locator("content_sha256 conflicts with locator"));
    }
    Ok(())
}

fn required_integer(object: &Map<String, Value>, key: &str) -> Result<i64, AppError> {
    object
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| invalid_locator(format!("{key} must be an integer")))
}

fn optional_integer(object: &Map<String, Value>, key: &str) -> Result<Option<i64>, AppError> {
    object
        .get(key)
        .map(|value| {
            value
                .as_i64()
                .ok_or_else(|| invalid_locator(format!("{key} must be an integer")))
        })
        .transpose()
}

fn optional_non_negative_integer(object: &Map<String, Value>, key: &str) -> Result<(), AppError> {
    if optional_integer(object, key)?.is_some_and(|value| value < 0) {
        return Err(invalid_locator(format!("{key} must be non-negative")));
    }
    Ok(())
}

fn required_finite_number(object: &Map<String, Value>, key: &str) -> Result<f64, AppError> {
    object
        .get(key)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| invalid_locator(format!("{key} must be a finite number")))
}

fn optional_finite_number(object: &Map<String, Value>, key: &str) -> Result<Option<f64>, AppError> {
    object
        .get(key)
        .map(|value| {
            value
                .as_f64()
                .filter(|number| number.is_finite())
                .ok_or_else(|| invalid_locator(format!("{key} must be a finite number")))
        })
        .transpose()
}

fn required_non_empty_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, AppError> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_locator(format!("{key} must be a string")))?;
    if value.trim().is_empty() {
        return Err(invalid_locator(format!("{key} must not be empty")));
    }
    Ok(value)
}

fn validate_optional_non_empty_string(
    object: &Map<String, Value>,
    key: &str,
) -> Result<(), AppError> {
    if let Some(value) = object.get(key) {
        let value = value
            .as_str()
            .ok_or_else(|| invalid_locator(format!("{key} must be a string")))?;
        if value.trim().is_empty() {
            return Err(invalid_locator(format!("{key} must not be empty")));
        }
    }
    Ok(())
}

fn validate_optional_uuid_string(object: &Map<String, Value>, key: &str) -> Result<(), AppError> {
    if let Some(value) = object.get(key) {
        let value = value
            .as_str()
            .ok_or_else(|| invalid_locator(format!("{key} must be a UUID string")))?;
        Uuid::parse_str(value).map_err(|_| invalid_locator(format!("{key} must be a UUID")))?;
    }
    Ok(())
}

fn normalize_tags(tags: Vec<String>) -> Result<Vec<String>, AppError> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() {
            return Err(AppError::bad_request(
                "invalid_annotation",
                "tags must not contain empty values",
            ));
        }
        if seen.insert(tag.to_string()) {
            normalized.push(tag.to_string());
        }
    }
    Ok(normalized)
}

fn normalize_optional_string(
    value: Option<String>,
    field: &str,
) -> Result<Option<String>, AppError> {
    value
        .map(|value| {
            let value = value.trim().to_string();
            if value.is_empty() {
                Err(AppError::bad_request(
                    "invalid_annotation",
                    format!("{field} must not be empty"),
                ))
            } else {
                Ok(value)
            }
        })
        .transpose()
}

fn normalize_patch_string(
    value: Option<Option<String>>,
    existing: Option<String>,
    field: &str,
) -> Result<Option<String>, AppError> {
    match value {
        Some(value) => normalize_optional_string(value, field),
        None => Ok(existing),
    }
}

fn normalize_optional_hash(value: Option<String>) -> Result<Option<String>, AppError> {
    value
        .map(|value| validate_hash(&value, "content_sha256"))
        .transpose()
}

fn normalize_patch_hash(
    value: Option<Option<String>>,
    existing: Option<String>,
) -> Result<Option<String>, AppError> {
    match value {
        Some(value) => normalize_optional_hash(value),
        None => Ok(existing),
    }
}

fn validate_hash(value: &str, field: &str) -> Result<String, AppError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::bad_request(
            "invalid_annotation_locator",
            format!("{field} must be a 64-character hexadecimal digest"),
        ));
    }
    Ok(value.to_ascii_lowercase())
}

fn normalize_query_value(value: Option<String>, field: &str) -> Result<Option<String>, AppError> {
    value
        .map(|value| {
            let value = value.trim().to_string();
            if value.is_empty() {
                Err(AppError::bad_request(
                    "invalid_annotation_query",
                    format!("{field} must not be empty"),
                ))
            } else {
                Ok(value)
            }
        })
        .transpose()
}

fn strings_to_json(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}

fn json_to_strings(value: Value) -> Vec<String> {
    value
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn sha256_json<T: Serialize>(value: &T) -> Result<String, AppError> {
    let bytes = serde_json::to_vec(value).map_err(|_| {
        AppError::internal(
            "annotation_serialization_error",
            "failed to serialize annotation request",
        )
    })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn sha256_text(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

fn ensure_idempotent_payload(
    record: &AnnotationRecord,
    payload_hash: &str,
) -> Result<(), AppError> {
    if record.idempotency_payload_sha256.as_deref() == Some(payload_hash) {
        Ok(())
    } else {
        Err(AppError::conflict(
            "annotation_idempotency_conflict",
            "client_request_id was already used with a different payload",
        ))
    }
}

async fn fetch_conversion_source_sentence(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    segment_id: Option<Uuid>,
    fallback: &str,
) -> Result<String, AppError> {
    let Some(segment_id) = segment_id else {
        return Ok(fallback.to_string());
    };
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT text FROM material_segments WHERE user_id = $1 AND id = $2 AND deleted_at IS NULL",
    )
    .bind(user_id)
    .bind(segment_id)
    .fetch_optional(&mut **tx)
    .await?
    .unwrap_or_else(|| fallback.to_string()))
}

async fn fetch_converted_learning_item(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: Uuid,
) -> Result<ConvertedLearningItemRecord, AppError> {
    sqlx::query_as::<_, ConvertedLearningItemRecord>(
        r#"
        SELECT id, material_id, segment_id, item_type, text, source_sentence,
               tags, status, review_state, created_at, updated_at
        FROM learning_items
        WHERE user_id = $1 AND id = $2
        "#,
    )
    .bind(user_id)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::not_found("learning_item_not_found", "learning item not found"))
}

fn conversion_text(annotation: &AnnotationRecord) -> Result<String, AppError> {
    let quoted_text = annotation
        .locator
        .get("quote")
        .and_then(Value::as_object)
        .and_then(|quote| quote.get("exact"))
        .and_then(Value::as_str);
    let text = quoted_text
        .filter(|text| !text.trim().is_empty())
        .or_else(|| {
            (!annotation.source_text.trim().is_empty()).then_some(annotation.source_text.as_str())
        })
        .or(annotation.note.as_deref())
        .unwrap_or_default();
    let text = text.trim();
    if text.is_empty() {
        return Err(AppError::bad_request(
            "annotation_not_convertible",
            "annotation must contain source_text or note",
        ));
    }
    Ok(text.to_string())
}

fn conversion_item_type(kind: &str, text: &str) -> &'static str {
    match kind {
        "grammar" => "grammar",
        "vocabulary" if text.split_whitespace().count() <= 1 => "word",
        "vocabulary" => "phrase",
        _ => "sentence",
    }
}

fn normalize_learning_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn annotation_to_dto(record: AnnotationRecord) -> AnnotationDto {
    AnnotationDto {
        id: record.id.to_string(),
        material_id: record.material_id.to_string(),
        segment_id: record.segment_id.map(|id| id.to_string()),
        kind: record.kind,
        locator: record.locator,
        source_text: record.source_text,
        material_revision: record.material_revision,
        content_sha256: record.content_sha256,
        color: record.color,
        note: record.note,
        tags: json_to_strings(record.tags),
        learning_item_id: record.learning_item_id.map(|id| id.to_string()),
        created_at: record.created_at.to_rfc3339(),
        updated_at: record.updated_at.to_rfc3339(),
    }
}

fn converted_learning_item_to_dto(record: ConvertedLearningItemRecord) -> ConvertedLearningItemDto {
    ConvertedLearningItemDto {
        id: record.id.to_string(),
        material_id: record.material_id.map(|id| id.to_string()),
        segment_id: record.segment_id.map(|id| id.to_string()),
        item_type: record.item_type,
        text: record.text,
        source_sentence: record.source_sentence,
        tags: json_to_strings(record.tags),
        status: record.status,
        review_state: record.review_state,
        created_at: record.created_at.to_rfc3339(),
        updated_at: record.updated_at.to_rfc3339(),
    }
}

fn annotation_not_found() -> AppError {
    AppError::not_found("annotation_not_found", "annotation not found")
}

fn invalid_locator(message: impl Into<String>) -> AppError {
    AppError::bad_request("invalid_annotation_locator", message)
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| code == "23505")
}
