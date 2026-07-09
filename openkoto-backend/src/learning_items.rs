use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::{ApiJson, AuthenticatedUser},
    error::AppError,
    routes::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningItemDto {
    pub id: String,
    #[serde(default)]
    pub material_id: Option<String>,
    #[serde(default)]
    pub segment_id: Option<String>,
    pub item_type: String,
    pub text: String,
    pub source_sentence: String,
    #[serde(default)]
    pub context_before: Option<String>,
    #[serde(default)]
    pub context_after: Option<String>,
    #[serde(default)]
    pub meaning_in_context: Option<String>,
    #[serde(default)]
    pub definition_en: Option<String>,
    #[serde(default)]
    pub definition_zh: Option<String>,
    #[serde(default)]
    pub collocations: Vec<Value>,
    #[serde(default)]
    pub examples: Vec<Value>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub status: String,
    pub priority: i32,
    #[serde(default)]
    pub difficulty: Option<i32>,
    #[serde(default)]
    pub ai_explanation: Option<Value>,
    pub review_state: Value,
    #[serde(default)]
    pub source_material_title: Option<String>,
    #[serde(default)]
    pub source_segment_order: Option<i32>,
    #[serde(default)]
    pub accepted_at: Option<String>,
    #[serde(default)]
    pub rejected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ListLearningItemsQuery {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub item_type: Option<String>,
    #[serde(default)]
    pub material_id: Option<Uuid>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateLearningItemRequest {
    #[serde(default)]
    pub id: Option<Uuid>,
    #[serde(default)]
    pub material_id: Option<Uuid>,
    #[serde(default)]
    pub segment_id: Option<Uuid>,
    #[serde(default)]
    pub item_type: Option<String>,
    pub text: String,
    #[serde(default)]
    pub source_sentence: Option<String>,
    #[serde(default)]
    pub context_before: Option<String>,
    #[serde(default)]
    pub context_after: Option<String>,
    #[serde(default)]
    pub meaning_in_context: Option<String>,
    #[serde(default)]
    pub definition_en: Option<String>,
    #[serde(default)]
    pub definition_zh: Option<String>,
    #[serde(default)]
    pub collocations: Vec<Value>,
    #[serde(default)]
    pub examples: Vec<Value>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub difficulty: Option<i32>,
    #[serde(default)]
    pub ai_explanation: Option<Value>,
    #[serde(default)]
    pub review_state: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct CreateLearningItemFromSelectionRequest {
    pub material_id: Uuid,
    #[serde(default)]
    pub segment_id: Option<Uuid>,
    pub selected_text: String,
    #[serde(default)]
    pub source_sentence: Option<String>,
    #[serde(default)]
    pub context_before: Option<String>,
    #[serde(default)]
    pub context_after: Option<String>,
    #[serde(default)]
    pub item_type: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PatchLearningItemRequest {
    #[serde(default)]
    pub material_id: Option<Uuid>,
    #[serde(default)]
    pub segment_id: Option<Uuid>,
    #[serde(default)]
    pub item_type: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub source_sentence: Option<String>,
    #[serde(default)]
    pub context_before: Option<String>,
    #[serde(default)]
    pub context_after: Option<String>,
    #[serde(default)]
    pub meaning_in_context: Option<String>,
    #[serde(default)]
    pub definition_en: Option<String>,
    #[serde(default)]
    pub definition_zh: Option<String>,
    #[serde(default)]
    pub collocations: Option<Vec<Value>>,
    #[serde(default)]
    pub examples: Option<Vec<Value>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub difficulty: Option<i32>,
    #[serde(default)]
    pub ai_explanation: Option<Value>,
    #[serde(default)]
    pub review_state: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct BulkLearningItemStatusRequest {
    pub ids: Vec<Uuid>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct BulkLearningItemStatusResponse {
    pub updated: usize,
    pub items: Vec<LearningItemDto>,
}

#[derive(Debug, Serialize)]
pub struct DeleteLearningItemResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct LearningItemRecord {
    id: Uuid,
    material_id: Option<Uuid>,
    segment_id: Option<Uuid>,
    item_type: String,
    text: String,
    source_sentence: String,
    context_before: Option<String>,
    context_after: Option<String>,
    meaning_in_context: Option<String>,
    definition_en: Option<String>,
    definition_zh: Option<String>,
    collocations: Value,
    examples: Value,
    tags: Value,
    status: String,
    priority: i32,
    difficulty: Option<i32>,
    ai_explanation: Option<Value>,
    review_state: Value,
    source_material_title: Option<String>,
    source_segment_order: Option<i32>,
    accepted_at: Option<DateTime<Utc>>,
    rejected_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct SourceContext {
    material_id: Option<Uuid>,
    segment_id: Option<Uuid>,
    segment_text: Option<String>,
}

#[derive(Debug, Clone)]
struct LearningItemValues {
    id: Uuid,
    material_id: Option<Uuid>,
    segment_id: Option<Uuid>,
    item_type: String,
    text: String,
    source_sentence: String,
    context_before: Option<String>,
    context_after: Option<String>,
    meaning_in_context: Option<String>,
    definition_en: Option<String>,
    definition_zh: Option<String>,
    collocations: Value,
    examples: Value,
    tags: Value,
    status: String,
    priority: i32,
    difficulty: Option<i32>,
    ai_explanation: Option<Value>,
    review_state: Value,
}

pub async fn list_learning_items(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Query(query): Query<ListLearningItemsQuery>,
) -> Result<Json<Vec<LearningItemDto>>, AppError> {
    if let Some(status) = query.status.as_deref() {
        validate_status(status)?;
    }
    if let Some(item_type) = query.item_type.as_deref() {
        validate_item_type(item_type)?;
    }

    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);
    let records = sqlx::query_as::<_, LearningItemRecord>(
        r#"
        SELECT li.id, li.material_id, li.segment_id, li.item_type, li.text,
               li.source_sentence, li.context_before, li.context_after, li.meaning_in_context,
               li.definition_en, li.definition_zh, li.collocations, li.examples, li.tags,
               li.status, li.priority, li.difficulty, li.ai_explanation, li.review_state,
               m.title AS source_material_title, ms.segment_order AS source_segment_order,
               li.accepted_at, li.rejected_at,
               li.created_at, li.updated_at
        FROM learning_items li
        LEFT JOIN materials m ON m.id = li.material_id AND m.user_id = li.user_id
        LEFT JOIN material_segments ms ON ms.id = li.segment_id AND ms.user_id = li.user_id
        WHERE li.user_id = $1
          AND ($2::TEXT IS NULL OR li.status = $2)
          AND ($3::TEXT IS NULL OR li.item_type = $3)
          AND ($4::UUID IS NULL OR li.material_id = $4)
        ORDER BY li.updated_at DESC, li.created_at DESC
        LIMIT $5 OFFSET $6
        "#,
    )
    .bind(user.id)
    .bind(query.status)
    .bind(query.item_type)
    .bind(query.material_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(records.into_iter().map(record_to_dto).collect()))
}

pub async fn create_learning_item(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<CreateLearningItemRequest>,
) -> Result<Json<LearningItemDto>, AppError> {
    let values = values_from_create_request(&state.pool, user.id, payload).await?;
    let record = insert_or_fetch_learning_item(&state.pool, user.id, values).await?;

    Ok(Json(record_to_dto(record)))
}

pub async fn create_learning_item_from_selection(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<CreateLearningItemFromSelectionRequest>,
) -> Result<Json<LearningItemDto>, AppError> {
    let source = resolve_source(
        &state.pool,
        user.id,
        Some(payload.material_id),
        payload.segment_id,
    )
    .await?;
    let selected_text = normalize_required_string(
        &payload.selected_text,
        "invalid_learning_item",
        "selected text must not be empty",
    )?;
    let source_sentence = payload
        .source_sentence
        .and_then(normalize_optional_string)
        .or_else(|| source.segment_text.clone())
        .unwrap_or_default();
    let item_type = payload
        .item_type
        .unwrap_or_else(|| infer_item_type(&selected_text));

    let values = LearningItemValues {
        id: Uuid::new_v4(),
        material_id: source.material_id,
        segment_id: source.segment_id,
        item_type,
        text: selected_text,
        source_sentence,
        context_before: payload.context_before.and_then(normalize_optional_string),
        context_after: payload.context_after.and_then(normalize_optional_string),
        meaning_in_context: None,
        definition_en: None,
        definition_zh: None,
        collocations: Value::Array(vec![]),
        examples: Value::Array(vec![]),
        tags: strings_to_json_array(payload.tags),
        status: "candidate".to_string(),
        priority: 0,
        difficulty: None,
        ai_explanation: None,
        review_state: Value::Object(Default::default()),
    };

    let values = validate_and_complete_values(values)?;
    let record = insert_or_fetch_learning_item(&state.pool, user.id, values).await?;

    Ok(Json(record_to_dto(record)))
}

pub async fn get_learning_item(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<LearningItemDto>, AppError> {
    let record = fetch_learning_item(&state.pool, user.id, id).await?;

    Ok(Json(record_to_dto(record)))
}

pub async fn patch_learning_item(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(payload): ApiJson<PatchLearningItemRequest>,
) -> Result<Json<LearningItemDto>, AppError> {
    let existing = fetch_learning_item(&state.pool, user.id, id).await?;
    let values = values_from_patch_request(&state.pool, user.id, existing, payload).await?;
    let record = update_learning_item(&state.pool, user.id, values).await?;

    Ok(Json(record_to_dto(record)))
}

pub async fn bulk_learning_item_status(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<BulkLearningItemStatusRequest>,
) -> Result<Json<BulkLearningItemStatusResponse>, AppError> {
    validate_status(&payload.status)?;
    if payload.ids.is_empty() {
        return Ok(Json(BulkLearningItemStatusResponse {
            updated: 0,
            items: vec![],
        }));
    }

    let now = Utc::now();
    sqlx::query(
        r#"
        UPDATE learning_items
        SET status = $3,
            accepted_at = CASE WHEN $3 = 'accepted' AND accepted_at IS NULL THEN $4 ELSE accepted_at END,
            rejected_at = CASE WHEN $3 = 'rejected' AND rejected_at IS NULL THEN $4 ELSE rejected_at END,
            updated_at = NOW()
        WHERE user_id = $1 AND id = ANY($2)
        "#,
    )
    .bind(user.id)
    .bind(&payload.ids)
    .bind(&payload.status)
    .bind(now)
    .execute(&state.pool)
    .await?;

    let records = fetch_learning_items_by_ids(&state.pool, user.id, &payload.ids).await?;
    Ok(Json(BulkLearningItemStatusResponse {
        updated: records.len(),
        items: records.into_iter().map(record_to_dto).collect(),
    }))
}

pub async fn delete_learning_item(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteLearningItemResponse>, AppError> {
    let deleted = sqlx::query_scalar::<_, Uuid>(
        r#"
        DELETE FROM learning_items
        WHERE id = $1 AND user_id = $2
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await?;

    if deleted.is_none() {
        return Err(learning_item_not_found());
    }

    Ok(Json(DeleteLearningItemResponse { deleted: true }))
}

async fn values_from_create_request(
    pool: &PgPool,
    user_id: Uuid,
    payload: CreateLearningItemRequest,
) -> Result<LearningItemValues, AppError> {
    let source = resolve_source(pool, user_id, payload.material_id, payload.segment_id).await?;
    let text = normalize_required_string(
        &payload.text,
        "invalid_learning_item",
        "learning item text must not be empty",
    )?;
    let item_type = payload.item_type.unwrap_or_else(|| infer_item_type(&text));
    let source_sentence = payload
        .source_sentence
        .and_then(normalize_optional_string)
        .or_else(|| source.segment_text.clone())
        .unwrap_or_default();
    let status = payload.status.unwrap_or_else(|| "candidate".to_string());

    validate_and_complete_values(LearningItemValues {
        id: payload.id.unwrap_or_else(Uuid::new_v4),
        material_id: source.material_id,
        segment_id: source.segment_id,
        item_type,
        text,
        source_sentence,
        context_before: payload.context_before.and_then(normalize_optional_string),
        context_after: payload.context_after.and_then(normalize_optional_string),
        meaning_in_context: payload
            .meaning_in_context
            .and_then(normalize_optional_string),
        definition_en: payload.definition_en.and_then(normalize_optional_string),
        definition_zh: payload.definition_zh.and_then(normalize_optional_string),
        collocations: Value::Array(payload.collocations),
        examples: Value::Array(payload.examples),
        tags: strings_to_json_array(payload.tags),
        status,
        priority: payload.priority.unwrap_or(0),
        difficulty: payload.difficulty,
        ai_explanation: payload.ai_explanation,
        review_state: payload
            .review_state
            .unwrap_or_else(|| Value::Object(Default::default())),
    })
}

async fn values_from_patch_request(
    pool: &PgPool,
    user_id: Uuid,
    existing: LearningItemRecord,
    payload: PatchLearningItemRequest,
) -> Result<LearningItemValues, AppError> {
    let requested_material_id = payload.material_id.or(existing.material_id);
    let requested_segment_id = payload.segment_id.or(existing.segment_id);
    let source = resolve_source(pool, user_id, requested_material_id, requested_segment_id).await?;

    validate_and_complete_values(LearningItemValues {
        id: existing.id,
        material_id: source.material_id,
        segment_id: source.segment_id,
        item_type: payload.item_type.unwrap_or(existing.item_type),
        text: payload.text.unwrap_or(existing.text),
        source_sentence: payload.source_sentence.unwrap_or(existing.source_sentence),
        context_before: payload.context_before.or(existing.context_before),
        context_after: payload.context_after.or(existing.context_after),
        meaning_in_context: payload.meaning_in_context.or(existing.meaning_in_context),
        definition_en: payload.definition_en.or(existing.definition_en),
        definition_zh: payload.definition_zh.or(existing.definition_zh),
        collocations: payload
            .collocations
            .map(Value::Array)
            .unwrap_or(existing.collocations),
        examples: payload
            .examples
            .map(Value::Array)
            .unwrap_or(existing.examples),
        tags: payload
            .tags
            .map(strings_to_json_array)
            .unwrap_or(existing.tags),
        status: payload.status.unwrap_or(existing.status),
        priority: payload.priority.unwrap_or(existing.priority),
        difficulty: payload.difficulty.or(existing.difficulty),
        ai_explanation: payload.ai_explanation.or(existing.ai_explanation),
        review_state: payload.review_state.unwrap_or(existing.review_state),
    })
}

async fn insert_or_fetch_learning_item(
    pool: &PgPool,
    user_id: Uuid,
    values: LearningItemValues,
) -> Result<LearningItemRecord, AppError> {
    let dedupe_key = dedupe_key(&values);
    let accepted_at = if values.status == "accepted" {
        Some(Utc::now())
    } else {
        None
    };
    let rejected_at = if values.status == "rejected" {
        Some(Utc::now())
    } else {
        None
    };

    let inserted = sqlx::query_scalar::<_, Uuid>(
        r#"
            INSERT INTO learning_items (
                id, user_id, material_id, segment_id, item_type, text, normalized_text,
                source_sentence, context_before, context_after, meaning_in_context,
                definition_en, definition_zh, collocations, examples, tags, status,
                priority, difficulty, ai_explanation, review_state, dedupe_key,
                accepted_at, rejected_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24
            )
            ON CONFLICT (user_id, dedupe_key) DO NOTHING
            RETURNING id
            "#,
    )
    .bind(values.id)
    .bind(user_id)
    .bind(values.material_id)
    .bind(values.segment_id)
    .bind(&values.item_type)
    .bind(&values.text)
    .bind(normalize_text(&values.text))
    .bind(&values.source_sentence)
    .bind(&values.context_before)
    .bind(&values.context_after)
    .bind(&values.meaning_in_context)
    .bind(&values.definition_en)
    .bind(&values.definition_zh)
    .bind(&values.collocations)
    .bind(&values.examples)
    .bind(&values.tags)
    .bind(&values.status)
    .bind(values.priority)
    .bind(values.difficulty)
    .bind(&values.ai_explanation)
    .bind(&values.review_state)
    .bind(&dedupe_key)
    .bind(accepted_at)
    .bind(rejected_at)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = inserted {
        return fetch_learning_item(pool, user_id, id).await;
    }

    fetch_learning_item_by_dedupe_key(pool, user_id, &dedupe_key).await
}

async fn update_learning_item(
    pool: &PgPool,
    user_id: Uuid,
    values: LearningItemValues,
) -> Result<LearningItemRecord, AppError> {
    let existing = fetch_learning_item(pool, user_id, values.id).await?;
    let dedupe_key = dedupe_key(&values);
    let accepted_at = if values.status == "accepted" && existing.accepted_at.is_none() {
        Some(Utc::now())
    } else {
        existing.accepted_at
    };
    let rejected_at = if values.status == "rejected" && existing.rejected_at.is_none() {
        Some(Utc::now())
    } else {
        existing.rejected_at
    };

    let updated_id = sqlx::query_scalar::<_, Uuid>(
        r#"
            UPDATE learning_items
            SET material_id = $3,
                segment_id = $4,
                item_type = $5,
                text = $6,
                normalized_text = $7,
                source_sentence = $8,
                context_before = $9,
                context_after = $10,
                meaning_in_context = $11,
                definition_en = $12,
                definition_zh = $13,
                collocations = $14,
                examples = $15,
                tags = $16,
                status = $17,
                priority = $18,
                difficulty = $19,
                ai_explanation = $20,
                review_state = $21,
                dedupe_key = $22,
                accepted_at = $23,
                rejected_at = $24,
                updated_at = NOW()
            WHERE id = $1 AND user_id = $2
            RETURNING id
            "#,
    )
    .bind(values.id)
    .bind(user_id)
    .bind(values.material_id)
    .bind(values.segment_id)
    .bind(&values.item_type)
    .bind(&values.text)
    .bind(normalize_text(&values.text))
    .bind(&values.source_sentence)
    .bind(&values.context_before)
    .bind(&values.context_after)
    .bind(&values.meaning_in_context)
    .bind(&values.definition_en)
    .bind(&values.definition_zh)
    .bind(&values.collocations)
    .bind(&values.examples)
    .bind(&values.tags)
    .bind(&values.status)
    .bind(values.priority)
    .bind(values.difficulty)
    .bind(&values.ai_explanation)
    .bind(&values.review_state)
    .bind(&dedupe_key)
    .bind(accepted_at)
    .bind(rejected_at)
    .fetch_optional(pool)
    .await?
    .ok_or_else(learning_item_not_found)?;

    fetch_learning_item(pool, user_id, updated_id).await
}

async fn fetch_learning_item(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<LearningItemRecord, AppError> {
    sqlx::query_as::<_, LearningItemRecord>(
        learning_item_select(
            r#"
            SELECT
            "#,
        )
        .as_str(),
    )
    .bind(user_id)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(learning_item_not_found)
}

async fn fetch_learning_items_by_ids(
    pool: &PgPool,
    user_id: Uuid,
    ids: &[Uuid],
) -> Result<Vec<LearningItemRecord>, AppError> {
    sqlx::query_as::<_, LearningItemRecord>(
        r#"
        SELECT li.id, li.material_id, li.segment_id, li.item_type, li.text,
               li.source_sentence, li.context_before, li.context_after, li.meaning_in_context,
               li.definition_en, li.definition_zh, li.collocations, li.examples, li.tags,
               li.status, li.priority, li.difficulty, li.ai_explanation, li.review_state,
               m.title AS source_material_title, ms.segment_order AS source_segment_order,
               li.accepted_at, li.rejected_at,
               li.created_at, li.updated_at
        FROM learning_items li
        LEFT JOIN materials m ON m.id = li.material_id AND m.user_id = li.user_id
        LEFT JOIN material_segments ms ON ms.id = li.segment_id AND ms.user_id = li.user_id
        WHERE li.user_id = $1 AND li.id = ANY($2)
        ORDER BY li.updated_at DESC
        "#,
    )
    .bind(user_id)
    .bind(ids)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

async fn fetch_learning_item_by_dedupe_key(
    pool: &PgPool,
    user_id: Uuid,
    dedupe_key: &str,
) -> Result<LearningItemRecord, AppError> {
    sqlx::query_as::<_, LearningItemRecord>(
        r#"
        SELECT li.id, li.material_id, li.segment_id, li.item_type, li.text,
               li.source_sentence, li.context_before, li.context_after, li.meaning_in_context,
               li.definition_en, li.definition_zh, li.collocations, li.examples, li.tags,
               li.status, li.priority, li.difficulty, li.ai_explanation, li.review_state,
               m.title AS source_material_title, ms.segment_order AS source_segment_order,
               li.accepted_at, li.rejected_at,
               li.created_at, li.updated_at
        FROM learning_items li
        LEFT JOIN materials m ON m.id = li.material_id AND m.user_id = li.user_id
        LEFT JOIN material_segments ms ON ms.id = li.segment_id AND ms.user_id = li.user_id
        WHERE li.user_id = $1 AND li.dedupe_key = $2
        "#,
    )
    .bind(user_id)
    .bind(dedupe_key)
    .fetch_one(pool)
    .await
    .map_err(AppError::from)
}

fn learning_item_select(prefix: &str) -> String {
    format!(
        "{prefix} li.id, li.material_id, li.segment_id, li.item_type, li.text,
         li.source_sentence, li.context_before, li.context_after, li.meaning_in_context,
         li.definition_en, li.definition_zh, li.collocations, li.examples, li.tags,
         li.status, li.priority, li.difficulty, li.ai_explanation, li.review_state,
         m.title AS source_material_title, ms.segment_order AS source_segment_order,
         li.accepted_at, li.rejected_at,
         li.created_at, li.updated_at
         FROM learning_items li
         LEFT JOIN materials m ON m.id = li.material_id AND m.user_id = li.user_id
         LEFT JOIN material_segments ms ON ms.id = li.segment_id AND ms.user_id = li.user_id
         WHERE li.user_id = $1 AND li.id = $2"
    )
}

async fn resolve_source(
    pool: &PgPool,
    user_id: Uuid,
    material_id: Option<Uuid>,
    segment_id: Option<Uuid>,
) -> Result<SourceContext, AppError> {
    if let Some(segment_id) = segment_id {
        let segment = sqlx::query_as::<_, (Uuid, String)>(
            r#"
            SELECT ms.material_id, ms.text
            FROM material_segments ms
            JOIN materials m ON m.id = ms.material_id AND m.user_id = ms.user_id
            WHERE ms.id = $1 AND ms.user_id = $2
            "#,
        )
        .bind(segment_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::not_found("segment_not_found", "material segment not found"))?;

        if let Some(material_id) = material_id {
            if material_id != segment.0 {
                return Err(AppError::bad_request(
                    "source_mismatch",
                    "segment does not belong to the requested material",
                ));
            }
        }

        return Ok(SourceContext {
            material_id: Some(segment.0),
            segment_id: Some(segment_id),
            segment_text: Some(segment.1),
        });
    }

    if let Some(material_id) = material_id {
        let exists = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT 1::BIGINT
            FROM materials
            WHERE id = $1 AND user_id = $2
            "#,
        )
        .bind(material_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::not_found("material_not_found", "material not found"))?;
        let _ = exists;

        return Ok(SourceContext {
            material_id: Some(material_id),
            segment_id: None,
            segment_text: None,
        });
    }

    Ok(SourceContext {
        material_id: None,
        segment_id: None,
        segment_text: None,
    })
}

fn validate_and_complete_values(
    mut values: LearningItemValues,
) -> Result<LearningItemValues, AppError> {
    values.text = normalize_required_string(
        &values.text,
        "invalid_learning_item",
        "learning item text must not be empty",
    )?;
    values.source_sentence = values.source_sentence.trim().to_string();
    values.context_before = values.context_before.and_then(normalize_optional_string);
    values.context_after = values.context_after.and_then(normalize_optional_string);
    values.meaning_in_context = values
        .meaning_in_context
        .and_then(normalize_optional_string);
    values.definition_en = values.definition_en.and_then(normalize_optional_string);
    values.definition_zh = values.definition_zh.and_then(normalize_optional_string);

    validate_item_type(&values.item_type)?;
    validate_status(&values.status)?;
    validate_priority(values.priority)?;
    validate_difficulty(values.difficulty)?;
    validate_json_array(&values.collocations, "collocations")?;
    validate_json_array(&values.examples, "examples")?;
    validate_json_array(&values.tags, "tags")?;
    validate_json_object(&values.review_state, "review_state")?;
    if let Some(ai_explanation) = &values.ai_explanation {
        validate_json_object(ai_explanation, "ai_explanation")?;
    }

    Ok(values)
}

fn validate_item_type(item_type: &str) -> Result<(), AppError> {
    if !matches!(item_type, "word" | "phrase" | "sentence" | "grammar") {
        return Err(AppError::bad_request(
            "invalid_learning_item",
            "learning item type must be word, phrase, sentence, or grammar",
        ));
    }

    Ok(())
}

fn validate_status(status: &str) -> Result<(), AppError> {
    if !matches!(status, "candidate" | "accepted" | "rejected" | "archived") {
        return Err(AppError::bad_request(
            "invalid_learning_item",
            "learning item status must be candidate, accepted, rejected, or archived",
        ));
    }

    Ok(())
}

fn validate_priority(priority: i32) -> Result<(), AppError> {
    if !(0..=100).contains(&priority) {
        return Err(AppError::bad_request(
            "invalid_learning_item",
            "learning item priority must be between 0 and 100",
        ));
    }

    Ok(())
}

fn validate_difficulty(difficulty: Option<i32>) -> Result<(), AppError> {
    if let Some(difficulty) = difficulty {
        if !(1..=5).contains(&difficulty) {
            return Err(AppError::bad_request(
                "invalid_learning_item",
                "learning item difficulty must be between 1 and 5",
            ));
        }
    }

    Ok(())
}

fn validate_json_array(value: &Value, field: &str) -> Result<(), AppError> {
    if !value.is_array() {
        return Err(AppError::bad_request(
            "invalid_learning_item",
            format!("learning item {field} must be an array"),
        ));
    }

    Ok(())
}

fn validate_json_object(value: &Value, field: &str) -> Result<(), AppError> {
    if !value.is_object() {
        return Err(AppError::bad_request(
            "invalid_learning_item",
            format!("learning item {field} must be an object"),
        ));
    }

    Ok(())
}

fn normalize_required_string(
    value: &str,
    code: &'static str,
    message: &'static str,
) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::bad_request(code, message));
    }

    Ok(value.to_string())
}

fn normalize_optional_string(value: String) -> Option<String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn normalize_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn strings_to_json_array(values: Vec<String>) -> Value {
    Value::Array(
        values
            .into_iter()
            .filter_map(normalize_optional_string)
            .map(Value::String)
            .collect(),
    )
}

fn infer_item_type(text: &str) -> String {
    let word_count = text.split_whitespace().count();
    if word_count <= 1 {
        "word".to_string()
    } else if word_count >= 6 || text.ends_with('.') || text.ends_with('?') || text.ends_with('!') {
        "sentence".to_string()
    } else {
        "phrase".to_string()
    }
}

fn dedupe_key(values: &LearningItemValues) -> String {
    let mut hasher = Sha256::new();
    hasher.update(values.item_type.as_bytes());
    hasher.update(b"\n");
    hasher.update(
        values
            .material_id
            .map(|id| id.to_string())
            .unwrap_or_default()
            .as_bytes(),
    );
    hasher.update(b"\n");
    hasher.update(
        values
            .segment_id
            .map(|id| id.to_string())
            .unwrap_or_default()
            .as_bytes(),
    );
    hasher.update(b"\n");
    hasher.update(normalize_text(&values.text).as_bytes());
    hasher.update(b"\n");
    hasher.update(normalize_text(&values.source_sentence).as_bytes());
    hex::encode(hasher.finalize())
}

fn record_to_dto(record: LearningItemRecord) -> LearningItemDto {
    LearningItemDto {
        id: record.id.to_string(),
        material_id: record.material_id.map(|id| id.to_string()),
        segment_id: record.segment_id.map(|id| id.to_string()),
        item_type: record.item_type,
        text: record.text,
        source_sentence: record.source_sentence,
        context_before: record.context_before,
        context_after: record.context_after,
        meaning_in_context: record.meaning_in_context,
        definition_en: record.definition_en,
        definition_zh: record.definition_zh,
        collocations: value_array(record.collocations),
        examples: value_array(record.examples),
        tags: string_array(record.tags),
        status: record.status,
        priority: record.priority,
        difficulty: record.difficulty,
        ai_explanation: record.ai_explanation,
        review_state: record.review_state,
        source_material_title: record.source_material_title,
        source_segment_order: record.source_segment_order,
        accepted_at: record.accepted_at.map(|value| value.to_rfc3339()),
        rejected_at: record.rejected_at.map(|value| value.to_rfc3339()),
        created_at: record.created_at.to_rfc3339(),
        updated_at: record.updated_at.to_rfc3339(),
    }
}

fn value_array(value: Value) -> Vec<Value> {
    match value {
        Value::Array(values) => values,
        _ => vec![],
    }
}

fn string_array(value: Value) -> Vec<String> {
    match value {
        Value::Array(values) => values
            .into_iter()
            .filter_map(|value| value.as_str().map(ToString::to_string))
            .collect(),
        _ => vec![],
    }
}

fn learning_item_not_found() -> AppError {
    AppError::not_found("learning_item_not_found", "learning item not found")
}
