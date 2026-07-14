use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::{collections::HashMap, future::Future, pin::Pin};
use uuid::Uuid;

use crate::{
    auth::{ApiJson, AuthenticatedUser},
    error::AppError,
    learning::{
        AgentTaskDto, ArtifactDto, BookmarkDto, FavoriteGrammarDto, FavoriteVocabularyDto,
        WordPackDto,
    },
    learning_items,
    material_library::{content_sha256_hex, normalize_source_url, validate_sha256},
    materials::{CreateMaterialRequest, MaterialSegmentInput},
    routes::AppState,
};

const DEFAULT_UNGROUPED_PACK_ID: &str = "system-ungrouped";
const DEFAULT_UNGROUPED_PACK_NAME: &str = "未分组";
const DEFAULT_SCHEMA_VERSION: &str = "legacy-import-v1";
type ImportFuture<'a> = Pin<Box<dyn Future<Output = Result<ImportedTarget, AppError>> + Send + 'a>>;

#[derive(Debug, Deserialize, Serialize)]
pub struct LegacyImportRequest {
    #[serde(default)]
    pub client_import_id: Option<String>,
    #[serde(default)]
    pub schema_version: Option<String>,
    #[serde(default)]
    pub source_label: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub config: Option<Value>,
    #[serde(default)]
    pub failed_items: Vec<LegacyImportFailedItem>,
    #[serde(default)]
    pub materials: Vec<LegacyImportItem<LegacyMaterialInput>>,
    #[serde(default)]
    pub word_packs: Vec<LegacyImportItem<WordPackDto>>,
    #[serde(default)]
    pub favorite_vocabularies: Vec<LegacyImportItem<FavoriteVocabularyDto>>,
    #[serde(default)]
    pub favorite_grammars: Vec<LegacyImportItem<FavoriteGrammarDto>>,
    #[serde(default)]
    pub bookmarks: Vec<LegacyImportItem<BookmarkDto>>,
    #[serde(default)]
    pub agent_tasks: Vec<LegacyImportItem<Value>>,
    #[serde(default)]
    pub artifacts: Vec<LegacyImportItem<Value>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LegacyImportItem<T> {
    pub source_id: String,
    pub payload: T,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LegacyImportFailedItem {
    pub source_kind: String,
    pub source_id: String,
    pub error: String,
    #[serde(default)]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LegacyMaterialInput {
    #[serde(default)]
    pub id: Option<String>,
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
    pub segments: Option<Vec<MaterialSegmentInput>>,
}

#[derive(Debug, Serialize)]
pub struct LegacyImportBatchDto {
    pub id: String,
    pub client_import_id: String,
    pub request_sha256: String,
    pub schema_version: String,
    pub source_label: Option<String>,
    pub status: String,
    pub total_items: i32,
    pub imported_items: i32,
    pub skipped_items: i32,
    pub failed_items: i32,
    pub metadata: Value,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub items: Vec<LegacyImportItemDto>,
}

#[derive(Debug, Serialize)]
pub struct LegacyImportItemDto {
    pub id: String,
    pub source_kind: String,
    pub source_id: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct LegacyImportBatchRecord {
    id: Uuid,
    client_import_id: String,
    request_sha256: String,
    schema_version: String,
    source_label: Option<String>,
    status: String,
    total_items: i32,
    imported_items: i32,
    skipped_items: i32,
    failed_items: i32,
    metadata: Value,
    created_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, sqlx::FromRow)]
struct LegacyImportItemRecord {
    id: Uuid,
    source_kind: String,
    source_id: String,
    target_kind: Option<String>,
    target_id: Option<String>,
    status: String,
    error: Option<String>,
    payload: Value,
    created_at: DateTime<Utc>,
}

#[derive(Debug)]
struct ImportedTarget {
    kind: &'static str,
    id: String,
}

#[derive(Debug, Default)]
struct ImportCounters {
    total: i32,
    imported: i32,
    skipped: i32,
    failed: i32,
}

pub async fn create_legacy_import(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<LegacyImportRequest>,
) -> Result<Json<LegacyImportBatchDto>, AppError> {
    let request_sha256 = request_sha256_hex(&payload)?;
    let client_import_id = payload
        .client_import_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::bad_request(
                "legacy_import_client_id_required",
                "legacy import client_import_id is required",
            )
        })?
        .to_string();
    let schema_version = payload
        .schema_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_SCHEMA_VERSION)
        .to_string();

    let batch_id = Uuid::new_v4();
    let metadata = payload
        .metadata
        .clone()
        .map(redact_sensitive_value)
        .unwrap_or_else(|| Value::Object(Default::default()));
    let total_items = payload.total_items();
    let inserted = insert_batch_if_absent(
        &state.pool,
        user.id,
        batch_id,
        client_import_id.clone(),
        request_sha256.clone(),
        schema_version,
        payload
            .source_label
            .clone()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        total_items,
        metadata,
    )
    .await?;
    if inserted.is_none() {
        let existing =
            fetch_batch_by_client_import_id(&state.pool, user.id, &client_import_id).await?;
        let existing = existing.ok_or_else(|| {
            AppError::internal(
                "legacy_import_race_not_found",
                "legacy import conflict could not be resolved",
            )
        })?;
        if existing.request_sha256 == request_sha256 {
            return Ok(Json(
                batch_from_record_with_items(&state.pool, user.id, existing).await?,
            ));
        }
        return Err(AppError::conflict(
            "legacy_import_payload_conflict",
            "client_import_id already exists with different payload",
        ));
    }

    let mut counters = ImportCounters {
        total: total_items,
        ..Default::default()
    };
    let mut material_id_map = HashMap::<String, String>::new();

    if let Some(config) = payload.config.clone() {
        record_import_item(
            &state.pool,
            batch_id,
            user.id,
            "config",
            "config.json",
            Some(ImportedTarget {
                kind: "config",
                id: "config.json".to_string(),
            }),
            None,
            redact_sensitive_value(config),
            &mut counters,
        )
        .await?;
    }

    for item in payload.failed_items {
        record_import_item(
            &state.pool,
            batch_id,
            user.id,
            item.source_kind.trim(),
            item.source_id.trim(),
            None,
            Some(item.error),
            item.payload
                .map(redact_sensitive_value)
                .unwrap_or_else(|| Value::Object(Default::default())),
            &mut counters,
        )
        .await?;
    }

    for item in payload.word_packs {
        import_item(
            &state.pool,
            batch_id,
            user.id,
            "word_pack",
            item.source_id,
            item.payload,
            &mut counters,
            |pool, user_id, payload| Box::pin(import_word_pack(pool, user_id, payload)),
        )
        .await?;
    }

    for item in payload.materials {
        let aliases = item.payload.aliases(&item.source_id);
        if let Some(target) = import_item(
            &state.pool,
            batch_id,
            user.id,
            "material",
            item.source_id,
            item.payload,
            &mut counters,
            |pool, user_id, payload| Box::pin(import_material(pool, user_id, payload)),
        )
        .await?
        {
            for alias in aliases {
                material_id_map.insert(alias, target.id.clone());
            }
        }
    }

    for item in payload.favorite_vocabularies {
        let mut payload = item.payload;
        rewrite_optional_article_id(&mut payload.source_article_id, &material_id_map);
        import_item(
            &state.pool,
            batch_id,
            user.id,
            "favorite_vocabulary",
            item.source_id,
            payload,
            &mut counters,
            |pool, user_id, payload| Box::pin(import_favorite_vocabulary(pool, user_id, payload)),
        )
        .await?;
    }

    for item in payload.favorite_grammars {
        let mut payload = item.payload;
        rewrite_optional_article_id(&mut payload.source_article_id, &material_id_map);
        import_item(
            &state.pool,
            batch_id,
            user.id,
            "favorite_grammar",
            item.source_id,
            payload,
            &mut counters,
            |pool, user_id, payload| Box::pin(import_favorite_grammar(pool, user_id, payload)),
        )
        .await?;
    }

    for item in payload.bookmarks {
        import_item(
            &state.pool,
            batch_id,
            user.id,
            "bookmark",
            item.source_id,
            item.payload,
            &mut counters,
            |pool, user_id, payload| Box::pin(import_bookmark(pool, user_id, payload)),
        )
        .await?;
    }

    for item in payload.agent_tasks {
        let mut payload = item.payload;
        rewrite_article_id_in_value(&mut payload, &material_id_map);
        import_item(
            &state.pool,
            batch_id,
            user.id,
            "agent_task",
            item.source_id,
            payload,
            &mut counters,
            |pool, user_id, payload| Box::pin(import_agent_task_value(pool, user_id, payload)),
        )
        .await?;
    }

    for item in payload.artifacts {
        let mut payload = item.payload;
        rewrite_article_id_in_value(&mut payload, &material_id_map);
        import_item(
            &state.pool,
            batch_id,
            user.id,
            "artifact",
            item.source_id,
            payload,
            &mut counters,
            |pool, user_id, payload| Box::pin(import_artifact_value(pool, user_id, payload)),
        )
        .await?;
    }

    finish_batch(&state.pool, batch_id, user.id, counters).await?;
    let batch = fetch_batch(&state.pool, user.id, batch_id).await?;
    Ok(Json(
        batch_from_record_with_items(&state.pool, user.id, batch).await?,
    ))
}

