use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    auth::{ApiJson, AuthenticatedUser},
    error::AppError,
    material_library::content_sha256_hex,
    routes::AppState,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentBlockInput {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default = "default_block_type")]
    pub block_type: String,
    #[serde(default)]
    pub block_order: Option<i32>,
    pub text: String,
    #[serde(default = "empty_object")]
    pub attrs: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEditPreviewRequest {
    pub base_revision: i64,
    pub blocks: Vec<DocumentBlockInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEditCommitRequest {
    pub base_revision: i64,
    pub client_request_id: String,
    #[serde(default)]
    pub preview_token: Option<String>,
    pub blocks: Vec<DocumentBlockInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRestoreRequest {
    pub base_revision: i64,
    pub client_request_id: String,
    #[serde(default)]
    pub preserve_draft: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSegmentDto {
    pub id: String,
    pub order: i32,
    pub block_segment_order: i32,
    pub text: String,
    #[serde(default)]
    pub reading_text: Option<String>,
    #[serde(default)]
    pub translation: Option<String>,
    #[serde(default)]
    pub explanation: Option<Value>,
    pub text_sha256: String,
    pub reading_status: String,
    pub translation_status: String,
    pub explanation_status: String,
    pub is_new_paragraph: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentBlockDto {
    pub id: String,
    pub block_type: String,
    pub block_order: i32,
    pub text: String,
    pub attrs: Value,
    pub segments: Vec<DocumentSegmentDto>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DerivedSummaryDto {
    pub segment_count: i64,
    pub current_readings: i64,
    pub stale_readings: i64,
    pub current_translations: i64,
    pub stale_translations: i64,
    pub current_explanations: i64,
    pub stale_explanations: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialDocumentDto {
    pub material_id: String,
    pub title: String,
    pub current_revision: i64,
    #[serde(default)]
    pub content_sha256: Option<String>,
    pub blocks: Vec<DocumentBlockDto>,
    pub derived_summary: DerivedSummaryDto,
    #[serde(default)]
    pub migration_report: Option<DocumentMigrationReportDto>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationReanchorSummaryDto {
    pub exact: i64,
    pub text: i64,
    pub ambiguous: i64,
    pub orphaned: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMigrationReportDto {
    pub source_authority: String,
    #[serde(default)]
    pub legacy_content_sha256: Option<String>,
    #[serde(default)]
    pub legacy_segments_sha256: Option<String>,
    #[serde(default)]
    pub canonical_content_sha256: Option<String>,
    pub content_mismatch: bool,
    #[serde(default)]
    pub legacy_content_backup: Option<String>,
    pub legacy_segment_count: i32,
    pub generated_block_count: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentEditImpactDto {
    pub inserted_blocks: i64,
    pub updated_blocks: i64,
    pub deleted_blocks: i64,
    pub moved_blocks: i64,
    pub changed_segments: i64,
    pub deleted_segments: i64,
    pub stale_readings: i64,
    pub stale_translations: i64,
    pub stale_explanations: i64,
    pub affected_annotations: i64,
    pub affected_learning_items: i64,
    pub annotation_reanchors: AnnotationReanchorSummaryDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEditPreviewResponse {
    pub base_revision: i64,
    pub next_revision: i64,
    #[serde(default)]
    pub content_sha256: Option<String>,
    pub impact: DocumentEditImpactDto,
    pub preview_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentEditCommitResponse {
    pub document: MaterialDocumentDto,
    pub impact: DocumentEditImpactDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRevisionSummaryDto {
    pub revision: i64,
    #[serde(default)]
    pub parent_revision: Option<i64>,
    pub action: String,
    #[serde(default)]
    pub content_sha256: Option<String>,
    pub change_summary: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRevisionDto {
    pub revision: i64,
    #[serde(default)]
    pub parent_revision: Option<i64>,
    pub action: String,
    #[serde(default)]
    pub content_sha256: Option<String>,
    pub snapshot: Value,
    pub change_summary: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentDraftRequest {
    pub base_revision: i64,
    pub blocks: Vec<DocumentBlockInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentDraftDto {
    pub material_id: String,
    pub base_revision: i64,
    pub blocks: Vec<DocumentBlockInput>,
    pub updated_at: String,
    pub is_stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentDerivedUpdate {
    pub segment_id: String,
    #[serde(default)]
    pub expected_text_sha256: Option<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub reading_text: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub translation: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    pub explanation: Option<Option<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentDerivedBatchRequest {
    pub updates: Vec<SegmentDerivedUpdate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditableDerivativeRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditableDerivativeResponse {
    pub source_material_id: String,
    pub derivative_material_id: String,
    pub created: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct MaterialDocumentRecord {
    id: Uuid,
    title: String,
    content: String,
    current_revision: i64,
    content_sha256: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct BlockRecord {
    id: Uuid,
    block_order: i32,
    block_type: String,
    attrs: Value,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct SegmentRecord {
    id: Uuid,
    block_id: Option<Uuid>,
    segment_order: i32,
    block_segment_order: Option<i32>,
    text: String,
    reading_text: Option<String>,
    translation: Option<String>,
    explanation: Option<Value>,
    text_sha256: Option<String>,
    reading_source_sha256: Option<String>,
    translation_source_sha256: Option<String>,
    explanation_source_sha256: Option<String>,
    is_new_paragraph: bool,
}

#[derive(Debug, Clone)]
struct PlannedBlock {
    id: Uuid,
    block_type: String,
    block_order: i32,
    attrs: Value,
    segments: Vec<PlannedSegment>,
}

#[derive(Debug, Clone)]
struct PlannedSegment {
    id: Uuid,
    order: i32,
    block_segment_order: i32,
    text: String,
    text_sha256: String,
    reading_text: Option<String>,
    translation: Option<String>,
    explanation: Option<Value>,
    reading_source_sha256: Option<String>,
    translation_source_sha256: Option<String>,
    explanation_source_sha256: Option<String>,
}

#[derive(Debug)]
struct PlannedDocument {
    blocks: Vec<PlannedBlock>,
    content: String,
    content_sha256: Option<String>,
    impact: DocumentEditImpactDto,
    lineage: Vec<(Option<Uuid>, Option<Uuid>, String)>,
}

fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

fn default_block_type() -> String {
    "paragraph".to_string()
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

pub(crate) fn source_text_sha256(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

fn normalized_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonicalize_json).collect()),
        Value::Object(values) => {
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort();
            let mut canonical = serde_json::Map::new();
            for key in keys {
                canonical.insert(key.clone(), canonicalize_json(&values[key]));
            }
            Value::Object(canonical)
        }
        _ => value.clone(),
    }
}

fn preview_token(
    user_id: Uuid,
    material_id: Uuid,
    base_revision: i64,
    current_content_sha256: Option<&str>,
    blocks: &[DocumentBlockInput],
    impact: &DocumentEditImpactDto,
) -> Result<String, AppError> {
    let blocks = blocks
        .iter()
        .enumerate()
        .map(|(order, block)| {
            json!({
                "id": block.id,
                "block_type": block.block_type.trim(),
                "block_order": order,
                "text": block.text.trim(),
                "attrs": canonicalize_json(&block.attrs),
            })
        })
        .collect::<Vec<_>>();
    let payload = json!({
        "version": 1,
        "user_id": user_id,
        "material_id": material_id,
        "base_revision": base_revision,
        "current_content_sha256": current_content_sha256,
        "blocks": blocks,
        "impact": impact,
    });
    let bytes = serde_json::to_vec(&canonicalize_json(&payload)).map_err(|_| {
        AppError::internal("document_preview_error", "could not serialize edit preview")
    })?;
    Ok(format!(
        "preview-v1:{}",
        source_text_sha256(&String::from_utf8_lossy(&bytes))
    ))
}

fn char_slice_matches(text: &str, start: usize, end: usize, expected: &str) -> bool {
    if start > end {
        return false;
    }
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect::<String>()
        == expected
}

fn annotation_quote(locator: &Value, source_text: &str) -> String {
    locator
        .pointer("/quote/exact")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(source_text)
        .to_string()
}

fn classify_annotation_reanchor(
    segment_id: Option<Uuid>,
    locator: &Value,
    source_text: &str,
    planned_segments: &HashMap<Uuid, String>,
) -> &'static str {
    let quote = annotation_quote(locator, source_text);
    if let Some(text) = segment_id.and_then(|id| planned_segments.get(&id)) {
        let start = locator.get("start_offset").and_then(Value::as_u64);
        let end = locator.get("end_offset").and_then(Value::as_u64);
        if let (Some(start), Some(end)) = (start, end) {
            if char_slice_matches(text, start as usize, end as usize, &quote) {
                return "exact";
            }
        }
    }
    if quote.is_empty() {
        return "orphaned";
    }
    let matches = planned_segments
        .values()
        .map(|text| text.match_indices(&quote).count())
        .sum::<usize>();
    match matches {
        0 => "orphaned",
        1 => "text",
        _ => "ambiguous",
    }
}

fn derived_status<T>(value: &Option<T>, source_hash: &Option<String>, text_hash: &str) -> String {
    if value.is_none() {
        "missing".to_string()
    } else if source_hash.as_deref() == Some(text_hash) {
        "current".to_string()
    } else {
        "stale".to_string()
    }
}

fn validate_block_type(value: &str) -> Result<(), AppError> {
    if matches!(
        value,
        "paragraph" | "heading" | "list_item" | "quote" | "divider"
    ) {
        Ok(())
    } else {
        Err(AppError::bad_request(
            "invalid_document_block",
            "block_type must be paragraph, heading, list_item, quote, or divider",
        ))
    }
}

fn validate_blocks(blocks: &[DocumentBlockInput]) -> Result<(), AppError> {
    let mut ids = HashSet::new();
    for block in blocks {
        validate_block_type(block.block_type.trim())?;
        if block.block_type != "divider" && block.text.trim().is_empty() {
            return Err(AppError::bad_request(
                "invalid_document_block",
                "block text must not be empty",
            ));
        }
        if !block.attrs.is_object() {
            return Err(AppError::bad_request(
                "invalid_document_block",
                "block attrs must be an object",
            ));
        }
        if let Some(id) = block.id.as_deref() {
            let id = Uuid::parse_str(id).map_err(|_| {
                AppError::bad_request("invalid_document_block", "block id must be a UUID")
            })?;
            if !ids.insert(id) {
                return Err(AppError::bad_request(
                    "invalid_document_block",
                    "block ids must be unique",
                ));
            }
        }
    }
    Ok(())
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.trim().chars().collect();
    for (index, value) in chars.iter().enumerate() {
        current.push(*value);
        let boundary = matches!(*value, '.' | '?' | '!' | '。' | '？' | '！')
            && chars.get(index + 1).is_none_or(|next| next.is_whitespace());
        if boundary {
            let sentence = current.trim().to_string();
            if !sentence.is_empty() {
                values.push(sentence);
            }
            current.clear();
        }
    }
    let remaining = current.trim();
    if !remaining.is_empty() {
        values.push(remaining.to_string());
    }
    if values.is_empty() && !text.trim().is_empty() {
        values.push(text.trim().to_string());
    }
    values
}

fn block_text(segments: &[DocumentSegmentDto]) -> String {
    segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn canonical_content(blocks: &[DocumentBlockInput]) -> String {
    blocks
        .iter()
        .map(|block| block.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn canonical_content_from_segments(segments: &[SegmentRecord]) -> String {
    let mut paragraphs = Vec::<String>::new();
    let mut current = Vec::<String>::new();
    for (index, segment) in segments.iter().enumerate() {
        if (index == 0 || segment.is_new_paragraph) && !current.is_empty() {
            paragraphs.push(current.join(" "));
            current.clear();
        }
        let text = segment.text.trim();
        if !text.is_empty() {
            current.push(text.to_string());
        }
    }
    if !current.is_empty() {
        paragraphs.push(current.join(" "));
    }
    paragraphs.join("\n\n")
}

async fn lock_material(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    material_id: Uuid,
) -> Result<MaterialDocumentRecord, AppError> {
    sqlx::query_as::<_, MaterialDocumentRecord>(
        r#"
        SELECT id, title, content, current_revision, content_sha256
        FROM materials
        WHERE user_id = $1 AND id = $2
        FOR UPDATE
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::not_found("material_not_found", "material not found"))
}

pub(crate) async fn initialize_document(
    pool: &PgPool,
    user_id: Uuid,
    material_id: Uuid,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    let material = lock_material(&mut tx, user_id, material_id).await?;
    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM material_blocks WHERE user_id = $1 AND material_id = $2",
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_one(&mut *tx)
    .await?;
    let legacy_segments = fetch_segments_tx(&mut tx, user_id, material_id, false).await?;
    let legacy_segment_count = legacy_segments.len() as i32;
    let source_authority = if legacy_segments.is_empty() {
        "content"
    } else {
        "segments"
    };
    let legacy_segments_content = canonical_content_from_segments(&legacy_segments);
    let legacy_content_sha256 = content_sha256_hex(&material.content);
    let legacy_segments_sha256 = content_sha256_hex(&legacy_segments_content);

    if count == 0 {
        let existing = &legacy_segments;
        if existing.is_empty() {
            let paragraphs = material
                .content
                .lines()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            let mut global_order = 0;
            for (block_order, paragraph) in paragraphs.into_iter().enumerate() {
                let block_id = Uuid::new_v4();
                insert_block(
                    &mut tx,
                    user_id,
                    material_id,
                    block_id,
                    block_order as i32,
                    "paragraph",
                    &empty_object(),
                )
                .await?;
                for (position, sentence) in split_sentences(&paragraph).into_iter().enumerate() {
                    insert_new_segment(
                        &mut tx,
                        user_id,
                        material_id,
                        block_id,
                        Uuid::new_v4(),
                        global_order,
                        position as i32,
                        &sentence,
                    )
                    .await?;
                    global_order += 1;
                }
            }
        } else {
            let mut block_id = None;
            let mut block_order = -1;
            let mut block_segment_order = 0;
            for (index, segment) in existing.iter().enumerate() {
                if segment.is_new_paragraph || index == 0 {
                    block_order += 1;
                    block_segment_order = 0;
                    let created = Uuid::new_v4();
                    insert_block(
                        &mut tx,
                        user_id,
                        material_id,
                        created,
                        block_order,
                        "paragraph",
                        &empty_object(),
                    )
                    .await?;
                    block_id = Some(created);
                }
                let text_hash = source_text_sha256(&segment.text);
                sqlx::query(
                    r#"
                    UPDATE material_segments
                    SET block_id = $4, block_segment_order = $5, text_sha256 = $6,
                        reading_source_sha256 = CASE WHEN reading_text IS NULL THEN NULL ELSE COALESCE(reading_source_sha256, $6) END,
                        translation_source_sha256 = CASE WHEN translation IS NULL THEN NULL ELSE COALESCE(translation_source_sha256, $6) END,
                        explanation_source_sha256 = CASE WHEN explanation IS NULL THEN NULL ELSE COALESCE(explanation_source_sha256, $6) END,
                        updated_at = NOW()
                    WHERE user_id = $1 AND material_id = $2 AND id = $3
                    "#,
                )
                .bind(user_id)
                .bind(material_id)
                .bind(segment.id)
                .bind(block_id.expect("block initialized"))
                .bind(block_segment_order)
                .bind(text_hash)
                .execute(&mut *tx)
                .await?;
                block_segment_order += 1;
            }
        }
    }

    let initialized_segments = fetch_segments_tx(&mut tx, user_id, material_id, false).await?;
    let canonical = canonical_content_from_segments(&initialized_segments);
    let canonical_sha256 = content_sha256_hex(&canonical);
    let content_mismatch = legacy_content_sha256 != canonical_sha256;
    let canonicalization_applied = count == 0 && source_authority == "segments" && content_mismatch;
    if canonicalization_applied {
        sqlx::query(
            r#"
            UPDATE materials
            SET content = $3, content_sha256 = $4, content_updated_at = NOW(), updated_at = NOW()
            WHERE user_id = $1 AND id = $2
            "#,
        )
        .bind(user_id)
        .bind(material_id)
        .bind(&canonical)
        .bind(&canonical_sha256)
        .execute(&mut *tx)
        .await?;
    }
    let generated_block_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM material_blocks WHERE user_id = $1 AND material_id = $2 AND deleted_at IS NULL",
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_one(&mut *tx)
    .await? as i32;
    sqlx::query(
        r#"
        INSERT INTO material_document_migration_reports (
            user_id, material_id, source_authority, legacy_content_sha256,
            legacy_segments_sha256, canonical_content_sha256, content_mismatch,
            legacy_content_backup, legacy_segment_count, generated_block_count, details
        ) VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
            jsonb_build_object(
                'canonicalization_applied', $11::BOOLEAN,
                'had_preexisting_blocks', $12::BOOLEAN
            )
        )
        ON CONFLICT (user_id, material_id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .bind(source_authority)
    .bind(&legacy_content_sha256)
    .bind(&legacy_segments_sha256)
    .bind(&canonical_sha256)
    .bind(content_mismatch)
    .bind(content_mismatch.then_some(material.content.as_str()))
    .bind(legacy_segment_count)
    .bind(generated_block_count)
    .bind(canonicalization_applied)
    .bind(count > 0)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE learning_items li
        SET source_segment_sha256 = ms.text_sha256
        FROM material_segments ms
        WHERE li.user_id = $1 AND li.material_id = $2
          AND li.user_id = ms.user_id AND li.segment_id = ms.id
          AND li.source_segment_sha256 IS NULL AND ms.text_sha256 IS NOT NULL
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    let document = fetch_document(pool, user_id, material_id).await?;
    let snapshot = serde_json::to_value(&document).map_err(|_| {
        AppError::internal("document_snapshot_error", "could not serialize document")
    })?;
    sqlx::query(
        r#"
        INSERT INTO material_revisions (
            id, user_id, material_id, revision, parent_revision, action,
            content_sha256, snapshot, change_summary
        )
        VALUES ($1, $2, $3, $4, NULL, 'migration', $5, $6, '{}'::jsonb)
        ON CONFLICT (user_id, material_id, revision) DO NOTHING
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(material_id)
    .bind(document.current_revision)
    .bind(&document.content_sha256)
    .bind(snapshot)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn initialize_pending_documents(pool: &PgPool) -> Result<u64, AppError> {
    let pending = sqlx::query_as::<_, (Uuid, Uuid)>(
        r#"
        SELECT m.user_id, m.id
        FROM materials m
        WHERE NOT EXISTS (
            SELECT 1
            FROM material_revisions mr
            WHERE mr.user_id = m.user_id
              AND mr.material_id = m.id
              AND mr.revision = m.current_revision
        ) OR NOT EXISTS (
            SELECT 1
            FROM material_document_migration_reports report
            WHERE report.user_id = m.user_id
              AND report.material_id = m.id
        )
        ORDER BY m.created_at, m.id
        "#,
    )
    .fetch_all(pool)
    .await?;
    let mut initialized = 0_u64;
    for (user_id, material_id) in pending {
        initialize_document(pool, user_id, material_id).await?;
        initialized += 1;
    }
    Ok(initialized)
}

async fn insert_block(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    material_id: Uuid,
    id: Uuid,
    order: i32,
    block_type: &str,
    attrs: &Value,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO material_blocks (id, user_id, material_id, block_order, block_type, attrs)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(material_id)
    .bind(order)
    .bind(block_type)
    .bind(attrs)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_new_segment(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    material_id: Uuid,
    block_id: Uuid,
    id: Uuid,
    order: i32,
    block_order: i32,
    text: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO material_segments (
            id, user_id, material_id, block_id, segment_order, block_segment_order,
            text, text_sha256, is_new_paragraph
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(material_id)
    .bind(block_id)
    .bind(order)
    .bind(block_order)
    .bind(text)
    .bind(source_text_sha256(text))
    .bind(block_order == 0)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn fetch_blocks(
    pool: &PgPool,
    user_id: Uuid,
    material_id: Uuid,
    include_deleted: bool,
) -> Result<Vec<BlockRecord>, AppError> {
    let deleted_clause = if include_deleted {
        ""
    } else {
        " AND deleted_at IS NULL"
    };
    sqlx::query_as::<_, BlockRecord>(&format!(
        "SELECT id, block_order, block_type, attrs FROM material_blocks WHERE user_id = $1 AND material_id = $2{deleted_clause} ORDER BY block_order"
    ))
    .bind(user_id)
    .bind(material_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

async fn fetch_segments_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    material_id: Uuid,
    include_deleted: bool,
) -> Result<Vec<SegmentRecord>, AppError> {
    let deleted_clause = if include_deleted {
        ""
    } else {
        " AND deleted_at IS NULL"
    };
    sqlx::query_as::<_, SegmentRecord>(&format!(
        r#"
        SELECT id, block_id, segment_order, block_segment_order, text, reading_text,
               translation, explanation, text_sha256, reading_source_sha256,
               translation_source_sha256, explanation_source_sha256,
               is_new_paragraph
        FROM material_segments
        WHERE user_id = $1 AND material_id = $2{deleted_clause}
        ORDER BY segment_order
        "#
    ))
    .bind(user_id)
    .bind(material_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(AppError::from)
}

async fn fetch_segments(
    pool: &PgPool,
    user_id: Uuid,
    material_id: Uuid,
    include_deleted: bool,
) -> Result<Vec<SegmentRecord>, AppError> {
    let deleted_clause = if include_deleted {
        ""
    } else {
        " AND deleted_at IS NULL"
    };
    sqlx::query_as::<_, SegmentRecord>(&format!(
        r#"
        SELECT id, block_id, segment_order, block_segment_order, text, reading_text,
               translation, explanation, text_sha256, reading_source_sha256,
               translation_source_sha256, explanation_source_sha256,
               is_new_paragraph
        FROM material_segments
        WHERE user_id = $1 AND material_id = $2{deleted_clause}
        ORDER BY segment_order
        "#
    ))
    .bind(user_id)
    .bind(material_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

type MigrationReportRow = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
    Option<String>,
    i32,
    i32,
    DateTime<Utc>,
);

fn migration_report_from_row(row: MigrationReportRow) -> DocumentMigrationReportDto {
    DocumentMigrationReportDto {
        source_authority: row.0,
        legacy_content_sha256: row.1,
        legacy_segments_sha256: row.2,
        canonical_content_sha256: row.3,
        content_mismatch: row.4,
        legacy_content_backup: row.5,
        legacy_segment_count: row.6,
        generated_block_count: row.7,
        created_at: row.8.to_rfc3339(),
    }
}

fn assemble_document(
    material: MaterialDocumentRecord,
    migration_report: Option<DocumentMigrationReportDto>,
    blocks: Vec<BlockRecord>,
    segments: Vec<SegmentRecord>,
) -> MaterialDocumentDto {
    let mut grouped = HashMap::<Uuid, Vec<DocumentSegmentDto>>::new();
    let mut summary = DerivedSummaryDto::default();
    for segment in segments {
        let Some(block_id) = segment.block_id else {
            continue;
        };
        let text_hash = segment
            .text_sha256
            .unwrap_or_else(|| source_text_sha256(&segment.text));
        let reading_status = derived_status(
            &segment.reading_text,
            &segment.reading_source_sha256,
            &text_hash,
        );
        let translation_status = derived_status(
            &segment.translation,
            &segment.translation_source_sha256,
            &text_hash,
        );
        let explanation_status = derived_status(
            &segment.explanation,
            &segment.explanation_source_sha256,
            &text_hash,
        );
        summary.segment_count += 1;
        match reading_status.as_str() {
            "current" => summary.current_readings += 1,
            "stale" => summary.stale_readings += 1,
            _ => {}
        }
        match translation_status.as_str() {
            "current" => summary.current_translations += 1,
            "stale" => summary.stale_translations += 1,
            _ => {}
        }
        match explanation_status.as_str() {
            "current" => summary.current_explanations += 1,
            "stale" => summary.stale_explanations += 1,
            _ => {}
        }
        grouped
            .entry(block_id)
            .or_default()
            .push(DocumentSegmentDto {
                id: segment.id.to_string(),
                order: segment.segment_order,
                block_segment_order: segment.block_segment_order.unwrap_or(0),
                text: segment.text,
                reading_text: segment.reading_text,
                translation: segment.translation,
                explanation: segment.explanation,
                text_sha256: text_hash,
                reading_status,
                translation_status,
                explanation_status,
                is_new_paragraph: segment.is_new_paragraph,
            });
    }
    let blocks = blocks
        .into_iter()
        .map(|block| {
            let mut segments = grouped.remove(&block.id).unwrap_or_default();
            segments.sort_by_key(|segment| segment.block_segment_order);
            DocumentBlockDto {
                id: block.id.to_string(),
                block_type: block.block_type,
                block_order: block.block_order,
                text: block_text(&segments),
                attrs: block.attrs,
                segments,
            }
        })
        .collect();
    MaterialDocumentDto {
        material_id: material.id.to_string(),
        title: material.title,
        current_revision: material.current_revision,
        content_sha256: material.content_sha256,
        blocks,
        derived_summary: summary,
        migration_report,
    }
}

async fn fetch_document(
    pool: &PgPool,
    user_id: Uuid,
    material_id: Uuid,
) -> Result<MaterialDocumentDto, AppError> {
    let material = sqlx::query_as::<_, MaterialDocumentRecord>(
        "SELECT id, title, content, current_revision, content_sha256 FROM materials WHERE user_id = $1 AND id = $2",
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::not_found("material_not_found", "material not found"))?;
    let migration_report = sqlx::query_as::<_, MigrationReportRow>(
        r#"
        SELECT source_authority, legacy_content_sha256, legacy_segments_sha256,
               canonical_content_sha256, content_mismatch, legacy_content_backup,
               legacy_segment_count, generated_block_count, created_at
        FROM material_document_migration_reports
        WHERE user_id = $1 AND material_id = $2
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_optional(pool)
    .await?
    .map(migration_report_from_row);
    let blocks = fetch_blocks(pool, user_id, material_id, false).await?;
    let segments = fetch_segments(pool, user_id, material_id, false).await?;
    Ok(assemble_document(
        material,
        migration_report,
        blocks,
        segments,
    ))
}

async fn fetch_document_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    material: MaterialDocumentRecord,
) -> Result<MaterialDocumentDto, AppError> {
    let material_id = material.id;
    let migration_report = sqlx::query_as::<_, MigrationReportRow>(
        r#"
        SELECT source_authority, legacy_content_sha256, legacy_segments_sha256,
               canonical_content_sha256, content_mismatch, legacy_content_backup,
               legacy_segment_count, generated_block_count, created_at
        FROM material_document_migration_reports
        WHERE user_id = $1 AND material_id = $2
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_optional(&mut **tx)
    .await?
    .map(migration_report_from_row);
    let blocks = sqlx::query_as::<_, BlockRecord>(
        r#"
        SELECT id, block_order, block_type, attrs
        FROM material_blocks
        WHERE user_id = $1 AND material_id = $2 AND deleted_at IS NULL
        ORDER BY block_order
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_all(&mut **tx)
    .await?;
    let segments = fetch_segments_tx(tx, user_id, material_id, false).await?;
    Ok(assemble_document(
        material,
        migration_report,
        blocks,
        segments,
    ))
}

fn document_as_inputs(document: &MaterialDocumentDto) -> Vec<DocumentBlockInput> {
    document
        .blocks
        .iter()
        .map(|block| DocumentBlockInput {
            id: Some(block.id.clone()),
            block_type: block.block_type.clone(),
            block_order: Some(block.block_order),
            text: block.text.clone(),
            attrs: block.attrs.clone(),
        })
        .collect()
}

async fn plan_document(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    material_id: Uuid,
    current: &MaterialDocumentDto,
    blocks: &[DocumentBlockInput],
) -> Result<PlannedDocument, AppError> {
    validate_blocks(blocks)?;
    let current_blocks = current
        .blocks
        .iter()
        .map(|block| {
            (
                Uuid::parse_str(&block.id).expect("stored block UUID"),
                block,
            )
        })
        .collect::<HashMap<_, _>>();
    let current_segments = current
        .blocks
        .iter()
        .flat_map(|block| block.segments.iter())
        .collect::<Vec<_>>();
    let mut remaining_incoming_texts = HashMap::<String, usize>::new();
    for block in blocks {
        for sentence in split_sentences(&block.text) {
            *remaining_incoming_texts
                .entry(normalized_text(&sentence))
                .or_default() += 1;
        }
    }
    let mut planned = Vec::new();
    let mut incoming_block_ids = HashSet::new();
    let mut incoming_segment_ids = HashSet::new();
    let mut touched_segment_ids = HashSet::new();
    let mut lineage = Vec::new();
    let mut impact = DocumentEditImpactDto::default();
    let mut global_order = 0;

    for (block_index, input) in blocks.iter().enumerate() {
        let id = input
            .id
            .as_deref()
            .map(Uuid::parse_str)
            .transpose()
            .map_err(|_| {
                AppError::bad_request("invalid_document_block", "block id must be a UUID")
            })?
            .unwrap_or_else(Uuid::new_v4);
        incoming_block_ids.insert(id);
        let existing = current_blocks.get(&id).copied();
        if let Some(existing) = existing {
            if normalized_text(&existing.text) != normalized_text(&input.text)
                || existing.block_type != input.block_type
                || existing.attrs != input.attrs
            {
                impact.updated_blocks += 1;
            }
            if existing.block_order != block_index as i32 {
                impact.moved_blocks += 1;
            }
        } else {
            impact.inserted_blocks += 1;
        }

        let old_segments = existing
            .map(|value| value.segments.as_slice())
            .unwrap_or(&[]);
        let sentences = split_sentences(&input.text);
        let sentence_count = sentences.len();
        let mut planned_segments = Vec::new();
        for (position, sentence) in sentences.into_iter().enumerate() {
            let normalized_sentence = normalized_text(&sentence);
            if let Some(remaining) = remaining_incoming_texts.get_mut(&normalized_sentence) {
                *remaining = remaining.saturating_sub(1);
            }
            let exact = current_segments
                .iter()
                .copied()
                .filter(|segment| {
                    !incoming_segment_ids
                        .contains(&Uuid::parse_str(&segment.id).expect("stored segment UUID"))
                        && normalized_text(&segment.text) == normalized_sentence
                })
                .min_by_key(|segment| segment.order.abs_diff(global_order));
            let positional = old_segments.get(position).filter(|segment| {
                let id = Uuid::parse_str(&segment.id).expect("stored segment UUID");
                !incoming_segment_ids.contains(&id)
                    && remaining_incoming_texts
                        .get(&normalized_text(&segment.text))
                        .copied()
                        .unwrap_or(0)
                        == 0
            });
            let previous = exact.or(positional);
            let segment_id = previous
                .map(|segment| Uuid::parse_str(&segment.id).expect("stored segment UUID"))
                .unwrap_or_else(Uuid::new_v4);
            if let Some(previous) = previous {
                if normalized_text(&previous.text) != normalized_text(&sentence) {
                    impact.changed_segments += 1;
                    touched_segment_ids.insert(segment_id);
                    if previous.reading_text.is_some() {
                        impact.stale_readings += 1;
                    }
                    if previous.translation.is_some() {
                        impact.stale_translations += 1;
                    }
                    if previous.explanation.is_some() {
                        impact.stale_explanations += 1;
                    }
                    lineage.push((Some(segment_id), Some(segment_id), "modified".to_string()));
                } else {
                    lineage.push((Some(segment_id), Some(segment_id), "preserved".to_string()));
                }
            } else {
                impact.changed_segments += 1;
                let split_from = (old_segments.len() == 1 && sentence_count > 1)
                    .then(|| Uuid::parse_str(&old_segments[0].id).expect("stored segment UUID"));
                lineage.push((
                    split_from,
                    Some(segment_id),
                    if split_from.is_some() {
                        "split".to_string()
                    } else {
                        "inserted".to_string()
                    },
                ));
            }
            incoming_segment_ids.insert(segment_id);
            planned_segments.push(PlannedSegment {
                id: segment_id,
                order: global_order,
                block_segment_order: position as i32,
                text_sha256: source_text_sha256(&sentence),
                text: sentence,
                reading_text: previous.and_then(|segment| segment.reading_text.clone()),
                translation: previous.and_then(|segment| segment.translation.clone()),
                explanation: previous.and_then(|segment| segment.explanation.clone()),
                reading_source_sha256: previous.and_then(|segment| {
                    (segment.reading_status == "current").then(|| segment.text_sha256.clone())
                }),
                translation_source_sha256: previous.and_then(|segment| {
                    (segment.translation_status == "current").then(|| segment.text_sha256.clone())
                }),
                explanation_source_sha256: previous.and_then(|segment| {
                    (segment.explanation_status == "current").then(|| segment.text_sha256.clone())
                }),
            });
            global_order += 1;
        }
        planned.push(PlannedBlock {
            id,
            block_type: input.block_type.trim().to_string(),
            block_order: block_index as i32,
            attrs: input.attrs.clone(),
            segments: planned_segments,
        });
    }

    for block in &current.blocks {
        let id = Uuid::parse_str(&block.id).expect("stored block UUID");
        if !incoming_block_ids.contains(&id) {
            impact.deleted_blocks += 1;
        }
        let merge_target = planned
            .iter()
            .find(|planned_block| planned_block.id == id)
            .filter(|planned_block| block.segments.len() > 1 && planned_block.segments.len() == 1)
            .map(|planned_block| planned_block.segments[0].id);
        for segment in &block.segments {
            let segment_id = Uuid::parse_str(&segment.id).expect("stored segment UUID");
            if !incoming_segment_ids.contains(&segment_id) {
                impact.deleted_segments += 1;
                touched_segment_ids.insert(segment_id);
                lineage.push((
                    Some(segment_id),
                    merge_target,
                    if merge_target.is_some() {
                        "merged".to_string()
                    } else {
                        "deleted".to_string()
                    },
                ));
            }
        }
    }
    let touched_segment_ids = touched_segment_ids.into_iter().collect::<Vec<_>>();
    if !touched_segment_ids.is_empty() {
        let annotations = sqlx::query_as::<_, (Option<Uuid>, Value, String)>(
            "SELECT segment_id, locator, source_text FROM annotations WHERE user_id = $1 AND material_id = $2 AND segment_id = ANY($3)",
        )
        .bind(user_id)
        .bind(material_id)
        .bind(&touched_segment_ids)
        .fetch_all(&mut **tx)
        .await?;
        impact.affected_annotations = annotations.len() as i64;
        let planned_segments = planned
            .iter()
            .flat_map(|block| block.segments.iter())
            .map(|segment| (segment.id, segment.text.clone()))
            .collect::<HashMap<_, _>>();
        for (segment_id, locator, source_text) in annotations {
            match classify_annotation_reanchor(
                segment_id,
                &locator,
                &source_text,
                &planned_segments,
            ) {
                "exact" => impact.annotation_reanchors.exact += 1,
                "text" => impact.annotation_reanchors.text += 1,
                "ambiguous" => impact.annotation_reanchors.ambiguous += 1,
                _ => impact.annotation_reanchors.orphaned += 1,
            }
        }
        impact.affected_learning_items = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM learning_items WHERE user_id = $1 AND material_id = $2 AND segment_id = ANY($3)",
        )
        .bind(user_id)
        .bind(material_id)
        .bind(&touched_segment_ids)
        .fetch_one(&mut **tx)
        .await?;
    }

    let content = canonical_content(blocks);
    Ok(PlannedDocument {
        content_sha256: content_sha256_hex(&content),
        content,
        blocks: planned,
        impact,
        lineage,
    })
}

fn planned_document_dto(
    current: &MaterialDocumentDto,
    plan: &PlannedDocument,
    revision: i64,
) -> MaterialDocumentDto {
    let mut summary = DerivedSummaryDto::default();
    let blocks = plan
        .blocks
        .iter()
        .map(|block| {
            let segments = block
                .segments
                .iter()
                .map(|segment| {
                    let reading_status = derived_status(
                        &segment.reading_text,
                        &segment.reading_source_sha256,
                        &segment.text_sha256,
                    );
                    let translation_status = derived_status(
                        &segment.translation,
                        &segment.translation_source_sha256,
                        &segment.text_sha256,
                    );
                    let explanation_status = derived_status(
                        &segment.explanation,
                        &segment.explanation_source_sha256,
                        &segment.text_sha256,
                    );
                    summary.segment_count += 1;
                    match reading_status.as_str() {
                        "current" => summary.current_readings += 1,
                        "stale" => summary.stale_readings += 1,
                        _ => {}
                    }
                    match translation_status.as_str() {
                        "current" => summary.current_translations += 1,
                        "stale" => summary.stale_translations += 1,
                        _ => {}
                    }
                    match explanation_status.as_str() {
                        "current" => summary.current_explanations += 1,
                        "stale" => summary.stale_explanations += 1,
                        _ => {}
                    }
                    DocumentSegmentDto {
                        id: segment.id.to_string(),
                        order: segment.order,
                        block_segment_order: segment.block_segment_order,
                        text: segment.text.clone(),
                        reading_text: segment.reading_text.clone(),
                        translation: segment.translation.clone(),
                        explanation: segment.explanation.clone(),
                        text_sha256: segment.text_sha256.clone(),
                        reading_status,
                        translation_status,
                        explanation_status,
                        is_new_paragraph: segment.block_segment_order == 0,
                    }
                })
                .collect::<Vec<_>>();
            DocumentBlockDto {
                id: block.id.to_string(),
                block_type: block.block_type.clone(),
                block_order: block.block_order,
                text: block_text(&segments),
                attrs: block.attrs.clone(),
                segments,
            }
        })
        .collect();
    MaterialDocumentDto {
        material_id: current.material_id.clone(),
        title: current.title.clone(),
        current_revision: revision,
        content_sha256: plan.content_sha256.clone(),
        blocks,
        derived_summary: summary,
        migration_report: current.migration_report.clone(),
    }
}

async fn apply_plan(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    material_id: Uuid,
    plan: &PlannedDocument,
) -> Result<(), AppError> {
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

    let mut live_block_ids = Vec::new();
    let mut live_segment_ids = Vec::new();
    for block in &plan.blocks {
        let row = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO material_blocks (
                id, user_id, material_id, block_order, block_type, attrs, deleted_at
            ) VALUES ($1, $2, $3, $4, $5, $6, NULL)
            ON CONFLICT (id) DO UPDATE
            SET block_order = EXCLUDED.block_order, block_type = EXCLUDED.block_type,
                attrs = EXCLUDED.attrs, deleted_at = NULL, updated_at = NOW()
            WHERE material_blocks.user_id = EXCLUDED.user_id
              AND material_blocks.material_id = EXCLUDED.material_id
            RETURNING id
            "#,
        )
        .bind(block.id)
        .bind(user_id)
        .bind(material_id)
        .bind(block.block_order)
        .bind(&block.block_type)
        .bind(&block.attrs)
        .fetch_optional(&mut **tx)
        .await?;
        if row.is_none() {
            return Err(AppError::conflict(
                "document_block_id_conflict",
                "block id already belongs to another material",
            ));
        }
        live_block_ids.push(block.id);

        for segment in &block.segments {
            let row = sqlx::query_scalar::<_, Uuid>(
                r#"
                INSERT INTO material_segments (
                    id, user_id, material_id, block_id, segment_order, block_segment_order,
                    text, reading_text, translation, explanation, text_sha256,
                    reading_source_sha256, translation_source_sha256,
                    explanation_source_sha256, is_new_paragraph, deleted_at
                ) VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, NULL
                )
                ON CONFLICT (id) DO UPDATE
                SET block_id = EXCLUDED.block_id, segment_order = EXCLUDED.segment_order,
                    block_segment_order = EXCLUDED.block_segment_order, text = EXCLUDED.text,
                    reading_text = EXCLUDED.reading_text, translation = EXCLUDED.translation,
                    explanation = EXCLUDED.explanation, text_sha256 = EXCLUDED.text_sha256,
                    reading_source_sha256 = EXCLUDED.reading_source_sha256,
                    translation_source_sha256 = EXCLUDED.translation_source_sha256,
                    explanation_source_sha256 = EXCLUDED.explanation_source_sha256,
                    is_new_paragraph = EXCLUDED.is_new_paragraph, deleted_at = NULL,
                    updated_at = NOW()
                WHERE material_segments.user_id = EXCLUDED.user_id
                  AND material_segments.material_id = EXCLUDED.material_id
                RETURNING id
                "#,
            )
            .bind(segment.id)
            .bind(user_id)
            .bind(material_id)
            .bind(block.id)
            .bind(segment.order)
            .bind(segment.block_segment_order)
            .bind(&segment.text)
            .bind(&segment.reading_text)
            .bind(&segment.translation)
            .bind(&segment.explanation)
            .bind(&segment.text_sha256)
            .bind(&segment.reading_source_sha256)
            .bind(&segment.translation_source_sha256)
            .bind(&segment.explanation_source_sha256)
            .bind(segment.block_segment_order == 0)
            .fetch_optional(&mut **tx)
            .await?;
            if row.is_none() {
                return Err(AppError::conflict(
                    "document_segment_id_conflict",
                    "segment id already belongs to another material",
                ));
            }
            live_segment_ids.push(segment.id);
        }
    }

    sqlx::query(
        "UPDATE material_segments SET deleted_at = NOW(), updated_at = NOW() WHERE user_id = $1 AND material_id = $2 AND deleted_at IS NULL AND NOT (id = ANY($3))",
    )
    .bind(user_id)
    .bind(material_id)
    .bind(&live_segment_ids)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE material_blocks SET deleted_at = NOW(), updated_at = NOW() WHERE user_id = $1 AND material_id = $2 AND deleted_at IS NULL AND NOT (id = ANY($3))",
    )
    .bind(user_id)
    .bind(material_id)
    .bind(&live_block_ids)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn commit_internal(
    state: &AppState,
    user_id: Uuid,
    material_id: Uuid,
    request: DocumentEditCommitRequest,
    action: &str,
    preserve_draft: bool,
) -> Result<DocumentEditCommitResponse, AppError> {
    if request.client_request_id.trim().is_empty() || request.client_request_id.len() > 255 {
        return Err(AppError::bad_request(
            "invalid_document_edit",
            "client_request_id must contain between 1 and 255 characters",
        ));
    }
    validate_blocks(&request.blocks)?;
    if action == "edit"
        && request
            .preview_token
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        return Err(AppError::bad_request(
            "document_preview_required",
            "document edit must include the preview_token returned by the preview endpoint",
        ));
    }
    let mut request_value = serde_json::to_value(&request)
        .map_err(|_| AppError::internal("document_request_error", "could not serialize request"))?;
    if preserve_draft {
        request_value["preserve_draft"] = Value::Bool(true);
    }
    let request_hash = source_text_sha256(&request_value.to_string());

    if let Some((existing_material_id, existing_hash, snapshot, summary)) =
        sqlx::query_as::<_, (Uuid, Option<String>, Value, Value)>(
            "SELECT material_id, request_sha256, snapshot, change_summary FROM material_revisions WHERE user_id = $1 AND client_request_id = $2",
        )
        .bind(user_id)
        .bind(request.client_request_id.trim())
        .fetch_optional(&state.pool)
        .await?
    {
        if existing_material_id != material_id || existing_hash.as_deref() != Some(&request_hash) {
            return Err(AppError::conflict(
                "document_edit_idempotency_conflict",
                "client_request_id was already used for a different document edit",
            ));
        }
        let document = serde_json::from_value(snapshot).map_err(|_| {
            AppError::internal("document_snapshot_error", "stored document snapshot is invalid")
        })?;
        let impact = serde_json::from_value(summary).unwrap_or_default();
        return Ok(DocumentEditCommitResponse { document, impact });
    }

    initialize_document(&state.pool, user_id, material_id).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await?;
    let locked = lock_material(&mut tx, user_id, material_id).await?;
    if locked.current_revision != request.base_revision {
        return Err(AppError::conflict(
            "material_revision_conflict",
            format!(
                "base revision {} does not match current revision {}",
                request.base_revision, locked.current_revision
            ),
        ));
    }
    sqlx::query(
        "SELECT id FROM material_blocks WHERE user_id = $1 AND material_id = $2 FOR UPDATE",
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query(
        "SELECT id FROM material_segments WHERE user_id = $1 AND material_id = $2 FOR UPDATE",
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_all(&mut *tx)
    .await?;
    let current = fetch_document_tx(&mut tx, user_id, locked).await?;
    let plan = plan_document(&mut tx, user_id, material_id, &current, &request.blocks).await?;
    if action == "edit" {
        let expected_preview_token = preview_token(
            user_id,
            material_id,
            request.base_revision,
            current.content_sha256.as_deref(),
            &request.blocks,
            &plan.impact,
        )?;
        if request.preview_token.as_deref() != Some(expected_preview_token.as_str()) {
            return Err(AppError::conflict(
                "document_preview_changed",
                "document edit impact changed after preview; preview the edit again",
            ));
        }
    }
    let next_revision = current.current_revision + 1;
    let next_document = planned_document_dto(&current, &plan, next_revision);
    let snapshot = serde_json::to_value(&next_document).map_err(|_| {
        AppError::internal("document_snapshot_error", "could not serialize document")
    })?;
    let impact_value = serde_json::to_value(&plan.impact).map_err(|_| {
        AppError::internal("document_snapshot_error", "could not serialize edit impact")
    })?;

    apply_plan(&mut tx, user_id, material_id, &plan).await?;
    let translated = next_document.derived_summary.segment_count > 0
        && next_document.derived_summary.current_translations
            == next_document.derived_summary.segment_count;
    sqlx::query(
        r#"
        UPDATE materials
        SET content = $3, content_sha256 = $4, current_revision = $5,
            content_updated_at = NOW(), updated_at = NOW(), translated = $6
        WHERE user_id = $1 AND id = $2
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .bind(&plan.content)
    .bind(&plan.content_sha256)
    .bind(next_revision)
    .bind(translated)
    .execute(&mut *tx)
    .await?;
    for (from_segment_id, to_segment_id, operation) in &plan.lineage {
        sqlx::query(
            r#"
            INSERT INTO material_segment_lineage (
                id, user_id, material_id, revision, from_segment_id, to_segment_id, operation
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(material_id)
        .bind(next_revision)
        .bind(from_segment_id)
        .bind(to_segment_id)
        .bind(operation)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        r#"
        INSERT INTO material_revisions (
            id, user_id, material_id, revision, parent_revision, action,
            content_sha256, snapshot, change_summary, client_request_id, request_sha256
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(material_id)
    .bind(next_revision)
    .bind(current.current_revision)
    .bind(action)
    .bind(&plan.content_sha256)
    .bind(snapshot)
    .bind(impact_value)
    .bind(request.client_request_id.trim())
    .bind(request_hash)
    .execute(&mut *tx)
    .await?;
    if !preserve_draft {
        sqlx::query("DELETE FROM material_edit_drafts WHERE user_id = $1 AND material_id = $2")
            .bind(user_id)
            .bind(material_id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(DocumentEditCommitResponse {
        document: next_document,
        impact: plan.impact,
    })
}

pub async fn get_document(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<MaterialDocumentDto>, AppError> {
    initialize_document(&state.pool, user.id, id).await?;
    Ok(Json(fetch_document(&state.pool, user.id, id).await?))
}

pub async fn preview_document_edit(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(request): ApiJson<DocumentEditPreviewRequest>,
) -> Result<Json<DocumentEditPreviewResponse>, AppError> {
    initialize_document(&state.pool, user.id, id).await?;
    let current = fetch_document(&state.pool, user.id, id).await?;
    if request.base_revision != current.current_revision {
        return Err(AppError::conflict(
            "material_revision_conflict",
            format!(
                "base revision {} does not match current revision {}",
                request.base_revision, current.current_revision
            ),
        ));
    }
    let mut tx = state.pool.begin().await?;
    let plan = plan_document(&mut tx, user.id, id, &current, &request.blocks).await?;
    tx.rollback().await?;
    let token = preview_token(
        user.id,
        id,
        request.base_revision,
        current.content_sha256.as_deref(),
        &request.blocks,
        &plan.impact,
    )?;
    Ok(Json(DocumentEditPreviewResponse {
        base_revision: request.base_revision,
        next_revision: request.base_revision + 1,
        content_sha256: plan.content_sha256,
        impact: plan.impact,
        preview_token: token,
    }))
}

pub async fn commit_document_edit(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(request): ApiJson<DocumentEditCommitRequest>,
) -> Result<Json<DocumentEditCommitResponse>, AppError> {
    Ok(Json(
        commit_internal(&state, user.id, id, request, "edit", false).await?,
    ))
}

pub async fn list_revisions(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<DocumentRevisionSummaryDto>>, AppError> {
    initialize_document(&state.pool, user.id, id).await?;
    let rows = sqlx::query_as::<
        _,
        (
            i64,
            Option<i64>,
            String,
            Option<String>,
            Value,
            DateTime<Utc>,
        ),
    >(
        r#"
        SELECT revision, parent_revision, action, content_sha256, change_summary, created_at
        FROM material_revisions
        WHERE user_id = $1 AND material_id = $2
        ORDER BY revision DESC
        "#,
    )
    .bind(user.id)
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|row| DocumentRevisionSummaryDto {
                revision: row.0,
                parent_revision: row.1,
                action: row.2,
                content_sha256: row.3,
                change_summary: row.4,
                created_at: row.5.to_rfc3339(),
            })
            .collect(),
    ))
}

pub async fn get_revision(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path((id, revision)): Path<(Uuid, i64)>,
) -> Result<Json<DocumentRevisionDto>, AppError> {
    initialize_document(&state.pool, user.id, id).await?;
    let row = sqlx::query_as::<_, (i64, Option<i64>, String, Option<String>, Value, Value, DateTime<Utc>)>(
        r#"
        SELECT revision, parent_revision, action, content_sha256, snapshot, change_summary, created_at
        FROM material_revisions
        WHERE user_id = $1 AND material_id = $2 AND revision = $3
        "#,
    )
    .bind(user.id)
    .bind(id)
    .bind(revision)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("material_revision_not_found", "material revision not found"))?;
    Ok(Json(DocumentRevisionDto {
        revision: row.0,
        parent_revision: row.1,
        action: row.2,
        content_sha256: row.3,
        snapshot: row.4,
        change_summary: row.5,
        created_at: row.6.to_rfc3339(),
    }))
}

pub async fn restore_revision(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path((id, revision)): Path<(Uuid, i64)>,
    ApiJson(request): ApiJson<DocumentRestoreRequest>,
) -> Result<Json<DocumentEditCommitResponse>, AppError> {
    initialize_document(&state.pool, user.id, id).await?;
    let snapshot = sqlx::query_scalar::<_, Value>(
        "SELECT snapshot FROM material_revisions WHERE user_id = $1 AND material_id = $2 AND revision = $3",
    )
    .bind(user.id)
    .bind(id)
    .bind(revision)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("material_revision_not_found", "material revision not found"))?;
    let document: MaterialDocumentDto = serde_json::from_value(snapshot).map_err(|_| {
        AppError::internal(
            "document_snapshot_error",
            "stored document snapshot is invalid",
        )
    })?;
    let commit = DocumentEditCommitRequest {
        base_revision: request.base_revision,
        client_request_id: request.client_request_id,
        preview_token: None,
        blocks: document_as_inputs(&document),
    };
    Ok(Json(
        commit_internal(
            &state,
            user.id,
            id,
            commit,
            "restore",
            request.preserve_draft,
        )
        .await?,
    ))
}

pub async fn get_draft(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Option<DocumentDraftDto>>, AppError> {
    initialize_document(&state.pool, user.id, id).await?;
    let current_revision = sqlx::query_scalar::<_, i64>(
        "SELECT current_revision FROM materials WHERE user_id = $1 AND id = $2",
    )
    .bind(user.id)
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    let row = sqlx::query_as::<_, (i64, Value, DateTime<Utc>)>(
        "SELECT base_revision, blocks, updated_at FROM material_edit_drafts WHERE user_id = $1 AND material_id = $2",
    )
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    let draft = row
        .map(|row| {
            let blocks = serde_json::from_value(row.1).map_err(|_| {
                AppError::internal("document_draft_error", "stored document draft is invalid")
            })?;
            Ok::<DocumentDraftDto, AppError>(DocumentDraftDto {
                material_id: id.to_string(),
                base_revision: row.0,
                blocks,
                updated_at: row.2.to_rfc3339(),
                is_stale: row.0 != current_revision,
            })
        })
        .transpose()?;
    Ok(Json(draft))
}

pub async fn put_draft(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(request): ApiJson<DocumentDraftRequest>,
) -> Result<Json<DocumentDraftDto>, AppError> {
    validate_blocks(&request.blocks)?;
    initialize_document(&state.pool, user.id, id).await?;
    let current_revision = sqlx::query_scalar::<_, i64>(
        "SELECT current_revision FROM materials WHERE user_id = $1 AND id = $2",
    )
    .bind(user.id)
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    let blocks = serde_json::to_value(&request.blocks).map_err(|_| {
        AppError::internal("document_draft_error", "could not serialize document draft")
    })?;
    let updated_at = sqlx::query_scalar::<_, DateTime<Utc>>(
        r#"
        INSERT INTO material_edit_drafts (user_id, material_id, base_revision, blocks)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id, material_id) DO UPDATE
        SET base_revision = EXCLUDED.base_revision, blocks = EXCLUDED.blocks, updated_at = NOW()
        RETURNING updated_at
        "#,
    )
    .bind(user.id)
    .bind(id)
    .bind(request.base_revision)
    .bind(blocks)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(DocumentDraftDto {
        material_id: id.to_string(),
        base_revision: request.base_revision,
        blocks: request.blocks,
        updated_at: updated_at.to_rfc3339(),
        is_stale: request.base_revision != current_revision,
    }))
}

pub async fn delete_draft(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM materials WHERE user_id = $1 AND id = $2)",
    )
    .bind(user.id)
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    if !exists {
        return Err(AppError::not_found(
            "material_not_found",
            "material not found",
        ));
    }
    let result =
        sqlx::query("DELETE FROM material_edit_drafts WHERE user_id = $1 AND material_id = $2")
            .bind(user.id)
            .bind(id)
            .execute(&state.pool)
            .await?;
    Ok(Json(json!({ "deleted": result.rows_affected() > 0 })))
}

pub async fn patch_segment_derived(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(request): ApiJson<SegmentDerivedBatchRequest>,
) -> Result<Json<MaterialDocumentDto>, AppError> {
    if request.updates.is_empty() || request.updates.len() > 500 {
        return Err(AppError::bad_request(
            "invalid_segment_updates",
            "updates must contain between 1 and 500 entries",
        ));
    }
    initialize_document(&state.pool, user.id, id).await?;
    let mut seen = HashSet::new();
    let mut tx = state.pool.begin().await?;
    for update in request.updates {
        let segment_id = Uuid::parse_str(&update.segment_id).map_err(|_| {
            AppError::bad_request("invalid_segment_updates", "segment_id must be a UUID")
        })?;
        if !seen.insert(segment_id) {
            return Err(AppError::bad_request(
                "invalid_segment_updates",
                "segment_id values must be unique",
            ));
        }
        if update.reading_text.is_none()
            && update.translation.is_none()
            && update.explanation.is_none()
        {
            return Err(AppError::bad_request(
                "invalid_segment_updates",
                "each update must include reading_text, translation, or explanation",
            ));
        }
        let current_hash = sqlx::query_scalar::<_, Option<String>>(
            r#"
            SELECT text_sha256 FROM material_segments
            WHERE user_id = $1 AND material_id = $2 AND id = $3 AND deleted_at IS NULL
            FOR UPDATE
            "#,
        )
        .bind(user.id)
        .bind(id)
        .bind(segment_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::not_found("segment_not_found", "segment not found"))?
        .ok_or_else(|| {
            AppError::internal("segment_hash_missing", "segment text hash is missing")
        })?;
        if update
            .expected_text_sha256
            .as_deref()
            .is_some_and(|expected| !expected.eq_ignore_ascii_case(&current_hash))
        {
            return Err(AppError::conflict(
                "segment_text_conflict",
                "segment text changed before derived content was saved",
            ));
        }
        let update_reading = update.reading_text.is_some();
        let reading_text = update.reading_text.flatten();
        let update_translation = update.translation.is_some();
        let translation = update.translation.flatten();
        let update_explanation = update.explanation.is_some();
        let explanation = update.explanation.flatten();
        sqlx::query(
            r#"
            UPDATE material_segments
            SET reading_text = CASE WHEN $4 THEN $5::TEXT ELSE reading_text END,
                reading_source_sha256 = CASE
                    WHEN NOT $4 THEN reading_source_sha256
                    WHEN $5::TEXT IS NULL THEN NULL
                    ELSE $10
                END,
                translation = CASE WHEN $6 THEN $7::TEXT ELSE translation END,
                translation_source_sha256 = CASE
                    WHEN NOT $6 THEN translation_source_sha256
                    WHEN $7::TEXT IS NULL THEN NULL
                    ELSE $10
                END,
                explanation = CASE WHEN $8 THEN $9::JSONB ELSE explanation END,
                explanation_source_sha256 = CASE
                    WHEN NOT $8 THEN explanation_source_sha256
                    WHEN $9::JSONB IS NULL THEN NULL
                    ELSE $10
                END,
                updated_at = NOW()
            WHERE user_id = $1 AND material_id = $2 AND id = $3 AND deleted_at IS NULL
            "#,
        )
        .bind(user.id)
        .bind(id)
        .bind(segment_id)
        .bind(update_reading)
        .bind(reading_text)
        .bind(update_translation)
        .bind(translation)
        .bind(update_explanation)
        .bind(explanation)
        .bind(current_hash)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(Json(fetch_document(&state.pool, user.id, id).await?))
}

pub async fn create_editable_derivative(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(request): ApiJson<EditableDerivativeRequest>,
) -> Result<Json<EditableDerivativeResponse>, AppError> {
    let source = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        "SELECT title, content, book_path, media_path FROM materials WHERE user_id = $1 AND id = $2",
    )
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("material_not_found", "material not found"))?;
    if source.2.is_none() && source.3.is_none() {
        return Err(AppError::bad_request(
            "editable_derivative_not_required",
            "only immutable file or media materials require an editable derivative",
        ));
    }
    if let Some(existing) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM materials WHERE user_id = $1 AND editable_source_material_id = $2",
    )
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    {
        return Ok(Json(EditableDerivativeResponse {
            source_material_id: id.to_string(),
            derivative_material_id: existing.to_string(),
            created: false,
        }));
    }
    let content = request.content.unwrap_or(source.1);
    if content.trim().is_empty() {
        return Err(AppError::bad_request(
            "editable_derivative_content_required",
            "editable derivative requires extracted or supplied text",
        ));
    }
    let title = request
        .title
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{} - editable copy", source.0));
    let derivative_id = Uuid::new_v4();
    let mut tx = state.pool.begin().await?;
    let inserted = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO materials (
            id, user_id, title, content, source_type, translated, metadata,
            content_sha256, current_revision, content_updated_at, editable_source_material_id
        ) VALUES (
            $1, $2, $3, $4, 'article', FALSE,
            jsonb_build_object('editable_derivative', TRUE, 'source_material_id', $5::TEXT),
            $6, 1, NOW(), $5
        )
        ON CONFLICT (user_id, editable_source_material_id)
            WHERE editable_source_material_id IS NOT NULL
        DO NOTHING
        RETURNING id
        "#,
    )
    .bind(derivative_id)
    .bind(user.id)
    .bind(title.trim())
    .bind(&content)
    .bind(id)
    .bind(content_sha256_hex(&content))
    .fetch_optional(&mut *tx)
    .await?;
    if inserted.is_none() {
        tx.rollback().await?;
        let existing = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM materials WHERE user_id = $1 AND editable_source_material_id = $2",
        )
        .bind(user.id)
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
        return Ok(Json(EditableDerivativeResponse {
            source_material_id: id.to_string(),
            derivative_material_id: existing.to_string(),
            created: false,
        }));
    }
    sqlx::query(
        r#"
        INSERT INTO material_relations (
            id, user_id, source_material_id, target_material_id, relation_type, metadata
        ) VALUES ($1, $2, $3, $4, 'editable_derivative', '{}'::jsonb)
        ON CONFLICT (user_id, source_material_id, target_material_id, relation_type) DO NOTHING
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user.id)
    .bind(id)
    .bind(derivative_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    initialize_document(&state.pool, user.id, derivative_id).await?;
    Ok(Json(EditableDerivativeResponse {
        source_material_id: id.to_string(),
        derivative_material_id: derivative_id.to_string(),
        created: true,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divider_is_the_only_empty_document_block() {
        let divider = DocumentBlockInput {
            id: None,
            block_type: "divider".to_string(),
            block_order: Some(0),
            text: String::new(),
            attrs: empty_object(),
        };
        assert!(validate_blocks(&[divider]).is_ok());
        let paragraph = DocumentBlockInput {
            id: None,
            block_type: "paragraph".to_string(),
            block_order: Some(0),
            text: String::new(),
            attrs: empty_object(),
        };
        assert!(validate_blocks(&[paragraph]).is_err());
    }
}
