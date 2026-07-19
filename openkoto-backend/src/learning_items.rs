use axum::{
    extract::{Path, Query, State},
    Json,
};
use std::collections::HashSet;

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::{
    auth::{ApiJson, AuthenticatedUser},
    error::AppError,
    learning_activity,
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
    #[serde(default)]
    pub quality_flags: Vec<String>,
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
    pub source_type: Option<String>,
    #[serde(default)]
    pub source_segment_order: Option<i32>,
    pub source_status: String,
    #[serde(default)]
    pub accepted_at: Option<String>,
    #[serde(default)]
    pub rejected_at: Option<String>,
    #[serde(default)]
    pub status_before_archive: Option<String>,
    #[serde(default)]
    pub merged_into_id: Option<String>,
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
    pub source_type: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub quality_flag: Option<String>,
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
    pub quality_flags: Vec<String>,
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
    #[serde(default)]
    pub quality_flags: Vec<String>,
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
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub context_before: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub context_after: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub meaning_in_context: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub definition_en: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub definition_zh: Option<Option<String>>,
    #[serde(default)]
    pub collocations: Option<Vec<Value>>,
    #[serde(default)]
    pub examples: Option<Vec<Value>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub quality_flags: Option<Vec<String>>,
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

fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
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
    #[serde(default)]
    pub results: Vec<BulkOrganizeResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BulkOrganizeLearningItemsRequest {
    #[serde(default)]
    pub items: Vec<BulkOrganizeOperation>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BulkOrganizeOperation {
    pub id: Uuid,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub quality_flags: Option<Vec<String>>,
    #[serde(default)]
    pub merge_into_id: Option<Uuid>,
    #[serde(default)]
    pub favorite_type: Option<AcceptedFavoriteType>,
    #[serde(default)]
    pub pack_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BulkOrganizeLearningItemsResponse {
    pub succeeded: usize,
    pub failed: usize,
    pub results: Vec<BulkOrganizeResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BulkOrganizeResult {
    pub id: Uuid,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<LearningItemDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_into_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BulkOrganizeError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BulkOrganizeError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct LegacyLearningItemMigrationRequest {
    #[serde(default = "default_true")]
    pub dry_run: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct LegacyMigrationConflict {
    pub source_type: String,
    pub source_id: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct LegacyLearningItemMigrationResponse {
    pub dry_run: bool,
    pub planned: usize,
    pub migrated: usize,
    pub already_migrated: usize,
    pub conflicts: Vec<LegacyMigrationConflict>,
}

#[derive(Debug, Serialize)]
pub struct DeleteLearningItemResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcceptedFavoriteType {
    Vocabulary,
    Grammar,
}

#[derive(Debug, Deserialize)]
pub struct AcceptLearningItemRequest {
    pub favorite_type: AcceptedFavoriteType,
    #[serde(default)]
    pub pack_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AcceptedFavoriteDto {
    Vocabulary { id: String, pack_ids: Vec<String> },
    Grammar { id: String },
}

#[derive(Debug, Serialize)]
pub struct AcceptLearningItemResponse {
    pub learning_item: LearningItemDto,
    pub favorite: AcceptedFavoriteDto,
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
    quality_flags: Value,
    status: String,
    priority: i32,
    difficulty: Option<i32>,
    ai_explanation: Option<Value>,
    review_state: Value,
    source_material_title: Option<String>,
    source_type: Option<String>,
    source_segment_order: Option<i32>,
    source_status: String,
    accepted_at: Option<DateTime<Utc>>,
    rejected_at: Option<DateTime<Utc>>,
    status_before_archive: Option<String>,
    merged_into_id: Option<Uuid>,
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
    quality_flags: Value,
    status: String,
    priority: i32,
    difficulty: Option<i32>,
    ai_explanation: Option<Value>,
    review_state: Value,
}

#[derive(Debug, sqlx::FromRow)]
struct LegacyVocabularyCandidate {
    id: String,
    word: String,
    meaning: String,
    explanation: Option<String>,
    example: Option<String>,
    source_article_id: Option<String>,
    source_article_title: Option<String>,
    learning_item_id: Option<Uuid>,
    linked_item_type: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct LegacyGrammarCandidate {
    id: String,
    point: String,
    explanation: String,
    example: Option<String>,
    source_article_id: Option<String>,
    source_article_title: Option<String>,
    learning_item_id: Option<Uuid>,
    linked_item_type: Option<String>,
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

    let source_type = query.source_type.and_then(normalize_optional_string);
    let source = query.source.and_then(normalize_optional_string);
    let tag = query.tag.and_then(normalize_optional_string);
    let quality_flag = query.quality_flag.and_then(normalize_optional_string);

    let limit = query.limit.unwrap_or(100).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);
    let mut builder = QueryBuilder::<Postgres>::new(
        r#"
        SELECT li.id, li.material_id, li.segment_id, li.item_type, li.text,
               li.source_sentence, li.context_before, li.context_after, li.meaning_in_context,
               li.definition_en, li.definition_zh, li.collocations, li.examples, li.tags,
               li.quality_flags, li.status, li.priority, li.difficulty, li.ai_explanation,
               li.review_state,
               COALESCE(m.title, li.source_material_title_snapshot) AS source_material_title,
               COALESCE(m.source_type, li.source_type_snapshot) AS source_type,
               ms.segment_order AS source_segment_order,
               CASE
                   WHEN m.id IS NULL THEN 'material_missing'
                   WHEN li.segment_id IS NULL THEN 'current'
                   WHEN ms.id IS NULL OR ms.deleted_at IS NOT NULL THEN 'deleted'
                   WHEN li.source_segment_sha256 IS NULL THEN 'changed'
                   WHEN li.source_segment_sha256 = ms.text_sha256 THEN 'current'
                   ELSE 'changed'
               END AS source_status,
               li.accepted_at, li.rejected_at,
               li.status_before_archive, li.merged_into_id,
               li.created_at, li.updated_at
        FROM learning_items li
        LEFT JOIN materials m ON m.id = li.material_id AND m.user_id = li.user_id
        LEFT JOIN material_segments ms ON ms.id = li.segment_id AND ms.user_id = li.user_id
        WHERE li.user_id =
        "#,
    );
    builder.push_bind(user.id);
    if let Some(status) = query.status {
        builder.push(" AND li.status = ").push_bind(status);
    }
    if let Some(item_type) = query.item_type {
        builder.push(" AND li.item_type = ").push_bind(item_type);
    }
    if let Some(material_id) = query.material_id {
        builder
            .push(" AND li.material_id = ")
            .push_bind(material_id);
    }
    if let Some(source_type) = source_type {
        builder
            .push(" AND COALESCE(m.source_type, li.source_type_snapshot) = ")
            .push_bind(source_type);
    }
    if let Some(source) = source {
        let pattern = format!("%{source}%");
        builder
            .push(" AND (COALESCE(m.title, li.source_material_title_snapshot) ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR li.source_sentence ILIKE ")
            .push_bind(pattern)
            .push(")");
    }
    if let Some(tag) = tag {
        builder
            .push(" AND li.tags @> jsonb_build_array(")
            .push_bind(tag)
            .push("::text)");
    }
    if let Some(quality_flag) = quality_flag {
        builder
            .push(" AND li.quality_flags @> jsonb_build_array(")
            .push_bind(quality_flag)
            .push("::text)");
    }
    builder
        .push(" ORDER BY li.updated_at DESC, li.created_at DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    let records = builder
        .build_query_as::<LearningItemRecord>()
        .fetch_all(&state.pool)
        .await?;

    Ok(Json(records.into_iter().map(record_to_dto).collect()))
}

pub async fn create_learning_item(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<CreateLearningItemRequest>,
) -> Result<Json<LearningItemDto>, AppError> {
    reject_direct_acceptance(payload.status.as_deref())?;
    let values = values_from_create_request(&state.pool, user.id, payload).await?;
    let mut tx = state.pool.begin().await?;
    let (record, created) = insert_or_fetch_learning_item(&mut tx, user.id, values).await?;
    if created {
        learning_activity::record_event_tx(
            &mut tx,
            user.id,
            Some(record.id),
            record.material_id,
            "create",
            json!({"origin": "api"}),
            Some(format!("create:learning-item:{}", record.id)),
        )
        .await?;
    }
    tx.commit().await?;

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
        quality_flags: strings_to_json_array(payload.quality_flags),
        status: "candidate".to_string(),
        priority: 0,
        difficulty: None,
        ai_explanation: None,
        review_state: Value::Object(Default::default()),
    };

    let values = validate_and_complete_values(values)?;
    let mut tx = state.pool.begin().await?;
    let (record, created) = insert_or_fetch_learning_item(&mut tx, user.id, values).await?;
    if created {
        learning_activity::record_event_tx(
            &mut tx,
            user.id,
            Some(record.id),
            record.material_id,
            "create",
            json!({"origin": "selection"}),
            Some(format!("create:learning-item:{}", record.id)),
        )
        .await?;
    }
    tx.commit().await?;

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
    reject_direct_acceptance(payload.status.as_deref())?;
    let mut tx = state.pool.begin().await?;
    let existing = fetch_learning_item_for_update(&mut tx, user.id, id).await?;
    let has_favorite_projection = favorite_projection_exists(&mut tx, user.id, &existing).await?;
    let previous_status = existing.status.clone();
    let material_id = existing.material_id;
    let organized = payload.tags.is_some() || payload.quality_flags.is_some();
    let values = values_from_patch_request(&state.pool, user.id, existing, payload).await?;
    let record = update_learning_item(&mut tx, user.id, values).await?;
    if has_favorite_projection {
        sync_favorite_projection_tx(&mut tx, user.id, &record).await?;
    }

    if record.status != previous_status {
        learning_activity::record_event_tx(
            &mut tx,
            user.id,
            Some(record.id),
            material_id,
            transition_event_type(&previous_status, &record.status),
            json!({"from_status": previous_status, "to_status": record.status}),
            None,
        )
        .await?;
    } else if organized {
        learning_activity::record_event_tx(
            &mut tx,
            user.id,
            Some(record.id),
            material_id,
            "organize",
            json!({"fields": ["tags", "quality_flags"]}),
            None,
        )
        .await?;
    }
    tx.commit().await?;

    Ok(Json(record_to_dto(record)))
}

pub async fn accept_learning_item(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(payload): ApiJson<AcceptLearningItemRequest>,
) -> Result<Json<AcceptLearningItemResponse>, AppError> {
    let mut tx = state.pool.begin().await?;
    let response = accept_learning_item_in_tx(&mut tx, user.id, id, payload).await?;
    tx.commit().await?;
    Ok(Json(response))
}

async fn accept_learning_item_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: Uuid,
    payload: AcceptLearningItemRequest,
) -> Result<AcceptLearningItemResponse, AppError> {
    let item = fetch_learning_item_for_update(tx, user_id, id).await?;
    validate_status_transition_for_item(&item, "accepted")?;

    validate_accepted_favorite_type(&item, payload.favorite_type)?;
    let mut pack_ids = dedupe_pack_ids(&payload.pack_ids);
    if payload.favorite_type == AcceptedFavoriteType::Grammar && !pack_ids.is_empty() {
        return Err(AppError::bad_request(
            "grammar_pack_ids_not_supported",
            "pack_ids are only supported for vocabulary favorites",
        ));
    }

    let favorite = match payload.favorite_type {
        AcceptedFavoriteType::Vocabulary => {
            if pack_ids.is_empty()
                && item.status == "archived"
                && item.status_before_archive.as_deref() == Some("accepted")
            {
                pack_ids = sqlx::query_scalar::<_, String>(
                    "SELECT pack_id FROM word_pack_learning_items \
                     WHERE user_id = $1 AND learning_item_id = $2 ORDER BY pack_id",
                )
                .bind(user_id)
                .bind(item.id)
                .fetch_all(&mut **tx)
                .await?;
            }
            let pack_ids = resolve_acceptance_pack_ids(tx, user_id, pack_ids).await?;
            let favorite_id = upsert_accepted_vocabulary(tx, user_id, &item).await?;
            replace_accepted_vocabulary_pack_links(tx, user_id, &favorite_id, &pack_ids).await?;
            replace_canonical_pack_links(tx, user_id, item.id, &pack_ids).await?;
            AcceptedFavoriteDto::Vocabulary {
                id: favorite_id,
                pack_ids,
            }
        }
        AcceptedFavoriteType::Grammar => {
            let favorite_id = upsert_accepted_grammar(tx, user_id, &item).await?;
            AcceptedFavoriteDto::Grammar { id: favorite_id }
        }
    };

    sqlx::query(
        r#"
        UPDATE learning_items
        SET status = 'accepted',
            accepted_at = COALESCE(accepted_at, NOW()),
            status_before_archive = NULL,
            updated_at = CASE WHEN status = 'accepted' THEN updated_at ELSE NOW() END
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(id)
    .bind(user_id)
    .execute(&mut **tx)
    .await?;

    if item.status != "accepted" {
        learning_activity::record_event_tx(
            tx,
            user_id,
            Some(id),
            item.material_id,
            "accept",
            json!({
                "from_status": item.status,
                "favorite_type": payload.favorite_type,
            }),
            None,
        )
        .await?;
    }
    let accepted_item = fetch_learning_item_in_transaction(tx, user_id, id).await?;

    Ok(AcceptLearningItemResponse {
        learning_item: record_to_dto(accepted_item),
        favorite,
    })
}

pub async fn bulk_learning_item_status(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<BulkLearningItemStatusRequest>,
) -> Result<Json<BulkLearningItemStatusResponse>, AppError> {
    validate_status(&payload.status)?;
    reject_direct_acceptance(Some(&payload.status))?;
    if payload.ids.is_empty() {
        return Ok(Json(BulkLearningItemStatusResponse {
            updated: 0,
            items: vec![],
            results: vec![],
        }));
    }
    if payload.ids.len() > 500 {
        return Err(AppError::bad_request(
            "learning_item_bulk_limit",
            "bulk status accepts at most 500 learning items",
        ));
    }

    let mut items = Vec::new();
    let mut results = Vec::with_capacity(payload.ids.len());
    for id in payload.ids {
        match apply_status_transition(&state.pool, user.id, id, &payload.status).await {
            Ok(item) => {
                items.push(item.clone());
                results.push(BulkOrganizeResult {
                    id,
                    success: true,
                    item: Some(item),
                    merged_into_id: None,
                    error: None,
                });
            }
            Err(error) => results.push(failed_bulk_result(id, &error)),
        }
    }
    Ok(Json(BulkLearningItemStatusResponse {
        updated: items.len(),
        items,
        results,
    }))
}

pub async fn bulk_organize_learning_items(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<BulkOrganizeLearningItemsRequest>,
) -> Result<Json<BulkOrganizeLearningItemsResponse>, AppError> {
    if payload.items.len() > 200 {
        return Err(AppError::bad_request(
            "learning_item_bulk_limit",
            "bulk organize accepts at most 200 operations",
        ));
    }
    let mut results = Vec::with_capacity(payload.items.len());
    let mut succeeded = 0;
    for operation in payload.items {
        let id = operation.id;
        match apply_organize_operation(&state.pool, user.id, operation).await {
            Ok(result) => {
                succeeded += 1;
                results.push(result);
            }
            Err(error) => results.push(failed_bulk_result(id, &error)),
        }
    }
    Ok(Json(BulkOrganizeLearningItemsResponse {
        succeeded,
        failed: results.len() - succeeded,
        results,
    }))
}

pub async fn migrate_legacy_learning_items(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<LegacyLearningItemMigrationRequest>,
) -> Result<Json<LegacyLearningItemMigrationResponse>, AppError> {
    let vocabularies = fetch_legacy_vocabularies(&state.pool, user.id).await?;
    let grammars = fetch_legacy_grammars(&state.pool, user.id).await?;
    let mut conflicts = Vec::new();
    let mut planned = 0;
    let mut already_migrated = 0;

    for item in &vocabularies {
        if item.word.trim().is_empty() {
            conflicts.push(legacy_conflict(
                "vocabulary",
                &item.id,
                "empty_favorite_text",
                "favorite vocabulary word is empty",
            ));
        } else if item.learning_item_id.is_some()
            && item.linked_item_type.as_deref() == Some("grammar")
        {
            conflicts.push(legacy_conflict(
                "vocabulary",
                &item.id,
                "favorite_type_conflict",
                "vocabulary favorite is linked to a grammar learning item",
            ));
        } else if item.learning_item_id.is_some() {
            already_migrated += 1;
        } else {
            planned += 1;
        }
    }
    for item in &grammars {
        if item.point.trim().is_empty() {
            conflicts.push(legacy_conflict(
                "grammar",
                &item.id,
                "empty_favorite_text",
                "favorite grammar point is empty",
            ));
        } else if item.learning_item_id.is_some()
            && item.linked_item_type.as_deref() != Some("grammar")
        {
            conflicts.push(legacy_conflict(
                "grammar",
                &item.id,
                "favorite_type_conflict",
                "grammar favorite is linked to a non-grammar learning item",
            ));
        } else if item.learning_item_id.is_some() {
            already_migrated += 1;
        } else {
            planned += 1;
        }
    }

    if payload.dry_run {
        return Ok(Json(LegacyLearningItemMigrationResponse {
            dry_run: true,
            planned,
            migrated: 0,
            already_migrated,
            conflicts,
        }));
    }

    let conflict_keys = conflicts
        .iter()
        .map(|conflict| (conflict.source_type.clone(), conflict.source_id.clone()))
        .collect::<HashSet<_>>();
    let mut tx = state.pool.begin().await?;
    let mut migrated = 0;
    for item in &vocabularies {
        if conflict_keys.contains(&("vocabulary".to_string(), item.id.clone())) {
            continue;
        }
        let was_unlinked = item.learning_item_id.is_none();
        let item_id = canonicalize_favorite_vocabulary_tx(
            &mut tx,
            user.id,
            &item.id,
            &item.word,
            &item.meaning,
            item.explanation.as_deref(),
            item.example.as_deref(),
            item.source_article_id.as_deref(),
            item.source_article_title.as_deref(),
        )
        .await?;
        if was_unlinked {
            migrated += 1;
            record_migration_event(&mut tx, user.id, item_id, "vocabulary", &item.id).await?;
        }
    }
    for item in &grammars {
        if conflict_keys.contains(&("grammar".to_string(), item.id.clone())) {
            continue;
        }
        let was_unlinked = item.learning_item_id.is_none();
        let item_id = canonicalize_favorite_grammar_tx(
            &mut tx,
            user.id,
            &item.id,
            &item.point,
            &item.explanation,
            item.example.as_deref(),
            item.source_article_id.as_deref(),
            item.source_article_title.as_deref(),
        )
        .await?;
        if was_unlinked {
            migrated += 1;
            record_migration_event(&mut tx, user.id, item_id, "grammar", &item.id).await?;
        }
    }
    tx.commit().await?;
    Ok(Json(LegacyLearningItemMigrationResponse {
        dry_run: false,
        planned,
        migrated,
        already_migrated,
        conflicts,
    }))
}

async fn fetch_legacy_vocabularies(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<LegacyVocabularyCandidate>, AppError> {
    sqlx::query_as::<_, LegacyVocabularyCandidate>(
        r#"
        SELECT fv.id, fv.word, fv.meaning, fv.explanation, fv.example,
               fv.source_article_id, fv.source_article_title, fv.learning_item_id,
               li.item_type AS linked_item_type
        FROM favorite_vocabularies fv
        LEFT JOIN learning_items li
          ON li.user_id = fv.user_id AND li.id = fv.learning_item_id
        WHERE fv.user_id = $1
        ORDER BY fv.id
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

async fn fetch_legacy_grammars(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<LegacyGrammarCandidate>, AppError> {
    sqlx::query_as::<_, LegacyGrammarCandidate>(
        r#"
        SELECT fg.id, fg.point, fg.explanation, fg.example,
               fg.source_article_id, fg.source_article_title, fg.learning_item_id,
               li.item_type AS linked_item_type
        FROM favorite_grammars fg
        LEFT JOIN learning_items li
          ON li.user_id = fg.user_id AND li.id = fg.learning_item_id
        WHERE fg.user_id = $1
        ORDER BY fg.id
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

async fn record_migration_event(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    learning_item_id: Uuid,
    source_type: &str,
    source_id: &str,
) -> Result<(), AppError> {
    let material_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT material_id FROM learning_items WHERE user_id = $1 AND id = $2",
    )
    .bind(user_id)
    .bind(learning_item_id)
    .fetch_one(&mut **tx)
    .await?;
    learning_activity::record_event_tx(
        tx,
        user_id,
        Some(learning_item_id),
        material_id,
        "migrate",
        json!({"source_type": source_type, "source_id": source_id}),
        Some(format!("legacy-migrate:{source_type}:{source_id}")),
    )
    .await?;
    Ok(())
}

fn legacy_conflict(
    source_type: &str,
    source_id: &str,
    code: &str,
    message: &str,
) -> LegacyMigrationConflict {
    LegacyMigrationConflict {
        source_type: source_type.to_string(),
        source_id: source_id.to_string(),
        code: code.to_string(),
        message: message.to_string(),
    }
}

async fn apply_status_transition(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    status: &str,
) -> Result<LearningItemDto, AppError> {
    let mut tx = pool.begin().await?;
    let item = apply_status_transition_tx(&mut tx, user_id, id, status).await?;
    tx.commit().await?;
    Ok(item)
}

async fn apply_status_transition_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: Uuid,
    status: &str,
) -> Result<LearningItemDto, AppError> {
    validate_status(status)?;
    reject_direct_acceptance(Some(status))?;
    let item = fetch_learning_item_for_update(tx, user_id, id).await?;
    validate_status_transition_for_item(&item, status)?;
    if item.status != status {
        sqlx::query(
            r#"
            UPDATE learning_items
            SET status = $3,
                rejected_at = CASE
                    WHEN $3 = 'rejected' THEN COALESCE(rejected_at, NOW())
                    ELSE rejected_at
                END,
                status_before_archive = CASE
                    WHEN $3 = 'archived' THEN status
                    WHEN status = 'archived' THEN NULL
                    ELSE status_before_archive
                END,
                merged_into_id = CASE WHEN status = 'archived' AND $3 <> 'archived'
                                      THEN NULL ELSE merged_into_id END,
                updated_at = NOW()
            WHERE user_id = $1 AND id = $2
            "#,
        )
        .bind(user_id)
        .bind(id)
        .bind(status)
        .execute(&mut **tx)
        .await?;
        learning_activity::record_event_tx(
            tx,
            user_id,
            Some(id),
            item.material_id,
            transition_event_type(&item.status, status),
            json!({"from_status": item.status, "to_status": status}),
            None,
        )
        .await?;
    }
    fetch_learning_item_in_transaction(tx, user_id, id)
        .await
        .map(record_to_dto)
}

async fn apply_organize_operation(
    pool: &PgPool,
    user_id: Uuid,
    operation: BulkOrganizeOperation,
) -> Result<BulkOrganizeResult, AppError> {
    let mut tx = pool.begin().await?;
    if let Some(target_id) = operation.merge_into_id {
        let item = merge_learning_items_tx(&mut tx, user_id, operation.id, target_id).await?;
        tx.commit().await?;
        return Ok(BulkOrganizeResult {
            id: operation.id,
            success: true,
            item: Some(item),
            merged_into_id: Some(target_id),
            error: None,
        });
    }

    let existing = fetch_learning_item_for_update(&mut tx, user_id, operation.id).await?;
    let tags = operation.tags.map(strings_to_json_array);
    let quality_flags = operation.quality_flags.map(strings_to_json_array);
    if let Some(tags) = &tags {
        validate_json_array(tags, "tags")?;
        validate_string_array(tags, "tags")?;
    }
    if let Some(quality_flags) = &quality_flags {
        validate_json_array(quality_flags, "quality_flags")?;
        validate_string_array(quality_flags, "quality_flags")?;
    }
    if tags.is_some() || quality_flags.is_some() {
        sqlx::query(
            r#"
            UPDATE learning_items
            SET tags = COALESCE($3, tags),
                quality_flags = COALESCE($4, quality_flags),
                updated_at = NOW()
            WHERE user_id = $1 AND id = $2
            "#,
        )
        .bind(user_id)
        .bind(operation.id)
        .bind(&tags)
        .bind(&quality_flags)
        .execute(&mut *tx)
        .await?;
    }

    let item = if operation.status.as_deref() == Some("accepted") {
        let favorite_type = operation.favorite_type.unwrap_or_else(|| {
            if existing.item_type == "grammar" {
                AcceptedFavoriteType::Grammar
            } else {
                AcceptedFavoriteType::Vocabulary
            }
        });
        accept_learning_item_in_tx(
            &mut tx,
            user_id,
            operation.id,
            AcceptLearningItemRequest {
                favorite_type,
                pack_ids: operation.pack_ids,
            },
        )
        .await?
        .learning_item
    } else if let Some(status) = operation.status.as_deref() {
        apply_status_transition_tx(&mut tx, user_id, operation.id, status).await?
    } else {
        if tags.is_some() || quality_flags.is_some() {
            learning_activity::record_event_tx(
                &mut tx,
                user_id,
                Some(operation.id),
                existing.material_id,
                "organize",
                json!({
                    "updated_tags": tags.is_some(),
                    "updated_quality_flags": quality_flags.is_some(),
                }),
                None,
            )
            .await?;
        }
        fetch_learning_item_in_transaction(&mut tx, user_id, operation.id)
            .await
            .map(record_to_dto)?
    };
    tx.commit().await?;
    Ok(BulkOrganizeResult {
        id: operation.id,
        success: true,
        item: Some(item),
        merged_into_id: None,
        error: None,
    })
}

async fn merge_learning_items_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    source_id: Uuid,
    target_id: Uuid,
) -> Result<LearningItemDto, AppError> {
    if source_id == target_id {
        return Err(AppError::bad_request(
            "learning_item_merge_self",
            "a learning item cannot be merged into itself",
        ));
    }
    let source = fetch_learning_item_for_update(tx, user_id, source_id).await?;
    let target = fetch_learning_item_for_update(tx, user_id, target_id).await?;
    if source.item_type != target.item_type {
        return Err(AppError::conflict(
            "learning_item_merge_type_mismatch",
            "learning items must have the same type to be merged",
        ));
    }
    if source.status == "accepted" {
        return Err(AppError::conflict(
            "learning_item_merge_accepted_source",
            "accepted learning items must not be merged while compatibility favorites exist",
        ));
    }
    if source.status == "archived" || target.status == "archived" {
        return Err(AppError::conflict(
            "learning_item_merge_archived",
            "archived learning items cannot be merged",
        ));
    }
    let tags = union_string_arrays(&target.tags, &source.tags);
    let quality_flags = union_string_arrays(&target.quality_flags, &source.quality_flags);
    sqlx::query(
        r#"
        UPDATE learning_items
        SET tags = $3, quality_flags = $4, updated_at = NOW()
        WHERE user_id = $1 AND id = $2
        "#,
    )
    .bind(user_id)
    .bind(target_id)
    .bind(tags)
    .bind(quality_flags)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"
        INSERT INTO word_pack_learning_items (user_id, pack_id, learning_item_id, added_at)
        SELECT user_id, pack_id, $3, added_at
        FROM word_pack_learning_items
        WHERE user_id = $1 AND learning_item_id = $2
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(source_id)
    .bind(target_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "DELETE FROM word_pack_learning_items WHERE user_id = $1 AND learning_item_id = $2",
    )
    .bind(user_id)
    .bind(source_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        r#"
        UPDATE learning_items
        SET status_before_archive = status,
            status = 'archived',
            merged_into_id = $3,
            updated_at = NOW()
        WHERE user_id = $1 AND id = $2
        "#,
    )
    .bind(user_id)
    .bind(source_id)
    .bind(target_id)
    .execute(&mut **tx)
    .await?;
    learning_activity::record_event_tx(
        tx,
        user_id,
        Some(source_id),
        source.material_id,
        "merge",
        json!({"merged_into_id": target_id}),
        None,
    )
    .await?;
    fetch_learning_item_in_transaction(tx, user_id, source_id)
        .await
        .map(record_to_dto)
}

fn union_string_arrays(first: &Value, second: &Value) -> Value {
    let mut seen = HashSet::new();
    let values = first
        .as_array()
        .into_iter()
        .flatten()
        .chain(second.as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert((*value).to_string()))
        .map(|value| Value::String(value.to_string()))
        .collect();
    Value::Array(values)
}

fn failed_bulk_result(id: Uuid, error: &AppError) -> BulkOrganizeResult {
    let (code, message) = app_error_detail(error);
    BulkOrganizeResult {
        id,
        success: false,
        item: None,
        merged_into_id: None,
        error: Some(BulkOrganizeError { code, message }),
    }
}

fn app_error_detail(error: &AppError) -> (String, String) {
    match error {
        AppError::BadRequest { code, message }
        | AppError::Unauthorized { code, message }
        | AppError::Conflict { code, message }
        | AppError::NotFound { code, message }
        | AppError::Internal { code, message } => ((*code).to_string(), message.clone()),
        AppError::Config(_) => ("config_error".into(), "backend configuration error".into()),
        AppError::Database(_) => ("database_error".into(), "database error".into()),
        AppError::Migration(_) => ("migration_error".into(), "database migration error".into()),
        AppError::Io(_) => ("io_error".into(), "io error".into()),
    }
}

pub async fn delete_learning_item(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteLearningItemResponse>, AppError> {
    let mut tx = state.pool.begin().await?;
    let item = fetch_learning_item_for_update(&mut tx, user.id, id).await?;
    if item.status == "accepted"
        || (item.status == "archived" && item.status_before_archive.as_deref() == Some("accepted"))
    {
        return Err(AppError::conflict(
            "accepted_learning_item_delete_forbidden",
            "accepted learning items must be archived instead of permanently deleted",
        ));
    }
    if item.merged_into_id.is_some() {
        return Err(AppError::conflict(
            "merged_learning_item_delete_forbidden",
            "merged learning items are retained as source evidence",
        ));
    }
    let has_merged_sources = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM learning_items \
         WHERE user_id = $1 AND merged_into_id = $2)",
    )
    .bind(user.id)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if has_merged_sources {
        return Err(AppError::conflict(
            "learning_item_merge_target_delete_forbidden",
            "learning items referenced by merged source evidence cannot be permanently deleted",
        ));
    }

    sqlx::query_scalar::<_, Uuid>(
        r#"
        DELETE FROM learning_items
        WHERE id = $1 AND user_id = $2
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(user.id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

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
        quality_flags: strings_to_json_array(payload.quality_flags),
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
    if payload.item_type.as_deref().is_some_and(|item_type| {
        item_type != existing.item_type
            && (existing.status == "accepted"
                || (existing.status == "archived"
                    && existing.status_before_archive.as_deref() == Some("accepted")))
    }) {
        return Err(AppError::conflict(
            "accepted_learning_item_type_immutable",
            "accepted learning item type cannot be changed while a favorite projection exists",
        ));
    }
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
        context_before: payload.context_before.unwrap_or(existing.context_before),
        context_after: payload.context_after.unwrap_or(existing.context_after),
        meaning_in_context: payload
            .meaning_in_context
            .unwrap_or(existing.meaning_in_context),
        definition_en: payload.definition_en.unwrap_or(existing.definition_en),
        definition_zh: payload.definition_zh.unwrap_or(existing.definition_zh),
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
        quality_flags: payload
            .quality_flags
            .map(strings_to_json_array)
            .unwrap_or(existing.quality_flags),
        status: payload.status.unwrap_or(existing.status),
        priority: payload.priority.unwrap_or(existing.priority),
        difficulty: payload.difficulty.or(existing.difficulty),
        ai_explanation: payload.ai_explanation.or(existing.ai_explanation),
        review_state: payload.review_state.unwrap_or(existing.review_state),
    })
}

async fn insert_or_fetch_learning_item(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    values: LearningItemValues,
) -> Result<(LearningItemRecord, bool), AppError> {
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
                definition_en, definition_zh, collocations, examples, tags, quality_flags,
                status, priority, difficulty, ai_explanation, review_state, dedupe_key,
                accepted_at, rejected_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25
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
    .bind(&values.quality_flags)
    .bind(&values.status)
    .bind(values.priority)
    .bind(values.difficulty)
    .bind(&values.ai_explanation)
    .bind(&values.review_state)
    .bind(&dedupe_key)
    .bind(accepted_at)
    .bind(rejected_at)
    .fetch_optional(&mut **tx)
    .await?;

    if let Some(id) = inserted {
        return Ok((
            fetch_learning_item_in_transaction(tx, user_id, id).await?,
            true,
        ));
    }

    Ok((
        fetch_learning_item_by_dedupe_key_in_transaction(tx, user_id, &dedupe_key).await?,
        false,
    ))
}

async fn update_learning_item(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    values: LearningItemValues,
) -> Result<LearningItemRecord, AppError> {
    let existing = fetch_learning_item_in_transaction(tx, user_id, values.id).await?;
    validate_status_transition_for_item(&existing, &values.status)?;
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
    let status_before_archive = if values.status == "archived" && existing.status != "archived" {
        Some(existing.status.clone())
    } else if existing.status == "archived" && values.status != "archived" {
        None
    } else {
        existing.status_before_archive.clone()
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
                quality_flags = $17,
                status = $18,
                priority = $19,
                difficulty = $20,
                ai_explanation = $21,
                review_state = $22,
                dedupe_key = $23,
                accepted_at = $24,
                rejected_at = $25,
                status_before_archive = $26,
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
    .bind(&values.quality_flags)
    .bind(&values.status)
    .bind(values.priority)
    .bind(values.difficulty)
    .bind(&values.ai_explanation)
    .bind(&values.review_state)
    .bind(&dedupe_key)
    .bind(accepted_at)
    .bind(rejected_at)
    .bind(status_before_archive)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(learning_item_not_found)?;

    fetch_learning_item_in_transaction(tx, user_id, updated_id).await
}

async fn fetch_learning_item_for_update(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: Uuid,
) -> Result<LearningItemRecord, AppError> {
    let query = format!("{} FOR UPDATE OF li", learning_item_select("SELECT"));
    sqlx::query_as::<_, LearningItemRecord>(&query)
        .bind(user_id)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(learning_item_not_found)
}

async fn fetch_learning_item_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: Uuid,
) -> Result<LearningItemRecord, AppError> {
    let query = learning_item_select("SELECT");
    sqlx::query_as::<_, LearningItemRecord>(&query)
        .bind(user_id)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(learning_item_not_found)
}

fn validate_accepted_favorite_type(
    item: &LearningItemRecord,
    favorite_type: AcceptedFavoriteType,
) -> Result<(), AppError> {
    let type_matches = match favorite_type {
        AcceptedFavoriteType::Grammar => item.item_type == "grammar",
        AcceptedFavoriteType::Vocabulary => item.item_type != "grammar",
    };
    if !type_matches {
        return Err(AppError::bad_request(
            "learning_item_favorite_type_mismatch",
            "favorite_type does not match the learning item type",
        ));
    }

    Ok(())
}

fn dedupe_pack_ids(pack_ids: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    pack_ids
        .iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

async fn resolve_acceptance_pack_ids(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    mut pack_ids: Vec<String>,
) -> Result<Vec<String>, AppError> {
    const DEFAULT_PACK_ID: &str = "system-ungrouped";

    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO word_packs (
            user_id, id, name, description, cover_url, author, language_from, language_to,
            tags, version, created_at, updated_at, is_system
        )
        VALUES ($1, $2, '未分组', '系统默认合集', NULL, 'OpenKoto', NULL, NULL,
                '["system"]'::jsonb, '1.0.0', $3, $3, TRUE)
        ON CONFLICT (user_id, id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(DEFAULT_PACK_ID)
    .bind(now)
    .execute(&mut **tx)
    .await?;

    if pack_ids.is_empty() {
        pack_ids.push(DEFAULT_PACK_ID.to_string());
    }

    let existing = sqlx::query_scalar::<_, String>(
        r#"
        SELECT id
        FROM word_packs
        WHERE user_id = $1 AND id = ANY($2)
        FOR KEY SHARE
        "#,
    )
    .bind(user_id)
    .bind(&pack_ids)
    .fetch_all(&mut **tx)
    .await?;

    let missing = pack_ids
        .iter()
        .filter(|id| !existing.iter().any(|existing_id| existing_id == *id))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(AppError::not_found(
            "word_pack_not_found",
            format!("word pack not found: {}", missing.join(", ")),
        ));
    }

    pack_ids.sort();
    Ok(pack_ids)
}

async fn upsert_accepted_vocabulary(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    item: &LearningItemRecord,
) -> Result<String, AppError> {
    ensure_no_opposite_favorite(tx, user_id, item.id, AcceptedFavoriteType::Vocabulary).await?;

    let favorite_id = format!("learning-item-{}", item.id);
    let meaning = accepted_item_meaning(item);
    let explanation = item
        .definition_en
        .as_deref()
        .or(item.meaning_in_context.as_deref());
    let example = first_example_text(&item.examples).or_else(|| {
        (!item.source_sentence.trim().is_empty()).then(|| item.source_sentence.trim().to_string())
    });
    let source_article_id = item.material_id.map(|id| id.to_string());
    let now = Utc::now();
    let due_date = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let created_at = now.to_rfc3339();

    sqlx::query_scalar::<_, String>(
        r#"
        INSERT INTO favorite_vocabularies (
            user_id, id, learning_item_id, word, meaning, usage, explanation, example, reading,
            source_article_id, source_article_title, srs_state, ease_factor, repetitions,
            interval_days, due_date, last_reviewed_at, review_count, created_at
        )
        VALUES ($1, $2, $3, $4, $5, '', $6, $7, NULL, $8, $9, 'new', 2.5, 0, 0,
                $10, NULL, 0, $11)
        ON CONFLICT (user_id, learning_item_id) DO UPDATE
        SET word = EXCLUDED.word,
            meaning = EXCLUDED.meaning,
            explanation = EXCLUDED.explanation,
            example = EXCLUDED.example,
            source_article_id = EXCLUDED.source_article_id,
            source_article_title = EXCLUDED.source_article_title
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(&favorite_id)
    .bind(item.id)
    .bind(item.text.trim())
    .bind(meaning)
    .bind(explanation)
    .bind(example)
    .bind(source_article_id)
    .bind(&item.source_material_title)
    .bind(due_date)
    .bind(created_at)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::from)
}

async fn favorite_projection_exists(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    item: &LearningItemRecord,
) -> Result<bool, AppError> {
    let table_name = if item.item_type == "grammar" {
        "favorite_grammars"
    } else {
        "favorite_vocabularies"
    };
    let query = format!(
        "SELECT EXISTS(SELECT 1 FROM {table_name} WHERE user_id = $1 AND learning_item_id = $2)"
    );
    sqlx::query_scalar::<_, bool>(&query)
        .bind(user_id)
        .bind(item.id)
        .fetch_one(&mut **tx)
        .await
        .map_err(AppError::from)
}

async fn sync_favorite_projection_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    item: &LearningItemRecord,
) -> Result<(), AppError> {
    if item.item_type == "grammar" {
        upsert_accepted_grammar(tx, user_id, item).await?;
        return Ok(());
    }

    let favorite_id = upsert_accepted_vocabulary(tx, user_id, item).await?;
    let mut pack_ids = sqlx::query_scalar::<_, String>(
        "SELECT pack_id FROM word_pack_learning_items \
         WHERE user_id = $1 AND learning_item_id = $2 ORDER BY pack_id",
    )
    .bind(user_id)
    .bind(item.id)
    .fetch_all(&mut **tx)
    .await?;
    if pack_ids.is_empty() {
        pack_ids = sqlx::query_scalar::<_, String>(
            "SELECT pack_id FROM favorite_vocabulary_packs \
             WHERE user_id = $1 AND vocabulary_id = $2 ORDER BY pack_id",
        )
        .bind(user_id)
        .bind(&favorite_id)
        .fetch_all(&mut **tx)
        .await?;
        pack_ids = resolve_acceptance_pack_ids(tx, user_id, pack_ids).await?;
        replace_canonical_pack_links(tx, user_id, item.id, &pack_ids).await?;
    }
    replace_accepted_vocabulary_pack_links(tx, user_id, &favorite_id, &pack_ids).await
}

async fn replace_accepted_vocabulary_pack_links(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    vocabulary_id: &str,
    pack_ids: &[String],
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        DELETE FROM favorite_vocabulary_packs
        WHERE user_id = $1 AND vocabulary_id = $2
        "#,
    )
    .bind(user_id)
    .bind(vocabulary_id)
    .execute(&mut **tx)
    .await?;

    for pack_id in pack_ids {
        sqlx::query(
            r#"
            INSERT INTO favorite_vocabulary_packs (user_id, vocabulary_id, pack_id)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(user_id)
        .bind(vocabulary_id)
        .bind(pack_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn replace_canonical_pack_links(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    learning_item_id: Uuid,
    pack_ids: &[String],
) -> Result<(), AppError> {
    sqlx::query(
        "DELETE FROM word_pack_learning_items WHERE user_id = $1 AND learning_item_id = $2",
    )
    .bind(user_id)
    .bind(learning_item_id)
    .execute(&mut **tx)
    .await?;
    for pack_id in pack_ids {
        sqlx::query(
            r#"
            INSERT INTO word_pack_learning_items (user_id, pack_id, learning_item_id)
            VALUES ($1, $2, $3)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(user_id)
        .bind(pack_id)
        .bind(learning_item_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn canonicalize_favorite_vocabulary_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    favorite_id: &str,
    word: &str,
    meaning: &str,
    explanation: Option<&str>,
    example: Option<&str>,
    source_article_id: Option<&str>,
    source_article_title: Option<&str>,
) -> Result<Uuid, AppError> {
    let linked_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT learning_item_id FROM favorite_vocabularies WHERE user_id = $1 AND id = $2",
    )
    .bind(user_id)
    .bind(favorite_id)
    .fetch_optional(&mut **tx)
    .await?
    .flatten();
    let material_id = resolve_legacy_material_tx(tx, user_id, source_article_id).await?;
    let dedupe_key = sha256_string(&format!("legacy-favorite:vocabulary:{favorite_id}"));
    let item_id = linked_id.unwrap_or_else(Uuid::new_v4);
    let item_type = infer_item_type(word);
    let source_sentence = example.unwrap_or_default().trim();
    let review_state = json!({
        "origin": "legacy_favorite",
        "favorite_type": "vocabulary",
        "favorite_id": favorite_id,
    });
    let inserted = if linked_id.is_none() {
        sqlx::query_scalar::<_, Uuid>(
            r#"
        INSERT INTO learning_items (
            id, user_id, material_id, item_type, text, normalized_text, source_sentence,
            meaning_in_context, definition_en, collocations, examples, tags, quality_flags,
            status, priority, review_state, dedupe_key, accepted_at,
            source_material_title_snapshot
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, '[]'::jsonb,
                '[]'::jsonb, '[]'::jsonb, '[]'::jsonb, 'accepted', 0, $10, $11, NOW(), $12)
        ON CONFLICT (user_id, dedupe_key) DO NOTHING
        RETURNING id
            "#,
        )
        .bind(item_id)
        .bind(user_id)
        .bind(material_id)
        .bind(&item_type)
        .bind(word.trim())
        .bind(normalize_text(word))
        .bind(source_sentence)
        .bind(meaning.trim())
        .bind(explanation.map(str::trim))
        .bind(&review_state)
        .bind(&dedupe_key)
        .bind(source_article_title)
        .fetch_optional(&mut **tx)
        .await?
    } else {
        None
    };
    let item_id = if let Some(item_id) = inserted {
        item_id
    } else if let Some(linked_id) = linked_id {
        sqlx::query(
            r#"
            UPDATE learning_items
            SET material_id = COALESCE($3, material_id), item_type = $4, text = $5,
                normalized_text = $6, source_sentence = $7, meaning_in_context = $8,
                definition_en = $9,
                status = CASE WHEN status = 'archived' THEN status ELSE 'accepted' END,
                accepted_at = COALESCE(accepted_at, NOW()),
                status_before_archive = CASE WHEN status = 'archived' THEN status_before_archive ELSE NULL END,
                merged_into_id = CASE WHEN status = 'archived' THEN merged_into_id ELSE NULL END,
                source_material_title_snapshot = COALESCE($10, source_material_title_snapshot),
                updated_at = NOW()
            WHERE user_id = $1 AND id = $2
            "#,
        )
        .bind(user_id)
        .bind(linked_id)
        .bind(material_id)
        .bind(&item_type)
        .bind(word.trim())
        .bind(normalize_text(word))
        .bind(source_sentence)
        .bind(meaning.trim())
        .bind(explanation.map(str::trim))
        .bind(source_article_title)
        .execute(&mut **tx)
        .await?;
        linked_id
    } else {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM learning_items WHERE user_id = $1 AND dedupe_key = $2",
        )
        .bind(user_id)
        .bind(&dedupe_key)
        .fetch_one(&mut **tx)
        .await?
    };
    sqlx::query(
        "UPDATE favorite_vocabularies SET learning_item_id = $3 WHERE user_id = $1 AND id = $2",
    )
    .bind(user_id)
    .bind(favorite_id)
    .bind(item_id)
    .execute(&mut **tx)
    .await?;
    let pack_ids = sqlx::query_scalar::<_, String>(
        "SELECT pack_id FROM favorite_vocabulary_packs \
         WHERE user_id = $1 AND vocabulary_id = $2 ORDER BY pack_id",
    )
    .bind(user_id)
    .bind(favorite_id)
    .fetch_all(&mut **tx)
    .await?;
    replace_canonical_pack_links(tx, user_id, item_id, &pack_ids).await?;
    if inserted.is_some() {
        record_canonicalized_favorite_events(
            tx,
            user_id,
            item_id,
            material_id,
            "vocabulary",
            favorite_id,
        )
        .await?;
    }
    Ok(item_id)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn canonicalize_favorite_grammar_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    favorite_id: &str,
    point: &str,
    explanation: &str,
    example: Option<&str>,
    source_article_id: Option<&str>,
    source_article_title: Option<&str>,
) -> Result<Uuid, AppError> {
    let linked_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT learning_item_id FROM favorite_grammars WHERE user_id = $1 AND id = $2",
    )
    .bind(user_id)
    .bind(favorite_id)
    .fetch_optional(&mut **tx)
    .await?
    .flatten();
    let material_id = resolve_legacy_material_tx(tx, user_id, source_article_id).await?;
    let dedupe_key = sha256_string(&format!("legacy-favorite:grammar:{favorite_id}"));
    let item_id = linked_id.unwrap_or_else(Uuid::new_v4);
    let review_state = json!({
        "origin": "legacy_favorite",
        "favorite_type": "grammar",
        "favorite_id": favorite_id,
    });
    let inserted = if linked_id.is_none() {
        sqlx::query_scalar::<_, Uuid>(
            r#"
        INSERT INTO learning_items (
            id, user_id, material_id, item_type, text, normalized_text, source_sentence,
            meaning_in_context, collocations, examples, tags, quality_flags, status, priority,
            review_state, dedupe_key, accepted_at, source_material_title_snapshot
        )
        VALUES ($1, $2, $3, 'grammar', $4, $5, $6, $7, '[]'::jsonb,
                '[]'::jsonb, '[]'::jsonb, '[]'::jsonb, 'accepted', 0, $8, $9, NOW(), $10)
        ON CONFLICT (user_id, dedupe_key) DO NOTHING
        RETURNING id
            "#,
        )
        .bind(item_id)
        .bind(user_id)
        .bind(material_id)
        .bind(point.trim())
        .bind(normalize_text(point))
        .bind(example.unwrap_or_default().trim())
        .bind(explanation.trim())
        .bind(&review_state)
        .bind(&dedupe_key)
        .bind(source_article_title)
        .fetch_optional(&mut **tx)
        .await?
    } else {
        None
    };
    let item_id = if let Some(item_id) = inserted {
        item_id
    } else if let Some(linked_id) = linked_id {
        sqlx::query(
            r#"
            UPDATE learning_items
            SET material_id = COALESCE($3, material_id), item_type = 'grammar', text = $4,
                normalized_text = $5, source_sentence = $6, meaning_in_context = $7,
                status = CASE WHEN status = 'archived' THEN status ELSE 'accepted' END,
                accepted_at = COALESCE(accepted_at, NOW()),
                status_before_archive = CASE WHEN status = 'archived' THEN status_before_archive ELSE NULL END,
                merged_into_id = CASE WHEN status = 'archived' THEN merged_into_id ELSE NULL END,
                source_material_title_snapshot = COALESCE($8, source_material_title_snapshot),
                updated_at = NOW()
            WHERE user_id = $1 AND id = $2
            "#,
        )
        .bind(user_id)
        .bind(linked_id)
        .bind(material_id)
        .bind(point.trim())
        .bind(normalize_text(point))
        .bind(example.unwrap_or_default().trim())
        .bind(explanation.trim())
        .bind(source_article_title)
        .execute(&mut **tx)
        .await?;
        linked_id
    } else {
        sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM learning_items WHERE user_id = $1 AND dedupe_key = $2",
        )
        .bind(user_id)
        .bind(&dedupe_key)
        .fetch_one(&mut **tx)
        .await?
    };
    sqlx::query(
        "UPDATE favorite_grammars SET learning_item_id = $3 WHERE user_id = $1 AND id = $2",
    )
    .bind(user_id)
    .bind(favorite_id)
    .bind(item_id)
    .execute(&mut **tx)
    .await?;
    if inserted.is_some() {
        record_canonicalized_favorite_events(
            tx,
            user_id,
            item_id,
            material_id,
            "grammar",
            favorite_id,
        )
        .await?;
    }
    Ok(item_id)
}

async fn record_canonicalized_favorite_events(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    item_id: Uuid,
    material_id: Option<Uuid>,
    favorite_type: &str,
    favorite_id: &str,
) -> Result<(), AppError> {
    let metadata = json!({
        "origin": "legacy_favorite_compatibility",
        "favorite_type": favorite_type,
        "favorite_id": favorite_id,
    });
    learning_activity::record_event_tx(
        tx,
        user_id,
        Some(item_id),
        material_id,
        "create",
        metadata.clone(),
        Some(format!("favorite-create:{favorite_type}:{favorite_id}")),
    )
    .await?;
    learning_activity::record_event_tx(
        tx,
        user_id,
        Some(item_id),
        material_id,
        "accept",
        metadata,
        Some(format!("favorite-accept:{favorite_type}:{favorite_id}")),
    )
    .await?;
    Ok(())
}

async fn resolve_legacy_material_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    source_article_id: Option<&str>,
) -> Result<Option<Uuid>, AppError> {
    let Some(id) = source_article_id.and_then(|value| Uuid::parse_str(value).ok()) else {
        return Ok(None);
    };
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM materials WHERE user_id = $1 AND id = $2)",
    )
    .bind(user_id)
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(exists.then_some(id))
}

fn sha256_string(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

async fn upsert_accepted_grammar(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    item: &LearningItemRecord,
) -> Result<String, AppError> {
    ensure_no_opposite_favorite(tx, user_id, item.id, AcceptedFavoriteType::Grammar).await?;

    let favorite_id = format!("learning-item-{}", item.id);
    let explanation = accepted_item_meaning(item);
    let example = first_example_text(&item.examples).or_else(|| {
        (!item.source_sentence.trim().is_empty()).then(|| item.source_sentence.trim().to_string())
    });
    let source_article_id = item.material_id.map(|id| id.to_string());

    sqlx::query_scalar::<_, String>(
        r#"
        INSERT INTO favorite_grammars (
            user_id, id, learning_item_id, point, explanation, example, source_article_id,
            source_article_title, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (user_id, learning_item_id) DO UPDATE
        SET point = EXCLUDED.point,
            explanation = EXCLUDED.explanation,
            example = EXCLUDED.example,
            source_article_id = EXCLUDED.source_article_id,
            source_article_title = EXCLUDED.source_article_title
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(&favorite_id)
    .bind(item.id)
    .bind(item.text.trim())
    .bind(explanation)
    .bind(example)
    .bind(source_article_id)
    .bind(&item.source_material_title)
    .bind(Utc::now().to_rfc3339())
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::from)
}

async fn ensure_no_opposite_favorite(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    learning_item_id: Uuid,
    favorite_type: AcceptedFavoriteType,
) -> Result<(), AppError> {
    let table_name = match favorite_type {
        AcceptedFavoriteType::Vocabulary => "favorite_grammars",
        AcceptedFavoriteType::Grammar => "favorite_vocabularies",
    };
    let query = format!(
        "SELECT EXISTS(SELECT 1 FROM {table_name} WHERE user_id = $1 AND learning_item_id = $2)"
    );
    let exists = sqlx::query_scalar::<_, bool>(&query)
        .bind(user_id)
        .bind(learning_item_id)
        .fetch_one(&mut **tx)
        .await?;
    if exists {
        return Err(AppError::conflict(
            "learning_item_favorite_conflict",
            "learning item is already linked to another favorite type",
        ));
    }

    Ok(())
}

fn accepted_item_meaning(item: &LearningItemRecord) -> String {
    item.meaning_in_context
        .as_deref()
        .or(item.definition_zh.as_deref())
        .or(item.definition_en.as_deref())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(item.text.as_str())
        .trim()
        .to_string()
}

fn first_example_text(examples: &Value) -> Option<String> {
    examples.as_array().and_then(|values| {
        values.iter().find_map(|value| {
            value
                .as_str()
                .or_else(|| value.get("text").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
    })
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

async fn fetch_learning_item_by_dedupe_key_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    dedupe_key: &str,
) -> Result<LearningItemRecord, AppError> {
    sqlx::query_as::<_, LearningItemRecord>(
        r#"
        SELECT li.id, li.material_id, li.segment_id, li.item_type, li.text,
               li.source_sentence, li.context_before, li.context_after, li.meaning_in_context,
               li.definition_en, li.definition_zh, li.collocations, li.examples, li.tags,
               li.quality_flags, li.status, li.priority, li.difficulty, li.ai_explanation,
               li.review_state,
               COALESCE(m.title, li.source_material_title_snapshot) AS source_material_title,
               COALESCE(m.source_type, li.source_type_snapshot) AS source_type,
               ms.segment_order AS source_segment_order,
               CASE
                   WHEN m.id IS NULL THEN 'material_missing'
                   WHEN li.segment_id IS NULL THEN 'current'
                   WHEN ms.id IS NULL OR ms.deleted_at IS NOT NULL THEN 'deleted'
                   WHEN li.source_segment_sha256 IS NULL THEN 'changed'
                   WHEN li.source_segment_sha256 = ms.text_sha256 THEN 'current'
                   ELSE 'changed'
               END AS source_status,
               li.accepted_at, li.rejected_at,
               li.status_before_archive, li.merged_into_id,
               li.created_at, li.updated_at
        FROM learning_items li
        LEFT JOIN materials m ON m.id = li.material_id AND m.user_id = li.user_id
        LEFT JOIN material_segments ms ON ms.id = li.segment_id AND ms.user_id = li.user_id
        WHERE li.user_id = $1 AND li.dedupe_key = $2
        "#,
    )
    .bind(user_id)
    .bind(dedupe_key)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::from)
}

fn learning_item_select(prefix: &str) -> String {
    format!(
        "{prefix} li.id, li.material_id, li.segment_id, li.item_type, li.text,
         li.source_sentence, li.context_before, li.context_after, li.meaning_in_context,
         li.definition_en, li.definition_zh, li.collocations, li.examples, li.tags,
         li.quality_flags, li.status, li.priority, li.difficulty, li.ai_explanation,
         li.review_state,
         COALESCE(m.title, li.source_material_title_snapshot) AS source_material_title,
         COALESCE(m.source_type, li.source_type_snapshot) AS source_type,
         ms.segment_order AS source_segment_order,
         CASE
             WHEN m.id IS NULL THEN 'material_missing'
             WHEN li.segment_id IS NULL THEN 'current'
             WHEN ms.id IS NULL OR ms.deleted_at IS NOT NULL THEN 'deleted'
             WHEN li.source_segment_sha256 IS NULL THEN 'changed'
             WHEN li.source_segment_sha256 = ms.text_sha256 THEN 'current'
             ELSE 'changed'
         END AS source_status,
         li.accepted_at, li.rejected_at,
         li.status_before_archive, li.merged_into_id,
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
            WHERE ms.id = $1 AND ms.user_id = $2 AND ms.deleted_at IS NULL
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
    validate_string_array(&values.tags, "tags")?;
    validate_json_array(&values.quality_flags, "quality_flags")?;
    validate_string_array(&values.quality_flags, "quality_flags")?;
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

fn reject_direct_acceptance(status: Option<&str>) -> Result<(), AppError> {
    if status == Some("accepted") {
        return Err(AppError::bad_request(
            "atomic_acceptance_required",
            "accepted status must be created through the learning item accept endpoint",
        ));
    }

    Ok(())
}

fn validate_status_transition_for_item(
    item: &LearningItemRecord,
    to: &str,
) -> Result<(), AppError> {
    let from = item.status.as_str();
    validate_status(from)?;
    validate_status(to)?;
    if item.merged_into_id.is_some() && to != "archived" {
        return Err(AppError::conflict(
            "merged_learning_item_restore_forbidden",
            "merged learning items must remain archived as source evidence",
        ));
    }
    let allowed = from == to
        || match from {
            "candidate" => matches!(to, "accepted" | "rejected" | "archived"),
            "rejected" => matches!(to, "candidate" | "archived"),
            "accepted" => to == "archived",
            "archived" => item.status_before_archive.as_deref().unwrap_or("candidate") == to,
            _ => false,
        };
    if !allowed {
        return Err(AppError::conflict(
            "invalid_learning_item_transition",
            format!("learning item cannot transition from {from} to {to}"),
        ));
    }
    Ok(())
}

fn transition_event_type(from: &str, to: &str) -> &'static str {
    match to {
        "accepted" => "accept",
        "rejected" => "reject",
        "archived" => "archive",
        "candidate" if matches!(from, "rejected" | "archived") => "restore",
        _ => "organize",
    }
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

fn validate_string_array(value: &Value, field: &str) -> Result<(), AppError> {
    let valid = value.as_array().is_some_and(|items| {
        items.iter().all(|item| {
            item.as_str()
                .is_some_and(|value| !value.trim().is_empty() && value.len() <= 100)
        })
    });
    if !valid {
        return Err(AppError::bad_request(
            "invalid_learning_item",
            format!("learning item {field} must contain non-empty strings up to 100 characters"),
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
        quality_flags: string_array(record.quality_flags),
        status: record.status,
        priority: record.priority,
        difficulty: record.difficulty,
        ai_explanation: record.ai_explanation,
        review_state: record.review_state,
        source_material_title: record.source_material_title,
        source_type: record.source_type,
        source_segment_order: record.source_segment_order,
        source_status: record.source_status,
        accepted_at: record.accepted_at.map(|value| value.to_rfc3339()),
        rejected_at: record.rejected_at.map(|value| value.to_rfc3339()),
        status_before_archive: record.status_before_archive,
        merged_into_id: record.merged_into_id.map(|value| value.to_string()),
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