pub async fn get_legacy_import(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<LegacyImportBatchDto>, AppError> {
    let record = fetch_batch(&state.pool, user.id, id).await?;
    Ok(Json(
        batch_from_record_with_items(&state.pool, user.id, record).await?,
    ))
}

impl LegacyImportRequest {
    fn total_items(&self) -> i32 {
        let mut total = 0usize;
        if self.config.is_some() {
            total += 1;
        }
        total += self.failed_items.len();
        total += self.materials.len();
        total += self.word_packs.len();
        total += self.favorite_vocabularies.len();
        total += self.favorite_grammars.len();
        total += self.bookmarks.len();
        total += self.agent_tasks.len();
        total += self.artifacts.len();
        total as i32
    }
}

impl LegacyMaterialInput {
    fn aliases(&self, source_id: &str) -> Vec<String> {
        let mut aliases = Vec::new();
        push_alias(&mut aliases, source_id);
        if let Some(id) = self.id.as_deref() {
            push_alias(&mut aliases, id);
        }
        if let Some(legacy_article_id) = self
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("legacy_article_id"))
            .and_then(Value::as_str)
        {
            push_alias(&mut aliases, legacy_article_id);
        }
        aliases
    }

    fn into_create_material_request(self) -> CreateMaterialRequest {
        CreateMaterialRequest {
            id: self
                .id
                .as_deref()
                .and_then(|value| Uuid::parse_str(value).ok()),
            title: self.title,
            content: self.content,
            source_type: self.source_type,
            source_url: self.source_url,
            media_path: self.media_path,
            book_path: self.book_path,
            book_type: self.book_type,
            translated: self.translated,
            active_mind_map_artifact_id: self.active_mind_map_artifact_id,
            metadata: self.metadata,
            file_sha256: None,
            duplicate_policy: Some("keep_copy".to_string()),
            segments: self.segments,
        }
    }
}

fn push_alias(aliases: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if value.is_empty() || aliases.iter().any(|existing| existing == value) {
        return;
    }
    aliases.push(value.to_string());
}

