use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    auth::{ApiJson, AuthenticatedUser},
    error::AppError,
    routes::AppState,
};

#[derive(Debug, Deserialize)]
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
    pub segments: Option<Vec<MaterialSegmentInput>>,
}

#[derive(Debug, Deserialize)]
pub struct PatchMaterialRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
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

#[derive(Debug, Clone, Deserialize)]
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
    pub translated: bool,
    pub active_mind_map_artifact_id: Option<String>,
    pub segments: Vec<ArticleSegmentDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArticleSegmentDto {
    pub id: String,
    pub article_id: String,
    #[serde(rename = "order")]
    pub order: i32,
    pub text: String,
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
}

#[derive(Debug, sqlx::FromRow)]
struct SegmentRecord {
    id: Uuid,
    material_id: Uuid,
    segment_order: i32,
    text: String,
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
) -> Result<Json<Vec<ArticleDto>>, AppError> {
    let records = sqlx::query_as::<_, MaterialRecord>(
        r#"
        SELECT id, title, content, source_type, source_url, media_path, book_path, book_type,
               translated, active_mind_map_artifact_id, metadata, created_at
        FROM materials
        WHERE user_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    let mut materials = Vec::with_capacity(records.len());
    for record in records {
        let segments = fetch_segments(&state.pool, user.id, record.id).await?;
        materials.push(article_from_records(record, segments));
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
    let segment_inputs = payload
        .segments
        .unwrap_or_else(|| create_segments_from_content(&payload.content));

    let mut tx = state.pool.begin().await?;
    let record = sqlx::query_as::<_, MaterialRecord>(
        r#"
        INSERT INTO materials (
            id, user_id, title, content, source_type, source_url, media_path, book_path,
            book_type, translated, active_mind_map_artifact_id, metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        RETURNING id, title, content, source_type, source_url, media_path, book_path, book_type,
                  translated, active_mind_map_artifact_id, metadata, created_at
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
    .fetch_one(&mut *tx)
    .await?;

    insert_segments(&mut tx, user.id, record.id, segment_inputs).await?;
    tx.commit().await?;

    let segments = fetch_segments(&state.pool, user.id, record.id).await?;
    Ok(Json(article_from_records(record, segments)))
}

pub async fn get_material(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ArticleDto>, AppError> {
    let record = fetch_material(&state.pool, user.id, id).await?;
    let segments = fetch_segments(&state.pool, user.id, id).await?;

    Ok(Json(article_from_records(record, segments)))
}

pub async fn patch_material(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(payload): ApiJson<PatchMaterialRequest>,
) -> Result<Json<ArticleDto>, AppError> {
    let existing = fetch_material(&state.pool, user.id, id).await?;

    if let Some(title) = &payload.title {
        validate_title(title)?;
    }

    let title = payload.title.unwrap_or(existing.title);
    let content = payload.content.unwrap_or(existing.content);
    let source_type = payload.source_type.or(existing.source_type);
    let source_url = payload.source_url.or(existing.source_url);
    let media_path = payload.media_path.or(existing.media_path);
    let book_path = payload.book_path.or(existing.book_path);
    let book_type = payload.book_type.or(existing.book_type);
    let translated = payload.translated.unwrap_or(existing.translated);
    let active_mind_map_artifact_id = payload
        .active_mind_map_artifact_id
        .or(existing.active_mind_map_artifact_id);
    let metadata = payload.metadata.unwrap_or(existing.metadata);

    let mut tx = state.pool.begin().await?;
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
            updated_at = NOW()
        WHERE id = $1 AND user_id = $2
        RETURNING id, title, content, source_type, source_url, media_path, book_path, book_type,
                  translated, active_mind_map_artifact_id, metadata, created_at
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
    .fetch_one(&mut *tx)
    .await?;

    if let Some(segments) = payload.segments {
        sqlx::query(
            r#"
            DELETE FROM material_segments
            WHERE material_id = $1 AND user_id = $2
            "#,
        )
        .bind(record.id)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
        insert_segments(&mut tx, user.id, record.id, segments).await?;
    }

    tx.commit().await?;

    let segments = fetch_segments(&state.pool, user.id, record.id).await?;
    Ok(Json(article_from_records(record, segments)))
}

pub async fn delete_material(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    let deleted = sqlx::query_scalar::<_, Uuid>(
        r#"
        DELETE FROM materials
        WHERE id = $1 AND user_id = $2
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await?;

    if deleted.is_none() {
        return Err(material_not_found());
    }

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
               translated, active_mind_map_artifact_id, metadata, created_at
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
        SELECT id, material_id, segment_order, text, reading_text, translation, explanation,
               start_time, end_time, is_new_paragraph, created_at
        FROM material_segments
        WHERE user_id = $1 AND material_id = $2
        ORDER BY segment_order ASC
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
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

fn article_from_records(record: MaterialRecord, segments: Vec<SegmentRecord>) -> ArticleDto {
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
        translated: record.translated,
        active_mind_map_artifact_id: record.active_mind_map_artifact_id,
        segments: segments.into_iter().map(segment_from_record).collect(),
    }
}

fn segment_from_record(record: SegmentRecord) -> ArticleSegmentDto {
    ArticleSegmentDto {
        id: record.id.to_string(),
        article_id: record.material_id.to_string(),
        order: record.segment_order,
        text: record.text,
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
