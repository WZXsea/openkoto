use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    auth::{ApiJson, AuthenticatedUser},
    error::AppError,
    learning_activity, learning_items,
    routes::AppState,
};

const DEFAULT_UNGROUPED_PACK_ID: &str = "system-ungrouped";
const DEFAULT_UNGROUPED_PACK_NAME: &str = "未分组";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordPackDto {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub language_from: Option<String>,
    #[serde(default)]
    pub language_to: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub version: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteVocabularyDto {
    pub id: String,
    pub word: String,
    pub meaning: String,
    pub usage: String,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(default)]
    pub example: Option<String>,
    #[serde(default)]
    pub reading: Option<String>,
    #[serde(default)]
    pub source_article_id: Option<String>,
    #[serde(default)]
    pub source_article_title: Option<String>,
    #[serde(default)]
    pub pack_ids: Vec<String>,
    #[serde(default = "default_srs_state")]
    pub srs_state: String,
    #[serde(default = "default_srs_ease_factor")]
    pub ease_factor: f64,
    #[serde(default)]
    pub repetitions: i32,
    #[serde(default)]
    pub interval_days: i32,
    #[serde(default)]
    pub due_date: String,
    #[serde(default)]
    pub last_reviewed_at: Option<String>,
    #[serde(default)]
    pub review_count: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteGrammarDto {
    pub id: String,
    pub point: String,
    pub explanation: String,
    #[serde(default)]
    pub example: Option<String>,
    #[serde(default)]
    pub source_article_id: Option<String>,
    #[serde(default)]
    pub source_article_title: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkDto {
    pub id: String,
    pub book_path: String,
    pub book_type: String,
    pub title: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub selected_text: Option<String>,
    #[serde(default)]
    pub page_number: Option<i32>,
    #[serde(default)]
    pub epub_cfi: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskDto {
    pub id: String,
    pub task_type: String,
    pub status: String,
    pub article_id: String,
    pub input: Value,
    pub progress: f64,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub worker_session_id: Option<String>,
    #[serde(default)]
    pub artifact_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactDto {
    pub id: String,
    pub task_id: String,
    pub article_id: String,
    pub artifact_type: String,
    pub version: String,
    pub content: Value,
    #[serde(default)]
    pub metadata: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ListBookmarksQuery {
    #[serde(default)]
    pub book_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DeleteResponse {
    pub deleted: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct WordPackRecord {
    id: String,
    name: String,
    description: Option<String>,
    cover_url: Option<String>,
    author: Option<String>,
    language_from: Option<String>,
    language_to: Option<String>,
    tags: Value,
    version: Option<String>,
    created_at: String,
    updated_at: String,
    is_system: bool,
}

#[derive(Debug, sqlx::FromRow)]
struct FavoriteVocabularyRecord {
    id: String,
    word: String,
    meaning: String,
    usage: String,
    explanation: Option<String>,
    example: Option<String>,
    reading: Option<String>,
    source_article_id: Option<String>,
    source_article_title: Option<String>,
    srs_state: String,
    ease_factor: f64,
    repetitions: i32,
    interval_days: i32,
    due_date: String,
    last_reviewed_at: Option<String>,
    review_count: i32,
    created_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct FavoriteGrammarRecord {
    id: String,
    point: String,
    explanation: String,
    example: Option<String>,
    source_article_id: Option<String>,
    source_article_title: Option<String>,
    created_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct BookmarkRecord {
    id: String,
    book_path: String,
    book_type: String,
    title: String,
    note: Option<String>,
    selected_text: Option<String>,
    page_number: Option<i32>,
    epub_cfi: Option<String>,
    created_at: String,
    color: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct AgentTaskRecord {
    id: String,
    task_type: String,
    status: String,
    article_id: String,
    input: Value,
    progress: f64,
    stage: Option<String>,
    message: Option<String>,
    error: Option<String>,
    worker_session_id: Option<String>,
    artifact_ids: Value,
    created_at: String,
    updated_at: String,
    started_at: Option<String>,
    finished_at: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ArtifactRecord {
    id: String,
    task_id: String,
    article_id: String,
    artifact_type: String,
    version: String,
    content: Value,
    metadata: Option<Value>,
    created_at: String,
    updated_at: String,
}

pub async fn list_word_packs(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
) -> Result<Json<Vec<WordPackDto>>, AppError> {
    ensure_default_word_pack(&state.pool, user.id).await?;
    let records = sqlx::query_as::<_, WordPackRecord>(
        r#"
        SELECT id, name, description, cover_url, author, language_from, language_to, tags,
               version, created_at, updated_at, is_system
        FROM word_packs
        WHERE user_id = $1
        ORDER BY is_system DESC, name ASC
        "#,
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        records.into_iter().map(word_pack_from_record).collect(),
    ))
}

pub async fn upsert_word_pack(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<WordPackDto>,
) -> Result<Json<WordPackDto>, AppError> {
    validate_id(
        &payload.id,
        "invalid_word_pack",
        "word pack id must not be empty",
    )?;
    validate_required(
        &payload.name,
        "invalid_word_pack",
        "word pack name must not be empty",
    )?;

    let record = upsert_word_pack_record(&state.pool, user.id, payload).await?;
    Ok(Json(word_pack_from_record(record)))
}

pub async fn get_word_pack(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<WordPackDto>, AppError> {
    ensure_default_word_pack(&state.pool, user.id).await?;
    let record = sqlx::query_as::<_, WordPackRecord>(
        r#"
        SELECT id, name, description, cover_url, author, language_from, language_to, tags,
               version, created_at, updated_at, is_system
        FROM word_packs
        WHERE user_id = $1 AND id = $2
        "#,
    )
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| not_found("word_pack_not_found", "word pack not found"))?;

    Ok(Json(word_pack_from_record(record)))
}

pub async fn patch_word_pack(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
    ApiJson(payload): ApiJson<WordPackDto>,
) -> Result<Json<WordPackDto>, AppError> {
    if id != payload.id {
        return Err(AppError::bad_request(
            "word_pack_id_mismatch",
            "word pack id does not match path",
        ));
    }

    upsert_word_pack(State(state), AuthenticatedUser { user }, ApiJson(payload)).await
}

pub async fn delete_word_pack(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, AppError> {
    if id == DEFAULT_UNGROUPED_PACK_ID {
        return Err(AppError::bad_request(
            "system_word_pack",
            "system word pack cannot be deleted",
        ));
    }

    ensure_default_word_pack(&state.pool, user.id).await?;
    let mut tx = state.pool.begin().await?;
    let affected_vocabularies = sqlx::query_as::<_, (String, Option<Uuid>)>(
        r#"
        SELECT fvp.vocabulary_id, fv.learning_item_id
        FROM favorite_vocabulary_packs fvp
        JOIN favorite_vocabularies fv
          ON fv.user_id = fvp.user_id AND fv.id = fvp.vocabulary_id
        WHERE fvp.user_id = $1 AND fvp.pack_id = $2
        "#,
    )
    .bind(user.id)
    .bind(&id)
    .fetch_all(&mut *tx)
    .await?;

    let deleted = sqlx::query_scalar::<_, String>(
        r#"
        DELETE FROM word_packs
        WHERE user_id = $1 AND id = $2 AND is_system = FALSE
        RETURNING id
        "#,
    )
    .bind(user.id)
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?;

    if deleted.is_none() {
        return Err(not_found("word_pack_not_found", "word pack not found"));
    }

    for (vocabulary_id, learning_item_id) in affected_vocabularies {
        let pack_count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM favorite_vocabulary_packs
            WHERE user_id = $1 AND vocabulary_id = $2
            "#,
        )
        .bind(user.id)
        .bind(&vocabulary_id)
        .fetch_one(&mut *tx)
        .await?;

        if pack_count == 0 {
            sqlx::query(
                r#"
                INSERT INTO favorite_vocabulary_packs (user_id, vocabulary_id, pack_id)
                VALUES ($1, $2, $3)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(user.id)
            .bind(&vocabulary_id)
            .bind(DEFAULT_UNGROUPED_PACK_ID)
            .execute(&mut *tx)
            .await?;
        }
        if let Some(learning_item_id) = learning_item_id {
            sqlx::query(
                "DELETE FROM word_pack_learning_items \
                 WHERE user_id = $1 AND learning_item_id = $2",
            )
            .bind(user.id)
            .bind(learning_item_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                r#"
                INSERT INTO word_pack_learning_items (user_id, pack_id, learning_item_id)
                SELECT user_id, pack_id, $3
                FROM favorite_vocabulary_packs
                WHERE user_id = $1 AND vocabulary_id = $2
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(user.id)
            .bind(&vocabulary_id)
            .bind(learning_item_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(Json(DeleteResponse { deleted: true }))
}

pub async fn list_favorite_vocabularies(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
) -> Result<Json<Vec<FavoriteVocabularyDto>>, AppError> {
    ensure_default_word_pack(&state.pool, user.id).await?;
    let records = sqlx::query_as::<_, FavoriteVocabularyRecord>(
        r#"
        SELECT id, word, meaning, usage, explanation, example, reading, source_article_id,
               source_article_title, srs_state, ease_factor, repetitions, interval_days,
               due_date, last_reviewed_at, review_count, created_at
        FROM favorite_vocabularies
        WHERE user_id = $1
          AND (
              learning_item_id IS NULL OR EXISTS (
                  SELECT 1 FROM learning_items li
                  WHERE li.user_id = favorite_vocabularies.user_id
                    AND li.id = favorite_vocabularies.learning_item_id
                    AND li.status = 'accepted'
              )
          )
        ORDER BY created_at DESC
        "#,
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    let mut items = Vec::with_capacity(records.len());
    for record in records {
        items.push(favorite_vocabulary_from_record(&state.pool, user.id, record).await?);
    }

    Ok(Json(items))
}

pub async fn upsert_favorite_vocabulary(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<FavoriteVocabularyDto>,
) -> Result<Json<FavoriteVocabularyDto>, AppError> {
    ensure_default_word_pack(&state.pool, user.id).await?;
    validate_favorite_vocabulary(&payload)?;

    let mut tx = state.pool.begin().await?;
    upsert_favorite_vocabulary_record(&mut tx, user.id, &payload).await?;
    replace_vocabulary_pack_links(&mut tx, user.id, &payload.id, &payload.pack_ids).await?;
    learning_items::canonicalize_favorite_vocabulary_tx(
        &mut tx,
        user.id,
        &payload.id,
        &payload.word,
        &payload.meaning,
        payload.explanation.as_deref(),
        payload.example.as_deref(),
        payload.source_article_id.as_deref(),
        payload.source_article_title.as_deref(),
    )
    .await?;
    tx.commit().await?;

    let record = fetch_favorite_vocabulary_record(&state.pool, user.id, &payload.id).await?;
    Ok(Json(
        favorite_vocabulary_from_record(&state.pool, user.id, record).await?,
    ))
}

pub async fn patch_favorite_vocabulary(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
    ApiJson(payload): ApiJson<FavoriteVocabularyDto>,
) -> Result<Json<FavoriteVocabularyDto>, AppError> {
    if id != payload.id {
        return Err(AppError::bad_request(
            "favorite_vocabulary_id_mismatch",
            "favorite vocabulary id does not match path",
        ));
    }

    upsert_favorite_vocabulary(State(state), AuthenticatedUser { user }, ApiJson(payload)).await
}

pub async fn get_favorite_vocabulary(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<FavoriteVocabularyDto>, AppError> {
    let record = fetch_favorite_vocabulary_record(&state.pool, user.id, &id).await?;
    Ok(Json(
        favorite_vocabulary_from_record(&state.pool, user.id, record).await?,
    ))
}

pub async fn delete_favorite_vocabulary(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, AppError> {
    let mut tx = state.pool.begin().await?;
    let learning_item_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT learning_item_id FROM favorite_vocabularies \
         WHERE user_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(user.id)
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| {
        not_found(
            "favorite_vocabulary_not_found",
            "favorite vocabulary not found",
        )
    })?;
    archive_learning_item_for_projection_delete(
        &mut tx,
        user.id,
        learning_item_id,
        "vocabulary",
        &id,
    )
    .await?;
    sqlx::query("DELETE FROM favorite_vocabularies WHERE user_id = $1 AND id = $2")
        .bind(user.id)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(DeleteResponse { deleted: true }))
}

pub async fn list_favorite_grammars(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
) -> Result<Json<Vec<FavoriteGrammarDto>>, AppError> {
    let records = sqlx::query_as::<_, FavoriteGrammarRecord>(
        r#"
        SELECT id, point, explanation, example, source_article_id, source_article_title, created_at
        FROM favorite_grammars
        WHERE user_id = $1
          AND (
              learning_item_id IS NULL OR EXISTS (
                  SELECT 1 FROM learning_items li
                  WHERE li.user_id = favorite_grammars.user_id
                    AND li.id = favorite_grammars.learning_item_id
                    AND li.status = 'accepted'
              )
          )
        ORDER BY created_at DESC
        "#,
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        records
            .into_iter()
            .map(favorite_grammar_from_record)
            .collect(),
    ))
}

pub async fn upsert_favorite_grammar(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<FavoriteGrammarDto>,
) -> Result<Json<FavoriteGrammarDto>, AppError> {
    validate_id(
        &payload.id,
        "invalid_favorite_grammar",
        "grammar id must not be empty",
    )?;
    validate_required(
        &payload.point,
        "invalid_favorite_grammar",
        "grammar point must not be empty",
    )?;
    validate_required(
        &payload.explanation,
        "invalid_favorite_grammar",
        "grammar explanation must not be empty",
    )?;

    let mut tx = state.pool.begin().await?;
    let record = sqlx::query_as::<_, FavoriteGrammarRecord>(
        r#"
        INSERT INTO favorite_grammars (
            user_id, id, point, explanation, example, source_article_id, source_article_title, created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (user_id, id) DO UPDATE
        SET point = EXCLUDED.point,
            explanation = EXCLUDED.explanation,
            example = EXCLUDED.example,
            source_article_id = EXCLUDED.source_article_id,
            source_article_title = EXCLUDED.source_article_title,
            created_at = EXCLUDED.created_at
        RETURNING id, point, explanation, example, source_article_id, source_article_title, created_at
        "#,
    )
    .bind(user.id)
    .bind(&payload.id)
    .bind(payload.point.trim())
    .bind(payload.explanation.trim())
    .bind(&payload.example)
    .bind(&payload.source_article_id)
    .bind(&payload.source_article_title)
    .bind(&payload.created_at)
    .fetch_one(&mut *tx)
    .await?;

    learning_items::canonicalize_favorite_grammar_tx(
        &mut tx,
        user.id,
        &payload.id,
        &payload.point,
        &payload.explanation,
        payload.example.as_deref(),
        payload.source_article_id.as_deref(),
        payload.source_article_title.as_deref(),
    )
    .await?;
    tx.commit().await?;

    Ok(Json(favorite_grammar_from_record(record)))
}

pub async fn get_favorite_grammar(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<FavoriteGrammarDto>, AppError> {
    let record = sqlx::query_as::<_, FavoriteGrammarRecord>(
        r#"
        SELECT id, point, explanation, example, source_article_id, source_article_title, created_at
        FROM favorite_grammars
        WHERE user_id = $1 AND id = $2
        "#,
    )
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| not_found("favorite_grammar_not_found", "favorite grammar not found"))?;

    Ok(Json(favorite_grammar_from_record(record)))
}

pub async fn delete_favorite_grammar(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, AppError> {
    let mut tx = state.pool.begin().await?;
    let learning_item_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT learning_item_id FROM favorite_grammars \
         WHERE user_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(user.id)
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| not_found("favorite_grammar_not_found", "favorite grammar not found"))?;
    archive_learning_item_for_projection_delete(&mut tx, user.id, learning_item_id, "grammar", &id)
        .await?;
    sqlx::query("DELETE FROM favorite_grammars WHERE user_id = $1 AND id = $2")
        .bind(user.id)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(DeleteResponse { deleted: true }))
}

pub async fn list_bookmarks(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Query(query): Query<ListBookmarksQuery>,
) -> Result<Json<Vec<BookmarkDto>>, AppError> {
    let records = if let Some(book_path) = query.book_path.filter(|value| !value.trim().is_empty())
    {
        sqlx::query_as::<_, BookmarkRecord>(
            r#"
            SELECT id, book_path, book_type, title, note, selected_text, page_number, epub_cfi,
                   created_at, color
            FROM bookmarks
            WHERE user_id = $1 AND book_path = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(user.id)
        .bind(book_path)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, BookmarkRecord>(
            r#"
            SELECT id, book_path, book_type, title, note, selected_text, page_number, epub_cfi,
                   created_at, color
            FROM bookmarks
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(user.id)
        .fetch_all(&state.pool)
        .await?
    };

    Ok(Json(
        records.into_iter().map(bookmark_from_record).collect(),
    ))
}

pub async fn upsert_bookmark(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    ApiJson(payload): ApiJson<BookmarkDto>,
) -> Result<Json<BookmarkDto>, AppError> {
    validate_bookmark(&payload)?;

    let record = sqlx::query_as::<_, BookmarkRecord>(
        r#"
        INSERT INTO bookmarks (
            user_id, id, book_path, book_type, title, note, selected_text, page_number,
            epub_cfi, created_at, color
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
        RETURNING id, book_path, book_type, title, note, selected_text, page_number, epub_cfi,
                  created_at, color
        "#,
    )
    .bind(user.id)
    .bind(payload.id)
    .bind(payload.book_path)
    .bind(payload.book_type)
    .bind(payload.title.trim())
    .bind(payload.note)
    .bind(payload.selected_text)
    .bind(payload.page_number)
    .bind(payload.epub_cfi)
    .bind(payload.created_at)
    .bind(payload.color)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(bookmark_from_record(record)))
}

pub async fn patch_bookmark(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
    ApiJson(payload): ApiJson<BookmarkDto>,
) -> Result<Json<BookmarkDto>, AppError> {
    if id != payload.id {
        return Err(AppError::bad_request(
            "bookmark_id_mismatch",
            "bookmark id does not match path",
        ));
    }

    upsert_bookmark(State(state), AuthenticatedUser { user }, ApiJson(payload)).await
}

pub async fn get_bookmark(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<BookmarkDto>, AppError> {
    let record = sqlx::query_as::<_, BookmarkRecord>(
        r#"
        SELECT id, book_path, book_type, title, note, selected_text, page_number, epub_cfi,
               created_at, color
        FROM bookmarks
        WHERE user_id = $1 AND id = $2
        "#,
    )
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| not_found("bookmark_not_found", "bookmark not found"))?;

    Ok(Json(bookmark_from_record(record)))
}

pub async fn delete_bookmark(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<DeleteResponse>, AppError> {
    delete_by_id(&state.pool, "bookmarks", user.id, &id, "bookmark_not_found").await?;
    Ok(Json(DeleteResponse { deleted: true }))
}

pub async fn upsert_agent_task(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
    ApiJson(payload): ApiJson<AgentTaskDto>,
) -> Result<Json<AgentTaskDto>, AppError> {
    if id != payload.id {
        return Err(AppError::bad_request(
            "agent_task_id_mismatch",
            "agent task id does not match path",
        ));
    }
    validate_agent_task(&payload)?;

    let record = sqlx::query_as::<_, AgentTaskRecord>(
        r#"
        INSERT INTO agent_tasks (
            user_id, id, task_type, status, article_id, input, progress, stage, message, error,
            worker_session_id, artifact_ids, created_at, updated_at, started_at, finished_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
        ON CONFLICT (user_id, id) DO UPDATE
        SET task_type = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN EXCLUDED.task_type
                ELSE agent_tasks.task_type
            END,
            status = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN EXCLUDED.status
                ELSE agent_tasks.status
            END,
            article_id = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN EXCLUDED.article_id
                ELSE agent_tasks.article_id
            END,
            input = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN EXCLUDED.input
                ELSE agent_tasks.input
            END,
            progress = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN EXCLUDED.progress
                ELSE agent_tasks.progress
            END,
            stage = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN EXCLUDED.stage
                ELSE agent_tasks.stage
            END,
            message = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN EXCLUDED.message
                ELSE agent_tasks.message
            END,
            error = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN EXCLUDED.error
                ELSE agent_tasks.error
            END,
            worker_session_id = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN EXCLUDED.worker_session_id
                ELSE agent_tasks.worker_session_id
            END,
            artifact_ids = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN (
                    SELECT COALESCE(jsonb_agg(value ORDER BY value), '[]'::jsonb)
                    FROM (
                        SELECT DISTINCT value
                        FROM jsonb_array_elements_text(agent_tasks.artifact_ids || EXCLUDED.artifact_ids) AS merged(value)
                    ) merged_values
                )
                ELSE agent_tasks.artifact_ids
            END,
            created_at = agent_tasks.created_at,
            updated_at = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN EXCLUDED.updated_at
                ELSE agent_tasks.updated_at
            END,
            started_at = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN COALESCE(agent_tasks.started_at, EXCLUDED.started_at)
                ELSE agent_tasks.started_at
            END,
            finished_at = CASE
                WHEN EXCLUDED.updated_at >= agent_tasks.updated_at
                     AND NOT (
                         agent_tasks.status IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                         AND EXCLUDED.status NOT IN ('succeeded', 'failed', 'cancelled', 'interrupted')
                     )
                THEN EXCLUDED.finished_at
                ELSE agent_tasks.finished_at
            END
        RETURNING id, task_type, status, article_id, input, progress, stage, message, error,
                  worker_session_id, artifact_ids, created_at, updated_at, started_at, finished_at
        "#,
    )
    .bind(user.id)
    .bind(payload.id)
    .bind(payload.task_type)
    .bind(payload.status)
    .bind(payload.article_id)
    .bind(payload.input)
    .bind(payload.progress.clamp(0.0, 1.0))
    .bind(payload.stage)
    .bind(payload.message)
    .bind(payload.error)
    .bind(payload.worker_session_id)
    .bind(serde_json::json!(payload.artifact_ids))
    .bind(payload.created_at)
    .bind(payload.updated_at)
    .bind(payload.started_at)
    .bind(payload.finished_at)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(agent_task_from_record(record)))
}

pub async fn get_agent_task(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<AgentTaskDto>, AppError> {
    let record = sqlx::query_as::<_, AgentTaskRecord>(
        r#"
        SELECT id, task_type, status, article_id, input, progress, stage, message, error,
               worker_session_id, artifact_ids, created_at, updated_at, started_at, finished_at
        FROM agent_tasks
        WHERE user_id = $1 AND id = $2
        "#,
    )
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| not_found("agent_task_not_found", "agent task not found"))?;

    Ok(Json(agent_task_from_record(record)))
}

pub async fn upsert_artifact(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
    ApiJson(payload): ApiJson<ArtifactDto>,
) -> Result<Json<ArtifactDto>, AppError> {
    if id != payload.id {
        return Err(AppError::bad_request(
            "artifact_id_mismatch",
            "artifact id does not match path",
        ));
    }
    validate_artifact(&payload)?;

    let record = sqlx::query_as::<_, ArtifactRecord>(
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
        RETURNING id, task_id, article_id, artifact_type, version, content, metadata, created_at,
                  updated_at
        "#,
    )
    .bind(user.id)
    .bind(payload.id)
    .bind(payload.task_id)
    .bind(payload.article_id)
    .bind(payload.artifact_type)
    .bind(payload.version)
    .bind(payload.content)
    .bind(payload.metadata)
    .bind(payload.created_at)
    .bind(payload.updated_at)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(artifact_from_record(record)))
}

pub async fn get_artifact(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path((article_id, id)): Path<(String, String)>,
) -> Result<Json<ArtifactDto>, AppError> {
    let record = sqlx::query_as::<_, ArtifactRecord>(
        r#"
        SELECT id, task_id, article_id, artifact_type, version, content, metadata, created_at,
               updated_at
        FROM artifacts
        WHERE user_id = $1 AND article_id = $2 AND id = $3
        "#,
    )
    .bind(user.id)
    .bind(article_id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| not_found("artifact_not_found", "artifact not found"))?;

    Ok(Json(artifact_from_record(record)))
}

async fn ensure_default_word_pack(pool: &PgPool, user_id: Uuid) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO word_packs (
            user_id, id, name, description, cover_url, author, language_from, language_to,
            tags, version, created_at, updated_at, is_system
        )
        VALUES ($1, $2, $3, $4, NULL, $5, NULL, NULL, $6, $7, $8, $8, TRUE)
        ON CONFLICT (user_id, id) DO NOTHING
        "#,
    )
    .bind(user_id)
    .bind(DEFAULT_UNGROUPED_PACK_ID)
    .bind(DEFAULT_UNGROUPED_PACK_NAME)
    .bind("系统默认合集")
    .bind("OpenKoto")
    .bind(serde_json::json!(["system"]))
    .bind("1.0.0")
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

async fn upsert_word_pack_record(
    pool: &PgPool,
    user_id: Uuid,
    payload: WordPackDto,
) -> Result<WordPackRecord, AppError> {
    let is_default_system_pack = payload.id == DEFAULT_UNGROUPED_PACK_ID;
    let record = sqlx::query_as::<_, WordPackRecord>(
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
            created_at = EXCLUDED.created_at,
            updated_at = EXCLUDED.updated_at,
            is_system = word_packs.is_system
        RETURNING id, name, description, cover_url, author, language_from, language_to, tags,
                  version, created_at, updated_at, is_system
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
    .bind(is_default_system_pack)
    .fetch_one(pool)
    .await?;

    Ok(record)
}

async fn upsert_favorite_vocabulary_record(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    payload: &FavoriteVocabularyDto,
) -> Result<(), AppError> {
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
    .bind(&payload.srs_state)
    .bind(payload.ease_factor)
    .bind(payload.repetitions)
    .bind(payload.interval_days)
    .bind(&payload.due_date)
    .bind(&payload.last_reviewed_at)
    .bind(payload.review_count)
    .bind(&payload.created_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn replace_vocabulary_pack_links(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    vocabulary_id: &str,
    pack_ids: &[String],
) -> Result<(), AppError> {
    let mut cleaned = dedupe_ids(pack_ids);
    if cleaned.is_empty() {
        cleaned.push(DEFAULT_UNGROUPED_PACK_ID.to_string());
    }

    let existing_pack_ids = sqlx::query_scalar::<_, String>(
        r#"
        SELECT id
        FROM word_packs
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_all(&mut **tx)
    .await?;
    cleaned.retain(|id| existing_pack_ids.iter().any(|existing| existing == id));
    if cleaned.is_empty() {
        cleaned.push(DEFAULT_UNGROUPED_PACK_ID.to_string());
    }

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

async fn fetch_favorite_vocabulary_record(
    pool: &PgPool,
    user_id: Uuid,
    id: &str,
) -> Result<FavoriteVocabularyRecord, AppError> {
    sqlx::query_as::<_, FavoriteVocabularyRecord>(
        r#"
        SELECT id, word, meaning, usage, explanation, example, reading, source_article_id,
               source_article_title, srs_state, ease_factor, repetitions, interval_days,
               due_date, last_reviewed_at, review_count, created_at
        FROM favorite_vocabularies
        WHERE user_id = $1 AND id = $2
        "#,
    )
    .bind(user_id)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        not_found(
            "favorite_vocabulary_not_found",
            "favorite vocabulary not found",
        )
    })
}

async fn favorite_vocabulary_from_record(
    pool: &PgPool,
    user_id: Uuid,
    record: FavoriteVocabularyRecord,
) -> Result<FavoriteVocabularyDto, AppError> {
    let mut pack_ids = sqlx::query_scalar::<_, String>(
        r#"
        SELECT pack_id
        FROM favorite_vocabulary_packs
        WHERE user_id = $1 AND vocabulary_id = $2
        ORDER BY pack_id ASC
        "#,
    )
    .bind(user_id)
    .bind(&record.id)
    .fetch_all(pool)
    .await?;

    if pack_ids.is_empty() {
        pack_ids.push(DEFAULT_UNGROUPED_PACK_ID.to_string());
    }

    Ok(FavoriteVocabularyDto {
        id: record.id,
        word: record.word,
        meaning: record.meaning,
        usage: record.usage,
        explanation: record.explanation,
        example: record.example,
        reading: record.reading,
        source_article_id: record.source_article_id,
        source_article_title: record.source_article_title,
        pack_ids,
        srs_state: record.srs_state,
        ease_factor: record.ease_factor,
        repetitions: record.repetitions,
        interval_days: record.interval_days,
        due_date: record.due_date,
        last_reviewed_at: record.last_reviewed_at,
        review_count: record.review_count,
        created_at: record.created_at,
    })
}

fn word_pack_from_record(record: WordPackRecord) -> WordPackDto {
    WordPackDto {
        id: record.id,
        name: record.name,
        description: record.description,
        cover_url: record.cover_url,
        author: record.author,
        language_from: record.language_from,
        language_to: record.language_to,
        tags: tags_from_value(record.tags),
        version: record.version,
        created_at: record.created_at,
        updated_at: record.updated_at,
        is_system: record.is_system,
    }
}

fn favorite_grammar_from_record(record: FavoriteGrammarRecord) -> FavoriteGrammarDto {
    FavoriteGrammarDto {
        id: record.id,
        point: record.point,
        explanation: record.explanation,
        example: record.example,
        source_article_id: record.source_article_id,
        source_article_title: record.source_article_title,
        created_at: record.created_at,
    }
}

fn bookmark_from_record(record: BookmarkRecord) -> BookmarkDto {
    BookmarkDto {
        id: record.id,
        book_path: record.book_path,
        book_type: record.book_type,
        title: record.title,
        note: record.note,
        selected_text: record.selected_text,
        page_number: record.page_number,
        epub_cfi: record.epub_cfi,
        created_at: record.created_at,
        color: record.color,
    }
}

fn agent_task_from_record(record: AgentTaskRecord) -> AgentTaskDto {
    AgentTaskDto {
        id: record.id,
        task_type: record.task_type,
        status: record.status,
        article_id: record.article_id,
        input: record.input,
        progress: record.progress,
        stage: record.stage,
        message: record.message,
        error: record.error,
        worker_session_id: record.worker_session_id,
        artifact_ids: string_vec_from_value(record.artifact_ids),
        created_at: record.created_at,
        updated_at: record.updated_at,
        started_at: record.started_at,
        finished_at: record.finished_at,
    }
}

fn artifact_from_record(record: ArtifactRecord) -> ArtifactDto {
    ArtifactDto {
        id: record.id,
        task_id: record.task_id,
        article_id: record.article_id,
        artifact_type: record.artifact_type,
        version: record.version,
        content: record.content,
        metadata: record.metadata,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn tags_from_value(value: Value) -> Vec<String> {
    string_vec_from_value(value)
}

fn string_vec_from_value(value: Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn dedupe_ids(ids: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

async fn delete_by_id(
    pool: &PgPool,
    table_name: &str,
    user_id: Uuid,
    id: &str,
    error_code: &'static str,
) -> Result<(), AppError> {
    let sql = format!("DELETE FROM {table_name} WHERE user_id = $1 AND id = $2 RETURNING id");
    let deleted = sqlx::query_scalar::<_, String>(&sql)
        .bind(user_id)
        .bind(id)
        .fetch_optional(pool)
        .await?;

    if deleted.is_none() {
        return Err(not_found(error_code, "record not found"));
    }

    Ok(())
}

async fn archive_learning_item_for_projection_delete(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    learning_item_id: Option<Uuid>,
    favorite_type: &str,
    favorite_id: &str,
) -> Result<(), AppError> {
    let Some(learning_item_id) = learning_item_id else {
        return Ok(());
    };
    let item = sqlx::query_as::<_, (String, Option<Uuid>)>(
        "SELECT status, material_id FROM learning_items \
         WHERE user_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(user_id)
    .bind(learning_item_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((status, material_id)) = item else {
        return Ok(());
    };
    if status == "accepted" {
        sqlx::query(
            r#"
            UPDATE learning_items
            SET status_before_archive = 'accepted', status = 'archived', updated_at = NOW()
            WHERE user_id = $1 AND id = $2
            "#,
        )
        .bind(user_id)
        .bind(learning_item_id)
        .execute(&mut **tx)
        .await?;
        learning_activity::record_event_tx(
            tx,
            user_id,
            Some(learning_item_id),
            material_id,
            "archive",
            serde_json::json!({
                "from_status": "accepted",
                "to_status": "archived",
                "origin": "favorite_projection_delete",
                "favorite_type": favorite_type,
                "favorite_id": favorite_id,
            }),
            None,
        )
        .await?;
    }
    Ok(())
}

fn validate_favorite_vocabulary(payload: &FavoriteVocabularyDto) -> Result<(), AppError> {
    validate_id(
        &payload.id,
        "invalid_favorite_vocabulary",
        "favorite vocabulary id must not be empty",
    )?;
    validate_required(
        &payload.word,
        "invalid_favorite_vocabulary",
        "favorite vocabulary word must not be empty",
    )?;
    validate_required(
        &payload.meaning,
        "invalid_favorite_vocabulary",
        "favorite vocabulary meaning must not be empty",
    )?;
    if !matches!(payload.srs_state.as_str(), "new" | "learning" | "review") {
        return Err(AppError::bad_request(
            "invalid_favorite_vocabulary",
            "favorite vocabulary srs state is invalid",
        ));
    }
    if payload.ease_factor < 1.3
        || payload.repetitions < 0
        || payload.interval_days < 0
        || payload.review_count < 0
    {
        return Err(AppError::bad_request(
            "invalid_favorite_vocabulary",
            "favorite vocabulary srs values are invalid",
        ));
    }

    Ok(())
}

fn validate_bookmark(payload: &BookmarkDto) -> Result<(), AppError> {
    validate_id(
        &payload.id,
        "invalid_bookmark",
        "bookmark id must not be empty",
    )?;
    validate_required(
        &payload.book_path,
        "invalid_bookmark",
        "bookmark book path must not be empty",
    )?;
    validate_required(
        &payload.title,
        "invalid_bookmark",
        "bookmark title must not be empty",
    )?;
    if !matches!(payload.book_type.as_str(), "epub" | "txt" | "pdf") {
        return Err(AppError::bad_request(
            "invalid_bookmark",
            "bookmark book type is invalid",
        ));
    }
    if payload.page_number.is_some_and(|page| page <= 0) {
        return Err(AppError::bad_request(
            "invalid_bookmark",
            "bookmark page number must be positive",
        ));
    }

    Ok(())
}

fn validate_agent_task(payload: &AgentTaskDto) -> Result<(), AppError> {
    validate_id(
        &payload.id,
        "invalid_agent_task",
        "agent task id must not be empty",
    )?;
    validate_required(
        &payload.article_id,
        "invalid_agent_task",
        "agent task article id must not be empty",
    )?;
    if !payload.input.is_object() {
        return Err(AppError::bad_request(
            "invalid_agent_task",
            "agent task input must be an object",
        ));
    }

    Ok(())
}

fn validate_artifact(payload: &ArtifactDto) -> Result<(), AppError> {
    validate_id(
        &payload.id,
        "invalid_artifact",
        "artifact id must not be empty",
    )?;
    validate_required(
        &payload.task_id,
        "invalid_artifact",
        "artifact task id must not be empty",
    )?;
    validate_required(
        &payload.article_id,
        "invalid_artifact",
        "artifact article id must not be empty",
    )?;
    validate_required(
        &payload.artifact_type,
        "invalid_artifact",
        "artifact type must not be empty",
    )?;
    validate_required(
        &payload.version,
        "invalid_artifact",
        "artifact version must not be empty",
    )?;

    Ok(())
}

fn validate_id(id: &str, code: &'static str, message: &'static str) -> Result<(), AppError> {
    validate_required(id, code, message)
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

fn default_srs_state() -> String {
    "new".to_string()
}

fn default_srs_ease_factor() -> f64 {
    2.5
}

fn not_found(code: &'static str, message: &'static str) -> AppError {
    AppError::not_found(code, message)
}