fn request_sha256_hex(payload: &LegacyImportRequest) -> Result<String, AppError> {
    let bytes = serde_json::to_vec(payload).map_err(|error| {
        AppError::bad_request(
            "invalid_legacy_import_payload",
            format!("legacy import payload is not serializable: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn rewrite_optional_article_id(
    article_id: &mut Option<String>,
    material_id_map: &HashMap<String, String>,
) {
    if let Some(value) = article_id {
        rewrite_article_id(value, material_id_map);
    }
}

fn rewrite_article_id(article_id: &mut String, material_id_map: &HashMap<String, String>) {
    if let Some(mapped_id) = material_id_map.get(article_id.as_str()) {
        *article_id = mapped_id.clone();
    }
}

fn rewrite_article_id_in_value(value: &mut Value, material_id_map: &HashMap<String, String>) {
    match value {
        Value::Object(object) => {
            if let Some(article_id) = object
                .get("article_id")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                if let Some(mapped_id) = material_id_map.get(article_id.as_str()) {
                    object.insert("article_id".to_string(), Value::String(mapped_id.clone()));
                }
            }
            if let Some(article_id) = object
                .get("source_article_id")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                if let Some(mapped_id) = material_id_map.get(article_id.as_str()) {
                    object.insert(
                        "source_article_id".to_string(),
                        Value::String(mapped_id.clone()),
                    );
                }
            }
            for child in object.values_mut() {
                rewrite_article_id_in_value(child, material_id_map);
            }
        }
        Value::Array(items) => {
            for item in items {
                rewrite_article_id_in_value(item, material_id_map);
            }
        }
        _ => {}
    }
}

fn clean_ledger_key(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        format!("{fallback}-{}", Uuid::new_v4())
    } else {
        value.to_string()
    }
}

fn redact_sensitive_value(value: Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| {
                    if is_sensitive_key(&key) {
                        (key, Value::String("[redacted]".to_string()))
                    } else {
                        (key, redact_sensitive_value(value))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(redact_sensitive_value).collect())
        }
        other => other,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key == "api_key"
        || key == "auth_token"
        || key == "token"
        || key == "access_token"
        || key == "refresh_token"
        || key == "authorization"
        || key == "cookie"
        || key == "session"
        || key == "secret"
        || key == "password"
        || key.ends_with("_secret")
        || key.ends_with("_key")
        || key.ends_with("_token")
}

async fn import_item<T, F>(
    pool: &PgPool,
    batch_id: Uuid,
    user_id: Uuid,
    source_kind: &'static str,
    source_id: String,
    payload: T,
    counters: &mut ImportCounters,
    importer: F,
) -> Result<Option<ImportedTarget>, AppError>
where
    T: Serialize,
    F: for<'a> FnOnce(&'a PgPool, Uuid, T) -> ImportFuture<'a>,
{
    let payload_json = serde_json::to_value(&payload).unwrap_or_else(|_| Value::Null);
    let result = importer(pool, user_id, payload).await;
    match result {
        Ok(target) => {
            let returned_target = ImportedTarget {
                kind: target.kind,
                id: target.id.clone(),
            };
            record_import_item(
                pool,
                batch_id,
                user_id,
                source_kind,
                &source_id,
                Some(target),
                None,
                payload_json,
                counters,
            )
            .await?;
            Ok(Some(returned_target))
        }
        Err(error) => {
            record_import_item(
                pool,
                batch_id,
                user_id,
                source_kind,
                &source_id,
                None,
                Some(error.to_string()),
                payload_json,
                counters,
            )
            .await?;
            Ok(None)
        }
    }
}

async fn insert_batch_if_absent(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    client_import_id: String,
    request_sha256: String,
    schema_version: String,
    source_label: Option<String>,
    total_items: i32,
    metadata: Value,
) -> Result<Option<LegacyImportBatchRecord>, AppError> {
    sqlx::query_as::<_, LegacyImportBatchRecord>(
        r#"
        INSERT INTO legacy_import_batches (
            id, user_id, client_import_id, request_sha256, schema_version, source_label, status,
            total_items, metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'running', $7, $8)
        ON CONFLICT (user_id, client_import_id) DO NOTHING
        RETURNING id, client_import_id, request_sha256, schema_version, source_label, status,
                  total_items, imported_items, skipped_items, failed_items, metadata, created_at,
                  finished_at
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(client_import_id)
    .bind(request_sha256)
    .bind(schema_version)
    .bind(source_label)
    .bind(total_items)
    .bind(metadata)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)
}

async fn finish_batch(
    pool: &PgPool,
    batch_id: Uuid,
    user_id: Uuid,
    counters: ImportCounters,
) -> Result<(), AppError> {
    let status = if counters.failed > 0 {
        "completed_with_errors"
    } else {
        "completed"
    };

    sqlx::query(
        r#"
        UPDATE legacy_import_batches
        SET status = $3,
            total_items = $4,
            imported_items = $5,
            skipped_items = $6,
            failed_items = $7,
            finished_at = NOW()
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(batch_id)
    .bind(user_id)
    .bind(status)
    .bind(counters.total)
    .bind(counters.imported)
    .bind(counters.skipped)
    .bind(counters.failed)
    .execute(pool)
    .await?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn record_import_item(
    pool: &PgPool,
    batch_id: Uuid,
    user_id: Uuid,
    source_kind: &str,
    source_id: &str,
    target: Option<ImportedTarget>,
    error: Option<String>,
    payload: Value,
    counters: &mut ImportCounters,
) -> Result<(), AppError> {
    let status = if error.is_some() {
        "failed"
    } else {
        "imported"
    };
    let source_kind = clean_ledger_key(source_kind, "unknown");
    let source_id = clean_ledger_key(source_id, "missing-source-id");
    let (target_kind, target_id) = target
        .map(|target| (Some(target.kind.to_string()), Some(target.id)))
        .unwrap_or((None, None));
    let payload = redact_sensitive_value(payload);

    let result = sqlx::query(
        r#"
        INSERT INTO legacy_import_items (
            id, batch_id, user_id, source_kind, source_id, target_kind, target_id, status, error,
            payload
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (batch_id, source_kind, source_id) DO NOTHING
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(batch_id)
    .bind(user_id)
    .bind(source_kind)
    .bind(source_id)
    .bind(target_kind)
    .bind(target_id)
    .bind(status)
    .bind(error)
    .bind(payload)
    .execute(pool)
    .await?;

    if result.rows_affected() > 0 {
        match status {
            "failed" => counters.failed += 1,
            "imported" => counters.imported += 1,
            _ => counters.skipped += 1,
        }
    }

    Ok(())
}

async fn fetch_batch(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
) -> Result<LegacyImportBatchRecord, AppError> {
    sqlx::query_as::<_, LegacyImportBatchRecord>(
        r#"
        SELECT id, client_import_id, request_sha256, schema_version, source_label, status,
               total_items, imported_items, skipped_items, failed_items, metadata, created_at,
               finished_at
        FROM legacy_import_batches
        WHERE id = $1 AND user_id = $2
        "#,
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("legacy_import_not_found", "legacy import not found"))
}

async fn fetch_batch_by_client_import_id(
    pool: &PgPool,
    user_id: Uuid,
    client_import_id: &str,
) -> Result<Option<LegacyImportBatchRecord>, AppError> {
    sqlx::query_as::<_, LegacyImportBatchRecord>(
        r#"
        SELECT id, client_import_id, request_sha256, schema_version, source_label, status,
               total_items, imported_items, skipped_items, failed_items, metadata, created_at,
               finished_at
        FROM legacy_import_batches
        WHERE user_id = $1 AND client_import_id = $2
        "#,
    )
    .bind(user_id)
    .bind(client_import_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)
}

async fn fetch_items(
    pool: &PgPool,
    user_id: Uuid,
    batch_id: Uuid,
) -> Result<Vec<LegacyImportItemRecord>, AppError> {
    sqlx::query_as::<_, LegacyImportItemRecord>(
        r#"
        SELECT id, source_kind, source_id, target_kind, target_id, status, error, payload,
               created_at
        FROM legacy_import_items
        WHERE user_id = $1 AND batch_id = $2
        ORDER BY created_at ASC, source_kind ASC, source_id ASC
        "#,
    )
    .bind(user_id)
    .bind(batch_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

async fn batch_from_record_with_items(
    pool: &PgPool,
    user_id: Uuid,
    record: LegacyImportBatchRecord,
) -> Result<LegacyImportBatchDto, AppError> {
    let items = fetch_items(pool, user_id, record.id)
        .await?
        .into_iter()
        .map(item_from_record)
        .collect();
    Ok(LegacyImportBatchDto {
        id: record.id.to_string(),
        client_import_id: record.client_import_id,
        request_sha256: record.request_sha256,
        schema_version: record.schema_version,
        source_label: record.source_label,
        status: record.status,
        total_items: record.total_items,
        imported_items: record.imported_items,
        skipped_items: record.skipped_items,
        failed_items: record.failed_items,
        metadata: record.metadata,
        created_at: record.created_at.to_rfc3339(),
        finished_at: record.finished_at.map(|value| value.to_rfc3339()),
        items,
    })
}

fn item_from_record(record: LegacyImportItemRecord) -> LegacyImportItemDto {
    LegacyImportItemDto {
        id: record.id.to_string(),
        source_kind: record.source_kind,
        source_id: record.source_id,
        target_kind: record.target_kind,
        target_id: record.target_id,
        status: record.status,
        error: record.error,
        payload: record.payload,
        created_at: record.created_at.to_rfc3339(),
    }
}

async fn import_material(
    pool: &PgPool,
    user_id: Uuid,
    payload: LegacyMaterialInput,
) -> Result<ImportedTarget, AppError> {
    let payload = payload.into_create_material_request();
    let material_id = payload.id.unwrap_or_else(Uuid::new_v4);
    if payload.title.trim().is_empty() {
        return Err(AppError::bad_request(
            "invalid_legacy_material",
            "material title must not be empty",
        ));
    }
    let source_type = payload
        .source_type
        .or_else(|| Some("article".to_string()))
        .filter(|value| !value.trim().is_empty());
    let metadata = payload
        .metadata
        .unwrap_or_else(|| Value::Object(Default::default()));
    let normalized_source_url = normalize_source_url(payload.source_url.as_deref())?;
    let content_sha256 = content_sha256_hex(&payload.content);
    let file_sha256 = validate_sha256(
        metadata.get("file_sha256").and_then(Value::as_str),
        "file_sha256",
    )?;
    let translated = payload.translated.unwrap_or(false);
    let segments = payload
        .segments
        .unwrap_or_else(|| fallback_segments_from_content(&payload.content));

    let mut tx = pool.begin().await?;
    let inserted = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO materials (
            id, user_id, title, content, source_type, source_url, media_path, book_path,
            book_type, translated, active_mind_map_artifact_id, metadata,
            normalized_source_url, content_sha256, file_sha256
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
        ON CONFLICT (id) DO UPDATE
        SET title = EXCLUDED.title,
            content = EXCLUDED.content,
            source_type = EXCLUDED.source_type,
            source_url = EXCLUDED.source_url,
            media_path = EXCLUDED.media_path,
            book_path = EXCLUDED.book_path,
            book_type = EXCLUDED.book_type,
            translated = EXCLUDED.translated,
            active_mind_map_artifact_id = EXCLUDED.active_mind_map_artifact_id,
            metadata = EXCLUDED.metadata,
            normalized_source_url = EXCLUDED.normalized_source_url,
            content_sha256 = EXCLUDED.content_sha256,
            file_sha256 = EXCLUDED.file_sha256,
            updated_at = NOW()
        WHERE materials.user_id = EXCLUDED.user_id
        RETURNING id
        "#,
    )
    .bind(material_id)
    .bind(user_id)
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
    .fetch_optional(&mut *tx)
    .await?;

    if inserted.is_none() {
        return Err(AppError::conflict(
            "legacy_material_id_conflict",
            "material id already belongs to another user",
        ));
    }

    sqlx::query("DELETE FROM material_segments WHERE user_id = $1 AND material_id = $2")
        .bind(user_id)
        .bind(material_id)
        .execute(&mut *tx)
        .await?;
    insert_legacy_segments(&mut tx, user_id, material_id, segments).await?;
    tx.commit().await?;

    Ok(ImportedTarget {
        kind: "material",
        id: material_id.to_string(),
    })
}

async fn import_word_pack(
    pool: &PgPool,
    user_id: Uuid,
    payload: WordPackDto,
) -> Result<ImportedTarget, AppError> {
    validate_required(
        &payload.id,
        "invalid_legacy_word_pack",
        "word pack id is required",
    )?;
    validate_required(
        &payload.name,
        "invalid_legacy_word_pack",
        "word pack name is required",
    )?;
    let is_system = payload.id == DEFAULT_UNGROUPED_PACK_ID;

    sqlx::query(
        r#"
        INSERT INTO word_packs (
            user_id, id, name, description, cover_url, author, language_from, language_to, tags,
            version, created_at, updated_at, is_system
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        ON CONFLICT (user_id, id) DO UPDATE
        SET name = EXCLUDED.name,
            description = EXCLUDED.description,
            cover_url = EXCLUDED.cover_url,
            author = EXCLUDED.author,
            language_from = EXCLUDED.language_from,
            language_to = EXCLUDED.language_to,
            tags = EXCLUDED.tags,
            version = EXCLUDED.version,
            updated_at = EXCLUDED.updated_at,
            is_system = word_packs.is_system
        "#,
    )
    .bind(user_id)
    .bind(&payload.id)
    .bind(payload.name.trim())
    .bind(payload.description)
    .bind(payload.cover_url)
    .bind(payload.author)
    .bind(payload.language_from)
    .bind(payload.language_to)
    .bind(serde_json::json!(payload.tags))
    .bind(payload.version)
    .bind(payload.created_at)
    .bind(payload.updated_at)
    .bind(is_system)
    .execute(pool)
    .await?;

    Ok(ImportedTarget {
        kind: "word_pack",
        id: payload.id,
    })
}

async fn import_favorite_vocabulary(
    pool: &PgPool,
    user_id: Uuid,
    payload: FavoriteVocabularyDto,
) -> Result<ImportedTarget, AppError> {
    ensure_default_word_pack(pool, user_id).await?;
    validate_required(
        &payload.id,
        "invalid_legacy_favorite_vocabulary",
        "favorite vocabulary id is required",
    )?;
    validate_required(
        &payload.word,
        "invalid_legacy_favorite_vocabulary",
        "favorite vocabulary word is required",
    )?;
    validate_required(
        &payload.meaning,
        "invalid_legacy_favorite_vocabulary",
        "favorite vocabulary meaning is required",
    )?;

    let mut tx = pool.begin().await?;
    let linked_learning_item_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT learning_item_id FROM favorite_vocabularies WHERE user_id = $1 AND id = $2",
    )
    .bind(user_id)
    .bind(&payload.id)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    sqlx::query(
        r#"
        INSERT INTO favorite_vocabularies (
            user_id, id, word, meaning, usage, explanation, example, reading, source_article_id,
            source_article_title, srs_state, ease_factor, repetitions, interval_days, due_date,
            last_reviewed_at, review_count, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
        ON CONFLICT (user_id, id) DO UPDATE
        SET word = EXCLUDED.word,
            meaning = EXCLUDED.meaning,
            usage = EXCLUDED.usage,
            explanation = EXCLUDED.explanation,
            example = EXCLUDED.example,
            reading = EXCLUDED.reading,
            source_article_id = EXCLUDED.source_article_id,
            source_article_title = EXCLUDED.source_article_title,
            srs_state = EXCLUDED.srs_state,
            ease_factor = EXCLUDED.ease_factor,
            repetitions = EXCLUDED.repetitions,
            interval_days = EXCLUDED.interval_days,
            due_date = EXCLUDED.due_date,
            last_reviewed_at = EXCLUDED.last_reviewed_at,
            review_count = EXCLUDED.review_count,
            created_at = EXCLUDED.created_at
        "#,
    )
    .bind(user_id)
    .bind(&payload.id)
    .bind(payload.word.trim())
    .bind(payload.meaning.trim())
    .bind(payload.usage.trim())
    .bind(&payload.explanation)
    .bind(&payload.example)
    .bind(&payload.reading)
    .bind(&payload.source_article_id)
    .bind(&payload.source_article_title)
    .bind(normalize_srs_state(&payload.srs_state))
    .bind(payload.ease_factor.max(1.3))
    .bind(payload.repetitions.max(0))
    .bind(payload.interval_days.max(0))
    .bind(&payload.due_date)
    .bind(&payload.last_reviewed_at)
    .bind(payload.review_count.max(0))
    .bind(&payload.created_at)
    .execute(&mut *tx)
    .await?;
    replace_pack_links(&mut tx, user_id, &payload.id, &payload.pack_ids).await?;
    if linked_learning_item_id.is_some() {
        learning_items::canonicalize_favorite_vocabulary_tx(
            &mut tx,
            user_id,
            &payload.id,
            &payload.word,
            &payload.meaning,
            payload.explanation.as_deref(),
            payload.example.as_deref(),
            payload.source_article_id.as_deref(),
            payload.source_article_title.as_deref(),
        )
        .await?;
    }
    tx.commit().await?;

    Ok(ImportedTarget {
        kind: "favorite_vocabulary",
        id: payload.id,
    })
}

async fn import_favorite_grammar(
    pool: &PgPool,
    user_id: Uuid,
    payload: FavoriteGrammarDto,
) -> Result<ImportedTarget, AppError> {
    validate_required(
        &payload.id,
        "invalid_legacy_favorite_grammar",
        "favorite grammar id is required",
    )?;
    validate_required(
        &payload.point,
        "invalid_legacy_favorite_grammar",
        "favorite grammar point is required",
    )?;
    validate_required(
        &payload.explanation,
        "invalid_legacy_favorite_grammar",
        "favorite grammar explanation is required",
    )?;

    let mut tx = pool.begin().await?;
    let linked_learning_item_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT learning_item_id FROM favorite_grammars WHERE user_id = $1 AND id = $2",
    )
    .bind(user_id)
    .bind(&payload.id)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    sqlx::query(
        r#"
        INSERT INTO favorite_grammars (
            user_id, id, point, explanation, example, source_article_id, source_article_title,
            created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (user_id, id) DO UPDATE
        SET point = EXCLUDED.point,
            explanation = EXCLUDED.explanation,
            example = EXCLUDED.example,
            source_article_id = EXCLUDED.source_article_id,
            source_article_title = EXCLUDED.source_article_title,
            created_at = EXCLUDED.created_at
        "#,
    )
    .bind(user_id)
    .bind(&payload.id)
    .bind(payload.point.trim())
    .bind(payload.explanation.trim())
    .bind(&payload.example)
    .bind(&payload.source_article_id)
    .bind(&payload.source_article_title)
    .bind(&payload.created_at)
    .execute(&mut *tx)
    .await?;
    if linked_learning_item_id.is_some() {
        learning_items::canonicalize_favorite_grammar_tx(
            &mut tx,
            user_id,
            &payload.id,
            &payload.point,
            &payload.explanation,
            payload.example.as_deref(),
            payload.source_article_id.as_deref(),
            payload.source_article_title.as_deref(),
        )
        .await?;
    }
    tx.commit().await?;

    Ok(ImportedTarget {
        kind: "favorite_grammar",
        id: payload.id,
    })
}

async fn import_bookmark(
    pool: &PgPool,
    user_id: Uuid,
    payload: BookmarkDto,
) -> Result<ImportedTarget, AppError> {
    validate_required(
        &payload.id,
        "invalid_legacy_bookmark",
        "bookmark id is required",
    )?;
    validate_required(
        &payload.book_path,
        "invalid_legacy_bookmark",
        "bookmark book path is required",
    )?;
    validate_required(
        &payload.title,
        "invalid_legacy_bookmark",
        "bookmark title is required",
    )?;

    sqlx::query(
        r#"
        INSERT INTO bookmarks (
            user_id, id, book_path, book_type, title, note, selected_text, page_number, epub_cfi,
            created_at, color
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (user_id, id) DO UPDATE
        SET book_path = EXCLUDED.book_path,
            book_type = EXCLUDED.book_type,
            title = EXCLUDED.title,
            note = EXCLUDED.note,
            selected_text = EXCLUDED.selected_text,
            page_number = EXCLUDED.page_number,
            epub_cfi = EXCLUDED.epub_cfi,
            created_at = EXCLUDED.created_at,
            color = EXCLUDED.color
        "#,
    )
    .bind(user_id)
    .bind(&payload.id)
    .bind(payload.book_path)
    .bind(payload.book_type)
    .bind(payload.title.trim())
    .bind(payload.note)
    .bind(payload.selected_text)
    .bind(payload.page_number)
    .bind(payload.epub_cfi)
    .bind(payload.created_at)
    .bind(payload.color)
    .execute(pool)
    .await?;

    Ok(ImportedTarget {
        kind: "bookmark",
        id: payload.id,
    })
}

async fn import_agent_task(
    pool: &PgPool,
    user_id: Uuid,
    payload: AgentTaskDto,
) -> Result<ImportedTarget, AppError> {
    upsert_agent_task(pool, user_id, &payload).await?;
    Ok(ImportedTarget {
        kind: "agent_task",
        id: payload.id,
    })
}

async fn import_agent_task_value(
    pool: &PgPool,
    user_id: Uuid,
    payload: Value,
) -> Result<ImportedTarget, AppError> {
    let task = normalize_agent_task_value(payload)?;
    import_agent_task(pool, user_id, task).await
}

async fn import_artifact(
    pool: &PgPool,
    user_id: Uuid,
    payload: ArtifactDto,
) -> Result<ImportedTarget, AppError> {
    ensure_agent_task_for_artifact(pool, user_id, &payload).await?;
    sqlx::query(
        r#"
        INSERT INTO artifacts (
            user_id, id, task_id, article_id, artifact_type, version, content, metadata,
            created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (user_id, id) DO UPDATE
        SET task_id = EXCLUDED.task_id,
            article_id = EXCLUDED.article_id,
            artifact_type = EXCLUDED.artifact_type,
            version = EXCLUDED.version,
            content = EXCLUDED.content,
            metadata = EXCLUDED.metadata,
            created_at = EXCLUDED.created_at,
            updated_at = EXCLUDED.updated_at
        "#,
    )
    .bind(user_id)
    .bind(&payload.id)
    .bind(&payload.task_id)
    .bind(&payload.article_id)
    .bind(payload.artifact_type)
    .bind(payload.version)
    .bind(payload.content)
    .bind(payload.metadata)
    .bind(payload.created_at)
    .bind(payload.updated_at)
    .execute(pool)
    .await?;

    Ok(ImportedTarget {
        kind: "artifact",
        id: payload.id,
    })
}

async fn import_artifact_value(
    pool: &PgPool,
    user_id: Uuid,
    payload: Value,
) -> Result<ImportedTarget, AppError> {
    let artifact = normalize_artifact_value(payload)?;
    import_artifact(pool, user_id, artifact).await
}

fn normalize_agent_task_value(mut payload: Value) -> Result<AgentTaskDto, AppError> {
    let now = Utc::now().to_rfc3339();
    let object = payload.as_object_mut().ok_or_else(|| {
        AppError::bad_request("invalid_legacy_agent_task", "agent task must be an object")
    })?;
    let article_id = object
        .get("article_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            object
                .get("input")
                .and_then(|input| input.get("article_id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .ok_or_else(|| {
            AppError::bad_request(
                "invalid_legacy_agent_task",
                "agent task article id is required",
            )
        })?;
    validate_material_reference_id(
        &article_id,
        "invalid_legacy_agent_task",
        "agent task article id",
    )?;

    object
        .entry("task_type")
        .or_insert_with(|| Value::String("mind_map_generate".to_string()));
    object
        .entry("status")
        .or_insert_with(|| Value::String("succeeded".to_string()));
    let status_is_succeeded = object.get("status").and_then(Value::as_str) == Some("succeeded");
    object
        .entry("article_id")
        .or_insert_with(|| Value::String(article_id.clone()));
    object.entry("progress").or_insert_with(|| {
        if status_is_succeeded {
            serde_json::json!(1.0)
        } else {
            serde_json::json!(0.0)
        }
    });
    object
        .entry("artifact_ids")
        .or_insert_with(|| Value::Array(Vec::new()));
    object
        .entry("created_at")
        .or_insert_with(|| Value::String(now.clone()));
    object
        .entry("updated_at")
        .or_insert_with(|| Value::String(now.clone()));

    let input = object
        .entry("input")
        .or_insert_with(|| Value::Object(Default::default()));
    let input_object = input.as_object_mut().ok_or_else(|| {
        AppError::bad_request(
            "invalid_legacy_agent_task",
            "agent task input must be an object",
        )
    })?;
    input_object
        .entry("article_id")
        .or_insert_with(|| Value::String(article_id));
    input_object
        .entry("display_language")
        .or_insert_with(|| Value::String("zh-CN".to_string()));
    input_object
        .entry("max_depth")
        .or_insert_with(|| serde_json::json!(0));
    input_object
        .entry("evidence_mode")
        .or_insert_with(|| Value::String("legacy_import".to_string()));
    input_object
        .entry("prefer_structure")
        .or_insert_with(|| Value::String("legacy_import".to_string()));

    serde_json::from_value(payload).map_err(|error| {
        AppError::bad_request(
            "invalid_legacy_agent_task",
            format!("failed to parse agent task: {error}"),
        )
    })
}

fn normalize_artifact_value(mut payload: Value) -> Result<ArtifactDto, AppError> {
    let now = Utc::now().to_rfc3339();
    let object = payload.as_object_mut().ok_or_else(|| {
        AppError::bad_request("invalid_legacy_artifact", "artifact must be an object")
    })?;
    let article_id = object
        .get("article_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::bad_request("invalid_legacy_artifact", "artifact article id is required")
        })?;
    validate_material_reference_id(article_id, "invalid_legacy_artifact", "artifact article id")?;
    object
        .entry("artifact_type")
        .or_insert_with(|| Value::String("mind_map".to_string()));
    object
        .entry("version")
        .or_insert_with(|| Value::String("1".to_string()));
    object
        .entry("content")
        .or_insert_with(|| Value::Object(Default::default()));
    object
        .entry("created_at")
        .or_insert_with(|| Value::String(now.clone()));
    object
        .entry("updated_at")
        .or_insert_with(|| Value::String(now));

    serde_json::from_value(payload).map_err(|error| {
        AppError::bad_request(
            "invalid_legacy_artifact",
            format!("failed to parse artifact: {error}"),
        )
    })
}

fn validate_material_reference_id(
    value: &str,
    code: &'static str,
    label: &'static str,
) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::bad_request(
            code,
            format!("{label} must not be empty"),
        ));
    }
    if Uuid::parse_str(value).is_err() {
        return Err(AppError::bad_request(
            code,
            format!("{label} must reference an imported material UUID"),
        ));
    }
    Ok(())
}

async fn upsert_agent_task(
    pool: &PgPool,
    user_id: Uuid,
    payload: &AgentTaskDto,
) -> Result<(), AppError> {
    validate_required(
        &payload.id,
        "invalid_legacy_agent_task",
        "agent task id is required",
    )?;
    validate_required(
        &payload.article_id,
        "invalid_legacy_agent_task",
        "agent task article id is required",
    )?;
    if !payload.input.is_object() {
        return Err(AppError::bad_request(
            "invalid_legacy_agent_task",
            "agent task input must be an object",
        ));
    }

    sqlx::query(
        r#"
        INSERT INTO agent_tasks (
            user_id, id, task_type, status, article_id, input, progress, stage, message, error,
            worker_session_id, artifact_ids, created_at, updated_at, started_at, finished_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
        ON CONFLICT (user_id, id) DO UPDATE
        SET task_type = EXCLUDED.task_type,
            status = EXCLUDED.status,
            article_id = EXCLUDED.article_id,
            input = EXCLUDED.input,
            progress = EXCLUDED.progress,
            stage = EXCLUDED.stage,
            message = EXCLUDED.message,
            error = EXCLUDED.error,
            worker_session_id = EXCLUDED.worker_session_id,
            artifact_ids = (
                SELECT COALESCE(jsonb_agg(value ORDER BY value), '[]'::jsonb)
                FROM (
                    SELECT DISTINCT value
                    FROM jsonb_array_elements_text(agent_tasks.artifact_ids || EXCLUDED.artifact_ids) AS merged(value)
                ) merged_values
            ),
            updated_at = EXCLUDED.updated_at,
            started_at = COALESCE(agent_tasks.started_at, EXCLUDED.started_at),
            finished_at = EXCLUDED.finished_at
        "#,
    )
    .bind(user_id)
    .bind(&payload.id)
    .bind(&payload.task_type)
    .bind(&payload.status)
    .bind(&payload.article_id)
    .bind(&payload.input)
    .bind(payload.progress.clamp(0.0, 1.0))
    .bind(&payload.stage)
    .bind(&payload.message)
    .bind(&payload.error)
    .bind(&payload.worker_session_id)
    .bind(serde_json::json!(payload.artifact_ids))
    .bind(&payload.created_at)
    .bind(&payload.updated_at)
    .bind(&payload.started_at)
    .bind(&payload.finished_at)
    .execute(pool)
    .await?;

    Ok(())
}

async fn ensure_agent_task_for_artifact(
    pool: &PgPool,
    user_id: Uuid,
    artifact: &ArtifactDto,
) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM agent_tasks WHERE user_id = $1 AND id = $2",
    )
    .bind(user_id)
    .bind(&artifact.task_id)
    .fetch_one(pool)
    .await?;
    if exists > 0 {
        return Ok(());
    }

    let input = serde_json::json!({
        "article_id": artifact.article_id,
        "display_language": "zh-CN",
        "max_depth": 0,
        "evidence_mode": "legacy_import",
        "prefer_structure": "legacy_import"
    });
    let task = AgentTaskDto {
        id: artifact.task_id.clone(),
        task_type: "mind_map_generate".to_string(),
        status: "succeeded".to_string(),
        article_id: artifact.article_id.clone(),
        input,
        progress: 1.0,
        stage: Some("legacy_import".to_string()),
        message: Some("Synthetic task created for legacy artifact import".to_string()),
        error: None,
        worker_session_id: None,
        artifact_ids: vec![artifact.id.clone()],
        created_at: artifact.created_at.clone(),
        updated_at: artifact.updated_at.clone(),
        started_at: Some(artifact.created_at.clone()),
        finished_at: Some(artifact.updated_at.clone()),
    };
    upsert_agent_task(pool, user_id, &task).await
}

async fn ensure_default_word_pack(pool: &PgPool, user_id: Uuid) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let default_pack = WordPackDto {
        id: DEFAULT_UNGROUPED_PACK_ID.to_string(),
        name: DEFAULT_UNGROUPED_PACK_NAME.to_string(),
        description: Some("系统默认合集".to_string()),
        cover_url: None,
        author: Some("OpenKoto".to_string()),
        language_from: None,
        language_to: None,
        tags: vec!["system".to_string()],
        version: Some("1.0.0".to_string()),
        created_at: now.clone(),
        updated_at: now,
        is_system: true,
    };
    import_word_pack(pool, user_id, default_pack).await?;
    Ok(())
}

async fn replace_pack_links(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    vocabulary_id: &str,
    pack_ids: &[String],
) -> Result<(), AppError> {
    let existing_pack_ids =
        sqlx::query_scalar::<_, String>("SELECT id FROM word_packs WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&mut **tx)
            .await?;

    let mut seen = std::collections::HashSet::new();
    let mut cleaned = pack_ids
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect::<Vec<_>>();
    let missing_pack_ids = cleaned
        .iter()
        .filter(|value| !existing_pack_ids.iter().any(|id| id == *value))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_pack_ids.is_empty() {
        return Err(AppError::bad_request(
            "missing_legacy_word_pack",
            format!("word pack not found: {}", missing_pack_ids.join(", ")),
        ));
    }
    if cleaned.is_empty() {
        cleaned.push(DEFAULT_UNGROUPED_PACK_ID.to_string());
    }

    sqlx::query("DELETE FROM favorite_vocabulary_packs WHERE user_id = $1 AND vocabulary_id = $2")
        .bind(user_id)
        .bind(vocabulary_id)
        .execute(&mut **tx)
        .await?;

    for pack_id in cleaned {
        sqlx::query(
            r#"
            INSERT INTO favorite_vocabulary_packs (user_id, vocabulary_id, pack_id)
            VALUES ($1, $2, $3)
            ON CONFLICT DO NOTHING
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

async fn insert_legacy_segments(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    material_id: Uuid,
    segments: Vec<MaterialSegmentInput>,
) -> Result<(), AppError> {
    for (index, segment) in segments.into_iter().enumerate() {
        let text = segment.text.trim().to_string();
        if text.is_empty() {
            continue;
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
        .bind(index as i32)
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

fn fallback_segments_from_content(content: &str) -> Vec<MaterialSegmentInput> {
    let text = content.trim();
    if text.is_empty() {
        return Vec::new();
    }

    vec![MaterialSegmentInput {
        id: Some(Uuid::new_v4().to_string()),
        order: Some(0),
        text: text.to_string(),
        reading_text: None,
        translation: None,
        explanation: None,
        start_time: None,
        end_time: None,
        is_new_paragraph: Some(true),
    }]
}

fn normalize_srs_state(value: &str) -> String {
    match value {
        "new" | "learning" | "review" => value.to_string(),
        _ => "new".to_string(),
    }
}

fn validate_required(
    value: &str,
    code: &'static str,
    message: &'static str,
) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::bad_request(code, message));
    }

    Ok(())
}
